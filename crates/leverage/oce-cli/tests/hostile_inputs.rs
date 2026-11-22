//! Hostile inputs through the built binary: binary junk, bad UTF-8, an over-size file refused
//! from its metadata before a byte is read, an over-size stdin cut off one byte past the bound,
//! a directory, a missing path, a wire object written as the list of its values (GitHub #544).
//! Every one is exit 2 with one typed error code, a well-formed JSON report on stdout, nothing on
//! stderr and no panic.

mod support;

use std::fs::File;

use oce_author::{MAX_TEXT_BYTES, MAX_WIRE_BYTES};
use support::{Run, crate_dir, oce_in, scratch};

/// The single error code of a refused run, after checking the run's shape.
fn refusal(run: &Run) -> String {
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["ok"], false);
    assert_eq!(report["findings"], serde_json::json!([]));
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    errors[0]["code"].as_str().expect("code").to_owned()
}

#[test]
fn binary_junk_is_refused_in_both_forms() {
    let dir = scratch("junk");
    let junk: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    std::fs::write(dir.join("junk.oce"), &junk).expect("write");
    std::fs::write(dir.join("junk.json"), &junk).expect("write");
    for command in ["validate", "emit", "fmt", "id"] {
        let text = refusal(&oce_in(&dir, &[command, "junk.oce"], b""));
        assert!(oce_author::PARSE_CODES.contains(&text.as_str()), "{text}");
        let wire = refusal(&oce_in(&dir, &[command, "junk.json"], b""));
        assert!(!wire.is_empty());
    }
}

#[test]
fn bad_utf8_is_refused_as_not_utf8() {
    let dir = scratch("bad_utf8");
    let bytes = b"sequence s\nderived_from none\nparam x : Real = \"\xff\"\n";
    std::fs::write(dir.join("bad.oce"), bytes).expect("write");
    std::fs::write(dir.join("bad.json"), b"{\"sequence\": \"\xc3\x28\"}").expect("write");
    for command in ["validate", "emit", "fmt", "id"] {
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "bad.oce"], b"")),
            "not-utf8"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "bad.json"], b"")),
            "not-utf8"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "-", "--from", "text"], bytes)),
            "not-utf8"
        );
    }
}

#[test]
fn a_hundred_megabyte_file_is_refused_before_it_is_read() {
    let dir = scratch("huge");
    for name in ["huge.oce", "huge.json"] {
        // A sparse file: 100 MB by its metadata, no data written.
        File::create(dir.join(name))
            .and_then(|file| file.set_len(100 * 1024 * 1024))
            .expect("create a sparse file");
        for command in ["validate", "emit", "fmt", "id"] {
            let run = oce_in(&dir, &[command, name], b"");
            assert_eq!(refusal(&run), "input-too-large");
            // The metadata path names the size; the bounded-read path cannot know it.
            let detail = run.report()["errors"][0]["detail"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(
                detail,
                "the input is 104857600 bytes; at most 4194304 are read"
            );
        }
    }
}

#[test]
fn a_file_at_the_bound_is_read_and_one_byte_over_is_refused() {
    let dir = scratch("bound");
    assert_eq!(MAX_TEXT_BYTES, MAX_WIRE_BYTES);
    // A parseable document of exactly the bound: comment lines of 8,000 bytes, then one shorter.
    let mut at = b"sequence s\nderived_from none\n".to_vec();
    while at.len() < MAX_TEXT_BYTES {
        let room = (MAX_TEXT_BYTES - at.len()).min(8_000);
        at.push(b'#');
        at.extend(std::iter::repeat_n(b'x', room.saturating_sub(2)));
        at.push(b'\n');
        at.truncate(MAX_TEXT_BYTES);
    }
    assert_eq!(at.len(), MAX_TEXT_BYTES);
    std::fs::write(dir.join("at.oce"), &at).expect("write");
    let mut over = at.clone();
    over.push(b'\n');
    std::fs::write(dir.join("over.oce"), &over).expect("write");
    // Read and parsed: stage 1 then judges it (no blocks), so the answer is no, not an error.
    let run = oce_in(&dir, &["validate", "--author-side", "at.oce"], b"");
    assert_eq!(run.code, 1, "{}", run.transcript());
    assert_eq!(run.report()["findings"][0]["code"], "empty-sequence");
    assert_eq!(
        refusal(&oce_in(&dir, &["fmt", "--check", "over.oce"], b"")),
        "input-too-large"
    );
}

#[test]
fn an_over_size_stdin_is_cut_off_one_byte_past_the_bound() {
    let dir = scratch("stdin_over");
    for form in ["text", "wire"] {
        let over = vec![b' '; MAX_TEXT_BYTES + 1];
        let run = oce_in(&dir, &["validate", "-", "--from", form], &over);
        assert_eq!(refusal(&run), "input-too-large");
        assert_eq!(
            run.report()["errors"][0]["detail"],
            "the input is larger than 4194304 bytes"
        );
    }
}

#[test]
fn a_directory_and_a_missing_path_are_refused() {
    let dir = scratch("paths");
    std::fs::create_dir_all(dir.join("folder.oce")).expect("mkdir");
    for command in ["validate", "emit", "fmt", "id"] {
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "folder.oce"], b"")),
            "not-a-file"
        );
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "missing.json"], b"")),
            "not-found"
        );
    }
}

#[test]
fn stdin_without_from_is_refused_for_every_reading_command() {
    let dir = scratch("stdin_from");
    for command in ["validate", "emit", "fmt", "id"] {
        assert_eq!(
            refusal(&oce_in(&dir, &[command, "-"], b"sequence s\n")),
            "missing-from"
        );
    }
}

#[test]
fn an_empty_input_is_a_parse_refusal_not_a_panic() {
    let dir = scratch("empty");
    std::fs::write(dir.join("empty.oce"), b"").expect("write");
    std::fs::write(dir.join("empty.json"), b"").expect("write");
    assert_eq!(
        refusal(&oce_in(&dir, &["validate", "empty.oce"], b"")),
        "missing-sequence"
    );
    assert_eq!(
        refusal(&oce_in(&dir, &["validate", "empty.json"], b"")),
        "json-syntax"
    );
}

#[test]
fn a_document_in_the_earlier_extends_spelling_is_refused_by_every_reading_command() {
    // Issue #529: the provenance statement and key are `derived_from`; `extends` keeps its CDL
    // meaning and the format has no reader for the earlier spelling, in either form.
    let dir = scratch("earlier_extends");
    let body = "in u : Real\nout y : Real\nblock abs = CDL.Reals.Abs\nu -> abs.u\nabs.y -> y\n";
    let current = format!("sequence s\nderived_from none\n{body}");
    std::fs::write(dir.join("current.oce"), &current).expect("write");
    std::fs::write(
        dir.join("earlier.oce"),
        format!("sequence s\nextends none\n{body}"),
    )
    .expect("write");
    let wire = oce_author::to_wire_string(&oce_author::parse_text(&current).expect("parses"));
    let earlier = wire.replacen("\"derived_from\":", "\"extends\":", 1);
    assert_ne!(earlier, wire);
    std::fs::write(dir.join("current.json"), &wire).expect("write");
    std::fs::write(dir.join("earlier.json"), earlier).expect("write");
    // `verify` reads its subject in either authored form too: a set the current spelling passes.
    std::fs::write(
        dir.join("abs.vectors.json"),
        r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 120}, "scenarios": [{"name": "negative", "inputs": {"u": -2.0}, "expect": [{"output": "y", "from_s": 0, "to_s": 120, "equals": 2.0}]}]}"#,
    )
    .expect("write");
    let commands: [&[&str]; 4] = [
        &["validate"],
        &["emit"],
        &["fmt"],
        &["verify", "abs.vectors.json", "--subject"],
    ];
    for command in commands {
        let run = |document: &str| oce_in(&dir, &[command, &[document]].concat(), b"");
        for current in ["current.oce", "current.json"] {
            let passed = run(current);
            assert_eq!(passed.code, 0, "{}", passed.transcript());
        }
        // The text form stops at the statement where `derived_from` must stand, line 2.
        let text = run("earlier.oce");
        assert_eq!(refusal(&text), "missing-derived-from", "{command:?}");
        assert_eq!(
            text.report()["errors"][0]["span"],
            serde_json::json!({ "line": 2, "column": 1 }),
            "{command:?}"
        );
        assert_eq!(refusal(&run("earlier.json")), "unknown-key", "{command:?}");
    }
}

#[test]
fn an_object_spelled_another_way_is_refused_by_every_reading_command_at_its_path() {
    // GitHub #544: the wire reader read a list positionally as the object it abbreviates, and a
    // signal type's map form `{"Real": null}` as the string. Each of the econ_enable bundle's
    // mutants below loaded before the fix; `validate` answered `"ok": true` for the first.
    let dir = scratch("closed_objects");
    let bundle = std::fs::read_to_string(
        crate_dir().join("../oce-author/tests/fixtures/wire/econ_enable.json"),
    )
    .expect("the econ_enable bundle");
    let semantic = "{\n        \"name\": \"\",\n        \"label\": \"\",\n        \"unit\": \"K\",\n        \
                    \"brick\": \"\",\n        \"s223\": \"\",\n        \"qudt\": \"\",\n        \
                    \"haystack\": [],\n        \"provisional\": false\n      }";
    assert!(
        bundle.contains(semantic),
        "the first boundary's semantic slot"
    );
    let cases = [
        (
            "semantic.json",
            bundle.replacen(semantic, r#"["", "", "K", "", "", "", [], false]"#, 1),
            "inputs[0].semantic",
            19,
        ),
        (
            "type.json",
            bundle.replacen(
                "\"type\": \"Real\",\n      \"quantity\"",
                "\"type\": {\"Real\": null},\n      \"quantity\"",
                1,
            ),
            "inputs[0].type",
            14,
        ),
        (
            "root.json",
            r#"["econ_enable", "", [], [], [], [], []]"#.to_owned(),
            "document",
            1,
        ),
    ];
    for (name, document, path, line) in &cases {
        assert_ne!(document, &bundle, "{name} changed the bundle");
        std::fs::write(dir.join(name), document).expect("write");
        let runs = [
            oce_in(&dir, &["validate", name], b""),
            oce_in(&dir, &["validate", "--author-side", name], b""),
            oce_in(&dir, &["emit", name], b""),
            oce_in(&dir, &["fmt", name], b""),
            oce_in(&dir, &["fmt", "--check", name], b""),
            oce_in(
                &dir,
                &["validate", "-", "--from", "wire"],
                document.as_bytes(),
            ),
            oce_in(&dir, &["emit", "-", "--from", "wire"], document.as_bytes()),
        ];
        for run in &runs {
            assert_eq!(refusal(run), "wrong-type", "{name}: {}", run.transcript());
            let error = &run.report()["errors"][0];
            let detail = error["detail"].as_str().expect("detail");
            assert!(
                detail.contains(&format!(" at {path}, line {line} ")),
                "{name}: {detail}"
            );
            assert_eq!(error["span"]["line"], *line, "{name}");
        }
    }
}

#[test]
fn every_wire_refusal_is_reported_under_its_published_code_detail_and_span() {
    // GitHub issue #495: the code and span of a wire refusal come from
    // `oce_author::WireError::code` and `location`. The literals below are what the tool printed
    // while it still derived both from the variants itself, one document per code in
    // `oce_author::WIRE_CODES` order, so a change to either accessor that moves a report fails
    // here. `Some((line, column))` is the span; `None` is `"span": null`.
    let with_connections = |connections: &str| {
        format!(
            r#"{{"sequence":"s","derived_from":"","params":[],"inputs":[],"outputs":[],"blocks":[],"connections":{connections}}}"#
        )
    };
    let with_value = |value: &str| {
        format!(
            r#"{{"sequence":"s","derived_from":"","params":[],"inputs":[],"outputs":[],"blocks":[{{"name":"b","class":"CDL.Reals.Sources.Constant","params":[{{"name":"k","value":{value}}}]}}],"connections":[]}}"#
        )
    };
    let with_min = |min: &str| {
        format!(
            r#"{{"sequence":"s","derived_from":"","params":[],"inputs":[{{"name":"u","type":"Real","quantity":"","display_unit":"","min":{min},"max":[],"semantic":{{"name":"","label":"","unit":"","brick":"","s223":"","qudt":"","haystack":[],"provisional":false}}}}],"outputs":[],"blocks":[],"connections":[]}}"#
        )
    };
    let plain = with_connections("[]");
    let mut not_utf8 = plain.clone().into_bytes();
    not_utf8[3] = 0xFF;
    let long_list = format!(
        "[{}]",
        vec!["\"a -> b\""; oce_author::MAX_WIRE_LIST_LEN + 1].join(",")
    );
    let long_name = format!("\"{}\"", "a".repeat(oce_author::MAX_WIRE_STRING_BYTES + 1));
    let nested = format!(
        "{}{}",
        "[".repeat(oce_author::MAX_WIRE_DEPTH),
        "]".repeat(oce_author::MAX_WIRE_DEPTH)
    );
    type Case = (&'static str, Vec<u8>, &'static str, Option<(u32, u32)>);
    let cases: Vec<Case> = vec![
        ("not-utf8", not_utf8, "byte 3 is not UTF-8", None),
        (
            "byte-order-mark",
            format!("\u{feff}{plain}").into_bytes(),
            "the document starts with a byte-order mark",
            None,
        ),
        (
            // The tool's bounded read refuses an over-size input before the wire reader sees
            // it, under the same code.
            "input-too-large",
            format!("{plain}{}", " ".repeat(MAX_WIRE_BYTES + 1)).into_bytes(),
            "the input is larger than 4194304 bytes",
            None,
        ),
        (
            "nesting-too-deep",
            with_connections(&nested).into_bytes(),
            "nesting deeper than 8 levels at line 1 column 105",
            Some((1, 105)),
        ),
        (
            "list-too-long",
            with_connections(&long_list).into_bytes(),
            "the list at line 1 column 98 has more than 65536 entries",
            Some((1, 98)),
        ),
        (
            "string-too-long",
            plain.replacen("\"s\"", &long_name, 1).into_bytes(),
            "the string at line 1 column 13 is longer than 4096 bytes",
            Some((1, 13)),
        ),
        (
            "duplicate-key",
            plain.replacen('{', "{\"sequence\":\"t\",", 1).into_bytes(),
            "duplicate key `sequence` at line 1 column 17",
            Some((1, 17)),
        ),
        (
            "non-finite-number",
            with_value("1e999").into_bytes(),
            "a non-finite number at line 1 column 161",
            Some((1, 161)),
        ),
        (
            "integer-out-of-range",
            with_value("9223372036854775808").into_bytes(),
            "an integer outside the 64-bit signed range at line 1 column 161",
            Some((1, 161)),
        ),
        (
            "control-character",
            with_value("\"a\\tb\"").into_bytes(),
            "a control character in a string at line 1 column 164",
            Some((1, 164)),
        ),
        (
            "unknown-key",
            plain.replacen('{', "{\"comment\":\"x\",", 1).into_bytes(),
            "unknown key `comment` at line 1 column 10",
            Some((1, 10)),
        ),
        (
            "missing-key",
            plain.replacen("\"derived_from\":\"\",", "", 1).into_bytes(),
            "missing key `derived_from` at line 1 column 82",
            Some((1, 82)),
        ),
        (
            "wrong-type",
            with_connections("{}").into_bytes(),
            "invalid type: map, expected a sequence at connections, line 1 column 97",
            Some((1, 97)),
        ),
        (
            "json-syntax",
            b"{".to_vec(),
            "not JSON: EOF while parsing an object at line 1 column 1",
            Some((1, 1)),
        ),
        (
            "bound-list-too-long",
            with_min("[0, 1]").into_bytes(),
            "a bound list holds at most one number; a second item at line 1 column 126",
            Some((1, 126)),
        ),
        (
            "malformed-connection",
            with_connections("[\"a.y b.u\"]").into_bytes(),
            "connection 0 `a.y b.u` is not `source -> target` with exactly one `->`",
            None,
        ),
    ];
    let listed: Vec<&str> = cases.iter().map(|case| case.0).collect();
    assert_eq!(listed, oce_author::WIRE_CODES, "one document per code");
    let dir = scratch("wire_refusal_codes");
    for (code, document, detail, span) in &cases {
        for command in ["validate", "emit", "fmt"] {
            let run = oce_in(&dir, &[command, "-", "--from", "wire"], document);
            assert_eq!(refusal(&run), *code, "{command}");
            let error = &run.report()["errors"][0];
            assert_eq!(error["detail"], *detail, "{command} {code}");
            let expected = span.map_or(
                serde_json::Value::Null,
                |(line, column)| serde_json::json!({ "column": column, "line": line }),
            );
            assert_eq!(error["span"], expected, "{command} {code}");
        }
    }
}

#[test]
fn diff_refuses_hostile_bytes_on_either_side_about_that_side() {
    // GitHub issue #668: each side of `oce diff` is read and parsed or loaded as the other verbs
    // read their input, and a refusal names its side.
    let dir = scratch("diff_hostile");
    let junk: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let good = "sequence s\nderived_from none\nin u : Real\nout y : Real\nblock abs = CDL.Reals.Abs\nu -> abs.u\nabs.y -> y\n";
    let parsed = oce_author::parse_text(good).expect("parses");
    let wire = oce_author::to_wire_string(&parsed);
    let cxf = oce_author::emit(&parsed, &oce_author::EmitOptions::default()).expect("emits");
    std::fs::write(dir.join("good.oce"), good).expect("write");
    std::fs::write(dir.join("good.json"), &wire).expect("write");
    std::fs::write(dir.join("good.jsonld"), &cxf).expect("write");
    for extension in ["oce", "json", "jsonld"] {
        let name = |stem: &str| format!("{stem}.{extension}");
        std::fs::write(dir.join(name("junk")), &junk).expect("write");
        std::fs::write(dir.join(name("utf8")), b"{\"a\": \"\xc3\x28\"}").expect("write");
        std::fs::write(dir.join(name("empty")), b"").expect("write");
        std::fs::create_dir_all(dir.join(name("folder"))).expect("mkdir");
        let good = name("good");
        for stem in ["junk", "utf8", "empty", "folder", "missing"] {
            let bad = name(stem);
            for (base, head, side) in [(&bad, &good, "base"), (&good, &bad, "head")] {
                let run = oce_in(&dir, &["diff", base, head], b"");
                let code = refusal(&run);
                assert_eq!(run.report()["errors"][0]["at"], side, "{base} {head}");
                assert!(run.report().get("diff").is_none());
                match stem {
                    "folder" => assert_eq!(code, "not-a-file"),
                    "missing" => assert_eq!(code, "not-found"),
                    _ if extension == "jsonld" => assert_eq!(code, "not-loadable"),
                    _ => assert!(!code.is_empty()),
                }
            }
            // Through stdin as well, on the head side.
            if !matches!(stem, "folder" | "missing") {
                let bytes = std::fs::read(dir.join(&bad)).expect("read");
                let from = match extension {
                    "oce" => "text",
                    "json" => "wire",
                    _ => "cxf",
                };
                let run = oce_in(&dir, &["diff", &good, "-", "--from", from], &bytes);
                refusal(&run);
                assert_eq!(run.report()["errors"][0]["at"], "head");
            }
        }
    }
}

#[test]
fn hostile_commissioning_bytes_never_panic_the_binary() {
    // GitHub issue #664: seeded byte edits of the spec, the mapping and a trend file of
    // `oce-verify`'s run fixture, each run through `oce verify --commission`. Every run answers
    // with one document on stdout and nothing on stderr: the receipt for a verdict (exit 0 or 1),
    // or the report with exactly one typed error (exit 2). A panic would print to stderr and
    // exit 101.
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let dir = support::commission::run_layout("commission_hostile_bytes", "pass");
    let originals: Vec<(&str, Vec<u8>)> = ["spec.json", "points.json", "inputs.csv"]
        .iter()
        .map(|name| {
            (
                *name,
                std::fs::read(dir.join(name)).expect("a fixture file"),
            )
        })
        .collect();
    let (mut verdicts, mut refusals) = (0, 0);
    for round in 0..150_usize {
        let (name, original) = &originals[round % originals.len()];
        let mut bytes = original.clone();
        for _ in 0..=(next() % 4) {
            let at = usize::try_from(next() % bytes.len() as u64).expect("fits");
            match next() % 3 {
                0 => bytes[at] = (next() & 0xff) as u8,
                1 => {
                    bytes.remove(at);
                }
                _ => {
                    let pool = b"{}[],:\"\\0e-9\n\xff";
                    let pick = usize::try_from(next() % pool.len() as u64).expect("fits");
                    bytes.insert(at, pool[pick]);
                }
            }
        }
        std::fs::write(dir.join(name), &bytes).expect("write the edit");
        let run = oce_in(&dir, &["verify", "--commission", "spec.json"], b"");
        assert!(
            run.stderr.is_empty(),
            "round {round} ({name}): {}",
            run.transcript()
        );
        match run.code {
            0 | 1 => {
                let receipt: serde_json::Value =
                    serde_json::from_slice(&run.stdout).expect("a receipt");
                assert_eq!(receipt["kind"], "commissioning", "round {round}");
                verdicts += 1;
            }
            2 => {
                let report = run.report();
                assert_eq!(
                    report["errors"].as_array().map(Vec::len),
                    Some(1),
                    "round {round}"
                );
                refusals += 1;
            }
            other => panic!("round {round} ({name}): exit {other}\n{}", run.transcript()),
        }
        std::fs::write(dir.join(name), original).expect("restore");
    }
    assert!(refusals > 50, "{refusals} refusals, {verdicts} verdicts");
}
