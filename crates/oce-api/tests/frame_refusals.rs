//! Refusal boundaries for complete observations and malformed replacement documents.
//! Add's exact dyadic inputs are hand-derived; no engine-output oracle is generated here.

use oce_api::{Engine, OcError, Value};

const MODEL: &[u8] = include_bytes!("../../oce-conformance/tests/fixtures/driver/free_add.jsonld");
const U1: &str = "http://example.org#DriverAdd.u1";
const U2: &str = "http://example.org#DriverAdd.u2";
const Y: &str = "http://example.org#DriverAdd.add.y";

fn loaded() -> Engine {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    let prepared = engine
        .prepare_frame(4.0, &[(U1, Value::Real(1.0)), (U2, Value::Real(2.0))])
        .unwrap();
    engine.execute_frame(prepared).unwrap();
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(3.0)));
    engine
}

fn preserved(engine: &Engine, before: &[u8]) {
    assert_eq!(engine.state_snapshot().unwrap().as_bytes(), before);
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(3.0)));
    assert!(
        matches!(engine.prepare_frame(3.0, &[]), Err(OcError::TimeRegression { now, prev })
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
        preserved(&engine, before.as_bytes());
    }
}

#[test]
fn incomplete_observations_never_stage_prefixes_or_reuse_prior_values() {
    let mut engine = loaded();
    let before = engine.state_snapshot().unwrap();
    assert!(
        matches!(engine.prepare_frame(4.0, &[(U1, Value::Real(8.0))]),
        Err(OcError::FrameMissingInput(path)) if path == U2)
    );
    preserved(&engine, before.as_bytes());
    assert!(matches!(engine.prepare_frame(4.0, &[
        (U1, Value::Real(8.0)), (U1, Value::Real(9.0)), (U2, Value::Real(2.0)),
    ]), Err(OcError::FrameDuplicateInput(path)) if path == U1));
    preserved(&engine, before.as_bytes());
    assert!(
        matches!(engine.prepare_frame(4.0, &[(U1, Value::Real(8.0)), (U2, Value::Boolean(false))]),
        Err(OcError::InputType(path)) if path == U2)
    );
    preserved(&engine, before.as_bytes());
    let prepared = engine
        .prepare_frame(4.0, &[(U1, Value::Real(3.0)), (U2, Value::Real(2.0))])
        .unwrap();
    let completed = engine.execute_frame(prepared).unwrap();
    assert_eq!(completed.sequence(), 2);
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(5.0)));
}
