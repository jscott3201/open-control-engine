//! Identifier and provenance-id rules: syntax, the 64-byte limit at its boundary, reserved words
//! and text-form keywords, uniqueness across declaration kinds, and the decision-5 grammar.

mod support;

use oce_author::{
    Boundary, Identifier, IdentifierError, MAX_IDENTIFIER_BYTES, MAX_PROVENANCE_ID_BYTES,
    MODELICA_RESERVED_WORDS, ProvenanceId, ProvenanceIdError, Sequence, SignalType,
    TEXT_FORM_KEYWORDS, check_declared_name, check_identifier_syntax, validate,
};

use support::{codes, econ_enable, id};

#[test]
fn identifier_syntax() {
    for good in ["a", "_", "_a1", "TOut", "u_s", "y1_1", "A9"] {
        assert_eq!(check_declared_name(good), Ok(()), "{good}");
    }
    assert_eq!(check_identifier_syntax(""), Err(IdentifierError::Empty));
    assert_eq!(
        check_identifier_syntax("1a"),
        Err(IdentifierError::BadStart)
    );
    assert_eq!(check_identifier_syntax("9"), Err(IdentifierError::BadStart));
    assert_eq!(
        check_identifier_syntax("a.b"),
        Err(IdentifierError::BadCharacter(1))
    );
    assert_eq!(
        check_identifier_syntax("a-b"),
        Err(IdentifierError::BadCharacter(1))
    );
    assert_eq!(
        check_identifier_syntax("ab c"),
        Err(IdentifierError::BadCharacter(2))
    );
    assert_eq!(check_identifier_syntax("é"), Err(IdentifierError::BadStart));
    assert_eq!(
        check_identifier_syntax("aé"),
        Err(IdentifierError::BadCharacter(1))
    );
}

#[test]
fn identifier_length_is_bounded_at_sixty_four_bytes() {
    let at_limit = "a".repeat(MAX_IDENTIFIER_BYTES);
    assert_eq!(check_declared_name(&at_limit), Ok(()));
    let past = "a".repeat(MAX_IDENTIFIER_BYTES + 1);
    assert_eq!(
        check_declared_name(&past),
        Err(IdentifierError::TooLong(65))
    );
    let mut s = econ_enable();
    s.blocks[0].name = Identifier::unchecked(past.as_str());
    let findings = validate(&s);
    assert_eq!(findings[0].code, "invalid-name");
    assert_eq!(findings[0].subject, past);
}

#[test]
fn reserved_words_and_keywords_are_refused_as_declared_names() {
    for word in MODELICA_RESERVED_WORDS {
        assert_eq!(
            check_declared_name(word),
            Err(IdentifierError::ReservedWord),
            "{word}"
        );
        assert_eq!(
            check_identifier_syntax(word),
            Ok(()),
            "syntax alone allows {word}"
        );
    }
    for keyword in TEXT_FORM_KEYWORDS {
        assert!(check_declared_name(keyword).is_err(), "{keyword}");
    }
    for keyword in [
        "sequence", "param", "out", "none", "Real", "Integer", "Boolean",
    ] {
        assert_eq!(
            check_declared_name(keyword),
            Err(IdentifierError::Keyword),
            "{keyword}"
        );
    }
    // Case matters: `Block` and `real` are ordinary names.
    assert_eq!(check_declared_name("Block"), Ok(()));
    assert_eq!(check_declared_name("real"), Ok(()));
    assert_eq!(MODELICA_RESERVED_WORDS.len(), 59);
    assert!(Identifier::new("model").is_err());
    assert_eq!(
        Identifier::new("model_").map(|i| i.to_string()),
        Ok("model_".to_owned())
    );
}

#[test]
fn every_declaration_kind_reports_invalid_names() {
    let mut s = Sequence::new(Identifier::unchecked("sequence"));
    s.params.push(oce_author::SequenceParameter {
        name: Identifier::unchecked("end"),
        value_type: SignalType::Real,
        value: oce_author::Literal::Real(1.0),
    });
    s.inputs.push(Boundary::new(
        Identifier::unchecked("1in"),
        SignalType::Real,
    ));
    s.outputs.push(Boundary::new(
        Identifier::unchecked("y.z"),
        SignalType::Real,
    ));
    s.blocks.push(oce_author::Block::new(
        Identifier::unchecked("Real"),
        "CDL.Reals.Sources.Constant",
    ));
    s.blocks[0].assignments.push(oce_author::Assignment {
        name: id("k"),
        value: oce_author::Literal::Real(1.0).into(),
    });
    assert_eq!(
        codes(&validate(&s)),
        vec![
            ("invalid-name", "sequence".to_owned()),
            ("invalid-name", "end".to_owned()),
            ("invalid-name", "1in".to_owned()),
            ("invalid-name", "y.z".to_owned()),
            ("invalid-name", "Real".to_owned()),
        ]
    );
}

#[test]
fn names_are_unique_across_parameters_inputs_outputs_and_blocks() {
    // Every ordered pair of kinds: the later declaration is the duplicate.
    let kinds = ["param", "input", "output", "block"];
    for (i, first) in kinds.iter().enumerate() {
        for second in &kinds[i..] {
            let mut s = econ_enable();
            let mut declare = |kind: &str, name: &str| match kind {
                "param" => s.params.push(oce_author::SequenceParameter {
                    name: id(name),
                    value_type: SignalType::Real,
                    value: oce_author::Literal::Real(0.0),
                }),
                "input" => s.inputs.push(Boundary::new(id(name), SignalType::Real)),
                "output" => s.outputs.push(Boundary::new(id(name), SignalType::Real)),
                _ => {
                    let mut b = oce_author::Block::new(id(name), "CDL.Reals.Abs");
                    b.assignments.clear();
                    s.blocks.push(b);
                }
            };
            declare(first, "x");
            declare(second, "x");
            let duplicates: Vec<_> = validate(&s)
                .into_iter()
                .filter(|f| f.code == "duplicate-name")
                .collect();
            assert_eq!(
                duplicates.len(),
                1,
                "{first} then {second}: {duplicates:#?}"
            );
            assert_eq!(duplicates[0].subject, "x");
            assert!(
                duplicates[0].detail.starts_with(second),
                "{}",
                duplicates[0].detail
            );
        }
    }
    // The sequence's own name may repeat a member's: the root is not under itself.
    let mut s = econ_enable();
    s.name = id("hys");
    assert!(validate(&s).is_empty());
}

#[test]
fn provenance_id_grammar() {
    for good in ["cxf:fnv1a128:0123abcd", "a:b", "m3:x/y?z=1#frag", "lib2:~!"] {
        assert_eq!(
            ProvenanceId::new(good).map(|p| p.to_string()),
            Ok(good.to_owned()),
            "{good}"
        );
    }
    let cases = [
        ("", ProvenanceIdError::Empty),
        ("abc", ProvenanceIdError::BadPrefix),
        (":abc", ProvenanceIdError::BadPrefix),
        ("CXF:abc", ProvenanceIdError::BadPrefix),
        ("1cxf:abc", ProvenanceIdError::BadPrefix),
        ("c-x:abc", ProvenanceIdError::BadPrefix),
        ("cxf:", ProvenanceIdError::EmptyBody),
        ("cxf:a b", ProvenanceIdError::BadCharacter(5)),
        ("cxf:a\tb", ProvenanceIdError::BadCharacter(5)),
        ("cxf:é", ProvenanceIdError::BadCharacter(4)),
    ];
    for (id, expected) in cases {
        assert_eq!(ProvenanceId::new(id), Err(expected), "{id:?}");
    }
    let at_limit = format!("cxf:{}", "a".repeat(MAX_PROVENANCE_ID_BYTES - 4));
    assert!(ProvenanceId::new(&at_limit).is_ok());
    let past = format!("{at_limit}a");
    assert_eq!(
        ProvenanceId::new(&past),
        Err(ProvenanceIdError::TooLong(257))
    );
}
