//! Typed public refusals and their non-transactional boundaries, without panic interception.
//! Stateful restart/store failures are additionally covered by the existing sim/store-backed suites.

use oce_api::{CollectSpec, Engine, InputSource, OcError, SimSpec, Value};

const MODEL: &[u8] = include_bytes!("../../oce-conformance/tests/fixtures/driver/free_add.jsonld");
const U1: &str = "http://example.org#DriverAdd.u1";
const U2: &str = "http://example.org#DriverAdd.u2";
const Y: &str = "http://example.org#DriverAdd.add.y";

fn loaded() -> Engine {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    engine.set_input(U1, Value::Real(1.0)).unwrap();
    engine.set_input(U2, Value::Real(2.0)).unwrap();
    engine.tick(4.0).unwrap();
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(3.0)));
    engine
}

fn spec() -> SimSpec {
    SimSpec {
        t_start: 0.0,
        t_stop: 1.0,
        step: 1.0,
        inputs: InputSource::None,
        collect: CollectSpec::All { stride: 1 },
    }
}

fn preserved(engine: &mut Engine, before: &[u8]) {
    assert_eq!(engine.state_snapshot().unwrap().as_bytes(), before);
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(3.0)));
    assert!(
        matches!(engine.tick(3.0), Err(OcError::TimeRegression { now, prev })
        if now.to_bits() == 3.0_f64.to_bits() && prev.to_bits() == 4.0_f64.to_bits())
    );
}

#[test]
fn malformed_serialized_inputs_return_json_errors_before_replacing_a_run() {
    let mut engine = loaded();
    let before = engine.state_snapshot().unwrap();
    for bytes in [
        b"".as_slice(),
        b"{",
        b"{not json",
        b"\xff",
        b"null trailing",
    ] {
        assert!(matches!(
            engine.load_cxf(bytes),
            Err(OcError::Cxf(oce_cxf::CxfError::Json(_)))
        ));
        preserved(&mut engine, before.as_bytes());
    }
}

#[test]
fn invalid_step_and_time_bounds_preserve_the_prior_run_exactly() {
    let mut engine = loaded();
    let before = engine.state_snapshot().unwrap();
    for step in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            engine.simulate(&SimSpec { step, ..spec() }),
            Err(OcError::Load { .. })
        ));
        preserved(&mut engine, before.as_bytes());
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for request in [
            SimSpec {
                t_start: bad,
                ..spec()
            },
            SimSpec {
                t_stop: bad,
                ..spec()
            },
        ] {
            assert!(
                matches!(engine.simulate(&request), Err(OcError::NonFiniteTime { now })
                if now.to_bits() == bad.to_bits())
            );
            preserved(&mut engine, before.as_bytes());
        }
    }
    assert!(
        matches!(engine.simulate(&SimSpec { t_start: 2.0, ..spec() }),
        Err(OcError::TimeRegression { now, prev })
        if now.to_bits() == 1.0_f64.to_bits() && prev.to_bits() == 2.0_f64.to_bits())
    );
    preserved(&mut engine, before.as_bytes());
}

#[test]
fn collection_constants_and_first_closure_lists_refuse_before_staging_or_restart() {
    let mut engine = loaded();
    let before = engine.state_snapshot().unwrap();
    for name in ["absent", U1] {
        let request = SimSpec {
            collect: CollectSpec::Named {
                points: vec![Y.to_owned(), name.to_owned()],
                stride: 1,
            },
            ..spec()
        };
        assert!(matches!(engine.simulate(&request), Err(OcError::UnknownPoint(p)) if p == name));
        preserved(&mut engine, before.as_bytes());
    }
    for (name, value, type_error) in [
        ("absent", Value::Real(1.0), false),
        (Y, Value::Real(1.0), false),
        (U1, Value::Boolean(true), true),
    ] {
        for closure in [false, true] {
            let pairs = vec![
                (U2.to_owned(), Value::Real(99.0)),
                (name.to_owned(), value.clone()),
            ];
            let inputs = if closure {
                InputSource::Closure(Box::new(move |_| pairs.clone()))
            } else {
                InputSource::Constant(pairs)
            };
            let error = engine.simulate(&SimSpec { inputs, ..spec() }).unwrap_err();
            if type_error {
                assert!(matches!(error, OcError::InputType(p) if p == name));
            } else {
                assert!(matches!(error, OcError::UnknownPoint(p) if p == name));
            }
            preserved(&mut engine, before.as_bytes());
        }
    }
}

#[test]
fn later_closure_refusals_keep_completed_output_and_valid_prefix_but_do_not_tick() {
    for type_error in [false, true] {
        let mut engine = loaded();
        let request = SimSpec {
            inputs: InputSource::Closure(Box::new(move |t| {
                if t.to_bits() == 0 {
                    vec![(U1.to_owned(), Value::Real(1.0))]
                } else {
                    vec![
                        (U2.to_owned(), Value::Real(7.0)),
                        if type_error {
                            (U1.to_owned(), Value::Boolean(false))
                        } else {
                            ("absent".to_owned(), Value::Real(9.0))
                        },
                    ]
                }
            })),
            ..spec()
        };
        let error = engine.simulate(&request).unwrap_err();
        if type_error {
            assert!(matches!(error, OcError::InputType(p) if p == U1));
        } else {
            assert!(matches!(error, OcError::UnknownPoint(p) if p == "absent"));
        }
        assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(3.0)));
        assert!(
            matches!(engine.tick(-1.0), Err(OcError::TimeRegression { now, prev })
            if now.to_bits() == (-1.0_f64).to_bits() && prev.to_bits() == 0)
        );
        // Equal time is accepted: only the first tick completed. The valid prefix 7 remains,
        // so the next evaluation yields the hand-derived 1 + 7, not the old 1 + 2 or bad 9 + 7.
        engine.tick(0.0).unwrap();
        assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(8.0)));
    }
}
