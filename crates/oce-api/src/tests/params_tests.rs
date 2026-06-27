//! Parameter-surface tests for block classes that need focused facade coverage.

use super::common::*;

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
