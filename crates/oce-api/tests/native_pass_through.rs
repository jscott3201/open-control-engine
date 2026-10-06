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

    let inputs = [
        ("realIn", Value::Real(f64::from_bits(0x4009_21fb_5444_2d18))),
        ("integerIn", Value::Integer(i64::from(i32::MIN))),
        ("booleanIn", Value::Boolean(true)),
    ]
    .map(|(suffix, value)| (format!("{ROOT}{suffix}"), value));
    let entries: Vec<_> = inputs
        .iter()
        .map(|(p, v)| (p.as_str(), v.clone()))
        .collect();
    let prepared = engine.prepare_frame(0.0, &entries).unwrap();
    engine.execute_frame(prepared).unwrap();

    for (suffix, expected) in [
        (
            "realOut",
            Value::Real(f64::from_bits(0x4009_21fb_5444_2d18)),
        ),
        ("integerOut", Value::Integer(i64::from(i32::MIN))),
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
