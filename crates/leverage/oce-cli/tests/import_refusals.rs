//! Every refusal `oce import` passes through from the importer (GitHub issue #514): one
//! document per code of `oce_author::IMPORT_ERROR_CODES` that a CXF document can earn, each a
//! hand edit of the worked example, refused with exit 2 under exactly that code with its subject
//! as the error's `at`, the same in both output forms and from a file as from stdin. The codes a
//! run cannot earn are named with the reason, so the two lists partition the importer's and a
//! code `oce-author` adds must be classified here. Hostile bytes are typed refusals, never a
//! panic, and nothing a document carries reaches a terminal as a control character.

mod support;

use std::fs;

use serde_json::{Value, json};
use support::cxf_edit::{ROOT, econ, edited, node, real};
use support::{Run, oce, oce_in, scratch};

/// The single error of a refused run, after checking the run's shape.
fn refusal(run: &Run) -> Value {
    assert_eq!(run.code, 2, "{}", run.transcript());
    assert!(run.stderr.is_empty(), "{}", run.transcript());
    let report = run.report();
    assert_eq!(report["command"], "import");
    assert_eq!(report["ok"], false);
    assert_eq!(report["findings"], json!([]));
    for absent in ["output", "bytes", "dropped", "normalised", "stages"] {
        assert!(
            report.get(absent).is_none(),
            "{absent}: {}",
            run.transcript()
        );
    }
    let errors = report["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 1, "{}", run.transcript());
    errors[0].clone()
}

/// A reference to the node `suffix` under the worked example's root.
fn reference(suffix: &str) -> Value {
    json!({ "@id": format!("{ROOT}{suffix}") })
}

/// One document per code a CXF document can earn: the code, the `at` it is reported at, and
/// the document.
fn one_document_per_code() -> Vec<(&'static str, String, Vec<u8>)> {
    let at = |suffix: &str| format!("{ROOT}{suffix}");
    let document = || "document".to_owned();
    vec![
        ("not-strict-json", document(), b"{\"@graph\": [}".to_vec()),
        (
            "unknown-key",
            at(".sub"),
            edited(|d| node(d, ".sub")["S231:bogus"] = json!(1)),
        ),
        (
            "namespace-mismatch",
            document(),
            edited(|d| d["@context"]["S231"] = json!("http://example.org/S231#")),
        ),
        (
            "missing-key",
            at(".dTHys"),
            edited(|d| {
                node(d, ".dTHys")
                    .as_object_mut()
                    .expect("a node")
                    .remove("S231:value");
            }),
        ),
        (
            "wrong-shape",
            at(""),
            edited(|d| node(d, "")["S231:containsBlock"] = json!("x")),
        ),
        (
            "no-root",
            document(),
            edited(|d| {
                node(d, "")
                    .as_object_mut()
                    .expect("a node")
                    .remove("S231:containsBlock");
            }),
        ),
        (
            "duplicate-id",
            at(".sub.y"),
            edited(|d| {
                let twin = node(d, ".sub.y").clone();
                d["@graph"].as_array_mut().expect("a graph").push(twin);
            }),
        ),
        (
            "unresolved-reference",
            at(""),
            edited(|d| {
                node(d, "")["S231:hasInput"]
                    .as_array_mut()
                    .expect("inputs")
                    .push(reference(".nowhere"));
            }),
        ),
        (
            "stray-node",
            at(".orphan"),
            edited(|d| {
                d["@graph"].as_array_mut().expect("a graph").push(json!({
                    "@id": format!("{ROOT}.orphan"),
                    "@type": "S231:RealInput",
                    "S231:isOfDataType": { "@id": "S231:Real" },
                }));
            }),
        ),
        (
            "invalid-name",
            at(".s\u{fc}b"),
            String::from_utf8(econ())
                .expect("UTF-8")
                .replace("econ_enable.sub", "econ_enable.s\u{fc}b")
                .into_bytes(),
        ),
        (
            "nested-composite",
            at(".sub"),
            edited(|d| node(d, ".sub")["S231:containsBlock"] = json!([reference(".hys")])),
        ),
        (
            "instance-member",
            at(""),
            edited(|d| node(d, "")["S231:hasInstance"] = json!([reference(".sub")])),
        ),
        (
            "constant-member",
            at(""),
            edited(|d| node(d, "")["S231:hasConstant"] = json!([reference(".dTHys")])),
        ),
        (
            "conditional-component",
            at(".sub"),
            edited(|d| node(d, ".sub")["S231:isConditionalComponent"] = json!(true)),
        ),
        (
            "array-connector",
            at(".TOut"),
            edited(|d| node(d, ".TOut")["S231:isArray"] = json!(true)),
        ),
        (
            "array-sequence-parameter",
            at(".dTHys"),
            edited(|d| node(d, ".dTHys")["S231:isArray"] = json!(true)),
        ),
        (
            "expression-value",
            at(".hys.uLow"),
            edited(|d| node(d, ".hys.uLow")["S231:value"] = json!("dTHys + 1")),
        ),
        (
            "port-attribute",
            at(".sub.u1"),
            edited(|d| node(d, ".sub.u1")["S231:max"] = json!(1)),
        ),
        (
            "connector-attribute",
            at(".TOut"),
            edited(|d| node(d, ".TOut")["S231:nominal"] = json!(1)),
        ),
        (
            "driving-sink",
            at(".ecoEna"),
            edited(|d| node(d, ".ecoEna")["S231:isConnectedTo"] = json!([reference(".sub.u1")])),
        ),
        (
            "class-outside-obc",
            at(".sub"),
            edited(|d| {
                node(d, ".sub")["@type"] = json!("http://example.org#Modelica.Blocks.Math.Add")
            }),
        ),
        (
            "datatype-mismatch",
            at(".TOut"),
            edited(|d| node(d, ".TOut")["S231:isOfDataType"] = json!({ "@id": "S231:Boolean" })),
        ),
        (
            "unknown-datatype",
            at(".dTHys"),
            edited(|d| node(d, ".dTHys")["S231:isOfDataType"] = json!({ "@id": "S231:String" })),
        ),
        (
            "unknown-member-type",
            at(".hys.uLow"),
            edited(|d| {
                node(d, ".hys.uLow")["S231:value"] =
                    json!("Buildings.Controls.OBC.CDL.Types.NoSuchType.member");
            }),
        ),
        (
            "malformed-literal",
            at(".hys.uLow"),
            edited(|d| node(d, ".hys.uLow")["S231:value"] = real("NaN")),
        ),
        (
            "array-shape",
            at(".hys.uLow"),
            edited(|d| {
                let parameter = node(d, ".hys.uLow");
                parameter["S231:isArray"] = json!(true);
                parameter["S231:numberDimensions"] = json!(1);
                parameter["S231:sizeOfDimensions"] = json!("(2)");
                parameter["S231:value"] = json!([real("1.0")]);
            }),
        ),
    ]
}

/// The importer codes no run of `oce import` earns, each with why.
const NOT_EARNED: [(&str, &str); 4] = [
    (
        "too-large",
        "the tool's bounded read stops at the importer's own limit and reports input-too-large",
    ),
    (
        "limit-exceeded",
        "a v0 graph JSON read bound; the CXF reader has none",
    ),
    ("string-value", "a v0 graph JSON parameter value"),
    ("malformed-connection", "a v0 graph JSON connection"),
];

#[test]
fn every_code_a_cxf_document_can_earn_is_reported_under_that_code_at_its_subject() {
    let dir = scratch("import_codes");
    let cases = one_document_per_code();
    for (code, at, document) in &cases {
        fs::write(dir.join("in.cxf.json"), document).expect("write");
        let file = oce_in(&dir, &["import", "in.cxf.json", "-o", "out.oce"], b"");
        let error = refusal(&file);
        assert_eq!(error["code"], *code, "{}", file.transcript());
        assert_eq!(error["at"], at.as_str(), "{code}");
        assert_eq!(error["span"], Value::Null, "{code}");
        assert!(
            error["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "{code}"
        );
        assert!(!dir.join("out.oce").exists(), "{code}: a file was written");
        // The same refusal from stdin and whatever form was asked for.
        for args in [&["import", "-"][..], &["import", "-", "--wire"][..]] {
            assert_eq!(refusal(&oce(args, document)), error, "{code} {args:?}");
        }
    }
    // The two lists partition the importer's, in its order.
    let earned: Vec<&str> = cases.iter().map(|(code, _, _)| *code).collect();
    let partition: Vec<&str> = oce_author::IMPORT_ERROR_CODES
        .iter()
        .copied()
        .filter(|code| !NOT_EARNED.iter().any(|(named, _)| named == code))
        .collect();
    assert_eq!(earned, partition);
    assert_eq!(
        earned.len() + NOT_EARNED.len(),
        oce_author::IMPORT_ERROR_CODES.len()
    );
    assert_eq!((earned.len(), NOT_EARNED.len()), (26, 4));
    for (code, why) in NOT_EARNED {
        assert!(oce_author::IMPORT_ERROR_CODES.contains(&code), "{code}");
        assert!(!why.is_empty());
    }
}

#[test]
fn vocab_lists_every_importer_code_with_a_meaning() {
    let report = oce(&["vocab", "--json"], b"").report();
    let listed: Vec<&str> = report["import_codes"]
        .as_array()
        .expect("import_codes")
        .iter()
        .map(|row| row["code"].as_str().expect("code"))
        .collect();
    assert_eq!(listed, oce_author::IMPORT_ERROR_CODES);
    // So every code a refusal can carry is one `oce vocab` prints: the importer's, or the
    // tool's own for the read, the spelling and the write.
    let tool: Vec<&str> = report["error_codes"]
        .as_array()
        .expect("error_codes")
        .iter()
        .map(|row| row["code"].as_str().expect("code"))
        .collect();
    for code in [
        "not-found",
        "not-a-file",
        "read-failed",
        "input-too-large",
        "unspellable",
        "write-failed",
    ] {
        assert!(tool.contains(&code), "{code}");
    }
    let text = oce(&["vocab"], b"");
    for code in oce_author::IMPORT_ERROR_CODES {
        assert!(
            text.stdout_text().contains(&format!("\n  {code} ")),
            "{code} is not in the text listing"
        );
    }
}

#[test]
fn a_node_with_an_empty_id_is_refused_without_an_empty_at() {
    let cxf = edited(|d| {
        let graph = d["@graph"].as_array_mut().expect("a graph");
        graph.push(json!({ "@id": "" }));
        graph.push(json!({ "@id": "" }));
    });
    let error = refusal(&oce(&["import", "-"], &cxf));
    assert_eq!(error["code"], "duplicate-id");
    assert!(error.get("at").is_none(), "{error}");
}

#[test]
fn hostile_bytes_are_typed_refusals_and_never_a_panic() {
    let dir = scratch("import_hostile");
    let junk: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let deep = [vec![b'['; 200_000], vec![b']'; 200_000]].concat();
    let cases: [(&str, &[u8], &str); 12] = [
        ("empty", b"", "not-strict-json"),
        ("junk", &junk, "not-strict-json"),
        (
            "bad UTF-8",
            b"{\"@graph\": \"\xc3\x28\"}",
            "not-strict-json",
        ),
        ("a byte-order mark", b"\xef\xbb\xbf{}", "not-strict-json"),
        ("deep nesting", &deep, "not-strict-json"),
        (
            "a truncated document",
            b"{\"@graph\": [{\"@id\": \"urn:a#top\"",
            "not-strict-json",
        ),
        (
            "a key written twice",
            b"{\"@graph\": [], \"@graph\": []}",
            "not-strict-json",
        ),
        (
            "an integer outside i64",
            b"{\"@graph\": [{\"@id\": \"urn:a#top\", \"S231:value\": 18446744073709551616}]}",
            "not-strict-json",
        ),
        ("null", b"null", "wrong-shape"),
        ("an empty object", b"{}", "missing-key"),
        ("an empty graph", b"{\"@graph\": []}", "no-root"),
        (
            "a graph of scalars",
            b"{\"@graph\": [1, \"a\", null]}",
            "wrong-shape",
        ),
    ];
    for (what, bytes, code) in cases {
        fs::write(dir.join("hostile.cxf.json"), bytes).expect("write");
        for flags in [&[][..], &["--wire"][..]] {
            let file = oce_in(
                &dir,
                &[&["import", "hostile.cxf.json"], flags].concat(),
                b"",
            );
            assert_eq!(refusal(&file)["code"], code, "{what}");
            let stdin = oce_in(&dir, &[&["import", "-"], flags].concat(), bytes);
            // The same refusal whichever way the bytes arrive.
            assert_eq!(refusal(&stdin), refusal(&file), "{what}");
        }
    }
}

#[test]
fn a_truncated_copy_of_an_importable_document_is_refused() {
    let cxf = econ();
    for cut in [1, cxf.len() / 3, cxf.len() / 2, cxf.len() - 2] {
        let error = refusal(&oce(&["import", "-"], &cxf[..cut]));
        assert_eq!(error["code"], "not-strict-json", "cut at {cut}");
        assert_eq!(error["at"], "document", "cut at {cut}");
    }
}

#[test]
fn nothing_a_document_carries_reaches_the_rendering_as_a_control_character() {
    // An escape sequence, a bell, a line feed, a right-to-left override and a line separator in
    // a key and in a node's `@id`: the JSON report carries them as the document wrote them, and the rendering
    // on stderr writes each as an escape.
    let hostile = "\u{1b}[31mred\u{7}\nnext\u{202e}\u{2028}";
    let key = edited(|d| node(d, ".sub")[format!("S231:{hostile}")] = json!(1));
    // The same characters as a JSON string writes them, spliced into every `@id` under `sub`.
    let escaped = serde_json::to_string(hostile).expect("a JSON string");
    let escaped = escaped.trim_matches('"');
    let id = String::from_utf8(econ())
        .expect("UTF-8")
        .replace("econ_enable.sub", &format!("econ_enable.s{escaped}b"))
        .into_bytes();
    for (document, code, quoted_in) in [(key, "unknown-key", "detail"), (id, "invalid-name", "at")]
    {
        let run = oce(&["import", "-", "--render", "always"], &document);
        assert_eq!(run.code, 2, "{}", run.transcript());
        let error = run.report()["errors"][0].clone();
        assert_eq!(error["code"], code);
        assert!(
            error[quoted_in]
                .as_str()
                .expect("a string")
                .contains(hostile),
            "{code}: the report quotes the document"
        );
        let rendered = String::from_utf8(run.stderr.clone()).expect("UTF-8");
        assert_eq!(rendered.matches('\n').count(), 2, "{rendered:?}");
        assert!(
            !rendered
                .chars()
                .any(|c| c != '\n' && (oce_api::is_format_or_control(c) || c == '\u{2028}')),
            "{rendered:?}"
        );
        if quoted_in == "detail" {
            assert!(
                rendered.contains("\\u{1b}[31mred\\u{7}\\u{a}next\\u{202e}\\u{2028}"),
                "{rendered:?}"
            );
        }
    }
}
