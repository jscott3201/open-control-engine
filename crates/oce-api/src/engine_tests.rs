//! Unit tests for the pure load tail: the plain-data [`LoadedModel`] a load returns, its
//! agreement with the engine it describes, and its independence from that engine.

use super::*;
use crate::PointDirection;
use oce_model::Value;

const ANALOG_WARNING: &str = include_str!(
    "../../oce-cxf/tests/fixtures/composite_contract/warned/analog_coerced_member.jsonld"
);
const ADD: &[u8] = include_bytes!("../tests/fixtures/legacy_frame_add.jsonld");

fn corpus() -> Vec<std::path::PathBuf> {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36");
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 47, "corpus size positive control");
    paths
}

fn model_json(loaded: &LoadedModel) -> Vec<u8> {
    serde_json::to_vec(&loaded.model).unwrap()
}

#[test]
fn loaded_model_agrees_with_its_report_and_engine_across_the_corpus() {
    for path in corpus() {
        let mut engine = Engine::new();
        let (report, loaded) = engine.load_cxf(&std::fs::read(&path).unwrap()).unwrap();
        let name = path.display();
        assert_eq!(loaded.model_id, report.model_id, "{name}");
        assert_eq!(loaded.model.model_id, loaded.model_id, "{name}");
        assert_eq!(loaded.model_id, engine.model_id, "{name}");
        assert_eq!(loaded.diagnostics, report.warnings, "{name}");
        // Semantic resolution is the last diagnostic-producing stage of a successful load.
        assert!(
            loaded.diagnostics.ends_with(&loaded.semantic_warnings),
            "{name}"
        );
        assert_eq!(loaded.semantic_warnings, engine.semantic_warnings, "{name}");
        assert_eq!(loaded.io.to_vec(), engine.io().to_vec(), "{name}");
        assert_eq!(loaded.io.len(), report.io.total, "{name}");
        assert_eq!(loaded.model.blocks.len(), report.block_count, "{name}");
        let inputs: Vec<_> = loaded.input_points().collect();
        let expected: Vec<_> = engine
            .io()
            .iter()
            .filter(|point| point.direction == PointDirection::In)
            .map(|point| (point.path, point.value_type))
            .collect();
        assert_eq!(
            inputs
                .iter()
                .map(|(path, value_type)| ((*path).to_owned(), *value_type))
                .collect::<Vec<_>>(),
            expected,
            "{name}"
        );
        // Every input the host must bind is a projected point with the same identity and type.
        for (path, value_type) in &inputs {
            let point = loaded
                .model
                .points
                .iter()
                .find(|point| point.key.as_str() == *path)
                .unwrap_or_else(|| panic!("{name}: input {path} missing from projection"));
            assert_eq!(point.value_type, *value_type, "{name}: {path}");
            assert_eq!(point.direction, PointDirection::In, "{name}: {path}");
        }
    }
}

#[test]
fn loaded_model_is_deterministic_and_identical_on_the_receipt_path() {
    for path in corpus() {
        let bytes = std::fs::read(&path).unwrap();
        let (_, first) = Engine::new().load_cxf(&bytes).unwrap();
        let (_, second) = Engine::new().load_cxf(&bytes).unwrap();
        let (receipt, third) = Engine::new().load_cxf_with_receipt(&bytes).unwrap();
        assert_eq!(
            model_json(&first),
            model_json(&second),
            "{}",
            path.display()
        );
        assert_eq!(model_json(&first), model_json(&third), "{}", path.display());
        assert_eq!(first.io.to_vec(), third.io.to_vec());
        assert_eq!(first.diagnostics, third.diagnostics);
        assert_eq!(receipt.report().warnings, third.diagnostics);
    }
}

#[test]
fn resolved_model_survives_a_serde_round_trip_bit_for_bit() {
    let (_, loaded) = Engine::new().load_cxf(ADD).unwrap();
    let encoded = model_json(&loaded);
    let decoded: oce_model::resolved_model::ResolvedModel =
        serde_json::from_slice(&encoded).unwrap();
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
    assert_eq!(decoded.model_id, loaded.model_id);
}

#[test]
fn dropping_or_mutating_the_loaded_model_never_reaches_the_engine() {
    let mut engine = Engine::new();
    let (_, mut loaded) = engine.load_cxf(ADD).unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    let io = engine.io().to_vec();
    loaded.model.points.clear();
    loaded.model.blocks.clear();
    loaded.model_id = DomainKey::new("host-owned");
    loaded.diagnostics.clear();
    drop(loaded);
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        snapshot.as_bytes()
    );
    assert_eq!(engine.io().to_vec(), io);
    let plan = engine
        .prepare_frame(
            0.0,
            &[
                ("urn:legacy-frame:a", oce_model::Value::Real(1.0)),
                ("urn:legacy-frame:b", oce_model::Value::Real(2.0)),
            ],
        )
        .unwrap();
    let frame = engine.execute_frame(plan).unwrap();
    assert!(frame.outputs()[0].1.bit_eq(&oce_model::Value::Real(3.0)));
}

#[test]
fn warned_load_returns_its_warning_in_both_results() {
    let mut engine = Engine::new();
    let (report, loaded) = engine.load_cxf(ANALOG_WARNING.as_bytes()).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(
        report.warnings[0].code,
        oce_diag::DiagCode::AnalogCoercedToReal
    );
    assert_eq!(loaded.diagnostics, report.warnings);
    assert!(loaded.semantic_warnings.is_empty());
}

#[path = "tests/reload_tests.rs"]
mod reload;
