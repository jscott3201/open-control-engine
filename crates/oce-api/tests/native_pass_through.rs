//! Runtime pins for reserved native boundary pass-through lowering.

use oce_api::{Engine, Value};

const FIXTURE: &str = include_str!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");
#[test]
fn staged_scalar_values_are_visible_at_boundary_outputs_on_the_same_tick() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(FIXTURE.as_bytes())
        .expect("pass-through fixture loads");
    assert_eq!(report.block_count, 4);

    for (path, zero) in [
        ("conn#3", Value::Real(0.0)),
        ("conn#5", Value::Integer(0)),
        ("conn#7", Value::Boolean(false)),
    ] {
        assert!(
            engine
                .get_output(path)
                .expect("pass-through output point")
                .bit_eq(&zero),
            "outputs are type-zero before the first staging/tick"
        );
    }

    for (path, value) in [
        ("conn#0", Value::Real(f64::from_bits(0x4009_21fb_5444_2d18))),
        ("conn#4", Value::Integer(i64::MIN)),
        ("conn#6", Value::Boolean(true)),
    ] {
        engine
            .set_input(path, value)
            .expect("pass-through input stages");
    }
    engine.tick(0.0).expect("pass-through model ticks");

    for (path, expected) in [
        ("conn#3", Value::Real(f64::from_bits(0x4009_21fb_5444_2d18))),
        ("conn#5", Value::Integer(i64::MIN)),
        ("conn#7", Value::Boolean(true)),
    ] {
        assert!(
            engine
                .get_output(path)
                .expect("pass-through output point")
                .bit_eq(&expected),
            "{path} must observe the staged input on the same tick"
        );
    }
}
