//! Hostile input for the minimal importer: the panics the #473 review found (a multi-byte root
//! `@id`, an overflowing `sizeOfDimensions`), lexical traps `serde_json` would resolve silently
//! (a duplicate key, an integer past `i64`), structural traps (giant lists, a datatype that
//! disagrees with its value, an unchecked `@type`), and a bounded deterministic mutation sweep
//! over every document the importer reads. Every case returns a typed result; none panics.

mod support;

use oce_author::{ImportError, import_cxf};

use support::cxf::corpus_fixture;
use support::fixture;

/// The corpus fixtures the importer reads (see `library_round_trip.rs`).
const IMPORTED: &[&str] = &[
    "cooling_only_system_requests",
    "generic_time_suppression",
    "multizone_vav_relief_fan_group",
    "reheat_overrides",
];

/// Every document the importer reads: the four corpus fixtures as committed, and every emitted
/// golden (the worked example and the every-element sequence included).
fn documents() -> Vec<(String, Vec<u8>)> {
    let mut all: Vec<(String, Vec<u8>)> = IMPORTED
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                std::fs::read(corpus_fixture(name)).expect("fixture"),
            )
        })
        .collect();
    let mut goldens: Vec<_> = std::fs::read_dir(fixture("emit"))
        .expect("goldens")
        .map(|e| e.expect("entry").path())
        .collect();
    goldens.sort();
    for path in goldens {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("name")
            .to_owned();
        all.push((name, std::fs::read(&path).expect("golden")));
    }
    all
}

fn econ() -> String {
    std::fs::read_to_string(fixture("emit/econ_enable.cxf.json")).expect("golden")
}

fn edited(from: &str, to: &str) -> Vec<u8> {
    let text = econ();
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1).into_bytes()
}

fn is_unsupported(bytes: &[u8], construct: &str) {
    match import_cxf(bytes) {
        Err(error @ ImportError::Unsupported { .. }) => {
            assert!(error.to_string().contains(construct), "{error}");
        }
        other => panic!("expected `{construct}`, got {other:?}"),
    }
}

fn is_malformed(bytes: &[u8], detail: &str) {
    match import_cxf(bytes) {
        Err(ImportError::Malformed { detail: got }) => {
            assert!(got.contains(detail), "{got}");
        }
        other => panic!("expected malformed `{detail}`, got {other:?}"),
    }
}

#[test]
fn a_multi_byte_root_id_is_refused_without_panicking() {
    // The review's reproduction: the last delimiter search used to slice inside `é`.
    let bytes =
        br#"{"@graph":[{"@id":"urn:x:\u00e9","@type":"S231:Block","S231:containsBlock":[]}]}"#;
    let raw = "{\"@graph\":[{\"@id\":\"urn:x:\u{e9}\",\"@type\":\"S231:Block\",\"S231:containsBlock\":[]}]}";
    for doc in [&bytes[..], raw.as_bytes()] {
        is_unsupported(doc, "does not end in an identifier");
    }
    // A multi-byte character at every position of the root `@id`.
    let root = "urn:oce:author:econ_enable";
    for at in 0..=root.len() {
        for ch in ['\u{e9}', '\u{20ac}', '\u{1f321}'] {
            let mut id = root.to_owned();
            id.insert(at, ch);
            let doc = econ().replace(&format!("\"{root}\""), &format!("\"{id}\""));
            // Either the name is no identifier or the members are no longer under the root.
            assert!(
                matches!(
                    import_cxf(doc.as_bytes()),
                    Err(ImportError::Unsupported { .. } | ImportError::Unresolved { .. })
                ),
                "{id}"
            );
        }
    }
}

#[test]
fn a_multi_byte_character_at_every_position_of_a_member_id_is_typed() {
    let member = "urn:oce:author:econ_enable.hys.uHigh";
    for at in 0..=member.len() {
        let mut id = member.to_owned();
        id.insert(at, '\u{e9}');
        let doc = econ().replace(&format!("\"{member}\""), &format!("\"{id}\""));
        // Both the relation and the node move together, so this may read; it must not panic.
        let _ = import_cxf(doc.as_bytes());
        let one = econ().replacen(&format!("\"{member}\""), &format!("\"{id}\""), 1);
        assert!(import_cxf(one.as_bytes()).is_err(), "{at}");
    }
}

/// The `uLow` value of the worked example's golden.
const LOW_VALUE: &str = "\"S231:value\": {\n        \"@value\": \"0.0\",\n        \"@type\": \"http://www.w3.org/2001/XMLSchema#double\"\n      }";

#[test]
fn hostile_array_sizes_are_refused_with_a_typed_error() {
    // Each refuses before any element is read; the first used to overflow `usize` (a panic in
    // debug, an empty array accepted in release).
    let cases = [
        ("(4294967296, 4294967296)", "2"),
        ("(18446744073709551615, 2)", "2"),
        ("(99999999999999999999999)", "1"),
        ("(0)", "1"),
        ("(-1)", "1"),
        ("(2, 0)", "2"),
        ("()", "0"),
        ("(1, 1, 1)", "3"),
        ("(2", "1"),
    ];
    for (size, rank) in cases {
        let text = String::from_utf8(econ().into_bytes()).expect("utf8");
        let node = "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",";
        assert!(text.contains(node));
        let doc = text.replacen(
            node,
            &format!(
                "{node}\n      \"S231:isArray\": true, \"S231:numberDimensions\": {rank}, \"S231:sizeOfDimensions\": \"{size}\","
            ),
            1,
        );
        // A list value, so the refusal is the size check itself, not "not a list".
        let doc = doc.replacen(LOW_VALUE, "\"S231:value\": [1.0, 2.0]", 1);
        match import_cxf(doc.as_bytes()) {
            Err(error @ ImportError::Unsupported { .. }) => {
                let text = error.to_string();
                assert!(
                    text.contains("sizeOfDimensions") || text.contains("element count"),
                    "{size}: {text}"
                );
            }
            other => panic!("{size}: {other:?}"),
        }
    }
}

#[test]
fn an_integer_beyond_i64_is_refused_not_guessed_as_a_real() {
    is_malformed(
        &edited(
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": 9223372036854775808",
        ),
        "outside the i64 range",
    );
    is_malformed(
        &edited(
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": 18446744073709551615",
        ),
        "outside the i64 range",
    );
    // `i64::MIN` and `i64::MAX` themselves read.
    for edge in ["-9223372036854775808", "9223372036854775807"] {
        let doc = edited(
            "\"S231:value\": \"dTHys\"",
            &format!("\"S231:value\": {edge}"),
        );
        assert!(import_cxf(&doc).is_ok(), "{edge}");
    }
}

#[test]
fn a_key_written_twice_is_refused() {
    is_malformed(
        &edited(
            "\"S231:value\": \"dTHys\"",
            "\"S231:value\": 1.0, \"S231:value\": \"dTHys\"",
        ),
        "written twice",
    );
    is_malformed(
        &edited(
            "\"@type\": \"S231:Block\",",
            "\"@type\": \"S231:Block\", \"@type\": \"S231:Block\",",
        ),
        "written twice",
    );
}

#[test]
fn a_value_that_disagrees_with_its_datatype_is_refused() {
    let low = "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Real\"";
    for (datatype, construct) in [
        ("S231:Boolean", "disagrees with its `isOfDataType`"),
        ("S231:String", "disagrees with its `isOfDataType`"),
        ("S231:Integer", "disagrees with its `isOfDataType`"),
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController",
            "disagrees with its `isOfDataType`",
        ),
        (
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes",
            "outside the S231 scalars",
        ),
        ("S231:Bogus", "outside the S231 scalars"),
    ] {
        let to = low.replace("S231:Real", datatype);
        is_unsupported(&edited(low, &to), construct);
    }
    // A reference agrees with any datatype; a Real takes an Integer literal.
    let high = "\"@id\": \"urn:oce:author:econ_enable.hys.uHigh\",\n      \"@type\": \"S231:Parameter\",\n      \"S231:isOfDataType\": {\n        \"@id\": \"S231:Real\"";
    assert!(import_cxf(&edited(high, &high.replace("S231:Real", "S231:Boolean"))).is_ok());
}

#[test]
fn a_parameter_type_other_than_parameter_is_refused() {
    is_unsupported(
        &edited(
            "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",",
            "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Constant\",",
        ),
        "`@type` is not `S231:Parameter`",
    );
    is_unsupported(
        &edited(
            "\"@id\": \"urn:oce:author:econ_enable.dTHys\",\n      \"@type\": \"S231:Parameter\",",
            "\"@id\": \"urn:oce:author:econ_enable.dTHys\",\n      \"@type\": [\"S231:Parameter\"],",
        ),
        "`@type` is not `S231:Parameter`",
    );
    is_malformed(
        &edited("\"@type\": \"S231:Block\",", "\"@type\": [\"S231:Block\"],"),
        "the root `@type` is not one string",
    );
}

#[test]
fn giant_lists_are_refused_in_bounded_time() {
    let start = std::time::Instant::now();
    // 100,000 references to nodes that do not exist (about 5 MB, under the document bound).
    let refs: Vec<String> = (0..100_000)
        .map(|i| format!("{{\"@id\": \"urn:oce:author:econ_enable.p{i}\"}}"))
        .collect();
    let doc = edited(
        "\"S231:hasParameter\": [",
        &format!("\"S231:hasParameter\": [{},", refs.join(",")),
    );
    assert!(matches!(
        import_cxf(&doc),
        Err(ImportError::Unresolved { .. })
    ));
    // A 1,000,000-element array value whose declared size is (2).
    let items = vec!["1.0"; 1_000_000].join(",");
    let node =
        "\"@id\": \"urn:oce:author:econ_enable.hys.uLow\",\n      \"@type\": \"S231:Parameter\",";
    let text = econ();
    let doc = text.replacen(
        node,
        &format!("{node}\n      \"S231:isArray\": true, \"S231:numberDimensions\": 1, \"S231:sizeOfDimensions\": \"(2)\","),
        1,
    );
    let doc = doc.replacen(LOW_VALUE, &format!("\"S231:value\": [{items}]"), 1);
    is_unsupported(doc.as_bytes(), "element count disagrees");
    assert!(start.elapsed() < std::time::Duration::from_secs(60));
}

/// The most offsets a sweep visits in one document.
const MAX_OFFSETS: usize = 300;

/// The offsets a sweep visits, at most [`MAX_OFFSETS`]: half spread evenly over the structural
/// bytes (`"`, `\`, braces, brackets, `:`, `,`) and the digit boundaries, half evenly over every
/// offset, so a large document is probed at its syntax and not only mid-string.
fn offsets(bytes: &[u8]) -> Vec<usize> {
    let structural: Vec<usize> = (0..bytes.len())
        .filter(|&i| {
            matches!(
                bytes[i],
                b'"' | b'\\' | b'{' | b'}' | b'[' | b']' | b':' | b','
            ) || (bytes[i].is_ascii_digit() != bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        })
        .collect();
    let spread = |all: &[usize], count: usize| -> Vec<usize> {
        let stride = all.len().div_ceil(count.max(1)).max(1);
        all.iter().copied().step_by(stride).collect()
    };
    let every: Vec<usize> = (0..bytes.len()).collect();
    let mut chosen = spread(&structural, MAX_OFFSETS / 2);
    chosen.extend(spread(&every, MAX_OFFSETS / 2));
    chosen.sort_unstable();
    chosen.dedup();
    chosen
}

/// Import `bytes`, turning a panic into a failure that names the mutant.
fn import_named(bytes: &[u8], document: &str, at: usize, mutation: &str) -> Result<(), ()> {
    match std::panic::catch_unwind(|| import_cxf(bytes)) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => Err(()),
        Err(_) => panic!("import_cxf panicked on {document} at byte {at} ({mutation})"),
    }
}

#[test]
fn a_bounded_mutation_sweep_over_every_document_never_panics() {
    // Per offset: bit flips, a multi-byte character, an escape, a quote, a backslash, a lone
    // surrogate escape or a bracket replacing the byte or inserted before it, and the truncation
    // there. A document above 16 KiB gets a reduced mutation set (one flip, a quote replacing or
    // inserted, the truncation; above 128 KiB only inserted), so every document runs a few
    // hundred offsets at bounded cost (about 20 s in a debug build).
    let full: [&[u8]; 8] = [
        "\u{e9}".as_bytes(),
        "\u{1f321}".as_bytes(),
        b"\\",
        b"\"",
        b"\\u0000",
        b"\\ud800",
        b"[",
        b"}",
    ];
    let reduced: [&[u8]; 1] = [full[3]];
    let mut cases = 0usize;
    for (name, bytes) in documents() {
        // A document the importer already refuses unmutated makes a weaker sweep (most mutations
        // stop at the same refusal) until the importer PR reads it.
        let read = import_cxf(&bytes);
        if support::BEYOND_MINIMAL_IMPORTER.contains(&name.as_str()) {
            support::assert_beyond_minimal_refusal(&name, &read);
        } else {
            assert!(read.is_ok(), "{name} reads unmutated: {read:?}");
        }
        let large = bytes.len() > 16 * 1024;
        // Above 128 KiB the replacement is dropped too: a flip, an insertion, a truncation.
        let huge = bytes.len() > 128 * 1024;
        let (flips, insertions): (&[u8], &[&[u8]]) = if large {
            (&[0x80], &reduced)
        } else {
            (&[0x01, 0x20, 0x80], &full)
        };
        let visited = offsets(&bytes);
        assert!(
            visited.len() >= 250.min(bytes.len()),
            "{name}: {} offsets",
            visited.len()
        );
        for at in visited {
            for &flip in flips {
                let mut mutated = bytes.clone();
                mutated[at] ^= flip;
                let _ = import_named(&mutated, &name, at, &format!("xor {flip:#04x}"));
                cases += 1;
            }
            for insert in insertions {
                let label = String::from_utf8_lossy(insert).into_owned();
                if !huge {
                    let mut replaced = bytes[..at].to_vec();
                    replaced.extend_from_slice(insert);
                    replaced.extend_from_slice(&bytes[at + 1..]);
                    let _ = import_named(&replaced, &name, at, &format!("replace with {label:?}"));
                    cases += 1;
                }
                let mut inserted = bytes[..at].to_vec();
                inserted.extend_from_slice(insert);
                inserted.extend_from_slice(&bytes[at..]);
                let _ = import_named(&inserted, &name, at, &format!("insert {label:?}"));
                cases += 1;
            }
            if at + 1 < bytes.len() {
                assert!(
                    import_named(&bytes[..at], &name, at, "truncate").is_err(),
                    "{name} truncated at {at}"
                );
                cases += 1;
            }
        }
    }
    assert!(cases > 10_000, "{cases} cases");
}

#[test]
fn integer_tokens_outside_i64_are_refused_at_any_depth() {
    for token in [
        "-9223372036854775809",
        "9223372036854775808",
        "18446744073709551616",
        "-18446744073709551616",
        "123456789012345678901234567890",
        "-123456789012345678901234567890",
    ] {
        is_malformed(
            &edited(
                "\"S231:value\": \"dTHys\"",
                &format!("\"S231:value\": {token}"),
            ),
            "outside the i64 range",
        );
        // Deep in an unread key and inside an array: still refused, lexically.
        is_malformed(
            &edited(
                "\"@type\": \"S231:Block\",",
                &format!("\"@type\": \"S231:Block\", \"x:y\": [[{{\"z\": [{token}]}}]],"),
            ),
            "outside the i64 range",
        );
    }
    // A real-shaped token of any size is a Real, and a digit run inside a string is text.
    for real in ["1e400", "123456789012345678901234567890.0", "-1.5e-3"] {
        let doc = edited(
            "\"S231:value\": \"dTHys\"",
            &format!("\"S231:value\": {real}"),
        );
        assert!(
            !matches!(import_cxf(&doc), Err(ImportError::Malformed { ref detail }) if detail.contains("i64")),
            "{real}"
        );
    }
}

#[test]
fn negative_zero_reads_as_the_integer_zero_as_on_the_wire() {
    let doc = edited("\"S231:value\": \"dTHys\"", "\"S231:value\": -0");
    let sequence = import_cxf(&doc).expect("reads");
    let hys = sequence
        .blocks
        .iter()
        .find(|b| b.name.as_str() == "hys")
        .expect("hys");
    let high = hys
        .assignments
        .iter()
        .find(|a| a.name.as_str() == "uHigh")
        .expect("uHigh");
    assert_eq!(
        high.value,
        oce_author::ParamValue::Literal(oce_author::Literal::Integer(0))
    );
    // `-0.0` is the Real negative zero.
    let doc = edited("\"S231:value\": \"dTHys\"", "\"S231:value\": -0.0");
    let sequence = import_cxf(&doc).expect("reads");
    let value = &sequence.blocks[1].assignments[1].value;
    assert!(
        matches!(value, oce_author::ParamValue::Literal(oce_author::Literal::Real(v)) if v.to_bits() == (-0.0f64).to_bits()),
        "{value:?}"
    );
}

#[test]
fn nesting_is_pinned_at_the_recursion_limit() {
    // Total nesting depth = the top object plus `k` arrays. 127 levels parse (and are then
    // refused for their shape); 128 and 129 hit `serde_json`'s recursion limit. Neither panics.
    let nested = |depth: usize| {
        let k = depth - 1;
        format!("{{\"@graph\": {}{}}}", "[".repeat(k), "]".repeat(k))
    };
    is_malformed(nested(127).as_bytes(), "a `@graph` entry is not an object");
    is_malformed(nested(128).as_bytes(), "recursion limit exceeded");
    is_malformed(nested(129).as_bytes(), "recursion limit exceeded");
}
