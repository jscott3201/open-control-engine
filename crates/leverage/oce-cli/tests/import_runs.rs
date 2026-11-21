//! `oce import` through the built binary, one golden transcript per outcome class (importer note
//! `docs/design/importer.md`; GitHub issue #514): the canonical text or the wire form on stdout
//! (exit 0), the report with the importer's `dropped` and `normalised` lists under `-o`, and the
//! report for every refusal (exit 2), one per construct outside the authoring subset. `--help`
//! is pinned here beside them. Every input is a tracked fixture read in
//! place and named relative to the crate directory: a G36 corpus fixture, an in-repo library
//! rule, and `oce-cxf`'s contract fixtures for the constructs the G36 corpus does not hold.
//! Regenerate with `OCE_BLESS_CLI=1`. `import_files.rs` holds the behaviour the transcripts do
//! not show, and `import_corpus.rs` the round trip over every tracked CXF document.

mod support;

use support::{case, case_in, crate_dir, scratch};

/// A G36 corpus fixture whose import drops ten descriptive attributes and normalises one
/// package-typed boundary.
const LIMITS: &str =
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld";
/// An in-repo library rule, which imports with nothing dropped.
const AHU_RULE: &str = "../../oce-api/tests/fixtures/library_rules/AHU-0016/rule.cxf.jsonld";

fn bytes_of(relative: &str) -> Vec<u8> {
    std::fs::read(crate_dir().join(relative)).expect("fixture")
}

/// `oce-cxf`'s fixture `name`, relative to the crate directory.
fn cxf_fixture(name: &str) -> String {
    format!("../../oce-cxf/tests/fixtures/{name}")
}

// --- the document on stdout ---

#[test]
fn a_corpus_fixture_prints_its_canonical_text() {
    case("import_text", 0, &["import", LIMITS], b"");
}

#[test]
fn a_library_rule_prints_its_wire_form_when_asked() {
    case("import_wire", 0, &["import", AHU_RULE, "--wire"], b"");
}

#[test]
fn a_document_on_stdin_needs_no_form_flag() {
    case("import_stdin", 0, &["import", "-"], &bytes_of(AHU_RULE));
}

#[test]
fn the_rendering_counts_what_was_not_carried() {
    case(
        "import_render_always",
        0,
        &["import", LIMITS, "--render", "always"],
        b"",
    );
}

// --- the report under -o ---

#[test]
fn the_report_lists_what_the_importer_dropped_and_normalised() {
    let dir = scratch("import_output_report");
    std::fs::write(dir.join("common.cxf.jsonld"), bytes_of(LIMITS)).expect("copy");
    case_in(
        &dir,
        "import_output_report",
        0,
        &["import", "common.cxf.jsonld", "-o", "common.oce"],
    );
    // The file is the document the plain run prints.
    let plain = support::oce(&["import", LIMITS], b"");
    assert_eq!(
        std::fs::read(dir.join("common.oce")).expect("the output"),
        plain.stdout
    );
}

#[test]
fn the_report_of_a_wire_output_names_its_form() {
    let dir = scratch("import_output_wire_report");
    std::fs::write(dir.join("rule.cxf.jsonld"), bytes_of(AHU_RULE)).expect("copy");
    case_in(
        &dir,
        "import_output_wire_report",
        0,
        &["import", "rule.cxf.jsonld", "--wire", "-o", "rule.json"],
    );
    let plain = support::oce(&["import", AHU_RULE, "--wire"], b"");
    assert_eq!(
        std::fs::read(dir.join("rule.json")).expect("the output"),
        plain.stdout
    );
}

// --- one refusal per construct outside the authoring subset ---

#[test]
fn a_nested_composite_is_refused() {
    case(
        "import_nested_composite",
        2,
        &["import", &cxf_fixture("g36/cooling_only_controller.jsonld")],
        b"",
    );
}

#[test]
fn a_conditional_component_is_refused() {
    case(
        "import_conditional_component",
        2,
        &[
            "import",
            &cxf_fixture("g36/multizone_vav_plant_requests.jsonld"),
        ],
        b"",
    );
}

#[test]
fn an_expression_value_is_refused() {
    case(
        "import_expression_value",
        2,
        &[
            "import",
            &cxf_fixture("g36/multizone_vav_relief_fan.jsonld"),
        ],
        b"",
    );
}

#[test]
fn a_block_port_attribute_is_refused() {
    case(
        "import_port_attribute",
        2,
        &["import", &cxf_fixture("g36/ahu_economizer.jsonld")],
        b"",
    );
}

#[test]
fn an_instance_member_is_refused() {
    case(
        "import_instance_member",
        2,
        &[
            "import",
            &cxf_fixture("composite_contract/rejected/vector_port_instance.jsonld"),
        ],
        b"",
    );
}

#[test]
fn a_constant_member_is_refused() {
    case(
        "import_constant_member",
        2,
        &[
            "import",
            &cxf_fixture("composite_contract/rejected/duplicate_declaration.jsonld"),
        ],
        b"",
    );
}

#[test]
fn an_array_connector_is_refused() {
    case(
        "import_array_connector",
        2,
        &[
            "import",
            &cxf_fixture("composite_contract/rejected/array_connector.jsonld"),
        ],
        b"",
    );
}

#[test]
fn an_array_sequence_parameter_is_refused() {
    case(
        "import_array_sequence_parameter",
        2,
        &[
            "import",
            &cxf_fixture("composite_contract/rejected/array_parameter.jsonld"),
        ],
        b"",
    );
}

// --- refusals of the read, the bytes and the spelling ---

#[test]
fn a_file_that_is_not_json_is_refused_by_the_importer() {
    case(
        "import_not_json",
        2,
        &["import", "tests/fixtures/inputs/unused_input.oce"],
        b"",
    );
}

#[test]
fn a_missing_file_is_an_error() {
    case("import_missing_file", 2, &["import", "missing.jsonld"], b"");
}

#[test]
fn a_directory_is_an_error() {
    case("import_directory", 2, &["import", "tests/fixtures"], b"");
}

#[test]
fn a_refusal_is_rendered_on_stderr_when_asked() {
    case(
        "import_render_refusal",
        2,
        &[
            "import",
            &cxf_fixture("g36/ahu_economizer.jsonld"),
            "--render",
            "always",
        ],
        b"",
    );
}

#[test]
fn a_sequence_the_text_form_cannot_spell_is_refused_and_its_report_keeps_the_lists() {
    // A class path with a space: the importer carries it for the validator to name, the text
    // form has no spelling for it, and the wire form does (`import_files.rs`).
    let dir = scratch("import_unspellable_text");
    let cxf = String::from_utf8(bytes_of(LIMITS))
        .expect("UTF-8")
        .replace("OBC.CDL.Logical.Not", "OBC.CDL.Logical.N ot");
    std::fs::write(dir.join("common.cxf.jsonld"), cxf).expect("write");
    case_in(
        &dir,
        "import_unspellable_text",
        2,
        &["import", "common.cxf.jsonld"],
    );
}

#[test]
fn there_is_no_form_flag() {
    case(
        "import_usage_from",
        2,
        &["import", AHU_RULE, "--from", "cxf"],
        b"",
    );
}

#[test]
fn help_is_pinned() {
    case("help_import", 0, &["import", "--help"], b"");
}
