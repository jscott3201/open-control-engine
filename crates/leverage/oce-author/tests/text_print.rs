//! `print_text` is total or refusing (issue #482 review, rulings A and B): a model the text form
//! cannot spell is refused with a typed `PrintError::Unspellable` naming the subject, never
//! printed as text that parses to a different model; and every model stage 1 accepts prints to
//! text within every line bound that parses back equal and is its own `fmt`.

mod support;

use oce_author::{
    Endpoint, Identifier, Literal, MAX_COMMENT_BYTES, MAX_HAYSTACK_TAGS, MAX_IDENTIFIER_BYTES,
    MAX_LINE_BYTES, MAX_SEMANTIC_FIELD_BYTES, MAX_STRING_BYTES, MemberRef, ParamValue, PrintError,
    ProvenanceId, Sequence, check_declared_name, fmt_text, parse_text, print_text,
    validate_author_side,
};

use support::fixture;
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
    let cases: [(Edit, &str, &str); 21] = [
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
            |s| s.params[0].value = Literal::Member(MemberRef::parse("PI")),
            "dTHys",
            "no `Type.` prefix",
        ),
        (
            |s| s.params[0].value_type = oce_author::ParameterType::Enum("Real".to_owned()),
            "dTHys",
            "reads as the scalar type",
        ),
        (
            |s| {
                s.params[0].value_type =
                    oce_author::ParameterType::Enum("Simple Controller".to_owned())
            },
            "dTHys",
            "is not a word",
        ),
        (
            |s| s.params[0].value_type = oce_author::ParameterType::Enum(String::new()),
            "dTHys",
            "is not a word",
        ),
        (
            |s| s.inputs[0].min = Some(oce_author::Bound::Real(f64::NAN)),
            "TOut",
            "not finite",
        ),
        (
            |s| s.outputs[0].max = Some(oce_author::Bound::Real(f64::NEG_INFINITY)),
            "ecoEna",
            "not finite",
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
    // econ_enable, every_element, the wide and array models, five numeric edges and the six clean
    // bundles (the abridged note example is not clean).
    assert_eq!(checked, 15, "clean models checked");
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

/// Whether `text` is within every parse bound a single statement can reach.
fn within_line_bound(text: &str) -> bool {
    text.lines().all(|l| l.len() <= MAX_LINE_BYTES)
}

#[test]
fn a_statement_too_long_for_one_line_is_refused_not_printed() {
    // The re-review's cases: each model loads (the first from the wire form) and each prints a
    // statement past the line bound, which `parse_text` would refuse.
    let mut wide = econ_enable();
    wide.inputs[0].semantic.haystack = (0..40).map(|_| "h".repeat(256)).collect();
    let wire = oce_author::to_wire_string(&wide);
    let loaded = oce_author::from_wire_str(&wire).expect("the wire form loads 40 tags");
    assert!(
        !validate_author_side(&loaded).is_empty(),
        "stage 1 refuses it"
    );
    unspellable(&loaded, "TOut", "printed statement exceeds 8192 bytes");

    // Past the 16-tag cap beside every other field at its bound: 17 tags still fit one line, so
    // the model prints (stage 1 refuses it); 30 do not, so it is refused.
    let mut full = clean_models().swap_remove(2);
    let tag = "h".repeat(MAX_SEMANTIC_FIELD_BYTES);
    full.inputs[0].semantic.haystack.push(tag.clone());
    let text = print_text(&full).expect("17 full-width tags fit one line");
    assert!(within_line_bound(&text));
    assert_eq!(parse_text(&text).expect("parses"), full);
    full.inputs[0].semantic.haystack = vec![tag; 30];
    let name = full.inputs[0].name.as_str().to_owned();
    unspellable(&full, &name, "printed statement exceeds");

    // A class path and a port of forty dotted 256-byte words: each word is a legal token, the
    // statement is not.
    let long = vec!["w".repeat(256); 40].join(".");
    let mut class = econ_enable();
    class.blocks[0].class_path = long.clone();
    unspellable(&class, "sub", "printed statement exceeds");
    let mut port = econ_enable();
    port.connections[0].target = Endpoint::parse(&format!("sub.{long}"));
    unspellable(&port, "connection[0]", "printed statement exceeds");
}

#[test]
fn a_document_past_the_byte_or_line_bound_is_refused_not_printed() {
    // A clean model whose comments alone pass 4 MiB.
    let mut big = econ_enable();
    big.trailing_comments = vec!["c".repeat(MAX_COMMENT_BYTES); 520];
    assert!(validate_author_side(&big).is_empty());
    unspellable(&big, "sequence", "4194304 bytes");
    // Past 100,000 lines: many comments, or one comment of many lines.
    let mut many = econ_enable();
    many.trailing_comments = vec!["c".to_owned(); 100_001];
    unspellable(&many, "sequence", "100000 lines");
    let mut one = econ_enable();
    one.trailing_comments = vec!["\n".repeat(100_001)];
    unspellable(&one, "sequence", "100000 lines");
    // At the bound it prints and reads back.
    let mut at = econ_enable();
    let room = 100_000 - print_text(&at).expect("prints").lines().count() - 1;
    at.trailing_comments = vec!["c".to_owned(); room];
    let text = print_text(&at).expect("at the line bound");
    assert_eq!(text.lines().count(), 100_000);
    assert_eq!(parse_text(&text).expect("parses"), at);
}

/// The model `parse_text` reads back from a printed model: comments split at line feeds and
/// trimmed of trailing spaces, the header blank line dropped.
fn normalized(model: &Sequence) -> Sequence {
    fn split(lines: &mut Vec<String>) {
        *lines = lines
            .iter()
            .flat_map(|l| l.split('\n'))
            .map(|l| l.trim_end_matches(' ').to_owned())
            .collect();
    }
    let mut m = model.clone();
    m.annotations.blank_line_before = false;
    split(&mut m.annotations.comments);
    split(&mut m.trailing_comments);
    for p in &mut m.params {
        split(&mut p.annotations.comments);
    }
    for b in m.inputs.iter_mut().chain(m.outputs.iter_mut()) {
        split(&mut b.annotations.comments);
    }
    for b in &mut m.blocks {
        split(&mut b.annotations.comments);
        for a in &mut b.assignments {
            split(&mut a.annotations.comments);
        }
    }
    for c in &mut m.connections {
        split(&mut c.annotations.comments);
    }
    m
}

type Slot = fn(&mut Sequence, &str);

/// Every string slot of the model, set on the worked example.
fn slots() -> Vec<(&'static str, Slot)> {
    fn id(v: &str) -> Identifier {
        Identifier::unchecked(v)
    }
    fn some(v: &str) -> Option<String> {
        Some(v.to_owned())
    }
    vec![
        ("sequence name", |s, v| s.name = id(v)),
        ("extends", |s, v| {
            s.extends = Some(ProvenanceId::unchecked(v))
        }),
        ("header comment", |s, v| {
            s.annotations.comments = vec![v.to_owned()]
        }),
        ("trailing comment", |s, v| {
            s.trailing_comments = vec![v.to_owned()]
        }),
        ("parameter name", |s, v| s.params[0].name = id(v)),
        ("parameter string", |s, v| {
            s.params[0].value = Literal::Text(v.to_owned())
        }),
        ("parameter member type", |s, v| {
            s.params[0].value = Literal::Member(MemberRef::parse(&format!("{v}.PI")));
        }),
        ("parameter member", |s, v| {
            s.params[0].value = Literal::Member(MemberRef::parse(&format!("T.{v}")));
        }),
        ("parameter suffix", |s, v| s.params[0].suffix = some(v)),
        ("parameter comment", |s, v| {
            s.params[0].annotations.comments = vec![v.to_owned()]
        }),
        ("input name", |s, v| s.inputs[0].name = id(v)),
        ("output name", |s, v| s.outputs[0].name = id(v)),
        ("quantity", |s, v| s.inputs[0].quantity = some(v)),
        ("display unit", |s, v| s.inputs[0].display_unit = some(v)),
        ("unit", |s, v| s.inputs[0].semantic.unit = some(v)),
        ("brick", |s, v| s.inputs[0].semantic.brick = some(v)),
        ("s223", |s, v| s.inputs[0].semantic.s223 = some(v)),
        ("qudt", |s, v| s.inputs[0].semantic.qudt = some(v)),
        ("haystack tag", |s, v| {
            s.inputs[0].semantic.haystack = vec![v.to_owned()]
        }),
        ("forty haystack tags", |s, v| {
            s.inputs[0].semantic.haystack = vec![v.to_owned(); 40]
        }),
        ("name", |s, v| s.inputs[0].semantic.name = some(v)),
        ("label", |s, v| s.inputs[0].semantic.label = some(v)),
        ("boundary comment", |s, v| {
            s.inputs[0].annotations.comments = vec![v.to_owned()]
        }),
        ("block name", |s, v| s.blocks[0].name = id(v)),
        ("class path", |s, v| s.blocks[0].class_path = v.to_owned()),
        ("block comment", |s, v| {
            s.blocks[0].annotations.comments = vec![v.to_owned()]
        }),
        ("assignment name", |s, v| {
            s.blocks[1].assignments[0].name = id(v)
        }),
        ("assignment string", |s, v| {
            s.blocks[1].assignments[0].value = ParamValue::Literal(Literal::Text(v.to_owned()));
        }),
        ("reference", |s, v| {
            s.blocks[1].assignments[1].value = ParamValue::Ref(id(v))
        }),
        ("assignment member type", |s, v| {
            s.blocks[1].assignments[0].value =
                ParamValue::Literal(Literal::Member(MemberRef::parse(&format!("{v}.PI"))));
        }),
        ("assignment member", |s, v| {
            s.blocks[1].assignments[0].value =
                ParamValue::Literal(Literal::Member(MemberRef::parse(&format!("T.{v}"))));
        }),
        ("assignment suffix", |s, v| {
            s.blocks[1].assignments[0].suffix = some(v)
        }),
        ("assignment comment", |s, v| {
            s.blocks[1].assignments[0].annotations.comments = vec![v.to_owned()];
        }),
        ("source boundary", |s, v| {
            s.connections[0].source = Endpoint::Boundary(id(v))
        }),
        ("source block", |s, v| {
            s.connections[2].source = Endpoint::parse(&format!("{v}.y"))
        }),
        ("source port", |s, v| {
            s.connections[2].source = Endpoint::parse(&format!("sub.{v}"))
        }),
        ("target boundary", |s, v| {
            s.connections[3].target = Endpoint::Boundary(id(v))
        }),
        ("target port", |s, v| {
            s.connections[0].target = Endpoint::parse(&format!("sub.{v}"))
        }),
        ("connection comment", |s, v| {
            s.connections[0].annotations.comments = vec![v.to_owned()]
        }),
    ]
}

#[test]
fn every_string_slot_with_hostile_text_is_refused_or_reads_back_equal() {
    // The re-review's probe: every slot × a hostile set. `print_text` either refuses, or writes
    // text within the line bound that parses back to the same model (up to the comment
    // normalizations). Never text that parses to something else, never text the parser refuses.
    let hostile: Vec<String> = [
        "->", "#", "\"", "\\", "\n", "\t", "\u{202e}", "param", "in", "block", "true", "nan",
        "Inf", "Type.a.b", "a b", "K", "s-1", "cxf:a#", "none", "", "ok_word", "x\n# y  ",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .chain([
        "x".repeat(256),
        "x".repeat(257),
        "x".repeat(9_000),
        "\u{e9}".repeat(2_049),
    ])
    .collect();
    let (mut refused, mut printed) = (0, 0);
    for (slot, set) in slots() {
        for value in &hostile {
            let mut model = econ_enable();
            set(&mut model, value);
            let label = format!("{slot} = {:?}", value.chars().take(20).collect::<String>());
            match print_text(&model) {
                Err(PrintError::Unspellable { .. }) => refused += 1,
                Err(other) => panic!("{label}: {other}"),
                Ok(text) => {
                    printed += 1;
                    assert!(within_line_bound(&text), "{label}: a line past the bound");
                    let back = parse_text(&text)
                        .unwrap_or_else(|e| panic!("{label}: printed text is refused: {e}"));
                    assert_eq!(back, normalized(&model), "{label}: read back differently");
                    assert_eq!(fmt_text(&text).as_deref(), Ok(text.as_str()), "{label}");
                }
            }
        }
    }
    assert_eq!(slots().len(), 39);
    assert!(
        refused > 300 && printed > 300,
        "{refused} refused, {printed} printed"
    );
}

#[test]
fn the_wire_fixtures_print_or_refuse_by_the_same_rule() {
    for name in ["econ_enable", "every_element", "note_example", "defects"] {
        let wire = std::fs::read_to_string(fixture(&format!("wire/{name}.json"))).expect("wire");
        let model = oce_author::from_wire_str(&wire).expect("reads");
        if let Ok(text) = print_text(&model) {
            assert!(within_line_bound(&text), "{name}");
            assert_eq!(parse_text(&text).expect(name), model, "{name}");
        }
    }
}
