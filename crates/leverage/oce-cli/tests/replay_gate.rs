//! The in-repo replay gate (vectors note decision 19, owner ruling 10; M3 gate row "In-repo rules
//! replay"; GitHub issue #522): the built `oce` binary verifies both in-repo library rules, read
//! in place under `crates/oce-api/tests/fixtures/library_rules/`, and each run must exit 0 with
//! stdout equal, byte for byte, to `oce-verify`'s golden receipt of that rule. It runs in the
//! leverage lane, so every leverage and engine PR executes it. A change to a rule, its vectors or
//! the engine that moves a receipt fails here until the receipts are re-blessed in `oce-verify`.

mod support;

use std::fs;

use support::{crate_dir, oce, oce_in, scratch};

const LIBRARY: &str = "../../oce-api/tests/fixtures/library_rules";

/// `oce-verify`'s golden receipt of `rule`.
fn golden(rule: &str) -> Vec<u8> {
    fs::read(crate_dir().join(format!(
        "../oce-verify/tests/fixtures/receipts/{rule}.receipt.json"
    )))
    .expect("the golden receipt")
}

#[test]
fn both_in_repo_rules_verify_to_their_golden_receipts() {
    for rule in ["AHU-0016", "AHU-0004"] {
        let vectors = format!("{LIBRARY}/{rule}/vectors.json");
        let subject = format!("{LIBRARY}/{rule}/rule.cxf.jsonld");
        let run = oce(&["verify", &vectors, "--subject", &subject], b"");
        assert_eq!(run.code, 0, "{rule}\n{}", run.transcript());
        assert!(run.stderr.is_empty(), "{rule}: {}", run.transcript());
        assert!(
            run.stdout == golden(rule),
            "{rule}: stdout is not the golden receipt\n{}",
            run.stdout_text()
        );
    }
}

#[test]
fn an_edited_expectation_fails_the_gate() {
    // The same rule with one expectation moved past the persistence delay: the run answers no,
    // and its receipt is not the golden one.
    let dir = scratch("replay_gate_edited");
    let rule = crate_dir().join(LIBRARY).join("AHU-0016");
    fs::copy(rule.join("rule.cxf.jsonld"), dir.join("rule.cxf.jsonld")).expect("copy the rule");
    let set = fs::read_to_string(rule.join("vectors.json")).expect("the set");
    let edited = set.replacen(r#""to_s": 840"#, r#""to_s": 900"#, 1);
    assert_ne!(edited, set, "the expectation to edit is in the set");
    fs::write(dir.join("vectors.json"), edited).expect("write the edited set");
    let run = oce_in(
        &dir,
        &["verify", "vectors.json", "--subject", "rule.cxf.jsonld"],
        b"",
    );
    assert_eq!(run.code, 1, "{}", run.transcript());
    assert_ne!(run.stdout, golden("AHU-0016"));
}
