//! The two questions the diff asks the catalog, against the facade's own tables.

use oce_api::{CatalogDefault, class_metadata};
use oce_author::{Literal, MemberRef};

use super::{content_id, element, indices, is_literal_default, is_real, scalar};

fn member(spelling: &str) -> Literal {
    Literal::Member(MemberRef::parse(spelling))
}

#[test]
fn the_content_id_is_the_facades() {
    assert_eq!(
        content_id(),
        oce_api::catalog_content_id(oce_api::catalog())
    );
    assert!(content_id().starts_with("catalog:1:fnv1a128:"));
    assert_eq!(content_id().len(), "catalog:1:fnv1a128:".len() + 32);
}

#[test]
fn a_literal_default_is_recognised_by_its_exact_value() {
    // The defaults these cases rest on, read from the facade so a catalog change fails here
    // by name and not as a wrong diff.
    let default = |class: &str, name: &str| scalar(class, name).expect("declared").default.clone();
    assert_eq!(
        default("CDL.Logical.TrueDelay", "delayOnInit"),
        CatalogDefault::Boolean(false)
    );
    assert_eq!(default("CDL.Reals.PID", "k"), CatalogDefault::Real(1.0));
    assert_eq!(default("CDL.Reals.PID", "yMin"), CatalogDefault::Real(0.0));
    assert_eq!(
        default("CDL.Reals.PID", "controllerType"),
        CatalogDefault::EnumMember("PI")
    );
    assert_eq!(
        default("CDL.Integers.Stage", "pre_y_start"),
        CatalogDefault::Integer(0)
    );

    let is = is_literal_default;
    assert!(is(
        "CDL.Logical.TrueDelay",
        "delayOnInit",
        &Literal::Boolean(false)
    ));
    assert!(!is(
        "CDL.Logical.TrueDelay",
        "delayOnInit",
        &Literal::Boolean(true)
    ));
    assert!(!is(
        "CDL.Logical.TrueDelay",
        "delayOnInit",
        &Literal::Integer(0)
    ));

    assert!(is("CDL.Reals.PID", "k", &Literal::Real(1.0)));
    assert!(
        is("CDL.Reals.PID", "k", &Literal::Integer(1)),
        "the Integer the Real means"
    );
    assert!(!is(
        "CDL.Reals.PID",
        "k",
        &Literal::Real(1.0 + f64::EPSILON)
    ));
    assert!(!is("CDL.Reals.PID", "k", &Literal::Integer(2)));
    assert!(is("CDL.Reals.PID", "yMin", &Literal::Real(0.0)));
    assert!(is("CDL.Reals.PID", "yMin", &Literal::Integer(0)));
    assert!(
        !is("CDL.Reals.PID", "yMin", &Literal::Real(-0.0)),
        "-0.0 is not +0.0"
    );
    assert!(!is("CDL.Reals.PID", "yMin", &Literal::Real(f64::NAN)));

    assert!(is(
        "CDL.Integers.Stage",
        "pre_y_start",
        &Literal::Integer(0)
    ));
    assert!(!is(
        "CDL.Integers.Stage",
        "pre_y_start",
        &Literal::Real(0.0)
    ));

    assert!(is(
        "CDL.Reals.PID",
        "controllerType",
        &member("SimpleController.PI")
    ));
    assert!(!is(
        "CDL.Reals.PID",
        "controllerType",
        &member("SimpleController.PID")
    ));
    assert!(
        !is("CDL.Reals.PID", "controllerType", &member("OtherType.PI")),
        "the member of another type"
    );
    assert!(
        !is("CDL.Reals.PID", "controllerType", &member("PI")),
        "no type at all"
    );
    assert!(!is(
        "CDL.Reals.PID",
        "controllerType",
        &Literal::Text("PI".to_owned())
    ));
}

#[test]
fn a_derived_default_a_required_parameter_and_an_array_element_never_match() {
    let default = |class: &str, name: &str| scalar(class, name).expect("declared").default.clone();
    assert!(matches!(
        default("CDL.Reals.LimitSlewRate", "fallingSlewRate"),
        CatalogDefault::Derived(_)
    ));
    assert_eq!(
        default("CDL.Logical.TrueDelay", "delayTime"),
        CatalogDefault::Required
    );
    for value in [Literal::Real(-1.0), Literal::Real(0.0), Literal::Integer(0)] {
        assert!(!is_literal_default(
            "CDL.Reals.LimitSlewRate",
            "fallingSlewRate",
            &value
        ));
        assert!(!is_literal_default(
            "CDL.Logical.TrueDelay",
            "delayTime",
            &value
        ));
    }
    // Every array template with a literal default: its elements are never "the default".
    let mut templates = 0;
    for class in class_metadata() {
        for parameter in &class.parameters {
            let Some((base, _)) = parameter.name.split_once("_<") else {
                continue;
            };
            templates += 1;
            assert!(scalar(class.class_path, parameter.name).is_none());
            let literal = match parameter.default {
                CatalogDefault::Real(value) => Literal::Real(value),
                CatalogDefault::Integer(value) => Literal::Integer(value),
                CatalogDefault::Boolean(value) => Literal::Boolean(value),
                _ => Literal::Integer(1),
            };
            for name in [
                format!("{base}_1"),
                format!("{base}_1_1"),
                parameter.name.to_owned(),
            ] {
                assert!(
                    !is_literal_default(class.class_path, &name, &literal),
                    "{} {name}",
                    class.class_path
                );
            }
        }
    }
    assert!(
        templates >= 5,
        "the catalog has array parameters: {templates}"
    );
}

#[test]
fn an_unknown_class_or_parameter_answers_no() {
    assert!(!is_literal_default(
        "CDL.Nowhere.Nothing",
        "k",
        &Literal::Real(1.0)
    ));
    assert!(!is_literal_default(
        "CDL.Reals.PID",
        "nothing",
        &Literal::Real(1.0)
    ));
    assert!(!is_literal_default("", "", &Literal::Real(1.0)));
    assert!(!is_real("CDL.Nowhere.Nothing", "k"));
    assert!(!is_real("CDL.Reals.PID", "nothing"));
    assert!(!is_real("", ""));
}

#[test]
fn a_real_parameter_is_told_from_the_others_scalars_and_array_elements_alike() {
    assert!(is_real("CDL.Logical.TrueDelay", "delayTime"));
    assert!(is_real("CDL.Reals.PID", "k"));
    assert!(!is_real("CDL.Logical.TrueDelay", "delayOnInit"));
    assert!(!is_real("CDL.Integers.Sources.Constant", "k"));
    assert!(!is_real("CDL.Reals.PID", "controllerType"));
    // `k_<i>` of MultiSum is a Real array; `nin` beside it is an Integer.
    assert!(element("CDL.Reals.MultiSum", "k_1").is_some());
    assert!(is_real("CDL.Reals.MultiSum", "k_1"));
    assert!(is_real("CDL.Reals.MultiSum", "k_12"));
    assert!(!is_real("CDL.Reals.MultiSum", "nin"));
    for not_an_element in [
        "k", "k_", "k_0", "k_01", "k_1_1", "k_x", "k_1x", "K_1", "k_<i>",
    ] {
        assert!(
            !is_real("CDL.Reals.MultiSum", not_an_element),
            "`{not_an_element}` is not an element of `k_<i>`"
        );
    }
    // A matrix element takes a row and a column.
    assert!(is_real("CDL.Reals.MatrixGain", "K_1_2"));
    assert!(!is_real("CDL.Reals.MatrixGain", "K_1"));
    assert!(!is_real("CDL.Reals.MatrixGain", "K_1_2_3"));
}

#[test]
fn a_template_is_matched_around_runs_of_digits() {
    assert_eq!(indices("k_<i>", "k_7"), Some(vec![7]));
    assert_eq!(indices("k_<i>", "k_70"), Some(vec![70]));
    assert_eq!(indices("K_<row>_<col>", "K_3_14"), Some(vec![3, 14]));
    assert_eq!(indices("k_<i>", "k_"), None);
    assert_eq!(indices("k_<i>", "k_07"), None, "no leading zero");
    assert_eq!(indices("k_<i>", "k_7z"), None);
    assert_eq!(indices("k_<i>", "j_7"), None);
    assert_eq!(indices("k", "k"), None, "a scalar name is not a template");
    assert_eq!(
        indices("k_<i>", "k_99999999999999999999999999"),
        None,
        "an index past usize"
    );
    assert_eq!(indices("k_<i>", ""), None);
}
