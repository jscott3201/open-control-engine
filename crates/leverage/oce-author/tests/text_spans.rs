//! Finding spans from text (issue #482; the `Finding::span` contract of GitHub #466): every
//! finding of a text document, stage 1 and stage 2, is placed on the line and column of its
//! subject, and a wire document's findings keep `None`.

use crate::support;

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

/// Every finding as `(code, subject, line, column)`, in the list's order.
fn placed(findings: &[Finding]) -> Vec<(&str, &str, u32, u32)> {
    findings
        .iter()
        .map(|f| {
            let at = f
                .span
                .unwrap_or_else(|| panic!("{} at {} is placed", f.code, f.subject));
            (f.code, f.subject.as_str(), at.line, at.column)
        })
        .collect()
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
derived_from CXF:abc
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
        ("invalid-provenance-id", "derived_from", 2, 14),
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
derived_from none
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
fn every_finding_about_a_repeat_is_placed_on_the_repeat() {
    // GitHub #585: only `duplicate-name` and `duplicate-parameter` went to the repeat; any other
    // finding about a repeated name or assignment, here the second block's reference to nothing
    // and the third block's unknown class, was placed on the first block (3:46 and 3:7).
    let text = "\
sequence s
derived_from none
block lim = CDL.Reals.Hysteresis(uLow = 0.0, uHigh = 1.0)
block lim = CDL.Reals.Hysteresis(uLow = 0.0, uHigh = q)
block lim = CDL.Reals.Nope
";
    let findings = validate_text_author_side(text).expect("parses");
    assert_eq!(
        placed(&findings),
        [
            ("duplicate-name", "lim", 4, 7),
            ("unknown-parameter-reference", "lim.uHigh", 4, 46),
            ("class-not-found", "lim", 5, 7),
            ("duplicate-name", "lim", 5, 7),
        ]
    );
}

#[test]
fn a_declaration_named_sequence_is_placed_on_its_declaration_not_on_the_header() {
    // GitHub #585: the header subject `sequence` was indexed before the declarations and the
    // first location taken, so every finding about a declaration named `sequence` landed on the
    // sequence name (1:10). One declaration of each kind under the header word, beside a header
    // finding of the same code and subject; each later one also repeats the name.
    let text = "\
sequence end
derived_from none
param sequence : Integer = 2.5
in sequence : Real @min 2 @max 1
out sequence : Real
block sequence = CDL.Reals.Nope
";
    let findings = validate_text_author_side(text).expect("parses");
    assert_eq!(
        placed(&findings),
        [
            ("invalid-name", "sequence", 1, 10),
            ("invalid-name", "sequence", 3, 7),
            ("parameter-kind-mismatch", "sequence", 3, 7),
            ("crossed-bounds", "sequence", 4, 4),
            ("duplicate-name", "sequence", 4, 4),
            ("invalid-name", "sequence", 4, 4),
            ("duplicate-name", "sequence", 5, 5),
            ("invalid-name", "sequence", 5, 5),
            ("class-not-found", "sequence", 6, 7),
            ("duplicate-name", "sequence", 6, 7),
            ("invalid-name", "sequence", 6, 7),
        ]
    );
    // The first is the header's own: the sequence name `end`, a reserved word.
    assert!(
        findings[0].detail.starts_with("sequence name `end`"),
        "{}",
        findings[0].detail
    );
    assert!(
        findings[1].detail.starts_with("parameter name `sequence`"),
        "{}",
        findings[1].detail
    );

    // A header finding no declaration can earn stays on the header beside one.
    let text = "sequence s\nderived_from none\nin sequence : Real\n";
    let findings = validate_text_author_side(text).expect("parses");
    assert_eq!(
        placed(&findings),
        [
            ("empty-sequence", "sequence", 1, 10),
            ("invalid-name", "sequence", 3, 4),
        ]
    );
}

#[test]
fn a_declaration_named_derived_from_is_placed_on_its_declaration_not_on_the_header() {
    // GitHub #585: `in derived_from : Real @min 2 @max 1` on line 4 had its `invalid-name` and
    // `crossed-bounds` placed on the `derived_from` operand (2:14). One declaration of each
    // kind under the header word, beside the header's own `invalid-provenance-id`.
    let text = "\
sequence s
derived_from CXF:abc
param derived_from : Integer = 2.5
in derived_from : Real @min 2 @max 1
out derived_from : Real
block derived_from = CDL.Reals.Nope
";
    let findings = validate_text_author_side(text).expect("parses");
    assert_eq!(
        placed(&findings),
        [
            ("invalid-provenance-id", "derived_from", 2, 14),
            ("invalid-name", "derived_from", 3, 7),
            ("parameter-kind-mismatch", "derived_from", 3, 7),
            ("crossed-bounds", "derived_from", 4, 4),
            ("duplicate-name", "derived_from", 4, 4),
            ("invalid-name", "derived_from", 4, 4),
            ("duplicate-name", "derived_from", 5, 5),
            ("invalid-name", "derived_from", 5, 5),
            ("class-not-found", "derived_from", 6, 7),
            ("duplicate-name", "derived_from", 6, 7),
            ("invalid-name", "derived_from", 6, 7),
        ]
    );

    // The map lists every element the word can mean, the header first, in model order.
    let (model, spans) = parse_text_with_spans(text).expect("parses");
    assert_eq!(
        spans.locations(&model, "derived_from"),
        [(2, 14), (3, 7), (4, 4), (5, 5), (6, 7)].map(|(line, column)| Location { line, column })
    );
}

#[test]
fn an_array_base_is_placed_on_the_arrays_first_authored_element() {
    // GitHub #617: the emitter writes an array's elements as one node named by the template's
    // base (`m.k`, `g.K`), and stage 2 anchors an engine finding about that node at the array's
    // first authored element, so the map holds the base there: a list's first value, or the
    // first element assignment as written, whatever its index.
    let text = "\
sequence s
derived_from none
block m = CDL.Reals.MultiSum(nin = 2, k = [1.5, 2.5])
block g = CDL.Reals.MatrixGain(K = [[1.0, 0.0], [0.0, 1.0]])
block h = CDL.Reals.MultiSum(
  nin = 3,
  k_2 = 2.0,
  k_1 = 1.0
)
block a = CDL.Reals.Abs
block n = CDL.Reals.Nope(k_1 = 1.0)
";
    let (model, spans) = parse_text_with_spans(text).expect("parses");
    let at = |line, column| vec![Location { line, column }];
    for (subject, expected) in [
        ("m.k", at(3, 44)),
        ("m.k_1", at(3, 44)),
        ("m.k_2", at(3, 49)),
        ("g.K", at(4, 38)),
        ("g.K_1_1", at(4, 38)),
        ("h.k", at(7, 3)),
        ("h.k_1", at(8, 3)),
        // A scalar parameter has no base, and a class the catalog does not register has no
        // template to read one from.
        ("h.nin", at(6, 3)),
        ("h.ni", Vec::new()),
        ("a.k", Vec::new()),
        ("n.k", Vec::new()),
        ("n.k_1", at(11, 26)),
    ] {
        assert_eq!(spans.locations(&model, subject), expected, "{subject}");
    }
}

#[test]
fn a_rewritten_subject_is_placed_only_where_it_names_one_statement() {
    // `apply` places a finding on the element its subject and its position both name. A caller
    // that rewrites the subject breaks the pair: the finding is then placed on the subject's
    // statement only when the subject names exactly one, and keeps `None` when it names several,
    // rather than taking the first of them (GitHub #585's misplacement; review of GitHub #616).
    let text = "\
sequence s
derived_from none
in sequence : Real
block b = CDL.Reals.Nope
";
    let (model, spans) = parse_text_with_spans(text).expect("parses");
    assert_eq!(
        spans.locations(&model, "sequence"),
        [at(1, 10), at(3, 4)].map(Option::unwrap)
    );
    let findings = validate_text_author_side(text).expect("parses");
    assert_eq!(
        placed(&findings),
        [
            ("invalid-name", "sequence", 3, 4),
            ("class-not-found", "b", 4, 7)
        ]
    );
    let place = |subject: &str| {
        let mut rewritten = findings[1].clone();
        rewritten.subject = subject.to_owned();
        spans.apply(&model, std::slice::from_mut(&mut rewritten));
        rewritten.span
    };
    // The subject it was reported with: the block.
    assert_eq!(place("b"), at(4, 7));
    // A subject that names one statement, not the block: that statement.
    assert_eq!(place("derived_from"), at(2, 14));
    // A subject that names two, neither of them the block: no span, not the header.
    assert_eq!(place("sequence"), None);
    // A subject that names nothing (there is no connection): no span.
    assert_eq!(place("connection[0]"), None);
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
        spans.locations(&model, "derived_from"),
        [Location {
            line: 3,
            column: 14
        }]
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
    let mut spellable = model.clone();
    spellable.inputs[0].semantic.unit = Some("degC".to_owned());
    for block in &mut spellable.blocks {
        if block.class_path.starts_with("urn:") {
            block.class_path = "CDL.Reals.Abs".to_owned();
        }
    }
    let text = print_text(&spellable).expect("spellable");
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

#[test]
fn a_subject_the_text_does_not_name_keeps_no_span() {
    // An engine subject no statement names (a `connector#N` the stage-2 mapping could not
    // place), and a path to nothing: `apply` leaves `None` rather than guessing.
    let (model, spans) = parse_text_with_spans(&note_text_example()).expect("parses");
    let text = note_text_example().replace("uHigh = dTHys", "uHigh = dTHysX");
    let mut findings = validate_text(&text).expect("parses");
    assert!(findings[0].span.is_some());
    for subject in ["connector#3", "nothing.here", "connection[99]", ""] {
        findings[0].subject = subject.to_owned();
        spans.apply(&model, &mut findings[..1]);
        assert_eq!(findings[0].span, None, "{subject}");
    }
}
