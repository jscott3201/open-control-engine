//! The typed builder: every refusal it can make without the catalog, and array element naming
//! through `oce_api::param_element_name` (decision 15).

mod support;

use oce_author::{
    Boundary, BuildError, Identifier, IdentifierError, Literal, ProvenanceIdError, SequenceBuilder,
    SignalType, validate, validate_author_side,
};

use support::{codes, econ_enable};

fn builder() -> SequenceBuilder {
    SequenceBuilder::new("s").expect("builder")
}

#[test]
fn the_builder_reproduces_the_worked_example() {
    let built = econ_enable();
    assert_eq!(built.blocks.len(), 2);
    assert_eq!(built.connections.len(), 4);
    assert!(validate(&built).is_empty());
}

#[test]
fn malformed_and_duplicate_names_are_refused() {
    assert_eq!(
        SequenceBuilder::new("in").err(),
        Some(BuildError::InvalidName {
            name: "in".to_owned(),
            error: IdentifierError::ReservedWord
        })
    );
    let b = builder().block("a", "CDL.Reals.Abs").expect("block");
    assert_eq!(
        b.clone()
            .parameter("a", SignalType::Real, Literal::Real(1.0))
            .err(),
        Some(BuildError::DuplicateName {
            name: "a".to_owned()
        })
    );
    assert_eq!(
        b.clone()
            .input(Boundary::new(Identifier::unchecked("a"), SignalType::Real))
            .err(),
        Some(BuildError::DuplicateName {
            name: "a".to_owned()
        })
    );
    assert_eq!(
        b.clone()
            .output(Boundary::new(
                Identifier::unchecked("x.y"),
                SignalType::Real
            ))
            .err(),
        Some(BuildError::InvalidName {
            name: "x.y".to_owned(),
            error: IdentifierError::BadCharacter(1)
        })
    );
    assert_eq!(
        b.block("2b", "CDL.Reals.Abs").err(),
        Some(BuildError::InvalidName {
            name: "2b".to_owned(),
            error: IdentifierError::BadStart
        })
    );
}

#[test]
fn a_malformed_provenance_id_is_refused() {
    assert_eq!(
        builder().extends("none").err(),
        Some(BuildError::InvalidProvenanceId {
            id: "none".to_owned(),
            error: ProvenanceIdError::BadPrefix
        })
    );
    assert!(builder().extends("cxf:abc").is_ok());
}

#[test]
fn a_malformed_semantic_field_is_refused() {
    let mut boundary = Boundary::new(Identifier::unchecked("u"), SignalType::Real);
    boundary.semantic.haystack = vec![String::new()];
    let error = builder().input(boundary).expect_err("refused");
    assert!(
        matches!(error, BuildError::InvalidSemanticPoint { ref name, ref problem } if name == "u" && problem.starts_with("`haystack[0]`")),
        "{error}"
    );
}

#[test]
fn assignments_need_a_declared_block_and_references_a_declared_parameter() {
    assert_eq!(
        builder().assign("nope", "k", Literal::Real(1.0)).err(),
        Some(BuildError::UnknownBlock {
            block: "nope".to_owned()
        })
    );
    let b = builder()
        .block("c", "CDL.Reals.Sources.Constant")
        .expect("block");
    assert_eq!(
        b.clone().assign_ref("c", "k", "p").err(),
        Some(BuildError::UnknownReference {
            name: "p".to_owned()
        })
    );
    assert_eq!(
        b.clone().assign("c", "k.x", Literal::Real(1.0)).err(),
        Some(BuildError::InvalidParameterName {
            name: "k.x".to_owned(),
            error: IdentifierError::BadCharacter(1)
        })
    );
    let b = b
        .parameter("p", SignalType::Real, Literal::Real(2.0))
        .expect("param");
    assert!(b.assign_ref("c", "k", "p").is_ok());
}

#[test]
fn array_elements_are_named_by_the_facade_template() {
    let s = builder()
        .block("sum", "CDL.Reals.MultiSum")
        .and_then(|b| b.assign("sum", "nin", Literal::Integer(2)))
        .and_then(|b| b.assign_element("sum", "k_<i>", &[1], Literal::Real(1.0)))
        .and_then(|b| b.assign_element("sum", "k_<i>", &[2], Literal::Real(-1.0)))
        .and_then(|b| b.block("gain", "CDL.Reals.MatrixGain"))
        .and_then(|b| b.assign_element("gain", "K_<row>_<col>", &[2, 1], Literal::Real(3.0)))
        .expect("builds")
        .build();
    let names: Vec<&str> = s
        .blocks
        .iter()
        .flat_map(|b| b.assignments.iter().map(|a| a.name.as_str()))
        .collect();
    assert_eq!(names, vec!["nin", "k_1", "k_2", "K_2_1"]);
    for (indices, name) in [(&[1usize][..], "k_1"), (&[2][..], "k_2")] {
        assert_eq!(
            oce_api::param_element_name("k_<i>", indices).as_deref(),
            Some(name)
        );
    }
    assert!(
        validate(&s).iter().all(|f| f.code != "unknown-parameter"),
        "{:#?}",
        validate(&s)
    );
    for (template, indices) in [
        ("k_<i>", &[0usize][..]),
        ("k_<i>", &[1, 1][..]),
        ("K_<row>_<col>", &[1][..]),
    ] {
        assert_eq!(
            builder()
                .block("b", "CDL.Reals.MultiSum")
                .and_then(|b| b.assign_element("b", template, indices, Literal::Real(1.0)))
                .err(),
            Some(BuildError::InvalidElementIndex {
                template: template.to_owned(),
                indices: indices.to_vec()
            })
        );
    }
}

#[test]
fn array_element_names_are_checked_against_the_class_templates() {
    let with = |name: &str| {
        let mut s = econ_enable();
        let mut sum = oce_author::Block::new(support::id("sum"), "CDL.Reals.MultiSum");
        sum.assignments.push(oce_author::Assignment::new(
            support::id("nin"),
            Literal::Integer(2),
        ));
        sum.assignments.push(oce_author::Assignment::new(
            Identifier::unchecked(name),
            Literal::Real(1.0),
        ));
        s.blocks.push(sum);
        codes(&validate_author_side(&s))
    };
    assert_eq!(with("k_2"), vec![]);
    for unknown in ["k_0", "k_02", "k", "k_", "k_1_1", "K_1"] {
        assert_eq!(
            with(unknown),
            vec![("unknown-parameter", format!("sum.{unknown}"))],
            "{unknown}"
        );
    }
    // An element past the configured width is not refused: `class_ports` ignores any name the
    // class does not read (GitHub #467, a facade issue), and stage 1 does not guess a width from
    // the resolved port count. Stage 2 judges the emitted array until #467 lands.
    assert_eq!(with("k_3"), vec![]);
}

#[test]
fn connection_endpoints_are_checked() {
    for (source, target) in [("a.b.c", "y"), ("", "y"), ("u", "1b.u"), ("u", "b.")] {
        assert!(
            matches!(
                builder().connect(source, target),
                Err(BuildError::MalformedEndpoint { .. })
            ),
            "{source} -> {target}"
        );
    }
    assert!(builder().connect("u", "b.u").is_ok());
}

#[test]
fn errors_display_what_is_wrong() {
    let error = SequenceBuilder::new("1x").expect_err("refused");
    assert_eq!(
        error.to_string(),
        "name `1x`: a name starts with an ASCII letter or `_`"
    );
}
