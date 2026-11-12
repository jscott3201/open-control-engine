//! The report contract over every golden run: one JSON object with the fixed common keys, keys
//! sorted at every depth, LF line endings and one final line feed, `ok` true exactly when the
//! exit is 0, `errors` non-empty exactly when it is 2, and no absolute path anywhere. Every error
//! is `code`, `detail` and `span`, plus the optional keys a `verify` refusal carries, each only
//! with a value (owner rulings of 2026-11-11: the place is `at`, never `subject`, which in a
//! `verify` report is the subject file). A `verify` receipt on stdout is a document, not a
//! report, and is skipped like CXF.

mod support;

use serde_json::Value;
use support::crate_dir;

/// Parse a golden transcript into (exit, stdout, stderr).
fn transcript(text: &str) -> (i32, &str, &str) {
    let rest = text.strip_prefix("exit: ").expect("exit line");
    let (exit, rest) = rest.split_once("\n--- stdout\n").expect("stdout marker");
    let (stdout, stderr) = rest.split_once("--- stderr\n").expect("stderr marker");
    (exit.parse().expect("exit"), stdout, stderr)
}

fn keys_sorted(value: &Value) {
    // serde_json's map is ordered by key; `written_key_order` checks the bytes as written.
    if let Value::Object(map) = value {
        let keys: Vec<&String> = map.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        map.values().for_each(keys_sorted);
    }
    if let Value::Array(items) = value {
        items.iter().for_each(keys_sorted);
    }
}

/// The top-level keys of an object in the order the text writes them.
fn written_key_order(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  \""))
        .filter_map(|line| line.split_once("\": ").or_else(|| line.split_once("\":")))
        .map(|(key, _)| key.to_owned())
        .collect()
}

#[test]
fn every_golden_report_keeps_the_contract() {
    let dir = crate_dir().join("tests/fixtures/runs");
    let mut reports = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("runs")
        .map(|e| e.expect("e").path())
        .collect();
    entries.sort();
    assert!(entries.len() >= 50);
    for path in entries {
        let text = std::fs::read_to_string(&path).expect("golden");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!text.contains('\r'), "{name}: CR in a transcript");
        for needle in ["/Users/", "/home/", "C:\\", "/tmp/", "/private/"] {
            assert!(!text.contains(needle), "{name}: an absolute path");
        }
        let (exit, stdout, stderr) = transcript(&text);
        assert!(
            stdout.is_empty() || stdout.ends_with('\n'),
            "{name}: no final line feed"
        );
        assert!(stderr.is_empty() || stderr.ends_with('\n'), "{name}");
        let Ok(report) = serde_json::from_str::<Value>(stdout) else {
            continue; // a document (CXF is JSON too, but it has no `tool`), help or usage text
        };
        if report.get("tool") != Some(&Value::from("oce")) {
            continue;
        }
        reports += 1;
        for key in [
            "command", "errors", "findings", "input", "ok", "schema", "tool",
        ] {
            assert!(report.get(key).is_some(), "{name}: missing {key}");
        }
        assert_eq!(report["schema"], oce_cli::REPORT_SCHEMA, "{name}");
        keys_sorted(&report);
        let order = written_key_order(stdout);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted, "{name}: keys not written sorted");
        assert_eq!(report["ok"] == true, exit == 0, "{name}: ok vs exit");
        let errors = report["errors"].as_array().expect("errors").len();
        assert_eq!(errors > 0, exit == 2, "{name}: errors vs exit");
        for error in report["errors"].as_array().expect("errors") {
            error_shape(&name, error);
        }
        if exit == 1 {
            assert!(
                report["findings"]
                    .as_array()
                    .expect("findings")
                    .iter()
                    .any(|f| f["severity"] == "error")
                    || report.get("canonical") == Some(&Value::Bool(false))
                    || (report["command"] == "verify" && report.get("passed").is_some()),
                "{name}: exit 1 without an error finding, a non-canonical file or a verdict"
            );
        }
    }
    assert!(reports >= 35, "only {reports} reports");
}

/// The keys an error may carry beside `code`, `detail` and `span`.
const OPTIONAL_ERROR_KEYS: [&str; 5] = ["at", "candidates", "cause", "diagnostic_codes", "path"];

/// One error object: the three fixed keys, and optional keys only from the pinned set, each with
/// a value of its type (a list is never empty, a string never empty).
fn error_shape(name: &str, error: &Value) {
    let map = error.as_object().expect("an error is an object");
    for key in ["code", "detail", "span"] {
        assert!(map.contains_key(key), "{name}: an error without {key}");
    }
    for (key, value) in map {
        match key.as_str() {
            "code" | "detail" => assert!(value.is_string(), "{name}: {key}"),
            "span" => assert!(value.is_null() || value.is_object(), "{name}: span"),
            "at" | "path" | "cause" => assert!(
                value.as_str().is_some_and(|text| !text.is_empty()),
                "{name}: {key} is a non-empty string"
            ),
            "candidates" | "diagnostic_codes" => assert!(
                value
                    .as_array()
                    .is_some_and(|items| !items.is_empty() && items.iter().all(Value::is_string)),
                "{name}: {key} is a non-empty list of strings"
            ),
            other => panic!("{name}: an error key outside the pinned set: {other}"),
        }
    }
}

#[test]
fn each_optional_error_key_is_written_when_its_refusal_has_a_value() {
    let error = |run: &str| -> Value {
        let path = crate_dir().join(format!("tests/fixtures/runs/{run}.txt"));
        let text = std::fs::read_to_string(&path).expect("golden");
        let (_, stdout, _) = transcript(&text);
        let report: Value = serde_json::from_str(stdout).expect("a report");
        report["errors"][0].clone()
    };
    let keys = |value: &Value| -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("object")
            .keys()
            .filter(|key| OPTIONAL_ERROR_KEYS.contains(&key.as_str()))
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    // `at` alone: an unresolved input name names its place and nothing else.
    let unknown = error("verify_unknown_input");
    assert_eq!(keys(&unknown), ["at"]);
    assert_eq!(unknown["at"], "scenarios[0].inputs.v");
    // `candidates`, in their order.
    let ambiguous = error("verify_ambiguous_input");
    assert_eq!(keys(&ambiguous), ["at", "candidates"]);
    assert_eq!(
        ambiguous["candidates"],
        serde_json::json!([
            "urn:verify:signals#top.ahu.T",
            "urn:verify:signals#top.zone.T"
        ])
    );
    // `path` and `cause`.
    let refused = error("verify_param_refused");
    assert_eq!(keys(&refused), ["at", "cause", "path"]);
    assert_eq!(refused["cause"], "param-range");
    assert_eq!(refused["path"], "urn:verify:hysteresis#top.hys.uHigh");
    // `diagnostic_codes`.
    let not_loading = error("verify_subject_not_loading");
    assert_eq!(keys(&not_loading), ["at", "diagnostic_codes"]);
    assert_eq!(
        not_loading["diagnostic_codes"],
        serde_json::json!(["class-not-found"])
    );
    // A vector file's refusal found in the text carries its place and its span.
    let malformed = error("verify_malformed_vectors");
    assert_eq!(keys(&malformed), ["at"]);
    assert!(malformed["span"].is_object(), "{malformed}");
    // Every other command's errors carry none of them.
    assert!(keys(&error("validate_missing_file")).is_empty());
}

#[test]
fn catalog_json_lists_every_class_in_catalog_order() {
    let run = support::oce(&["catalog", "--json"], b"");
    assert_eq!(run.code, 0);
    let report = run.report();
    let classes = report["classes"].as_array().expect("classes");
    let expected: Vec<&str> = oce_api::class_metadata()
        .iter()
        .map(|m| m.class_path)
        .collect();
    let actual: Vec<&str> = classes
        .iter()
        .map(|c| c["class_path"].as_str().unwrap())
        .collect();
    assert_eq!(actual, expected);
    for class in classes {
        let ports = oce_api::class_ports(class["class_path"].as_str().unwrap(), &[]);
        assert_eq!(class["inputs"].is_null(), ports.is_err());
        assert_eq!(class["outputs"].is_null(), ports.is_err());
        let reserved = oce_api::catalog()
            .iter()
            .any(|e| e.class_path == class["class_path"] && e.reserved);
        assert_eq!(class["reserved"], reserved);
    }
}

#[test]
fn vocab_tags_are_exactly_the_tags_the_text_form_reads() {
    let run = support::oce(&["vocab", "--json"], b"");
    let report = run.report();
    for tag in report["tags"].as_array().expect("tags") {
        let tag = tag.as_str().unwrap();
        let operand = match tag {
            "@provisional" => "",
            "@name" | "@label" => " \"x\"",
            _ => " x",
        };
        let text = format!("sequence s\nderived_from none\nin u : Real {tag}{operand}\n");
        assert!(oce_author::parse_text(&text).is_ok(), "{tag} is not read");
    }
    let unknown = oce_author::parse_text("sequence s\nderived_from none\nin u : Real @bogus x\n");
    assert_eq!(unknown.unwrap_err().code, "unknown-tag");
    let codes = |key: &str| -> Vec<String> {
        report[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["code"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(codes("parse_codes"), oce_author::PARSE_CODES);
    let findings = codes("finding_codes");
    assert_eq!(
        &findings[..oce_author::STAGE_ONE_CODES.len()],
        oce_author::STAGE_ONE_CODES
    );
    assert_eq!(
        report["keywords"],
        serde_json::json!(oce_author::TEXT_FORM_KEYWORDS)
    );
    assert_eq!(
        report["base_units"],
        serde_json::json!(oce_author::BASE_UNIT_SYMBOLS)
    );
}

#[test]
fn vocab_enumerations_are_the_facade_table_in_order() {
    let report = support::oce(&["vocab", "--json"], b"").report();
    let rows = report["enumerations"].as_array().expect("enumerations");
    let table = oce_api::enumerations();
    assert_eq!(rows.len(), table.len());
    for (row, class) in rows.iter().zip(table) {
        assert_eq!(row["class_path"], class.class_path);
        let kind = match class.kind {
            oce_api::EnumerationKind::Enumeration => "enumeration",
            oce_api::EnumerationKind::ConstantPackage => "constant-package",
        };
        assert_eq!(row["kind"], kind);
        let members = row["members"].as_array().expect("members");
        let names: Vec<&str> = members
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, class.members);
        for member in members {
            let expected = match class.member_value(member["name"].as_str().unwrap()) {
                Some(oce_api::MemberValue::Ordinal(v)) => serde_json::json!(v),
                Some(oce_api::MemberValue::Integer(v)) => serde_json::json!(v),
                None => serde_json::Value::Null,
            };
            assert_eq!(member["value"], expected);
        }
    }
}

#[test]
fn catalog_names_the_enumeration_of_every_enumeration_parameter() {
    let report = support::oce(&["catalog", "--json"], b"").report();
    let mut named = 0;
    for (class, meta) in report["classes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oce_api::class_metadata())
    {
        for (parameter, expected) in class["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&meta.parameters)
        {
            match expected.value_type {
                oce_api::ValueType::Enum(id) => {
                    assert_eq!(parameter["type"], "Enum");
                    assert_eq!(
                        parameter["enumeration"],
                        oce_api::enumeration_class_path(id).unwrap()
                    );
                    named += 1;
                }
                _ => assert!(parameter["enumeration"].is_null()),
            }
        }
    }
    assert!(named > 0);
}
