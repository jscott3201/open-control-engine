//! `oce verify` through the built binary, one golden transcript per outcome class (vectors note,
//! "CLI"; GitHub issue #522): the receipt on stdout for a verdict (exit 0 or 1), the report for
//! every refusal (exit 2), and each exit-only flag flipping the status with the same receipt
//! bytes. `--help` is pinned with every subcommand's in `runs.rs`. Every input is named relative
//! to the crate directory, and the in-repo library rules and `oce-verify`'s fixtures are read in
//! place. Regenerate with `OCE_BLESS_CLI=1`; a receipt embeds the engine version, so a workspace
//! version bump re-blesses these.

mod support;

use support::{case, oce};

const LIBRARY: &str = "../../oce-api/tests/fixtures/library_rules";
const VERIFY_FIXTURES: &str = "../oce-verify/tests/fixtures";
const INPUTS: &str = "tests/fixtures/inputs/verify";

fn ahu_rule() -> String {
    format!("{LIBRARY}/AHU-0016/rule.cxf.jsonld")
}

fn subject(name: &str) -> String {
    format!("{VERIFY_FIXTURES}/subjects/{name}.cxf.json")
}

fn input(name: &str) -> String {
    format!("{INPUTS}/{name}")
}

// --- verdicts: the receipt on stdout ---

#[test]
fn a_passing_set_prints_its_receipt_and_answers_yes() {
    let vectors = format!("{LIBRARY}/AHU-0016/vectors.json");
    case(
        "verify_pass",
        0,
        &["verify", &vectors, "--subject", &ahu_rule()],
        b"",
    );
}

#[test]
fn a_failing_set_prints_its_receipt_and_answers_no() {
    let vectors = format!("{VERIFY_FIXTURES}/receipts/both_open_900.vectors.json");
    case(
        "verify_fail",
        1,
        &["verify", &vectors, "--subject", &ahu_rule()],
        b"",
    );
}

#[test]
fn a_set_that_checks_nothing_is_no_data_and_answers_no() {
    let vectors = format!("{VERIFY_FIXTURES}/receipts/empty_expect.vectors.json");
    case(
        "verify_no_data",
        1,
        &["verify", &vectors, "--subject", &ahu_rule()],
        b"",
    );
}

#[test]
fn a_set_with_mutants_scores_them_in_the_receipt() {
    let vectors = format!("{VERIFY_FIXTURES}/AHU-0016/vectors.json");
    case(
        "verify_mutants",
        0,
        &["verify", &vectors, "--subject", &ahu_rule()],
        b"",
    );
}

#[test]
fn a_text_subject_and_a_wire_subject_are_validated_and_emitted_first() {
    let vectors = input("econ.vectors.json");
    case(
        "verify_text_subject",
        0,
        &[
            "verify",
            &vectors,
            "--subject",
            "../oce-author/tests/fixtures/text/econ_enable.oce",
        ],
        b"",
    );
    case(
        "verify_wire_subject",
        0,
        &[
            "verify",
            &vectors,
            "--subject",
            "../oce-author/tests/fixtures/wire/econ_enable.json",
        ],
        b"",
    );
}

// --- the exit-only flags: the status flips, the receipt bytes do not ---

#[test]
fn require_mutants_caught_flips_a_surviving_mutant_to_no_with_the_same_receipt() {
    let vectors = format!("{VERIFY_FIXTURES}/AHU-0016/vectors.json");
    let without = oce(&["verify", &vectors, "--subject", &ahu_rule()], b"");
    case(
        "verify_require_mutants_caught",
        1,
        &[
            "verify",
            &vectors,
            "--subject",
            &ahu_rule(),
            "--require-mutants-caught",
        ],
        b"",
    );
    let with = oce(
        &[
            "verify",
            &vectors,
            "--subject",
            &ahu_rule(),
            "--require-mutants-caught",
        ],
        b"",
    );
    assert_eq!(without.code, 0);
    assert_eq!(
        with.stdout, without.stdout,
        "the receipt bytes do not change"
    );
}

#[test]
fn require_mutants_caught_fails_a_set_that_lists_no_mutant_with_the_same_receipt() {
    // Owner ruling of 2026-11-11: a set with no `mutants` key, or an empty list, cannot show its
    // mutants caught, so the flag answers no; without the flag the same set answers yes.
    for (vectors, subject) in [
        (format!("{LIBRARY}/AHU-0016/vectors.json"), ahu_rule()),
        (input("empty_mutants.json"), subject("hysteresis")),
    ] {
        let without = oce(&["verify", &vectors, "--subject", &subject], b"");
        let with = oce(
            &[
                "verify",
                &vectors,
                "--subject",
                &subject,
                "--require-mutants-caught",
            ],
            b"",
        );
        assert_eq!((without.code, with.code), (0, 1), "{vectors}");
        assert_eq!(
            with.stdout, without.stdout,
            "{vectors}: the receipt bytes do not change"
        );
    }
    case(
        "verify_require_mutants_caught_none_listed",
        1,
        &[
            "verify",
            &input("empty_mutants.json"),
            "--subject",
            &subject("hysteresis"),
            "--require-mutants-caught",
        ],
        b"",
    );
}

#[test]
fn deny_warnings_flips_an_unmatched_warning_to_no_with_the_same_receipt() {
    let args = |deny: bool| {
        let mut args = vec![
            "verify".to_owned(),
            input("warnings.json"),
            "--subject".to_owned(),
            subject("signals"),
        ];
        if deny {
            args.push("--deny-warnings".to_owned());
        }
        args
    };
    let run = |deny| {
        let args = args(deny);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        oce(&args, b"")
    };
    let (without, with) = (run(false), run(true));
    let shown = args(false);
    let shown: Vec<&str> = shown.iter().map(String::as_str).collect();
    case("verify_warnings", 0, &shown, b"");
    let denied = args(true);
    let denied: Vec<&str> = denied.iter().map(String::as_str).collect();
    case("verify_deny_warnings", 1, &denied, b"");
    assert_eq!((without.code, with.code), (0, 1));
    assert_eq!(
        with.stdout, without.stdout,
        "the receipt bytes do not change"
    );
}

// --- refusals: exit 2, the report, no receipt ---

#[test]
fn an_unreadable_vector_file_is_an_error() {
    case(
        "verify_missing_vectors",
        2,
        &[
            "verify",
            &input("does_not_exist.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}

#[test]
fn a_refused_vector_file_is_an_error_at_its_line_and_column() {
    case(
        "verify_malformed_vectors",
        2,
        &[
            "verify",
            &input("malformed.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}

#[test]
fn a_subject_the_engine_refuses_is_an_error_with_the_engines_codes() {
    case(
        "verify_subject_not_loading",
        2,
        &[
            "verify",
            &input("hysteresis.vectors.json"),
            "--subject",
            &input("unknown_class.cxf.json"),
        ],
        b"",
    );
}

#[test]
fn an_unresolved_name_is_an_error_naming_its_place() {
    case(
        "verify_unknown_input",
        2,
        &[
            "verify",
            &input("unknown_input.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}

#[test]
fn an_ambiguous_name_is_an_error_listing_its_candidates() {
    case(
        "verify_ambiguous_input",
        2,
        &[
            "verify",
            &input("ambiguous_input.json"),
            "--subject",
            &subject("signals"),
        ],
        b"",
    );
}

#[test]
fn a_refused_parameter_edit_is_an_error_with_its_path_and_cause() {
    case(
        "verify_param_refused",
        2,
        &[
            "verify",
            &input("param_refused.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}

#[test]
fn an_invalid_mutant_is_an_error_naming_it() {
    case(
        "verify_identical_mutant",
        2,
        &[
            "verify",
            &input("identical_mutant/vectors.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}

#[test]
fn a_mutant_path_with_a_dot_dot_segment_is_refused_with_the_set() {
    case(
        "verify_dotdot_mutant",
        2,
        &[
            "verify",
            &input("dotdot_mutant/vectors.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}

#[test]
fn a_subject_whose_name_gives_no_form_is_unknown_subject_form() {
    case(
        "verify_unknown_subject_form",
        2,
        &[
            "verify",
            &input("hysteresis.vectors.json"),
            "--subject",
            "tests/fixtures/inputs/plain.txt",
        ],
        b"",
    );
}

#[test]
fn a_refusal_renders_for_people_on_stderr_when_asked() {
    case(
        "verify_render_always",
        2,
        &[
            "--render",
            "always",
            "verify",
            &input("unknown_input.json"),
            "--subject",
            &subject("hysteresis"),
        ],
        b"",
    );
}
