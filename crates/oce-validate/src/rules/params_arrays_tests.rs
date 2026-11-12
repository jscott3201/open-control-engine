//! The four vector rules over synthetic parameter tables, through the load gate's own entry for
//! one block (`validate_block_parameters`), each refusal pinned as an exact diagnostic list:
//! every one of the eight vector templates past its resolved length (GitHub #626), the length
//! omitted, index 0, an in-length control, and the spellings a hostile document may use.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use crate::validate_block_parameters;

/// The block's diagnostics as `(code, message)`, the subject checked to be the block.
fn diagnostics(class: &str, params: &[(&str, Value)]) -> Vec<(String, String)> {
    let table = ParamTable {
        values: params
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    };
    let diags = match validate_block_parameters(class, &table) {
        Ok(warnings) => warnings,
        Err(error) => error.diagnostics,
    };
    diags
        .into_iter()
        .map(|d| {
            assert_eq!(d.subject.as_deref(), Some("block#0"), "{d:?}");
            (d.code.as_str().to_owned(), d.message)
        })
        .collect()
}

fn past(name: &str, class: &str, n: usize) -> (String, String) {
    (
        "parameter-out-of-range".to_owned(),
        format!("parameter `{name}` on block `{class}` is outside resolved array length {n}"),
    )
}

/// One vector template: its class, its element base, the parameters every case carries, the
/// length they resolve, and a value an element of the vector may hold.
struct Template {
    class: String,
    base: &'static str,
    fixed: Vec<(&'static str, Value)>,
    n: usize,
    element: Value,
}

fn templates() -> Vec<Template> {
    let mut all = vec![
        Template {
            class: "CDL.Reals.MultiSum".to_owned(),
            base: "k",
            fixed: vec![("nin", Value::Integer(2))],
            n: 2,
            element: Value::Real(3.0),
        },
        Template {
            class: "CDL.Integers.MultiSum".to_owned(),
            base: "k",
            fixed: vec![("nin", Value::Integer(2))],
            n: 2,
            element: Value::Integer(3),
        },
    ];
    for kind in ["Real", "Integer", "Boolean"] {
        all.push(Template {
            class: format!("CDL.Routing.{kind}ExtractSignal"),
            base: "extract",
            fixed: vec![("nin", Value::Integer(2)), ("nout", Value::Integer(1))],
            n: 1,
            element: Value::Integer(1),
        });
        // `nout = 2` keeps the mask's true count right with its two defaulted `true` members.
        all.push(Template {
            class: format!("CDL.Routing.{kind}VectorFilter"),
            base: "msk",
            fixed: vec![("nin", Value::Integer(2)), ("nout", Value::Integer(2))],
            n: 2,
            element: Value::Boolean(true),
        });
    }
    all
}

fn with(template: &Template, elements: &[(&str, Value)]) -> Vec<(String, String)> {
    let mut params = template.fixed.clone();
    params.extend(elements.iter().cloned());
    diagnostics(&template.class, &params)
}

#[test]
fn an_element_inside_the_resolved_length_is_accepted() {
    for template in &templates() {
        let name = format!("{}_{}", template.base, template.n);
        assert_eq!(
            with(template, &[(&name, template.element.clone())]),
            vec![],
            "{}",
            template.class
        );
    }
}

#[test]
fn an_element_one_past_the_resolved_length_is_refused() {
    for template in &templates() {
        let last = format!("{}_{}", template.base, template.n);
        let next = format!("{}_{}", template.base, template.n + 1);
        assert_eq!(
            with(
                template,
                &[
                    (&last, template.element.clone()),
                    (&next, template.element.clone())
                ]
            ),
            vec![past(&next, &template.class, template.n)],
            "{}",
            template.class
        );
    }
}

#[test]
fn a_lone_element_far_past_the_resolved_length_is_refused() {
    for template in &templates() {
        let far = format!("{}_9", template.base);
        assert_eq!(
            with(template, &[(&far, template.element.clone())]),
            vec![past(&far, &template.class, template.n)],
            "{}",
            template.class
        );
    }
}

#[test]
fn index_zero_is_outside_every_length() {
    for template in &templates() {
        let zero = format!("{}_0", template.base);
        assert_eq!(
            with(template, &[(&zero, template.element.clone())]),
            vec![past(&zero, &template.class, template.n)],
            "{}",
            template.class
        );
    }
}

#[test]
fn an_element_past_its_length_is_refused_whatever_its_value() {
    // The kind check is for elements inside the length; outside it, the index is the refusal.
    for template in &templates() {
        let next = format!("{}_{}", template.base, template.n + 1);
        assert_eq!(
            with(template, &[(&next, Value::String(Arc::from("x")))]),
            vec![past(&next, &template.class, template.n)],
            "{}",
            template.class
        );
    }
}

#[test]
fn an_omitted_length_resolves_to_the_class_default() {
    // `MultiSum.nin` defaults to 0, so every gain is past it; `ExtractSignal.nout` to 1.
    for (class, element) in [
        ("CDL.Reals.MultiSum", Value::Real(1.0)),
        ("CDL.Integers.MultiSum", Value::Integer(1)),
    ] {
        assert_eq!(
            diagnostics(class, &[("k_1", element.clone()), ("k_2", element)]),
            vec![past("k_1", class, 0), past("k_2", class, 0)],
            "{class}"
        );
        assert_eq!(diagnostics(class, &[]), vec![], "{class}: nothing to judge");
    }
    for kind in ["Real", "Integer", "Boolean"] {
        let class = format!("CDL.Routing.{kind}ExtractSignal");
        assert_eq!(
            diagnostics(
                &class,
                &[
                    ("nin", Value::Integer(2)),
                    ("extract_1", Value::Integer(2)),
                    ("extract_2", Value::Integer(1)),
                ]
            ),
            vec![past("extract_2", &class, 1)],
            "{class}"
        );
    }
}

#[test]
fn a_required_length_left_out_is_its_own_refusal_and_no_element_is_judged() {
    // `VectorFilter.nin` has no default: the `Required` rule refuses its absence, and with no
    // length known no element is judged, inside a length or past it.
    for kind in ["Real", "Integer", "Boolean"] {
        let class = format!("CDL.Routing.{kind}VectorFilter");
        assert_eq!(
            diagnostics(
                &class,
                &[("nout", Value::Integer(1)), ("msk_9", Value::Boolean(true))]
            ),
            vec![(
                "missing-required-parameter".to_owned(),
                format!("block `{class}` is missing required parameter `nin`")
            )],
            "{class}"
        );
    }
}

#[test]
fn a_length_the_load_gate_refuses_judges_no_element() {
    // A negative or non-Integer length is refused by its own rules; the elements wait for it.
    let class = "CDL.Reals.MultiSum";
    let nin = |message: &str| {
        (
            "parameter-out-of-range".to_owned(),
            format!("parameter `nin` on block `{class}` {message}"),
        )
    };
    assert_eq!(
        diagnostics(
            class,
            &[("nin", Value::Integer(-1)), ("k_9", Value::Real(1.0))]
        ),
        vec![nin("must be >= 0; got -1")]
    );
    assert_eq!(
        diagnostics(
            class,
            &[("nin", Value::Real(2.0)), ("k_9", Value::Real(1.0))]
        ),
        vec![
            nin("must be an integer and <= 1048576"),
            nin("must be an integer and >= 0")
        ]
    );
}

#[test]
fn every_spelling_a_hostile_document_may_use_meets_exactly_one_rule_or_none() {
    let class = "CDL.Reals.MultiSum";
    let one = |name: &str| {
        diagnostics(
            class,
            &[("nin", Value::Integer(2)), (name, Value::Real(1.0))],
        )
    };
    // An index too large for `usize` (2^128 here) is past every length.
    let huge = "k_340282366920938463463374607431768211456";
    assert_eq!(one(huge), vec![past(huge, class, 2)]);
    // A sign or a leading zero is the spelling check's refusal (GitHub #470), and only that.
    for name in ["k_01", "k_00", "k_+1", "k_+3", "k_-1", "k_-0"] {
        assert_eq!(
            one(name),
            vec![(
                "non-canonical-element-name".to_owned(),
                format!(
                    "parameter `{name}` on block `{class}` spells an array element with a \
                     non-canonical index; write decimal indices with no sign or leading zero"
                )
            )],
            "{name}"
        );
    }
    // Not an element of this vector at all (no index, not decimal, a matrix spelling on a
    // vector, a non-ASCII digit, another base): no rule of the load gate reads the name, so
    // nothing is reported. Unknown names are GitHub #467's remaining half.
    for name in ["k_", "k_x", "k_1x", "k_1_2", "k_\u{0661}", "kk_3", "k3"] {
        assert_eq!(one(name), vec![], "{name}");
    }
}

#[test]
fn a_repeated_element_past_the_length_is_reported_at_each_occurrence() {
    let class = "CDL.Reals.MultiSum";
    assert_eq!(
        diagnostics(
            class,
            &[
                ("nin", Value::Integer(2)),
                ("k_3", Value::Real(1.0)),
                ("k_3", Value::Real(2.0)),
            ]
        ),
        vec![
            (
                "duplicate-parameter".to_owned(),
                format!("block `{class}` carries duplicate parameter name `k_3`")
            ),
            past("k_3", class, 2),
            past("k_3", class, 2),
        ]
    );
}

#[test]
fn every_element_past_the_length_is_refused_once_and_those_inside_are_still_judged() {
    let class = "CDL.Reals.MultiSum";
    let names: Vec<String> = (3..=40).map(|i| format!("k_{i}")).collect();
    let mut params = vec![
        ("nin", Value::Integer(2)),
        ("k_1", Value::Real(1.0)),
        ("k_2", Value::Boolean(true)),
    ];
    params.extend(names.iter().map(|name| (name.as_str(), Value::Real(1.0))));
    let mut expected = vec![(
        "parameter-out-of-range".to_owned(),
        format!("parameter `k_2` on block `{class}` must be a numeric real array element"),
    )];
    expected.extend(names.iter().map(|name| past(name, class, 2)));
    expected.sort();
    assert_eq!(diagnostics(class, &params), expected);
}
