//! `oce id` through the built binary, one golden transcript per outcome class (content-id note,
//! "Mapping table for the library re-stamp"; GitHub issue #513): the id and a line feed on stdout for each input form (exit
//! 0), the report under `--json`, and the report for every refusal (exit 2). `--help` is pinned
//! here beside them. Every input is named relative to the crate directory; the in-repo library
//! rule and the `oce-author` fixtures are read in place. Regenerate with `OCE_BLESS_CLI=1`.
//! `id_files.rs` holds the printed ids to `Engine::content_id`.

mod support;

use support::{author_fixture, case, input};

const AHU_RULE: &str = "../../oce-api/tests/fixtures/library_rules/AHU-0016/rule.cxf.jsonld";

fn econ_text() -> String {
    author_fixture("text/econ_enable.oce")
}

fn econ_wire() -> String {
    author_fixture("wire/econ_enable.json")
}

fn econ_cxf() -> String {
    author_fixture("emit/econ_enable.cxf.json")
}

fn bytes_of(relative: &str) -> Vec<u8> {
    std::fs::read(support::crate_dir().join(relative)).expect("fixture")
}

// --- the id: one line on stdout ---

#[test]
fn a_cxf_document_prints_its_id_alone() {
    case("id_cxf", 0, &["id", AHU_RULE], b"");
}

#[test]
fn a_text_file_prints_the_id_of_its_emitted_document() {
    case("id_text", 0, &["id", &econ_text()], b"");
}

#[test]
fn a_wire_file_prints_the_id_of_its_emitted_document() {
    case("id_wire", 0, &["id", &econ_wire()], b"");
}

#[test]
fn a_cxf_document_on_stdin_prints_its_id() {
    case(
        "id_stdin_cxf",
        0,
        &["id", "-", "--from", "cxf"],
        &bytes_of(&econ_cxf()),
    );
}

#[test]
fn a_text_file_on_stdin_prints_its_id() {
    case(
        "id_stdin_text",
        0,
        &["id", "-", "--from", "text"],
        &bytes_of(&econ_text()),
    );
}

#[test]
fn a_namespace_is_accepted_for_an_authored_file() {
    case(
        "id_text_namespace",
        0,
        &["id", &econ_text(), "--namespace", "urn:plant:ahu1#"],
        b"",
    );
}

#[test]
fn render_always_repeats_the_id_on_stderr_and_leaves_stdout_alone() {
    case(
        "id_render_always",
        0,
        &["id", AHU_RULE, "--render", "always"],
        b"",
    );
}

// --- the report under --json ---

#[test]
fn json_prints_the_report_with_the_id_of_a_cxf_document() {
    case("id_cxf_json", 0, &["id", AHU_RULE, "--json"], b"");
}

#[test]
fn json_prints_the_report_with_the_id_and_stages_of_a_text_file() {
    case("id_text_json", 0, &["id", &econ_text(), "--json"], b"");
}

// --- refusals: the report, exit 2 ---

#[test]
fn a_cxf_document_the_engine_refuses_is_an_error_with_the_engines_codes() {
    case(
        "id_cxf_not_loading",
        2,
        &["id", &input("verify/unknown_class.cxf.json")],
        b"",
    );
}

#[test]
fn a_cxf_document_that_is_not_json_is_an_error() {
    case(
        "id_cxf_not_json",
        2,
        &["id", &input("malformed.oce"), "--from", "cxf"],
        b"",
    );
}

#[test]
fn a_text_file_with_stage_one_findings_has_no_id() {
    case(
        "id_text_stage_one_findings",
        2,
        &["id", &input("stage_one.oce")],
        b"",
    );
}

#[test]
fn a_text_file_the_engine_stage_refuses_has_no_id() {
    case(
        "id_text_engine_finding",
        2,
        &["id", &input("unused_input.oce")],
        b"",
    );
}

#[test]
fn a_wire_file_with_stage_one_findings_has_no_id() {
    let defects = author_fixture("wire/defects.json");
    case("id_wire_findings", 2, &["id", &defects], b"");
}

#[test]
fn malformed_text_is_an_error_at_its_span() {
    case(
        "id_malformed_text",
        2,
        &["id", &input("malformed.oce")],
        b"",
    );
}

#[test]
fn malformed_wire_is_an_error_at_its_span() {
    case(
        "id_malformed_wire",
        2,
        &["id", &input("malformed.json")],
        b"",
    );
}

#[test]
fn a_malformed_namespace_is_an_error() {
    case(
        "id_bad_namespace",
        2,
        &["id", &econ_text(), "--namespace", "urn:plant"],
        b"",
    );
}

#[test]
fn a_namespace_beside_a_cxf_document_is_an_error() {
    case(
        "id_namespace_on_cxf",
        2,
        &["id", AHU_RULE, "--namespace", "urn:plant:"],
        b"",
    );
}

#[test]
fn stdin_without_from_is_an_error() {
    case("id_stdin_without_from", 2, &["id", "-"], b"{}");
}

#[test]
fn a_name_that_gives_no_form_is_an_error() {
    case("id_unknown_form", 2, &["id", &input("plain.txt")], b"");
}

#[test]
fn a_missing_file_is_an_error() {
    case("id_missing_file", 2, &["id", "missing.cxf.json"], b"");
}

#[test]
fn a_directory_is_an_error() {
    case(
        "id_directory",
        2,
        &["id", "tests/fixtures", "--from", "cxf"],
        b"",
    );
}

#[test]
fn a_refusal_is_rendered_on_stderr_when_asked() {
    case(
        "id_render_refusal",
        2,
        &[
            "id",
            &input("verify/unknown_class.cxf.json"),
            "--render",
            "always",
        ],
        b"",
    );
}

#[test]
fn an_unknown_form_value_is_a_usage_error() {
    case(
        "id_usage_bad_from",
        2,
        &["id", AHU_RULE, "--from", "jsonld"],
        b"",
    );
}

#[test]
fn help_is_pinned() {
    case("help_id", 0, &["id", "--help"], b"");
}
