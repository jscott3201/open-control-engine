//! `print_text` is total or refusing (issue #482 review, rulings A and B): a model the text form
//! cannot spell is refused with a typed `PrintError::Unspellable` naming the subject, never
//! printed as text that parses to a different model; and every model stage 1 accepts prints to
//! text within every line bound that parses back equal and is its own `fmt`.

mod support;

use oce_author::{
    Endpoint, EnumMember, Identifier, Literal, MAX_COMMENT_BYTES, MAX_HAYSTACK_TAGS,
    MAX_IDENTIFIER_BYTES, MAX_LINE_BYTES, MAX_SEMANTIC_FIELD_BYTES, MAX_STRING_BYTES, ParamValue,
    PrintError, ProvenanceId, Sequence, check_declared_name, fmt_text, parse_text, print_text,
    validate_author_side,
};

use support::text::{bundle_model, bundles};
use support::{econ_enable, every_element};

/// Assert `model` is refused at `subject` with a detail containing `needle`.
fn unspellable(model: &Sequence, subject: &str, needle: &str) {
    match print_text(model) {
        Err(PrintError::Unspellable {
            subject: got,
            detail,
        }) => {
            assert_eq!(got, subject, "{detail}");
            assert!(detail.contains(needle), "{detail}");
        }
        other => panic!("expected {subject} to be unspellable, got {other:?}"),
    }
}

#[test]
fn a_name_with_a_line_feed_cannot_inject_a_statement() {
    // The reviewer's case: a wire name that would print as a second statement.
    let mut s = econ_enable();
    s.inputs[1].name = Identifier::unchecked("TOutCut : Real\nin evil");
    unspellable(&s, "TOutCut : Real\nin evil", "is not a word");
}

#[test]
fn a_comment_with_a_line_feed_prints_as_comment_lines_and_injects_nothing() {
    let mut s = econ_enable();
    s.params[0].annotations.comments = vec![" one\nparam evil : Real = 1\n# two  ".to_owned()];
    let text = print_text(&s).expect("comments always print");
    assert!(
        text.contains("# one\n#param evil : Real = 1\n## two\nparam dTHys"),
        "{text}"
    );
    let back = parse_text(&text).expect("parses");
    assert_eq!(back.params.len(), 1, "no statement was injected");
    assert_eq!(
        back.params[0].annotations.comments,
        [" one", "param evil : Real = 1", "# two"]
    );
    // A comment piece the lexer would refuse is refused instead.
    s.params[0].annotations.comments = vec!["tab\there".to_owned()];
    unspellable(&s, "dTHys", "U+0009");
    s.params[0].annotations.comments = vec!["x".repeat(MAX_COMMENT_BYTES + 1)];
    unspellable(&s, "dTHys", "comment line");
}

#[test]
fn every_value_the_lexer_cannot_read_back_is_refused_by_subject() {
    type Edit = fn(&mut Sequence);
    let cases: [(Edit, &str, &str); 16] = [
        (
            |s| s.name = Identifier::unchecked("econ enable"),
            "sequence",
            "is not a word",
        ),
        (
            |s| s.extends = Some(ProvenanceId::unchecked("none")),
            "extends",
            "other than `none`",
        ),
        (
            |s| s.extends = Some(ProvenanceId::unchecked("cxf:a b")),
            "extends",
            "printable ASCII",
        ),
        (
            |s| s.inputs[0].semantic.unit = Some("deg C".to_owned()),
            "TOut",
            "deg C",
        ),
        (
            |s| s.inputs[0].quantity = Some("a#b".to_owned()),
            "TOut",
            "a#b",
        ),
        (
            |s| s.inputs[0].semantic.label = Some("rtl \u{202e}".to_owned()),
            "TOut",
            "U+202E",
        ),
        (
            |s| s.blocks[0].class_path = "urn:x#Y".to_owned(),
            "sub",
            "is not a word",
        ),
        (
            |s| s.params[0].suffix = Some("h".to_owned()),
            "dTHys",
            "vocabulary v1",
        ),
        (
            |s| s.params[0].value = Literal::Real(f64::NAN),
            "dTHys",
            "not finite",
        ),
        (
            |s| s.params[0].value = Literal::EnumMember(EnumMember::parse("PI")),
            "dTHys",
            "no `Type.` prefix",
        ),
        (
            |s| s.blocks[1].assignments[1].value = ParamValue::Ref(Identifier::unchecked("true")),
            "hys.uHigh",
            "reads as a literal",
        ),
        (
            |s| s.blocks[1].assignments[1].value = ParamValue::Ref(Identifier::unchecked("Inf")),
            "hys.uHigh",
            "reads as a literal",
        ),
        (
            |s| s.blocks[1].assignments[1].suffix = Some("K".to_owned()),
            "hys.uHigh",
            "not a number",
        ),
        (
            |s| {
                s.blocks[1].assignments[0].value =
                    ParamValue::Literal(Literal::Text("a\tb".to_owned()));
            },
            "hys.uLow",
            "U+0009",
        ),
        (
            |s| s.connections[0].source = Endpoint::parse("param"),
            "connection[0]",
            "reads as a statement",
        ),
        (
            |s| s.connections[1].target = Endpoint::Boundary(Identifier::unchecked("a.b")),
            "connection[1]",
            "is not a word",
        ),
    ];
    for (edit, subject, needle) in cases {
        let mut s = econ_enable();
        edit(&mut s);
        unspellable(&s, subject, needle);
    }
}

#[test]
fn non_finite_words_are_not_declared_names() {
    for name in ["nan", "NaN", "inf", "Inf", "infinity", "INFINITY"] {
        assert!(check_declared_name(name).is_err(), "{name}");
    }
    assert!(check_declared_name("info").is_ok());
}

/// Models stage 1 accepts, each pushing one printed shape to its bound.
fn clean_models() -> Vec<Sequence> {
    let mut all = vec![econ_enable(), every_element()];
    // A boundary with every field at its bound: the longest line a clean model prints.
    let mut wide = every_element();
    let atom = |c: char| c.to_string().repeat(MAX_SEMANTIC_FIELD_BYTES);
    let b = &mut wide.inputs[0];
    b.name = Identifier::unchecked(format!("T{}", "s".repeat(MAX_IDENTIFIER_BYTES - 1)));
    b.quantity = Some(atom('q'));
    b.display_unit = Some(atom('d'));
    b.semantic.unit = Some(atom('u'));
    b.semantic.brick = Some(atom('b'));
    b.semantic.s223 = Some(atom('s'));
    b.semantic.qudt = Some(atom('k'));
    b.semantic.haystack = (0..MAX_HAYSTACK_TAGS).map(|_| atom('h')).collect();
    b.semantic.name = Some("\"".repeat(MAX_SEMANTIC_FIELD_BYTES));
    b.semantic.label = Some("\\".repeat(MAX_SEMANTIC_FIELD_BYTES));
    let old = "TSup";
    let new = b.name.clone();
    for c in &mut wide.connections {
        if matches!(&c.source, Endpoint::Boundary(n) if n.as_str() == old) {
            c.source = Endpoint::Boundary(new.clone());
        }
    }
    // The longest string a clean assignment holds, and the longest comments.
    wide.blocks[5].assignments[0].value =
        ParamValue::Literal(Literal::Text("\"".repeat(MAX_STRING_BYTES / 2)));
    wide.blocks[5].assignments[0].annotations.comments = vec!["c".repeat(MAX_COMMENT_BYTES)];
    wide.blocks[5].annotations.comments = vec!["c".repeat(MAX_COMMENT_BYTES)];
    wide.params[0].annotations.blank_line_before = true;
    wide.trailing_comments = vec!["t".repeat(MAX_COMMENT_BYTES)];
    all.push(wide);
    // Arrays: a vector of 64 and a matrix whose rows exceed one line.
    let mut arrays = every_element();
    let sum = &mut arrays.blocks[1];
    sum.assignments = vec![oce_author::Assignment::new(
        Identifier::unchecked("nin"),
        ParamValue::Literal(Literal::Integer(64)),
    )];
    for i in 1..=64 {
        let mut a = oce_author::Assignment::new(
            Identifier::unchecked(format!("k_{i}")),
            ParamValue::Literal(Literal::Real(f64::from(i) * 0.123_456_789)),
        );
        a.suffix = (i % 2 == 0).then(|| "kg/m.s2".to_owned());
        sum.assignments.push(a);
    }
    let gain = &mut arrays.blocks[2];
    gain.assignments = ["nin", "nout"]
        .into_iter()
        .zip([40, 2])
        .map(|(name, n)| {
            oce_author::Assignment::new(
                Identifier::unchecked(name),
                ParamValue::Literal(Literal::Integer(n)),
            )
        })
        .collect();
    for r in 1..=2 {
        for c in 1..=40 {
            gain.assignments.push(oce_author::Assignment::new(
                Identifier::unchecked(format!("K_{r}_{c}")),
                ParamValue::Literal(Literal::Real(f64::from(r * c) / 7.0)),
            ));
        }
    }
    all.push(arrays);
    // Numeric edges a clean parameter takes.
    for (i, value) in [
        Literal::Integer(oce_author::MAX_EXACT_INTEGER),
        Literal::Real(-0.0),
        Literal::Real(5e-324),
        Literal::Real(f64::MAX),
        Literal::Real(1e16),
    ]
    .into_iter()
    .enumerate()
    {
        let mut s = econ_enable();
        s.params[0].value = value;
        s.params[0].suffix = ["K", "s", "m3/s", "1/s", "J.kg-1.K-1"]
            .get(i)
            .map(|x| (*x).to_owned());
        all.push(s);
    }
    for (_, wire) in bundles() {
        all.push(bundle_model(&wire));
    }
    all
}

#[test]
fn every_clean_model_prints_parseable_text_that_reads_back_equal() {
    let mut checked = 0;
    for model in clean_models() {
        if !validate_author_side(&model).is_empty() {
            // Only the abridged note example is not clean; every generated model must be.
            let findings = validate_author_side(&model);
            assert_eq!(model.name.as_str(), "econ_enable", "{findings:#?}");
            continue;
        }
        let text = print_text(&model).expect("a clean model is spellable");
        let longest = text.lines().map(str::len).max().unwrap_or(0);
        assert!(
            longest <= MAX_LINE_BYTES,
            "{}: a {longest}-byte line",
            model.name
        );
        let back = parse_text(&text).unwrap_or_else(|e| panic!("{}: {e}", model.name));
        assert_eq!(back, model, "{}", model.name);
        let once = fmt_text(&text).expect("fmt");
        assert_eq!(once, text, "{}: canonical", model.name);
        assert_eq!(fmt_text(&once).expect("fmt"), once);
        checked += 1;
    }
    assert!(checked >= 13, "{checked} clean models checked");
}

#[test]
fn the_widest_clean_boundary_and_matrix_rows_stay_within_the_line_bound() {
    let models = clean_models();
    let wide = print_text(&models[2]).expect("prints");
    let line = wide
        .lines()
        .find(|l| l.starts_with("in Ts"))
        .expect("the wide input");
    assert!(
        line.len() > 6_000 && line.len() <= MAX_LINE_BYTES,
        "{}",
        line.len()
    );
    let arrays = print_text(&models[3]).expect("prints");
    assert!(
        arrays.contains("  K = [\n    [\n      0.14285714285714285,\n"),
        "{arrays}"
    );
    assert!(
        arrays.contains("  k = [\n    0.123456789,\n    0.246913578 kg/m.s2,\n"),
        "{arrays}"
    );
}
