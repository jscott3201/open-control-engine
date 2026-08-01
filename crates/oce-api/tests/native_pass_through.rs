//! Runtime pins for reserved native boundary pass-through lowering.

use oce_api::{Engine, Value};

const FIXTURE: &str = include_str!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");
const ROOT: &str = "http://example.org#PassThroughMiniature.";

#[test]
fn staged_scalar_values_are_visible_at_boundary_outputs_on_the_same_tick() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(FIXTURE.as_bytes())
        .expect("pass-through fixture loads");
    assert_eq!(report.block_count, 4);

    for (suffix, zero) in [
        ("realOut", Value::Real(0.0)),
        ("integerOut", Value::Integer(0)),
        ("booleanOut", Value::Boolean(false)),
    ] {
        assert!(
            engine
                .get_output(&format!("{ROOT}{suffix}"))
                .expect("pass-through output point")
                .bit_eq(&zero),
            "outputs are type-zero before the first staging/tick"
        );
    }

    for (suffix, value) in [
        ("realIn", Value::Real(f64::from_bits(0x4009_21fb_5444_2d18))),
        ("integerIn", Value::Integer(i64::MIN)),
        ("booleanIn", Value::Boolean(true)),
    ] {
        engine
            .set_input(&format!("{ROOT}{suffix}"), value)
            .expect("pass-through input stages");
    }
    engine.tick(0.0).expect("pass-through model ticks");

    for (suffix, expected) in [
        (
            "realOut",
            Value::Real(f64::from_bits(0x4009_21fb_5444_2d18)),
        ),
        ("integerOut", Value::Integer(i64::MIN)),
        ("booleanOut", Value::Boolean(true)),
    ] {
        assert!(
            engine
                .get_output(&format!("{ROOT}{suffix}"))
                .expect("pass-through output point")
                .bit_eq(&expected),
            "{suffix} must observe the staged input on the same tick"
        );
    }
}
