//! The report contract over every golden run: one JSON object with the fixed common keys, keys
//! sorted at every depth, LF line endings and one final line feed, `ok` true exactly when the
//! exit is 0, `errors` non-empty exactly when it is 2, and no absolute path anywhere.

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
        if exit == 1 {
            assert!(
                report["findings"]
                    .as_array()
                    .expect("findings")
                    .iter()
                    .any(|f| f["severity"] == "error")
                    || report.get("canonical") == Some(&Value::Bool(false)),
                "{name}: exit 1 without an error finding or a non-canonical file"
            );
        }
    }
    assert!(reports >= 35, "only {reports} reports");
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
        let text = format!("sequence s\nextends none\nin u : Real {tag}{operand}\n");
        assert!(oce_author::parse_text(&text).is_ok(), "{tag} is not read");
    }
    let unknown = oce_author::parse_text("sequence s\nextends none\nin u : Real @bogus x\n");
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
