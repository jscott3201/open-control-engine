//! A §7.4.1 term (`S231:unit`, `S231:quantity`, `S231:displayUnit`) carrying a Unicode format
//! (`Cf`) or control (`Cc`) character is refused as `malformed-document`, subject the node, on
//! every role and in every term form (issue #629; owner ruling of 2026-11-12, the rule GitHub #641
//! applies to a typing token). The message names the first such code point and quotes the term as
//! an excerpt with each such character written `\u{…}`, never as it is.
//!
//! Before, no such term was blank to `str::trim`, so `"K\u{200B}"` loaded: the unit gate and the
//! host's point list read it as a unit that looks like `K` and equals no other.
//!
//! The blank rule comes first: a term of whitespace alone is blank even where that whitespace is
//! also `Cc` (a tab, a line feed, U+0085). Around or inside a visible term the same characters are
//! this refusal, since nothing trims them away. Every refusal is pinned as the complete diagnostic
//! list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf, is_format_or_control};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Map, Value as Json, json};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const TERMS: [&str; 3] = ["unit", "quantity", "displayUnit"];

/// The roles of [`base`]: a Real block port, a boundary input, a boundary output and a parameter.
const PORT: &str = "http://example.org#U.add.u1";
const BOUNDARY_INPUT: &str = "http://example.org#U.uIn";
const BOUNDARY_OUTPUT: &str = "http://example.org#U.yOut";
const PARAMETER: &str = "http://example.org#U.con.k";
const ROLES: [&str; 4] = [PORT, BOUNDARY_INPUT, BOUNDARY_OUTPUT, PARAMETER];
/// The connectors of [`base`] that permit no term at all.
const INTEGER_OUTPUT: &str = "http://example.org#U.icon.y";
const BOOLEAN_OUTPUT: &str = "http://example.org#U.bcon.y";
/// A Real input no node lists, so nothing reads it.
const ORPHAN: &str = "http://example.org#U.orphan";
const BLOCKS: usize = 4;

/// Each character the ruling names, with how a message names it: the format characters a
/// zero-width space, a byte-order mark, U+2060, U+180E, a soft hyphen, two bidirectional controls
/// and an astral tag; the control characters NUL, two more C0 controls (U+001F is no whitespace),
/// DEL and a C1 control.
const INVISIBLE: [(char, &str); 13] = [
    ('\u{200B}', "U+200B, a format character (Cf)"),
    ('\u{FEFF}', "U+FEFF, a format character (Cf)"),
    ('\u{2060}', "U+2060, a format character (Cf)"),
    ('\u{180E}', "U+180E, a format character (Cf)"),
    ('\u{AD}', "U+00AD, a format character (Cf)"),
    ('\u{202E}', "U+202E, a format character (Cf)"),
    ('\u{2066}', "U+2066, a format character (Cf)"),
    ('\u{E0001}', "U+E0001, a format character (Cf)"),
    ('\u{0}', "U+0000, a control character (Cc)"),
    ('\u{1}', "U+0001, a control character (Cc)"),
    ('\u{1F}', "U+001F, a control character (Cc)"),
    ('\u{7F}', "U+007F, a control character (Cc)"),
    ('\u{9F}', "U+009F, a control character (Cc)"),
];

/// The control characters that are also Unicode `White_Space`, which `str::trim` removes.
const WHITESPACE_CONTROLS: [(char, &str); 6] = [
    ('\t', "U+0009, a control character (Cc)"),
    ('\n', "U+000A, a control character (Cc)"),
    ('\u{B}', "U+000B, a control character (Cc)"),
    ('\u{C}', "U+000C, a control character (Cc)"),
    ('\r', "U+000D, a control character (Cc)"),
    ('\u{85}', "U+0085, a control character (Cc)"),
];

/// A term's three forms, each wrapping its lexical text.
#[derive(Clone, Copy, Debug)]
enum Form {
    Bare,
    Typed,
    Iri,
}

const FORMS: [Form; 3] = [Form::Bare, Form::Typed, Form::Iri];

impl Form {
    fn wrap(self, text: &str) -> Json {
        match self {
            Form::Bare => json!(text),
            Form::Typed => json!({ "@value": text, "@type": XSD_STRING }),
            Form::Iri => json!({ "@id": text }),
        }
    }

    /// What a refusal calls this form's text `quoted` carrying the character `named`.
    fn carrying(self, quoted: &str, named: &str) -> String {
        match self {
            Form::Bare => {
                format!("the string `{quoted}`, which carries {named} that no reader can see")
            }
            Form::Typed => format!(
                "a typed literal whose @value `{quoted}` carries {named} that no reader can see"
            ),
            Form::Iri => {
                format!("an IRI node whose @id `{quoted}` carries {named} that no reader can see")
            }
        }
    }

    /// What a refusal calls this form's text when it is blank.
    fn blank(self) -> &'static str {
        match self {
            Form::Bare => "an empty or whitespace-only string",
            Form::Typed => "a typed literal whose @value is empty or whitespace-only",
            Form::Iri => "an IRI node whose @id is empty or whitespace-only",
        }
    }
}

/// A root with a boundary input and output, a Real `Constant` (parameter `k`) driving an `Add`,
/// an Integer `Constant`, a Boolean `Constant`, and one orphan Real input, carrying no term.
fn base() -> Json {
    let real = |id: &str, kind: &str, to: Option<&str>| {
        let mut node = json!({
            "@id": format!("http://example.org#U.{id}"),
            "@type": format!("S231:Real{kind}"),
            "S231:isOfDataType": { "@id": "S231:Real" }
        });
        if let Some(to) = to {
            node["S231:isConnectedTo"] = json!({ "@id": format!("http://example.org#U.{to}") });
        }
        node
    };
    // A source leaf `id` of `class`, and its parameter `k` valued `k`.
    let leaf = |id: &str, class: &str, k: Json| {
        [
            json!({
                "@id": format!("http://example.org#U.{id}"),
                "@type": format!("http://example.org#Buildings.Controls.OBC.CDL.{class}"),
                "S231:hasParameter": { "@id": format!("http://example.org#U.{id}.k") },
                "S231:hasOutput": { "@id": format!("http://example.org#U.{id}.y") }
            }),
            json!({ "@id": format!("http://example.org#U.{id}.k"), "S231:value": k }),
        ]
    };
    let [con, con_k] = leaf("con", "Reals.Sources.Constant", json!(1.0));
    let [icon, icon_k] = leaf("icon", "Integers.Sources.Constant", json!(1));
    let [bcon, bcon_k] = leaf("bcon", "Logical.Sources.Constant", json!(true));
    let graph = vec![
        json!({
            "@id": "http://example.org#U",
            "@type": "S231:Block",
            "S231:containsBlock": [
                { "@id": "http://example.org#U.con" },
                { "@id": "http://example.org#U.add" },
                { "@id": "http://example.org#U.icon" },
                { "@id": "http://example.org#U.bcon" }
            ],
            "S231:hasInput": { "@id": BOUNDARY_INPUT },
            "S231:hasOutput": { "@id": BOUNDARY_OUTPUT }
        }),
        con,
        con_k,
        real("con.y", "Output", Some("add.u1")),
        json!({
            "@id": "http://example.org#U.add",
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
            "S231:hasInput": [{ "@id": PORT }, { "@id": "http://example.org#U.add.u2" }],
            "S231:hasOutput": { "@id": "http://example.org#U.add.y" }
        }),
        real("add.u1", "Input", None),
        real("add.u2", "Input", None),
        real("add.y", "Output", Some("yOut")),
        real("uIn", "Input", Some("add.u2")),
        real("yOut", "Output", None),
        icon,
        icon_k,
        json!({
            "@id": INTEGER_OUTPUT,
            "@type": "S231:IntegerOutput",
            "S231:isOfDataType": { "@id": "S231:Integer" }
        }),
        bcon,
        bcon_k,
        json!({
            "@id": BOOLEAN_OUTPUT,
            "@type": "S231:BooleanOutput",
            "S231:isOfDataType": { "@id": "S231:Boolean" }
        }),
        real("orphan", "Input", None),
    ];
    json!({ "@context": { "S231": "http://data.ashrae.org/S231#" }, "@graph": graph })
}

fn node_mut<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .and_then(Json::as_object_mut)
        .unwrap_or_else(|| panic!("no node `{id}`"))
}

/// `doc` with `S231:<key>` set to `value` on the node `id`.
fn with(mut doc: Json, id: &str, key: &str, value: Json) -> Json {
    node_mut(&mut doc, id).insert(format!("S231:{key}"), value);
    doc
}

fn import(doc: &Json) -> Result<usize, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, _)| graph.blocks.len())
}

/// Every diagnostic of a resolver refusal, each checked to carry no invisible character as it is.
fn refusal(doc: &Json) -> Vec<Diagnostic> {
    let diags = match import(doc) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    };
    for diag in &diags {
        let subject = diag.subject.as_deref().unwrap_or_default();
        for text in [diag.message.as_str(), subject] {
            assert!(
                !text.chars().any(|c| {
                    is_format_or_control(c)
                        || INVISIBLE
                            .iter()
                            .chain(&WHITESPACE_CONTROLS)
                            .any(|&(raw, _)| raw == c)
                }),
                "a message or subject carries an invisible character as it is: {text:?}"
            );
        }
    }
    diags
}

/// The refusal of `term` on `node` of [`base`] that is `found`.
fn refused(node: &str, term: &str, found: &str) -> Diagnostic {
    refused_on(node, node == PARAMETER, term, found)
}

/// The refusal of `term` on `node`, a parameter or not, that is `found`.
fn refused_on(node: &str, parameter: bool, term: &str, found: &str) -> Diagnostic {
    let on = if parameter { " on a parameter" } else { "" };
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!("S231:{term}{on} is {found} — malformed term"),
    )
    .with_subject(node)
}

/// `text` as a message quotes it: each of `invisible` written as `\u{…}`.
fn quoted(text: &str, invisible: &[char]) -> String {
    text.chars()
        .map(|c| {
            if invisible.contains(&c) {
                format!("\\u{{{:04X}}}", u32::from(c))
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[test]
fn the_issue_s_term_is_refused_with_its_message_spelled_out() {
    // Before: loaded, a unit that looks like `K` and equals no other.
    let doc = with(base(), BOUNDARY_INPUT, "unit", json!("K\u{200B}"));
    assert_eq!(
        refusal(&doc),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "S231:unit is the string `K\\u{200B}`, which carries U+200B, a format character \
                 (Cf) that no reader can see — malformed term",
            )
            .with_subject(BOUNDARY_INPUT)
        ]
    );
    let doc = with(
        base(),
        PARAMETER,
        "quantity",
        json!({ "@id": "\u{FEFF}quantitykind:Temperature" }),
    );
    assert_eq!(
        refusal(&doc),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "S231:quantity on a parameter is an IRI node whose @id \
                 `\\u{FEFF}quantitykind:Temperature` carries U+FEFF, a format character (Cf) that \
                 no reader can see — malformed term",
            )
            .with_subject(PARAMETER)
        ]
    );
}

#[test]
fn every_invisible_character_is_refused_at_every_position_on_every_role_term_and_form() {
    assert_eq!(import(&base()).expect("the base loads"), BLOCKS);
    for (c, named) in INVISIBLE {
        for text in [
            format!("{c}K"),
            format!("de{c}gC"),
            format!("K{c}"),
            c.to_string(),
        ] {
            let found = |form: Form| form.carrying(&quoted(&text, &[c]), named);
            for term in TERMS {
                for form in FORMS {
                    for node in ROLES {
                        assert_eq!(
                            refusal(&with(base(), node, term, form.wrap(&text))),
                            vec![refused(node, term, &found(form))],
                            "{node} {term} {form:?} {text:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn every_place_a_parameter_is_read_refuses_an_invisible_term() {
    // A root declaration, a leaf's `hasParameter`, the same node through `hasConstant`, a nested
    // composite's declaration and a classified `hasInstance` parameter member (GitHub #594's sites).
    const RELIEF: &str = include_str!("fixtures/g36/multizone_vav_relief_damper.jsonld");
    const NESTED: &str = include_str!("fixtures/nested_composite.jsonld");
    const MEMBERS: &str =
        include_str!("fixtures/composite_contract/accepted/mixed_member_interface.jsonld");
    let root = "http://example.org#g36.source.multizone_vav_relief_damper.dpBuiSet";
    let leaf = "http://example.org#g36.source.multizone_vav_relief_damper.zerDam";
    let leaf_k = "http://example.org#g36.source.multizone_vav_relief_damper.zerDam.k";
    let parse = |text: &str| -> Json { serde_json::from_str(text).expect("a fixture") };
    let mut constant = parse(RELIEF);
    let listed = node_mut(&mut constant, leaf);
    let reference = listed
        .remove("S231:hasParameter")
        .expect("the leaf lists k");
    listed.insert("S231:hasConstant".into(), reference);
    let sites = [
        (parse(RELIEF), root),
        (parse(RELIEF), leaf_k),
        (constant, leaf_k),
        (
            parse(NESTED),
            "http://example.org#g36.profile.nested_composite.sub.innerK",
        ),
        (
            parse(MEMBERS),
            "http://example.org#cc.mixed_member_interface.con.k",
        ),
    ];
    for (doc, parameter) in sites {
        assert!(import(&doc).is_ok(), "{parameter}: the fixture loads");
        for (c, named) in [INVISIBLE[0], INVISIBLE[8], WHITESPACE_CONTROLS[0]] {
            let text = format!("Pa{c}");
            for term in TERMS {
                for form in FORMS {
                    assert_eq!(
                        refusal(&with(doc.clone(), parameter, term, form.wrap(&text))),
                        vec![refused_on(
                            parameter,
                            true,
                            term,
                            &form.carrying(&quoted(&text, &[c]), named)
                        )],
                        "{parameter} {term} {form:?} {text:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn whitespace_that_is_also_a_control_is_blank_alone_and_refused_beside_a_visible_term() {
    for (c, named) in WHITESPACE_CONTROLS {
        for term in TERMS {
            for form in FORMS {
                for node in ROLES {
                    // Alone, or beside other whitespace only: blank, as before, and once.
                    for text in [c.to_string(), format!(" {c} "), format!("{c}{c}\u{A0}")] {
                        assert_eq!(
                            refusal(&with(base(), node, term, form.wrap(&text))),
                            vec![refused(node, term, form.blank())],
                            "{node} {term} {form:?} {text:?}"
                        );
                    }
                    // Around or inside a visible term: this refusal, since nothing trims it away.
                    // Before, `"K\t"` loaded as a unit that is not `K`.
                    for text in [format!("K{c}"), format!("{c}K"), format!("de{c}gC")] {
                        let found = form.carrying(&quoted(&text, &[c]), named);
                        assert_eq!(
                            refusal(&with(base(), node, term, form.wrap(&text))),
                            vec![refused(node, term, &found)],
                            "{node} {term} {form:?} {text:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_padded_invisible_term_names_its_first_invisible_character_once() {
    let cases = [
        // Padded with spaces: not blank, so the invisible character is the refusal.
        (
            " \u{200B} ",
            " \\u{200B} ",
            "U+200B, a format character (Cf)",
        ),
        (
            " K\u{FEFF} ",
            " K\\u{FEFF} ",
            "U+FEFF, a format character (Cf)",
        ),
        // A tab before a format character: the first of the two is named, both are escaped.
        (
            "\t\u{200B}",
            "\\u{0009}\\u{200B}",
            "U+0009, a control character (Cc)",
        ),
        (
            "K\u{2060}\u{0}",
            "K\\u{2060}\\u{0000}",
            "U+2060, a format character (Cf)",
        ),
    ];
    for (text, shown, named) in cases {
        for term in TERMS {
            for form in FORMS {
                for node in ROLES {
                    assert_eq!(
                        refusal(&with(base(), node, term, form.wrap(text))),
                        vec![refused(node, term, &form.carrying(shown, named))],
                        "{node} {term} {form:?} {text:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_long_term_is_quoted_as_an_excerpt_with_its_invisible_character_still_named() {
    let long = format!("K\u{200B}{}", "x".repeat(1 << 20));
    let past = format!("{}\u{200B}", "x".repeat(100));
    for (text, shown) in [
        (long, format!("K\\u{{200B}}{}…", "x".repeat(60))),
        // The character lies past the 64 bytes quoted; the message still names it.
        (past, format!("{}…", "x".repeat(64))),
    ] {
        let diags = refusal(&with(base(), PORT, "unit", json!(text)));
        let found = Form::Bare.carrying(&shown, "U+200B, a format character (Cf)");
        assert_eq!(diags, vec![refused(PORT, "unit", &found)]);
        assert!(diags[0].message.len() < 300, "{}", diags[0].message.len());
    }
}

#[test]
fn the_earlier_refusals_come_first_and_the_term_is_refused_once() {
    let invisible = "K\u{200B}";
    // A key beside the shape, or both `@id` and `@value`: that shape is the problem reported.
    for (value, found) in [
        (
            json!({ "@value": invisible, "@type": XSD_STRING, "@language": "en" }),
            "a typed literal carrying the key `@language` beside @value and @type",
        ),
        (
            json!({ "@id": invisible, "@value": invisible }),
            "an object carrying both @id and @value",
        ),
        (
            json!([invisible]),
            "not a string, typed literal, or IRI node",
        ),
    ] {
        assert_eq!(
            refusal(&with(base(), PORT, "unit", value)),
            vec![refused(PORT, "unit", found)]
        );
    }
    // An Integer or Boolean connector permits no term: that refusal alone, whatever the term.
    assert_eq!(
        refusal(&with(base(), INTEGER_OUTPUT, "unit", json!(invisible))),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "§7.4.1 term (unit/quantity/displayUnit) declared on an Integer connector, which \
                 permits only min and max",
            )
            .with_subject(INTEGER_OUTPUT)
        ]
    );
    assert_eq!(
        refusal(&with(base(), BOOLEAN_OUTPUT, "unit", json!(invisible))),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "§7.4.1 attribute (unit/quantity/displayUnit/min/max) declared on a Boolean \
                 connector, which permits none",
            )
            .with_subject(BOOLEAN_OUTPUT)
        ]
    );
    // A `@context` in a term object is a scoped context, refused by the expansion pass alone.
    let diags = refusal(&with(
        base(),
        PORT,
        "unit",
        json!({ "@id": invisible, "@context": {} }),
    ));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
}

#[test]
fn visible_terms_and_what_the_rule_does_not_name_still_load() {
    // Visible non-ASCII units, and characters outside Cf and Cc: a no-break space (Zs), a line
    // separator (Zl), a variation selector (Mn), a Hangul filler (Lo) and a private-use character
    // (Co). Outside the ruling, they load as before.
    for text in [
        "°C",
        "m³/h",
        "µS/cm",
        "Ω",
        "deg\u{A0}C",
        "K\u{2028}",
        "K\u{FE0F}",
        "K\u{3164}",
        "K\u{E000}",
        "K ",
    ] {
        for term in TERMS {
            for form in FORMS {
                for node in ROLES {
                    assert_eq!(
                        import(&with(base(), node, term, form.wrap(text))).ok(),
                        Some(BLOCKS),
                        "{node} {term} {form:?} {text:?}"
                    );
                }
            }
        }
    }
    // A typed literal's `@type` is the datatype, not the term, and nothing reads it.
    let datatype =
        json!({ "@value": "K", "@type": "http://www.w3.org/2001/XMLSchema#string\u{200B}" });
    assert_eq!(
        import(&with(base(), PORT, "unit", datatype)).ok(),
        Some(BLOCKS)
    );
    // A parameter's bound is not a term: a string bound carrying one is not this rule's.
    assert_eq!(
        import(&with(base(), PARAMETER, "min", json!("yMin\u{200B}"))).ok(),
        Some(BLOCKS)
    );
    // The check runs where the node is read, as every term check does: an orphan is not read.
    assert_eq!(
        import(&with(base(), ORPHAN, "unit", json!("K\u{200B}"))).ok(),
        Some(BLOCKS)
    );
}

#[test]
fn every_invisible_term_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = base();
    for (node, term, text) in [
        (PORT, "unit", "K\u{200B}"),
        (BOUNDARY_INPUT, "displayUnit", "\u{0}"),
        (BOUNDARY_OUTPUT, "quantity", "T\u{AD}"),
        (PARAMETER, "unit", "K\t"),
        (PORT, "quantity", "\t"),
    ] {
        doc = with(doc, node, term, json!(text));
    }
    let first = refusal(&doc);
    let carrying =
        |text: &str, c: char, named: &str| Form::Bare.carrying(&quoted(text, &[c]), named);
    let mut expected = vec![
        refused(
            PORT,
            "unit",
            &carrying("K\u{200B}", '\u{200B}', "U+200B, a format character (Cf)"),
        ),
        refused(PORT, "quantity", Form::Bare.blank()),
        refused(
            BOUNDARY_INPUT,
            "displayUnit",
            &carrying("\u{0}", '\u{0}', "U+0000, a control character (Cc)"),
        ),
        refused(
            BOUNDARY_OUTPUT,
            "quantity",
            &carrying("T\u{AD}", '\u{AD}', "U+00AD, a format character (Cf)"),
        ),
        refused(
            PARAMETER,
            "unit",
            &carrying("K\t", '\t', "U+0009, a control character (Cc)"),
        ),
    ];
    let by_subject = |a: &Diagnostic, b: &Diagnostic| {
        (a.subject.as_deref(), &a.message).cmp(&(b.subject.as_deref(), &b.message))
    };
    expected.sort_by(by_subject);
    let mut sorted = first.clone();
    sorted.sort_by(by_subject);
    assert_eq!(sorted, expected, "the same set");
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first, "the same order every time");
    }
}
