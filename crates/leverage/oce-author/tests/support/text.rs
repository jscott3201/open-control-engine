//! Text-form test support: the bundles every round-trip test runs over, a deterministic
//! enumeration of models (no RNG, per TESTING.md "Conventions"), and the text fixtures.

use oce_author::{
    Annotations, Assignment, Block, Bound, Boundary, Connection, Endpoint, Identifier, Literal,
    MemberRef, ParamValue, ParameterType, ProvenanceId, Sequence, SequenceParameter, SignalType,
    from_wire_str, import_cxf,
};

use super::cxf::corpus_fixture;
use super::fixture;

/// The corpus fixtures the minimal importer reads (as in `library_round_trip.rs`).
pub const IMPORTED: &[&str] = &[
    "cooling_only_system_requests",
    "generic_time_suppression",
    "multizone_vav_relief_fan_group",
    "reheat_overrides",
];

/// The wire bundles under `tests/fixtures/wire/` the text form must carry. `defects.json` is
/// left out on purpose: it holds values no text spelling has (a `unit` atom with a space, a
/// non-path class `urn:oce:lowering#PassThrough.Real`), which stage 1 refuses; see
/// `text_round_trip.rs` for its own test.
pub const WIRE_BUNDLES: &[&str] = &["econ_enable", "every_element", "note_example"];

/// Every bundle: `(name, canonical wire bytes)`, the wire fixtures then the imported corpus
/// fixtures written in the wire form.
pub fn bundles() -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = WIRE_BUNDLES
        .iter()
        .map(|name| {
            let path = fixture(&format!("wire/{name}.json"));
            (
                (*name).to_owned(),
                std::fs::read_to_string(path).expect("bundle"),
            )
        })
        .collect();
    for name in IMPORTED {
        let bytes = std::fs::read(corpus_fixture(name)).expect("corpus fixture");
        let model = import_cxf(&bytes).expect("imports");
        all.push(((*name).to_owned(), oce_author::to_wire_string(&model)));
    }
    all
}

/// The model of a bundle.
pub fn bundle_model(wire: &str) -> Sequence {
    from_wire_str(wire).expect("bundle reads")
}

/// The note's worked text example, copied verbatim into `tests/fixtures/note/text_example.oce`.
pub fn note_text_example() -> String {
    std::fs::read_to_string(fixture("note/text_example.oce")).expect("text example")
}

fn id(name: &str) -> Identifier {
    Identifier::unchecked(name)
}

fn annotations(variant: usize) -> Annotations {
    let mut a = Annotations::default();
    match variant % 5 {
        1 => a.comments = vec![" one comment".to_owned()],
        2 => a.blank_line_before = true,
        3 => {
            a.blank_line_before = true;
            a.comments = vec![String::new(), "  indented # with a hash".to_owned()];
        }
        4 => a.comments = vec!["\u{e9}t\u{e9} \u{1f321}".to_owned()],
        _ => {}
    }
    a
}

/// Literal values covering every kind and the spelling edges of each.
pub fn literals() -> Vec<Literal> {
    vec![
        Literal::Integer(0),
        Literal::Integer(-1),
        Literal::Integer(i64::MAX),
        Literal::Integer(i64::MIN),
        Literal::Integer(9_007_199_254_740_993),
        Literal::Real(0.0),
        Literal::Real(-0.0),
        Literal::Real(1.5),
        Literal::Real(1e-7),
        Literal::Real(1e16),
        Literal::Real(0.1 + 0.2),
        Literal::Real(f64::MAX),
        Literal::Real(f64::MIN_POSITIVE),
        Literal::Real(5e-324),
        Literal::Real(-2.5e300),
        Literal::Boolean(true),
        Literal::Boolean(false),
        Literal::Text(String::new()),
        Literal::Text("a \"quoted\" back\\slash\nnew line".to_owned()),
        Literal::Text("# not a comment, ) ] -> @tag".to_owned()),
        Literal::Text("\u{e9}\u{b0}\u{1f321}".to_owned()),
        Literal::Member(MemberRef::parse("SimpleController.PI")),
        Literal::Member(MemberRef::parse("A.B.C")),
        Literal::Member(MemberRef::parse("OperationModes.occupied")),
    ]
}

/// Bound values covering both kinds and the spelling edges of each (issue #492).
pub fn bounds() -> Vec<Bound> {
    vec![
        Bound::Integer(0),
        Bound::Integer(-1),
        Bound::Integer(i64::MAX),
        Bound::Integer(i64::MIN),
        Bound::Real(0.0),
        Bound::Real(-0.0),
        Bound::Real(1.5),
        Bound::Real(-1e-7),
        Bound::Real(0.1 + 0.2),
        Bound::Real(f64::MAX),
        Bound::Real(5e-324),
    ]
}

/// Sequence parameter types: the three scalars, an enumeration and a word the validator refuses.
pub fn parameter_types() -> Vec<ParameterType> {
    vec![
        ParameterType::Real,
        ParameterType::Integer,
        ParameterType::Boolean,
        ParameterType::parse("SimpleController"),
        ParameterType::parse("NoSuchType"),
    ]
}

/// Unit suffixes: none, and base-unit expressions of each shape.
pub const SUFFIXES: &[Option<&str>] = &[
    None,
    Some("K"),
    Some("m3/s"),
    Some("J.kg-1.K-1"),
    Some("1/s"),
    Some("kg/m3"),
    Some("s"),
];

fn numeric(literal: &Literal) -> bool {
    matches!(literal, Literal::Integer(_) | Literal::Real(_))
}

fn assignment(name: &str, value: impl Into<ParamValue>, ann: usize, suffix: usize) -> Assignment {
    let value = value.into();
    let mut a = Assignment::new(id(name), value.clone());
    if matches!(&value, ParamValue::Literal(l) if numeric(l)) {
        a.suffix = SUFFIXES[suffix % SUFFIXES.len()].map(str::to_owned);
    }
    a.annotations = annotations(ann);
    a
}

fn real(v: f64) -> ParamValue {
    ParamValue::Literal(Literal::Real(v))
}

/// The array-element shapes the printer must regroup or keep flat.
fn array_block(variant: usize) -> Block {
    let mut b = Block::new(id("arr"), "CDL.Reals.MultiSum");
    let plain = |name: &str, v: f64| assignment(name, real(v), 0, 0);
    b.assignments = match variant % 7 {
        // A complete vector, annotated on its first element, with suffixes.
        0 => vec![
            assignment("k_1", real(1.0), 1, 1),
            assignment("k_2", ParamValue::Literal(Literal::Integer(-2)), 0, 1),
            assignment("k_3", real(1e-7), 0, 0),
        ],
        // A gap: flat.
        1 => vec![plain("k_1", 1.0), plain("k_3", 3.0)],
        // Out of order: flat.
        2 => vec![plain("k_2", 2.0), plain("k_1", 1.0)],
        // An annotated second element: flat.
        3 => vec![plain("k_1", 1.0), assignment("k_2", real(2.0), 2, 0)],
        // A run, a scalar, then a repeat of index 1 (a second run).
        4 => vec![
            plain("k_1", 1.0),
            plain("k_2", 2.0),
            assignment("nin", ParamValue::Ref(id("n")), 0, 0),
            plain("k_1", 5.0),
        ],
        // References and an enum member as elements.
        5 => vec![
            assignment("k_1", ParamValue::Ref(id("p0")), 0, 0),
            assignment("k_2", ParamValue::Ref(id("p1")), 0, 0),
        ],
        _ => {
            let mut m = Block::new(id("arr"), "CDL.Reals.MatrixGain");
            m.assignments = vec![
                plain("K_1_1", 0.5),
                plain("K_1_2", 0.5),
                plain("K_1_3", 0.25),
                plain("K_2_1", 1.5),
                plain("K_2_2", -1.5),
                plain("K_2_3", 0.0),
                // An incomplete third row: the two full rows group, the rest stays flat.
                plain("K_3_1", 9.0),
            ];
            return m;
        }
    };
    b
}

/// A deterministic enumeration of models that every round-trip property runs over: every literal
/// kind and spelling edge, every parameter type shape, every bound kind and spelling edge on both
/// sides of a boundary, every suffix shape, every annotation shape, every boundary tag, array
/// runs that regroup and that stay flat, unknown classes, one-line and multi-line blocks, header
/// and trailing comments, and out-of-section authored orders.
pub fn enumerated_models() -> Vec<Sequence> {
    let lits = literals();
    let types = [SignalType::Real, SignalType::Integer, SignalType::Boolean];
    let param_types = parameter_types();
    let bounds = bounds();
    let mut out = Vec::new();
    for i in 0..48usize {
        let mut s = Sequence::new(id(if i % 2 == 0 { "enumerated" } else { "none_" }));
        if i % 3 == 1 {
            s.extends = Some(ProvenanceId::unchecked("cxf:fnv1a128:0123#abc"));
        }
        if i % 3 == 0 {
            s.annotations.comments = vec![" header".to_owned(), String::new()];
        }
        if i % 4 == 1 {
            s.trailing_comments = vec![" the end".to_owned(), "#".to_owned()];
        }
        for k in 0..(i % 4) {
            let literal = lits[(i * 3 + k) % lits.len()].clone();
            let mut p = SequenceParameter::new(
                id(&format!("p{k}")),
                param_types[(i + k) % param_types.len()].clone(),
                literal.clone(),
            );
            if numeric(&literal) {
                p.suffix = SUFFIXES[(i + k) % SUFFIXES.len()].map(str::to_owned);
            }
            p.annotations = annotations(i + k);
            s.params.push(p);
        }
        let mut b = Boundary::new(id("uIn"), types[i % 3]);
        b.annotations = annotations(i + 1);
        let bit = |n: usize| (i >> n) & 1 == 1;
        let atom = |a: &str| Some(a.to_owned());
        if bit(0) {
            b.semantic.unit = atom("K");
            b.quantity = atom("ThermodynamicTemperature");
        }
        if bit(1) {
            b.display_unit = atom("degC");
            b.semantic.brick = atom("brick:Temperature_Sensor");
            b.semantic.haystack = vec!["air".to_owned(), "temp".to_owned()];
        }
        if bit(2) {
            b.semantic.s223 = atom("s223:Sensor");
            b.semantic.qudt = atom("quantitykind:Temperature");
            b.semantic.name = Some("Name with \"quotes\" \u{b0}".to_owned());
        }
        if bit(3) {
            b.semantic.label = Some("label # not a comment".to_owned());
            b.semantic.provisional = true;
        }
        if i % 3 != 2 {
            b.min = Some(bounds[i % bounds.len()]);
        }
        if i % 4 != 3 {
            b.max = Some(bounds[(i * 5 + 1) % bounds.len()]);
        }
        s.inputs.push(b);
        let mut o = Boundary::new(id("yOut"), types[(i + 1) % 3]);
        o.semantic.provisional = bit(4);
        if bit(5) {
            o.max = Some(bounds[(i + 2) % bounds.len()]);
        }
        s.outputs.push(o);

        let mut hys = Block::new(id("hys"), "CDL.Reals.Hysteresis");
        hys.annotations = annotations(i + 2);
        hys.assignments = vec![
            assignment(
                "uLow",
                ParamValue::Literal(lits[i % lits.len()].clone()),
                i,
                i,
            ),
            assignment("uHigh", ParamValue::Ref(id("p0")), i + 3, 0),
        ];
        let empty = Block::new(id("sub"), "CDL.Reals.Subtract");
        let mut unknown = Block::new(id("odd"), "CDL.Reals.Nope");
        unknown.assignments = vec![
            assignment("x_1", real(1.0), 0, 0),
            assignment("x_2", real(2.0), 0, 0),
            assignment(
                "msg",
                ParamValue::Literal(Literal::Text("x".repeat(40 + i * 3))),
                0,
                0,
            ),
        ];
        let arrays = array_block(i);
        // Authored order inside the block section varies; nothing sorts it.
        s.blocks = match i % 3 {
            0 => vec![hys, empty, unknown, arrays],
            1 => vec![arrays, unknown, hys, empty],
            _ => vec![empty, arrays, hys],
        };
        for (j, (from, to)) in [("uIn", "hys.u"), ("hys.y", "yOut"), ("a.b.c", "none")]
            .iter()
            .enumerate()
        {
            let mut c = Connection::new(Endpoint::parse(from), Endpoint::parse(to));
            c.annotations = annotations(i + j);
            s.connections.push(c);
        }
        out.push(s);
    }
    out
}

/// Print a model the text form can spell.
pub fn print(sequence: &Sequence) -> String {
    oce_author::print_text(sequence).unwrap_or_else(|e| panic!("{e}"))
}
