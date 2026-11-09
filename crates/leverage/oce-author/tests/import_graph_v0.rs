//! The v0 graph JSON importer (importer note, "The v0 graph JSON importer"; decisions 17 and 18;
//! issue #496) over the prototype's three vendored golden pairs (`tests/fixtures/v0/`, see its
//! `PROVENANCE.md`): each graph imports; emitting it with the default namespace and loading it
//! gives the same canonical content id as loading the CXF the prototype wrote (no re-stamp:
//! `urn:mechanical-muse:<name>#` strips to the same names); the comment annotations and the
//! report match a golden; v0 to model to wire to model is identity; and the comment rules keep
//! any v0 text printable, parseable and `fmt`-stable.

mod support;

use oce_author::{
    DroppedProperty, EmitOptions, ImportOptions, Imported, MAX_V0_COMMENT_LINE_BYTES, ProvenanceId,
    Sequence, SourceForm, V0_PROVENANCE_COMMENT, emit, fmt_text, from_wire_str, import_graph_v0,
    parse_text, print_text, to_wire_string,
};

use support::cxf::{identity, load, load_clean};
use support::{compare_or_bless, fixture};

/// The vendored golden pairs, by name.
const PAIRS: &[&str] = &["ahu_0005", "ahu_0027", "width_max_min"];

fn read(name: &str, extension: &str) -> Vec<u8> {
    std::fs::read(fixture(&format!("v0/{name}.{extension}"))).expect("vendored fixture")
}

fn import(bytes: &[u8]) -> Imported {
    import_graph_v0(bytes, &ImportOptions::new()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn every_vendored_graph_loads_to_the_content_id_of_the_prototypes_cxf() {
    for name in PAIRS {
        let imported = import(&read(name, "graph.json"));
        assert_eq!(imported.report.source, SourceForm::GraphV0);
        assert_eq!(imported.sequence.name.as_str(), *name);
        let emitted =
            emit(&imported.sequence, &EmitOptions::new()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let from_model = load_clean(&emitted);
        let (prototype, _) = load(&read(name, "cxf.jsonld"))
            .unwrap_or_else(|e| panic!("{name}: the prototype's CXF loads: {e}"));
        assert_eq!(identity(&from_model), identity(&prototype), "{name}");
    }
}

/// What the importer made of a graph, as reviewable text: the report, then the canonical text
/// form (comments included).
fn rendering(imported: &Imported) -> String {
    let mut out = String::from("# report\n");
    for dropped in &imported.report.dropped {
        out.push_str(&format!(
            "dropped {} {}: {:?}\n",
            dropped.subject,
            dropped.property.as_str(),
            dropped.value
        ));
    }
    for truncated in &imported.report.truncated {
        out.push_str(&format!(
            "truncated {} {} line {}: {} -> {} bytes\n",
            truncated.subject,
            truncated.property.as_str(),
            truncated.line,
            truncated.bytes,
            truncated.kept
        ));
    }
    out.push_str("# text\n");
    out.push_str(&print_text(&imported.sequence).expect("prints"));
    out
}

#[test]
fn the_comments_and_the_report_match_the_golden() {
    for name in PAIRS {
        let imported = import(&read(name, "graph.json"));
        assert!(imported.report.normalised.is_empty() && imported.report.truncated.is_empty());
        compare_or_bless(
            &fixture(&format!("v0/{name}.import.txt")),
            &rendering(&imported),
        );
    }
}

#[test]
fn every_unit_and_description_is_a_comment_and_a_dropped_entry_never_a_point_unit() {
    for name in PAIRS {
        let imported = import(&read(name, "graph.json"));
        let sequence = &imported.sequence;
        assert_eq!(sequence.annotations.comments, [V0_PROVENANCE_COMMENT]);
        for boundary in sequence.inputs.iter().chain(&sequence.outputs) {
            assert_eq!(boundary.semantic.unit, None, "{name}.{}", boundary.name);
            let dropped: Vec<DroppedProperty> = imported
                .report
                .dropped
                .iter()
                .filter(|d| d.subject == boundary.name.as_str())
                .map(|d| d.property)
                .collect();
            let is_input = sequence.inputs.contains(boundary);
            let expected: &[DroppedProperty] = if is_input {
                &[DroppedProperty::Description, DroppedProperty::Unit]
            } else {
                &[DroppedProperty::Description]
            };
            assert_eq!(dropped, expected, "{name}.{}", boundary.name);
            let last = boundary.annotations.comments.last().expect("a comment");
            assert_eq!(last.starts_with(" unit: "), is_input, "{last}");
        }
    }
}

/// The model without its text-only annotations (the wire form carries none).
fn without_annotations(mut sequence: Sequence) -> Sequence {
    sequence.annotations = Default::default();
    sequence.trailing_comments.clear();
    for boundary in sequence
        .inputs
        .iter_mut()
        .chain(sequence.outputs.iter_mut())
    {
        boundary.annotations = Default::default();
    }
    sequence
}

#[test]
fn v0_to_model_to_wire_to_model_is_identity() {
    for name in PAIRS {
        let model = import(&read(name, "graph.json")).sequence;
        let wire = to_wire_string(&model);
        let back = from_wire_str(&wire).expect("the wire form reads back");
        assert_eq!(back, without_annotations(model), "{name}");
    }
}

#[test]
fn the_printed_text_parses_back_to_the_imported_model_and_fmt_is_idempotent() {
    for name in PAIRS {
        let model = import(&read(name, "graph.json")).sequence;
        let text = print_text(&model).expect("prints");
        assert_eq!(parse_text(&text).expect("parses"), model, "{name}");
        assert_eq!(fmt_text(&text).expect("fmt"), text, "{name}");
    }
}

#[test]
fn the_options_set_extends_and_the_provenance_never_reaches_the_wire_or_the_emitter() {
    let bytes = read("ahu_0027", "graph.json");
    let plain = import(&bytes).sequence;
    assert_eq!(plain.extends, None);
    let id = ProvenanceId::new("cxf:fnv1a128:00112233445566778899aabbccddeeff").expect("id");
    let with = import_graph_v0(&bytes, &ImportOptions::new().with_extends(id.clone()))
        .expect("import")
        .sequence;
    assert_eq!(with.extends, Some(id));
    let wire = to_wire_string(&plain);
    assert!(!wire.contains("imported from"), "{wire}");
    let emitted =
        String::from_utf8(emit(&plain, &EmitOptions::new()).expect("emit")).expect("UTF-8");
    assert!(!emitted.contains("imported from") && !emitted.contains("S231:unit"));
}

/// A minimal v0 document with one input whose description is `description` (raw JSON text).
fn with_description(description: &str) -> String {
    format!(
        "{{\"name\": \"d\", \"inputs\": [{{\"name\": \"u\", \"type\": \"real\", \"unit\": \"degC\", \"description\": \"{description}\"}}], \"outputs\": [{{\"name\": \"y\", \"type\": \"real\", \"description\": \"\"}}], \"blocks\": [{{\"id\": \"g\", \"class\": \"Reals.MultiplyByParameter\", \"params\": [{{\"name\": \"k\", \"value\": 2}}]}}], \"connections\": [\"u -> g.u\", \"g.y -> y\"]}}"
    )
}

fn round_trips_through_the_text_form(model: &Sequence) {
    let text = print_text(model).expect("prints");
    assert!(
        text.lines()
            .all(|line| line.len() <= oce_author::MAX_LINE_BYTES),
        "a line over the bound"
    );
    let parsed = parse_text(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(&parsed, model);
    let once = fmt_text(&text).expect("fmt");
    assert_eq!(fmt_text(&once).expect("fmt"), once);
}

#[test]
fn a_description_with_controls_and_line_feeds_prints_parses_and_fmts() {
    // Line feeds, CR, tab, NUL, DEL, a C1 control and a bidirectional override, spelled as JSON
    // escapes, padded to the v0 string bound of 4,096 bytes (decoded).
    let head = "line one\\r\\twith\\u0000nul\\u007f\\u0085\\u202e\\nline two\\n\\nline four";
    let decoded = "line one\r\twith\0nul\u{7f}\u{85}\u{202e}\nline two\n\nline four";
    let padding = "x".repeat(4_096 - decoded.len());
    let imported = import(with_description(&format!("{head}{padding}")).as_bytes());
    let input = &imported.sequence.inputs[0];
    assert_eq!(
        input.annotations.comments,
        [
            " line one  with nul   ".trim_end().to_owned(),
            " line two".to_owned(),
            String::new(),
            format!(" line four{padding}"),
            " unit: degC".to_owned(),
        ]
    );
    assert!(imported.report.truncated.is_empty());
    round_trips_through_the_text_form(&imported.sequence);
    // An empty description writes no line, and the output's `description` is still reported.
    assert_eq!(
        imported.sequence.outputs[0].annotations.comments,
        Vec::<String>::new()
    );
    assert!(
        imported
            .report
            .dropped
            .iter()
            .any(|d| d.subject == "y" && d.property == DroppedProperty::Description)
    );
}

#[test]
fn a_line_longer_than_the_comment_bound_is_truncated_at_a_character_boundary_and_reported() {
    // One line of 4,096 bytes ending in a two-byte character: `# ` plus the line would be 4,098
    // bytes, so the line keeps 4,093 bytes (the `é` straddling the cut goes whole).
    let line = format!("{}\u{e9}", "a".repeat(4_094 - 1));
    assert_eq!(line.len(), 4_095);
    let line = format!("{line}b");
    assert_eq!(line.len(), 4_096);
    let imported = import(with_description(&line).as_bytes());
    let comments = &imported.sequence.inputs[0].annotations.comments;
    assert_eq!(comments[0], format!(" {}", "a".repeat(4_093)));
    assert!(2 + comments[0].len() - 1 <= MAX_V0_COMMENT_LINE_BYTES);
    let truncated: Vec<(&str, DroppedProperty, usize, usize, usize)> = imported
        .report
        .truncated
        .iter()
        .map(|t| (t.subject.as_str(), t.property, t.line, t.bytes, t.kept))
        .collect();
    assert_eq!(
        truncated,
        [("u", DroppedProperty::Description, 0, 4_096, 4_093)]
    );
    round_trips_through_the_text_form(&imported.sequence);
}
