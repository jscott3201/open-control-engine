//! Existing hosted determinism cells may retain these exact canonical per-frame vectors.
//! File IO lives only in this test harness, never in the engine or record codec.

use oce_api::{
    CompatibilityDescriptor, Engine, ReplayError, ReplayRecord, StatePortability, Value,
};

fn artifact(variable: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(std::env::var_os(variable)?);
    Some(if path.is_absolute() {
        path
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    })
}

#[test]
fn portable_and_target_bound_vectors_repeat_and_match_the_state_placement_policy() {
    for (class, variable, portable) in [
        ("CDL.Reals.Add", "OCE_PORTABLE_REPLAY_OUT", true),
        ("CDL.Reals.Atan2", "OCE_TARGET_REPLAY_OUT", false),
    ] {
        let model =
            include_str!("fixtures/legacy_frame_add.jsonld").replace("CDL.Reals.Add", class);
        let mut previous = None;
        for _ in 0..3 {
            let mut engine = Engine::in_memory();
            engine.load_cxf(model.as_bytes()).unwrap();
            let plan = engine
                .prepare_frame(
                    -0.0,
                    &[
                        ("urn:legacy-frame:a", Value::Real(1.5)),
                        ("urn:legacy-frame:b", Value::Real(2.25)),
                    ],
                )
                .unwrap();
            let receipt = engine.execute_frame(plan).unwrap();
            let record = receipt.replay_record().unwrap();
            assert_eq!(
                record.portability(),
                engine.state_snapshot().unwrap().portability()
            );
            assert_eq!(
                matches!(record.portability(), StatePortability::Portable),
                portable
            );
            record
                .check_compatible(&CompatibilityDescriptor::current(None).unwrap())
                .unwrap();
            record.verify(&receipt).unwrap();
            if let Some(previous) = &previous {
                assert_eq!(record.as_bytes(), previous);
            }
            previous = Some(record.as_bytes().to_vec());
            // Portability was captured with the accepted result, not re-read after reload.
            engine
                .load_cxf(include_bytes!("fixtures/frame_pre.jsonld"))
                .unwrap();
            assert_eq!(
                receipt.replay_record().unwrap().as_bytes(),
                record.as_bytes()
            );
        }
        if let Some(path) = artifact(variable) {
            std::fs::write(path, previous.unwrap()).unwrap();
        }
    }
}

#[test]
fn foreign_matrix_record_refuses_before_any_engine_mutation_when_supplied() {
    let Some(path) = artifact("OCE_FOREIGN_TARGET_REPLAY_IN") else {
        return;
    };
    let record = ReplayRecord::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(include_bytes!("fixtures/legacy_frame_add.jsonld"))
        .unwrap();
    let before = engine.state_snapshot().unwrap();
    assert_eq!(
        record.check_compatible(&CompatibilityDescriptor::current(None).unwrap()),
        Err(ReplayError::TargetMismatch)
    );
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
    engine.restore_state(&before).unwrap();
}
