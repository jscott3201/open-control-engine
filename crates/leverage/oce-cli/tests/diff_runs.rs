//! `oce diff` through the built binary, one golden transcript per outcome class (diff note,
//! "CLI", decisions 1 and 16; GitHub issue #668): exit 0 for a pair with nothing different, exit
//! 1 with the JSON envelope or the Markdown table of each of the note's worked examples, in the
//! text form and as CXF, and exit 2 with the report for every refusal. `--help` is pinned beside
//! them. Inputs are named relative to the crate directory (`oce-diff`'s examples and `oce-author`'s
//! fixtures are read in place), or relative to a scratch directory for the CXF the test emits.
//! Regenerate with `OCE_BLESS_CLI=1`. `diff_files.rs` holds every diff printed here to
//! `oce-diff`'s own goldens.

mod support;

use oce_api::Engine;
use support::diff_inputs::{emitted, example, example_forms, own, text_of, write};
use support::{author_fixture, case, case_in, input, scratch};

/// The three worked examples of the note, as `oce-diff` names them.
const WORKED: [&str; 3] = ["site_retune", "pressure_request", "plant_reset"];

fn reference() -> String {
    own("reference.oce")
}

// --- exit 0: nothing differs ---

#[test]
fn a_sequence_against_itself_is_equal_and_exits_zero() {
    let base = example("site_retune", "base");
    case("diff_equal", 0, &["diff", &base, &base], b"");
}

#[test]
fn a_cxf_document_against_a_reordering_of_itself_is_equal() {
    // Order is not compared at the CXF level: the reordered draft emits a document with another
    // block and connection order and the same content id.
    let dir = scratch("diff_runs_cxf_reordered");
    write(&dir, "reference.jsonld", &emitted(&text_of(&reference())));
    write(
        &dir,
        "reordered.jsonld",
        &emitted(&text_of(&own("reordered.oce"))),
    );
    case_in(
        &dir,
        "diff_cxf_reordered",
        0,
        &["diff", "reference.jsonld", "reordered.jsonld"],
    );
}

// --- exit 1: a difference, the diff on stdout ---

#[test]
fn each_worked_example_prints_its_json_envelope() {
    for name in WORKED {
        let (base, head) = (example(name, "base"), example(name, "head"));
        case(&format!("diff_{name}"), 1, &["diff", &base, &head], b"");
    }
}

#[test]
fn each_worked_example_prints_its_markdown_table() {
    for name in WORKED {
        let (base, head) = (example(name, "base"), example(name, "head"));
        case(
            &format!("diff_{name}_markdown"),
            1,
            &["diff", &base, &head, "--format", "markdown"],
            b"",
        );
    }
}

#[test]
fn each_worked_example_as_cxf_prints_its_json_envelope() {
    let dir = example_forms("diff_runs_cxf");
    for name in WORKED {
        let (base, head) = (format!("{name}.base.jsonld"), format!("{name}.head.jsonld"));
        case_in(
            &dir,
            &format!("diff_cxf_{name}"),
            1,
            &["diff", &base, &head],
        );
    }
}

#[test]
fn render_always_writes_the_text_rendering_to_stderr() {
    let (base, head) = (
        example("plant_reset", "base"),
        example("plant_reset", "head"),
    );
    case(
        "diff_render_always",
        1,
        &[
            "diff", &base, &head, "--format", "markdown", "--render", "always",
        ],
        b"",
    );
}

#[test]
fn a_reordered_only_pair_differs_with_the_same_structure() {
    case(
        "diff_reordered_only",
        1,
        &["diff", &reference(), &own("reordered.oce")],
        b"",
    );
}

#[test]
fn no_renames_reports_the_rename_pass_off() {
    let (base, head) = (
        example("plant_reset", "base"),
        example("plant_reset", "head"),
    );
    case(
        "diff_no_renames",
        1,
        &["diff", &base, &head, "--no-renames", "--format", "markdown"],
        b"",
    );
}

#[test]
fn an_invalid_draft_that_parses_is_diffed_not_refused() {
    case(
        "diff_invalid_draft",
        1,
        &["diff", &reference(), &own("draft.oce")],
        b"",
    );
}

#[test]
fn one_side_is_read_from_stdin_with_from() {
    case(
        "diff_stdin_head",
        1,
        &["diff", &reference(), "-", "--from", "text"],
        text_of(&own("reordered.oce")).as_bytes(),
    );
}

// --- exit 2: no diff, the report on stdout ---

#[test]
fn a_mixed_pair_is_an_error() {
    let wire = author_fixture("wire/econ_enable.json");
    case(
        "diff_mixed_forms",
        2,
        &["diff", &author_fixture("text/econ_enable.oce"), &wire],
        b"",
    );
}

#[test]
fn stdin_on_both_sides_is_an_error() {
    case(
        "diff_stdin_twice",
        2,
        &["diff", "-", "-", "--from", "text"],
        b"",
    );
}

#[test]
fn an_unparsable_base_is_an_error_about_the_base() {
    case(
        "diff_unparsable_base",
        2,
        &["diff", &input("malformed.oce"), &reference()],
        b"",
    );
}

#[test]
fn an_unparsable_head_is_an_error_about_the_head() {
    case(
        "diff_unparsable_head",
        2,
        &["diff", &reference(), &input("malformed.oce")],
        b"",
    );
}

#[test]
fn an_unparsable_wire_head_is_an_error_at_its_span() {
    case(
        "diff_unparsable_wire_head",
        2,
        &[
            "diff",
            &author_fixture("wire/econ_enable.json"),
            &input("malformed.json"),
        ],
        b"",
    );
}

#[test]
fn a_cxf_document_the_engine_refuses_is_an_error_with_the_engines_codes() {
    case(
        "diff_cxf_not_loading",
        2,
        &[
            "diff",
            &author_fixture("emit/econ_enable.cxf.json"),
            &input("verify/unknown_class.cxf.json"),
        ],
        b"",
    );
}

#[test]
fn a_document_the_engine_exported_is_outside_its_root() {
    let dir = scratch("diff_runs_outside_root");
    let original = emitted(&text_of(&reference()));
    let mut engine = Engine::new();
    engine.load_cxf(&original).expect("the reference loads");
    let export = engine.export_cxf().expect("the model exports");
    write(&dir, "reference.jsonld", &original);
    write(&dir, "exported.jsonld", &export.bytes);
    case_in(
        &dir,
        "diff_outside_root",
        2,
        &["diff", "reference.jsonld", "exported.jsonld"],
    );
}

#[test]
fn stdin_without_from_is_an_error_about_its_side() {
    case(
        "diff_stdin_without_from",
        2,
        &["diff", "-", &reference()],
        b"",
    );
}

#[test]
fn a_name_that_gives_no_form_is_an_error() {
    case(
        "diff_unknown_form",
        2,
        &["diff", &reference(), &input("plain.txt")],
        b"",
    );
}

#[test]
fn a_missing_head_is_an_error_about_the_head() {
    case(
        "diff_missing_file",
        2,
        &["diff", &reference(), "missing.oce"],
        b"",
    );
}

#[test]
fn a_refusal_of_the_head_is_rendered_at_the_heads_name() {
    case(
        "diff_render_refusal",
        2,
        &[
            "diff",
            &reference(),
            &input("malformed.oce"),
            "--render",
            "always",
        ],
        b"",
    );
}

#[test]
fn a_format_the_verb_does_not_have_is_a_usage_error() {
    case(
        "diff_usage_bad_format",
        2,
        &["diff", &reference(), &reference(), "--format", "human"],
        b"",
    );
}

#[test]
fn one_side_alone_is_a_usage_error() {
    case("diff_usage_one_side", 2, &["diff", &reference()], b"");
}

#[test]
fn help_is_pinned() {
    case("help_diff", 0, &["diff", "--help"], b"");
}
