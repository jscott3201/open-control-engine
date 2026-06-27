//! Parameter-surface tests for block classes that need focused facade coverage.

use super::common::*;

fn calendar_time_model() -> ModelGraph {
    let mut mb = Mb::new();
    mb.block(
        "CDL.Reals.Sources.CalendarTime",
        &[],
        &[
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Integer,
            ValueType::Real,
            ValueType::Integer,
        ],
        vec![
            (
                Arc::from("zerTim"),
                Value::Enum {
                    class: oce_model::EnumClassId::ZERO_TIME,
                    ordinal: 11,
                },
            ),
            (Arc::from("yearRef"), Value::Integer(2016)),
            (Arc::from("offset"), Value::Real(0.0)),
        ],
    );
    mb.finish()
}

fn sun_rise_set_model() -> ModelGraph {
    let mut mb = Mb::new();
    mb.block(
        "CDL.Utilities.SunRiseSet",
        &[],
        &[ValueType::Real, ValueType::Real, ValueType::Boolean],
        vec![rp("lat", 0.0), rp("lon", 0.0), rp("timZon", 0.0)],
    );
    mb.finish()
}

#[test]
fn calendar_time_param_rules_surface_bounds_and_reject_invalid_edits_at_rest() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(calendar_time_model(), None)
        .expect("valid CalendarTime loads");
    let rows = eng.params().to_vec();

    let (_, zer_tim_value, zer_tim_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.zerTim")
        .expect("zerTim param must be present");
    assert!(zer_tim_value.bit_eq(&Value::Enum {
        class: oce_model::EnumClassId::ZERO_TIME,
        ordinal: 11,
    }));
    assert_eq!(
        zer_tim_attrs.value_type,
        ValueType::Enum(oce_model::EnumClassId::ZERO_TIME)
    );
    assert_eq!(zer_tim_attrs.min, None);
    assert_eq!(zer_tim_attrs.max, None);

    let (_, year_ref_value, year_ref_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.yearRef")
        .expect("yearRef param must be present");
    assert!(year_ref_value.bit_eq(&Value::Integer(2016)));
    assert_eq!(year_ref_attrs.min, Some(2010.0));
    assert_eq!(year_ref_attrs.max, Some(2031.0));

    let (_, offset_value, offset_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.offset")
        .expect("offset param must be present");
    assert!(offset_value.bit_eq(&Value::Real(0.0)));
    assert_eq!(offset_attrs.min, None);
    assert_eq!(offset_attrs.max, None);

    eng.halt().unwrap();
    assert!(matches!(
        eng.set_param(
            "b0.zerTim",
            Value::Enum {
                class: oce_model::EnumClassId::ZERO_TIME,
                ordinal: 45,
            },
        ),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.yearRef", Value::Integer(2009)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.yearRef", Value::Integer(2032)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.offset", Value::Real(f64::NAN)),
        Err(OcError::ParamRange { .. })
    ));
    eng.set_param(
        "b0.zerTim",
        Value::Enum {
            class: oce_model::EnumClassId::ZERO_TIME,
            ordinal: 12,
        },
    )
    .expect("valid ZeroTime ordinal is accepted");
    eng.set_param("b0.yearRef", Value::Integer(2031))
        .expect("CalendarTime yearRef upper boundary is valid");
    eng.set_param("b0.offset", Value::Real(-3_600.0))
        .expect("finite offset is valid");
}

#[test]
fn sun_rise_set_param_rules_surface_bounds_and_reject_invalid_edits_at_rest() {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(sun_rise_set_model(), None)
        .expect("valid SunRiseSet loads");
    let rows = eng.params().to_vec();

    let (_, lat_value, lat_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.lat")
        .expect("lat param must be present");
    assert!(lat_value.bit_eq(&Value::Real(0.0)));
    assert_eq!(lat_attrs.min, Some(-std::f64::consts::FRAC_PI_2));
    assert_eq!(lat_attrs.max, Some(std::f64::consts::FRAC_PI_2));

    let (_, lon_value, lon_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.lon")
        .expect("lon param must be present");
    assert!(lon_value.bit_eq(&Value::Real(0.0)));
    assert_eq!(lon_attrs.min, Some(-std::f64::consts::PI));
    assert_eq!(lon_attrs.max, Some(std::f64::consts::PI));

    let (_, tim_zon_value, tim_zon_attrs) = rows
        .iter()
        .find(|(p, _, _)| p == "b0.timZon")
        .expect("timZon param must be present");
    assert!(tim_zon_value.bit_eq(&Value::Real(0.0)));
    assert_eq!(tim_zon_attrs.min, None);
    assert_eq!(tim_zon_attrs.max, None);

    eng.halt().unwrap();
    assert!(matches!(
        eng.set_param("b0.lat", Value::Real(std::f64::consts::FRAC_PI_2 + 0.001)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.lon", Value::Real(std::f64::consts::PI + 0.001)),
        Err(OcError::ParamRange { .. })
    ));
    assert!(matches!(
        eng.set_param("b0.timZon", Value::Real(f64::NAN)),
        Err(OcError::ParamRange { .. })
    ));
    eng.set_param("b0.lat", Value::Real(std::f64::consts::FRAC_PI_2))
        .expect("latitude upper boundary is valid");
    eng.set_param("b0.lon", Value::Real(-std::f64::consts::PI))
        .expect("longitude lower boundary is valid");
    eng.set_param("b0.timZon", Value::Real(50_400.0))
        .expect("finite timezone offset is valid");
}
