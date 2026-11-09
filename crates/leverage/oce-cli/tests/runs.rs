//! Every subcommand × outcome class through the built binary: the exact exit status, stdout and
//! stderr of each run, pinned as one golden transcript per case in `tests/fixtures/runs/`
//! (`exit: N`, `--- stdout`, `--- stderr`). Stdout is the JSON report, or the command's document
//! (CXF, canonical text, a listing); stderr is clap's message or, under `--render always`, the
//! human-readable rendering. Regenerate with `OCE_BLESS_CLI=1`.

mod support;

use support::{author_fixture, compare_or_bless, input, oce};

const ECON_TEXT: &str = "text/econ_enable.oce";
const ECON_WIRE: &str = "wire/econ_enable.json";

/// Run one case, check its exit status, and compare its transcript with the golden.
fn case(name: &str, expected_exit: i32, args: &[&str], stdin: &[u8]) {
    let run = oce(args, stdin);
    assert_eq!(
        run.code,
        expected_exit,
        "{name}: exit status\n{}",
        run.transcript()
    );
    compare_or_bless(&format!("runs/{name}.txt"), &run.transcript());
}

fn econ_text() -> String {
    author_fixture(ECON_TEXT)
}

fn econ_wire() -> String {
    author_fixture(ECON_WIRE)
}

fn bytes_of(relative: &str) -> Vec<u8> {
    std::fs::read(support::crate_dir().join(relative)).expect("fixture")
}

// --- validate ---

#[test]
fn validate_clean_text_answers_yes() {
    case("validate_text_clean", 0, &["validate", &econ_text()], b"");
}

#[test]
fn validate_clean_wire_answers_yes() {
    case("validate_wire_clean", 0, &["validate", &econ_wire()], b"");
}

#[test]
fn validate_wire_with_stage_one_defects_answers_no() {
    let defects = author_fixture("wire/defects.json");
    case("validate_wire_findings", 1, &["validate", &defects], b"");
}

#[test]
fn validate_text_findings_carry_spans() {
    case(
        "validate_text_findings",
        1,
        &["validate", &input("stage_one.oce")],
        b"",
    );
}

#[test]
fn validate_reports_an_engine_finding_from_stage_two() {
    case(
        "validate_engine_finding",
        1,
        &["validate", &input("unused_input.oce")],
        b"",
    );
}

#[test]
fn validate_author_side_skips_the_engine_stage() {
    case(
        "validate_author_side",
        0,
        &["validate", "--author-side", &input("unused_input.oce")],
        b"",
    );
}

#[test]
fn validate_malformed_text_is_an_error() {
    case(
        "validate_malformed_text",
        2,
        &["validate", &input("malformed.oce")],
        b"",
    );
}

#[test]
fn validate_malformed_wire_is_an_error() {
    case(
        "validate_malformed_wire",
        2,
        &["validate", &input("malformed.json")],
        b"",
    );
}

#[test]
fn validate_missing_file_is_an_error() {
    case(
        "validate_missing_file",
        2,
        &["validate", &input("does_not_exist.oce")],
        b"",
    );
}

#[test]
fn validate_directory_is_an_error() {
    case(
        "validate_directory",
        2,
        &["validate", "--from", "text", "tests/fixtures/inputs"],
        b"",
    );
}

#[test]
fn validate_unknown_extension_is_an_error() {
    case(
        "validate_unknown_form",
        2,
        &["validate", &input("plain.txt")],
        b"",
    );
}

#[test]
fn validate_stdin_without_from_is_an_error() {
    case(
        "validate_stdin_without_from",
        2,
        &["validate", "-"],
        &bytes_of(&econ_text()),
    );
}

#[test]
fn validate_stdin_text_with_from_answers_yes() {
    case(
        "validate_stdin_text",
        0,
        &["validate", "-", "--from", "text"],
        &bytes_of(&econ_text()),
    );
}

#[test]
fn validate_stdin_wire_with_from_answers_yes() {
    case(
        "validate_stdin_wire",
        0,
        &["validate", "-", "--from", "wire"],
        &bytes_of(&econ_wire()),
    );
}

#[test]
fn validate_renders_findings_on_stderr_when_asked() {
    case(
        "validate_render_always",
        1,
        &["validate", "--render", "always", &input("stage_one.oce")],
        b"",
    );
}

#[test]
fn validate_renders_a_refusal_on_stderr_when_asked() {
    case(
        "validate_render_refusal",
        2,
        &["validate", "--render", "always", &input("malformed.oce")],
        b"",
    );
}

// --- emit ---

#[test]
fn emit_text_writes_cxf_to_stdout() {
    case("emit_text_stdout", 0, &["emit", &econ_text()], b"");
}

#[test]
fn emit_wire_under_a_namespace_writes_cxf_to_stdout() {
    case(
        "emit_wire_namespace",
        0,
        &[
            "emit",
            &econ_wire(),
            "--namespace",
            "https://example.org/plant#",
        ],
        b"",
    );
}

#[test]
fn emit_refuses_stage_one_findings_and_writes_nothing() {
    case(
        "emit_stage_one_refusal",
        1,
        &["emit", &input("stage_one.oce")],
        b"",
    );
}

#[test]
fn emit_refuses_engine_findings_and_writes_nothing() {
    case(
        "emit_engine_refusal",
        1,
        &["emit", &input("unused_input.oce")],
        b"",
    );
}

#[test]
fn emit_malformed_namespace_is_an_error() {
    case(
        "emit_bad_namespace",
        2,
        &["emit", &econ_text(), "--namespace", "urn:plant"],
        b"",
    );
}

#[test]
fn emit_malformed_input_is_an_error() {
    case("emit_malformed", 2, &["emit", &input("malformed.oce")], b"");
}

#[test]
fn emit_stdin_without_from_is_an_error() {
    case(
        "emit_stdin_without_from",
        2,
        &["emit", "-"],
        &bytes_of(&econ_text()),
    );
}

// --- fmt ---

#[test]
fn fmt_prints_canonical_text() {
    let comments = author_fixture("text/fmt/comments.in.oce");
    case("fmt_text", 0, &["fmt", &comments], b"");
}

#[test]
fn fmt_converts_wire_to_text() {
    case("fmt_wire_to_text", 0, &["fmt", &econ_wire()], b"");
}

#[test]
fn fmt_check_on_canonical_text_answers_yes() {
    case(
        "fmt_check_canonical",
        0,
        &["fmt", "--check", &econ_text()],
        b"",
    );
}

#[test]
fn fmt_check_on_non_canonical_text_answers_no() {
    let arrays = author_fixture("text/fmt/arrays.in.oce");
    case(
        "fmt_check_not_canonical",
        1,
        &["fmt", "--check", &arrays],
        b"",
    );
}

#[test]
fn fmt_check_on_canonical_wire_answers_yes() {
    case(
        "fmt_check_wire_canonical",
        0,
        &["fmt", "--check", &econ_wire()],
        b"",
    );
}

#[test]
fn fmt_check_on_non_canonical_wire_answers_no() {
    let compact: Vec<u8> = bytes_of(&econ_wire())
        .into_iter()
        .filter(|b| *b != b'\n')
        .collect();
    case(
        "fmt_check_wire_not_canonical",
        1,
        &["fmt", "--check", "-", "--from", "wire"],
        &compact,
    );
}

#[test]
fn fmt_malformed_input_is_an_error() {
    case("fmt_malformed", 2, &["fmt", &input("malformed.oce")], b"");
}

#[test]
fn fmt_write_refuses_stdin() {
    case(
        "fmt_write_stdin",
        2,
        &["fmt", "--write", "-", "--from", "text"],
        &bytes_of(&econ_text()),
    );
}

#[test]
fn fmt_write_refuses_a_wire_file() {
    case("fmt_write_wire", 2, &["fmt", "--write", &econ_wire()], b"");
}

#[test]
fn fmt_check_and_write_together_are_a_usage_error() {
    case(
        "fmt_check_write_conflict",
        2,
        &["fmt", "--check", "--write", &econ_text()],
        b"",
    );
}

#[test]
fn fmt_stdin_without_from_is_an_error() {
    case(
        "fmt_stdin_without_from",
        2,
        &["fmt", "-"],
        &bytes_of(&econ_text()),
    );
}

// --- catalog ---

#[test]
fn catalog_lists_every_class() {
    case("catalog_list", 0, &["catalog"], b"");
}

#[test]
fn catalog_describes_one_class() {
    case(
        "catalog_class",
        0,
        &[
            "catalog",
            "CDL.Reals.Hysteresis",
            "--param",
            "uLow=-0.5",
            "--param",
            "uHigh=0.5",
        ],
        b"",
    );
}

#[test]
fn catalog_resolves_ports_at_given_parameters() {
    case(
        "catalog_class_params_json",
        0,
        &[
            "catalog",
            "CDL.Reals.MultiSum",
            "--json",
            "--param",
            "nin=3",
            "--param",
            "k_2=0.5",
        ],
        b"",
    );
}

#[test]
fn catalog_takes_an_enumeration_member_by_name() {
    case(
        "catalog_enum_param",
        0,
        &[
            "catalog",
            "CDL.Reals.PID",
            "--json",
            "--param",
            "controllerType=SimpleController.P",
        ],
        b"",
    );
}

#[test]
fn catalog_unknown_enumeration_member_is_an_error() {
    case(
        "catalog_enum_unknown_member",
        2,
        &["catalog", "CDL.Reals.PID", "--param", "controllerType=PIX"],
        b"",
    );
}

#[test]
fn catalog_class_with_required_parameters_does_not_resolve_at_defaults() {
    case(
        "catalog_unresolved",
        1,
        &["catalog", "CDL.Reals.Limiter", "--json"],
        b"",
    );
}

#[test]
fn catalog_reports_the_class_rules_refusal() {
    case(
        "catalog_rule_refusal",
        1,
        &["catalog", "CDL.Reals.MultiSum", "--param", "nin=-1"],
        b"",
    );
}

#[test]
fn catalog_unknown_class_answers_no() {
    case(
        "catalog_unknown_class",
        1,
        &["catalog", "CDL.Reals.Nope"],
        b"",
    );
}

#[test]
fn catalog_param_without_equals_is_an_error() {
    case(
        "catalog_malformed_param",
        2,
        &["catalog", "CDL.Reals.MultiSum", "--param", "nin"],
        b"",
    );
}

#[test]
fn catalog_unknown_param_is_an_error() {
    case(
        "catalog_unknown_param",
        2,
        &["catalog", "CDL.Reals.MultiSum", "--param", "k_0=1"],
        b"",
    );
}

#[test]
fn catalog_ill_typed_value_is_an_error() {
    case(
        "catalog_malformed_value",
        2,
        &["catalog", "CDL.Reals.MultiSum", "--param", "nin=three"],
        b"",
    );
}

#[test]
fn catalog_repeated_param_is_an_error() {
    case(
        "catalog_duplicate_param",
        2,
        &[
            "catalog",
            "CDL.Reals.MultiSum",
            "--param",
            "nin=2",
            "--param",
            "nin=3",
        ],
        b"",
    );
}

#[test]
fn catalog_param_without_class_is_a_usage_error() {
    case(
        "catalog_param_without_class",
        2,
        &["catalog", "--param", "nin=3"],
        b"",
    );
}

// --- vocab ---

#[test]
fn vocab_prints_text() {
    case("vocab_text", 0, &["vocab"], b"");
}

#[test]
fn vocab_prints_json() {
    case("vocab_json", 0, &["vocab", "--json"], b"");
}

// --- usage ---

#[test]
fn no_arguments_is_a_usage_error() {
    case("usage_no_arguments", 2, &[], b"");
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    case("usage_unknown_subcommand", 2, &["check"], b"");
}

#[test]
fn unknown_flag_is_a_usage_error() {
    case(
        "usage_unknown_flag",
        2,
        &["validate", "--author-only", &econ_text()],
        b"",
    );
}

#[test]
fn bad_render_value_is_a_usage_error() {
    case(
        "usage_bad_render",
        2,
        &["vocab", "--render", "sometimes"],
        b"",
    );
}

// --- help (flag drift is visible here) ---

#[test]
fn help_is_pinned_for_the_tool_and_every_subcommand() {
    for (name, args) in [
        ("help_oce", &["--help"][..]),
        ("help_emit", &["emit", "--help"][..]),
        ("help_validate", &["validate", "--help"][..]),
        ("help_fmt", &["fmt", "--help"][..]),
        ("help_catalog", &["catalog", "--help"][..]),
        ("help_vocab", &["vocab", "--help"][..]),
    ] {
        case(name, 0, args, b"");
    }
}

#[test]
fn version_is_the_workspace_version() {
    let run = oce(&["--version"], b"");
    assert_eq!(run.code, 0);
    assert_eq!(
        run.stdout_text(),
        format!("oce {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(run.stderr.is_empty());
}
