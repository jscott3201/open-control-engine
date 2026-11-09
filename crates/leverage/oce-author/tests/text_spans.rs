//! Finding spans from text (issue #482; the `Finding::span` contract of GitHub #466): every
//! finding of a text document, stage 1 and stage 2, is placed on the line and column of its
//! subject, and a wire document's findings keep `None`.

mod support;

use oce_author::{
    DUPLICATE_NAME, DUPLICATE_PARAMETER, Finding, Location, UNKNOWN_PARAMETER_REFERENCE,
    from_wire_str, parse_text_with_spans, print_text, validate, validate_text,
    validate_text_author_side,
};

use support::fixture;
use support::text::note_text_example;

fn at(line: u32, column: u32) -> Option<Location> {
    Some(Location { line, column })
}

fn find<'a>(findings: &'a [Finding], code: &str, subject: &str) -> &'a Finding {
    findings
        .iter()
        .find(|f| f.code == code && f.subject == subject)
        .unwrap_or_else(|| panic!("no {code} at {subject}: {findings:#?}"))
}

#[test]
fn a_finding_on_an_assignment_points_at_the_assignment() {
    // The worked example with `uHigh` referencing a parameter that does not exist.
    let text = note_text_example().replace("uHigh = dTHys", "uHigh = dTHysX");
    let findings = validate_text(&text).expect("parses");
    let finding = find(&findings, UNKNOWN_PARAMETER_REFERENCE, "hys.uHigh");
    // `block hys = CDL.Reals.Hysteresis(uLow = 0.0 K, uHigh = dTHysX)` is line 12; `uHigh` is
    // at column 48.
    assert_eq!(finding.span, at(12, 48));
    let line = text.lines().nth(11).expect("line 12");
    assert_eq!(&line[47..52], "uHigh");
}

#[test]
fn stage_two_findings_are_placed_too() {
    // Drop the connection that uses `TOut`: the engine reports the unused input (decision 14).
    let text = note_text_example().replace("TOut -> sub.u2\n", "TOutCut -> sub.u2\n");
    let findings = validate_text(&text).expect("parses");
    let unused = find(&findings, "unused-boundary-input", "TOut");
    assert_eq!(unused.span, at(7, 4));
    assert!(findings.iter().all(|f| f.span.is_some()), "{findings:#?}");
}

#[test]
fn every_subject_kind_is_placed_on_its_statement() {
    let text = "\
sequence end
extends CXF:abc
param p : Integer = 2.5
in u : Real @unit deg
out y : Real
block b = CDL.Reals.Nope
block h = CDL.Reals.Hysteresis(
  uLow = 1.0,
  uHigh = q
)
block m = CDL.Reals.MultiSum(nin = 2, k = [1.0, true])
block s = CDL.Reals.Subtract
a.b.c -> y
u -> s.u9
";
    let findings = validate_text_author_side(text).expect("parses");
    for (code, subject, line, column) in [
        ("invalid-name", "sequence", 1, 10),
        ("invalid-provenance-id", "extends", 2, 9),
        ("parameter-kind-mismatch", "p", 3, 7),
        ("class-not-found", "b", 6, 7),
        ("unknown-parameter-reference", "h.uHigh", 9, 3),
        ("parameter-kind-mismatch", "m.k_2", 11, 49),
        ("malformed-connection", "connection[0]", 13, 1),
        ("unknown-port", "connection[1]", 14, 1),
    ] {
        assert_eq!(
            find(&findings, code, subject).span,
            at(line, column),
            "{code} {subject}"
        );
    }
    assert!(findings.iter().all(|f| f.span.is_some()), "{findings:#?}");
}

#[test]
fn a_duplicate_is_placed_on_the_repeat_not_the_first() {
    let text = "\
sequence s
extends none
block x = CDL.Reals.Hysteresis(uLow = 0.0, uHigh = 1.0, uLow = 0.5)
in x : Real
out x : Real
x.y -> x
";
    let findings = validate_text_author_side(text).expect("parses");
    let dups: Vec<Option<Location>> = findings
        .iter()
        .filter(|f| f.code == DUPLICATE_NAME)
        .map(|f| f.span)
        .collect();
    // In model order the input (line 4) comes first, then the output (line 5), then the block
    // (line 3); the repeats are the output and the block.
    assert_eq!(dups, [at(5, 5), at(3, 7)]);
    let param = find(&findings, DUPLICATE_PARAMETER, "x.uLow");
    assert_eq!(param.span, at(3, 57));
}

#[test]
fn the_source_map_lists_every_location_a_subject_can_mean() {
    let (model, spans) = parse_text_with_spans(&note_text_example()).expect("parses");
    assert_eq!(
        spans.locations(&model, "sequence"),
        [Location {
            line: 2,
            column: 10
        }]
    );
    assert_eq!(
        spans.locations(&model, "extends"),
        [Location { line: 3, column: 9 }]
    );
    assert_eq!(
        spans.locations(&model, "dTHys"),
        [Location { line: 5, column: 7 }]
    );
    assert_eq!(
        spans.locations(&model, "ecoEna"),
        [Location { line: 9, column: 5 }]
    );
    assert_eq!(
        spans.locations(&model, "connection[3]"),
        [Location {
            line: 17,
            column: 1
        }]
    );
    assert!(spans.locations(&model, "connection[4]").is_empty());
    assert!(spans.locations(&model, "nothing").is_empty());
}

#[test]
fn wire_findings_keep_no_span() {
    let wire = std::fs::read_to_string(fixture("wire/defects.json")).expect("defects");
    let model = from_wire_str(&wire).expect("reads");
    assert!(validate(&model).iter().all(|f| f.span.is_none()));
    // The same model through the text form (where it can be spelled) gains spans, nothing else.
    let text = print_text(&model)
        .replace("@unit deg C", "@unit degC")
        .replace("urn:oce:lowering#PassThrough.Real", "CDL.Reals.Abs");
    let spanned = validate_text_author_side(&text).expect("parses");
    let mut stripped = spanned.clone();
    for f in &mut stripped {
        f.span = None;
    }
    let mut expected =
        oce_author::validate_author_side(&oce_author::parse_text(&text).expect("parses"));
    for f in &mut expected {
        f.span = None;
    }
    assert_eq!(stripped, expected);
}
