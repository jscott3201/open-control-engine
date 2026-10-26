//! Producer provenance, retained failure context and independent machine-order controls.

use super::*;
use crate::Engine;
use oce_diag::{DiagCode, Diagnostic, Severity};
use serde_json::{Value, json};

const WARNING: &str = include_str!(
    "../../../oce-cxf/tests/fixtures/composite_contract/warned/analog_coerced_member.jsonld"
);
const DISPLAY_WARNING: &str =
    include_str!("../../../oce-cxf/tests/fixtures/invalid/display_unit_divergence.jsonld");
const LOOP: &[u8] = include_bytes!("fixtures/analog_warning_algebraic_loop.jsonld");

fn keys(receipt: &DiagnosticReceipt) -> Vec<DiagnosticKey> {
    receipt
        .records()
        .iter()
        .map(|record| record.key().clone())
        .collect()
}

#[test]
fn machine_order_excludes_prose_and_retains_duplicate_records() {
    let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Validation);
    let warning = Diagnostic::warning(DiagCode::AnalogCoercedToReal, "z prose");
    for subject in [Some("z"), Some(""), None, Some("urn:authored"), Some("b0")] {
        let mut diagnostic = warning.clone();
        diagnostic.subject = subject.map(Into::into);
        capture.record(&[diagnostic]);
    }
    capture.enter(DiagnosticStage::Import);
    capture.record(std::slice::from_ref(&warning));
    let mut different_prose = warning.clone();
    different_prose.message = "a prose".into();
    capture.record(&[different_prose]);
    let receipt = capture.finish();
    let observed: Vec<_> = receipt
        .records()
        .iter()
        .map(|record| (record.key().stage().rank(), record.key().subject().clone()))
        .collect();
    assert_eq!(
        observed,
        [
            (0, DiagnosticSubject::Absent),
            (0, DiagnosticSubject::Absent),
            (3, DiagnosticSubject::Absent),
            (3, DiagnosticSubject::Opaque("".into())),
            (3, DiagnosticSubject::Opaque("b0".into())),
            (3, DiagnosticSubject::Opaque("urn:authored".into())),
            (3, DiagnosticSubject::Opaque("z".into())),
        ]
    );
    assert_eq!(receipt.records()[0].key(), receipt.records()[1].key());
    assert_eq!(receipt.records()[0].message(), "z prose");
    assert_eq!(receipt.records()[1].message(), "a prose");
    let mut changed = receipt.clone();
    for record in &mut changed.records {
        record.message = "different human wording".into();
    }
    assert_eq!(keys(&receipt), keys(&changed));
    changed.records[0].key.stage = DiagnosticStage::Export;
    assert_ne!(keys(&receipt), keys(&changed));
    changed = receipt.clone();
    changed.records[0].key.subject = DiagnosticSubject::Opaque(String::new());
    assert_ne!(keys(&receipt), keys(&changed));
    changed = receipt.clone();
    changed.records[0].key.code.push_str("-changed");
    assert_ne!(keys(&receipt), keys(&changed));
}

#[test]
fn code_precedes_severity_and_stage_precedes_subject() {
    let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Validation);
    for (code, severity) in [
        (DiagCode::DuplicateId, Severity::Error),
        (DiagCode::AnalogCoercedToReal, Severity::Info),
        (DiagCode::AnalogCoercedToReal, Severity::Warning),
        (DiagCode::AnalogCoercedToReal, Severity::Error),
    ] {
        let mut diagnostic = Diagnostic::warning(code, "same");
        diagnostic.severity = severity;
        capture.record(&[diagnostic]);
    }
    capture.enter(DiagnosticStage::Import);
    capture.record(&[Diagnostic::warning(DiagCode::DuplicateId, "same").with_subject("z")]);
    capture.enter(DiagnosticStage::Validation);
    capture.record(&[Diagnostic::warning(DiagCode::DuplicateId, "same").with_subject("a")]);
    capture.record(&[Diagnostic::warning(DiagCode::AnalogCoercedToReal, "same").with_subject("z")]);
    let receipt = capture.finish();
    let observed: Vec<_> = receipt
        .records()
        .iter()
        .map(|record| {
            let key = record.key();
            (key.stage().rank(), key.code(), key.severity().rank())
        })
        .collect();
    assert_eq!(
        observed,
        [
            (0, "duplicate-id", 1),
            (3, "analog-coerced-to-real", 0),
            (3, "analog-coerced-to-real", 1),
            (3, "analog-coerced-to-real", 2),
            (3, "duplicate-id", 0),
            (3, "duplicate-id", 1),
            (3, "analog-coerced-to-real", 1)
        ]
    );
}

#[test]
fn stage_ranks_are_explicit_and_total() {
    use DiagnosticStage::*;
    assert_eq!(
        [
            Import,
            Flatten,
            AttributeUnification,
            Validation,
            Instantiation,
            Schedule,
            Semantics,
            Projection,
            StoreRecovery,
            StoreSave,
            StoreInputs,
            Export
        ]
        .map(DiagnosticStage::rank),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    );
}

#[test]
fn successful_evidence_survives_mutation_of_the_separated_legacy_report() {
    let mut engine = Engine::in_memory();
    let receipt = engine.load_cxf_with_receipt(WARNING.as_bytes()).unwrap();
    let expected = keys(receipt.diagnostics());
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].stage(), DiagnosticStage::Import);
    assert_eq!(expected[0].code(), "analog-coerced-to-real");
    let (mut legacy, evidence) = receipt.into_parts();
    legacy.warnings.clear();
    legacy.warnings.push(Diagnostic::error(
        DiagCode::DuplicateId,
        "caller fabricated",
    ));
    assert_eq!(keys(&evidence), expected);
    assert_eq!(
        engine.load_cxf(WARNING.as_bytes()).unwrap().warnings.len(),
        1
    );
}

#[test]
fn diagnostic_free_refusals_retain_stage_and_original_error_source() {
    use std::error::Error;
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(b"{bad json")
        .unwrap_err();
    assert_eq!(failure.stage(), DiagnosticStage::Import);
    assert!(failure.diagnostics().records().is_empty());
    assert!(matches!(
        failure.error(),
        OcError::Cxf(oce_cxf::CxfError::Json(_))
    ));
    assert_eq!(failure.to_string(), failure.error().to_string());
    assert!(
        failure
            .source()
            .unwrap()
            .downcast_ref::<OcError>()
            .is_some()
    );
    let failure = Engine::in_memory().export_cxf_with_receipt().unwrap_err();
    assert_eq!(failure.stage(), DiagnosticStage::Export);
    assert_eq!(failure.diagnostics().records().len(), 1);
    assert_eq!(
        failure.diagnostics().records()[0].key().code(),
        "export-unsupported"
    );
    assert_eq!(
        failure.diagnostics().records()[0].key().stage(),
        DiagnosticStage::Export
    );
    assert!(matches!(failure.into_error(), OcError::Cxf(_)));
}

#[test]
fn prior_import_warning_precedes_loop_records_in_a_refused_load() {
    let failure = Engine::in_memory().load_cxf_with_receipt(LOOP).unwrap_err();
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    let records = failure.diagnostics().records();
    assert_eq!(records.len(), 5);
    assert_eq!(records[0].key().stage(), DiagnosticStage::Import);
    assert_eq!(records[0].key().code(), "analog-coerced-to-real");
    assert!(failure.error().diagnostics().is_empty());
    assert_eq!(failure.error().all_diagnostics().count(), 1);
    assert!(matches!(failure.error(), OcError::LoadContext(_)));
}

#[test]
fn unification_evidence_survives_structural_refusal_at_its_actual_producer() {
    let mut document: Value = serde_json::from_str(DISPLAY_WARNING).unwrap();
    let node = document["@graph"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["@id"] == "http://example.org#U.con")
        .unwrap();
    node.as_object_mut().unwrap().remove("S231:hasParameter");
    let failure = Engine::in_memory()
        .load_cxf_with_receipt(&serde_json::to_vec(&document).unwrap())
        .unwrap_err();
    assert_eq!(failure.stage(), DiagnosticStage::Validation);
    let observed: Vec<_> = failure
        .diagnostics()
        .records()
        .iter()
        .map(|record| (record.key().stage(), record.key().code()))
        .collect();
    assert_eq!(
        observed,
        [
            (
                DiagnosticStage::AttributeUnification,
                "display-unit-divergence"
            ),
            (DiagnosticStage::Validation, "missing-required-parameter")
        ]
    );
}

#[test]
fn successful_load_retains_import_and_unification_producers() {
    let mut document: Value = serde_json::from_str(DISPLAY_WARNING).unwrap();
    for (id, kind) in [
        ("http://example.org#U.con.y", "S231:AnalogOutput"),
        ("http://example.org#U.add.u1", "S231:AnalogInput"),
    ] {
        let node = document["@graph"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|node| node["@id"] == id)
            .unwrap();
        node["@type"] = json!(kind);
        node.as_object_mut().unwrap().remove("S231:isOfDataType");
    }
    let receipt = Engine::in_memory()
        .load_cxf_with_receipt(&serde_json::to_vec(&document).unwrap())
        .unwrap();
    let observed: Vec<_> = receipt
        .diagnostics()
        .records()
        .iter()
        .map(|record| (record.key().stage(), record.key().code()))
        .collect();
    assert_eq!(
        observed,
        [
            (DiagnosticStage::Import, "analog-coerced-to-real"),
            (DiagnosticStage::Import, "analog-coerced-to-real"),
            (
                DiagnosticStage::AttributeUnification,
                "display-unit-divergence"
            )
        ]
    );
}

#[test]
fn export_receipt_preserves_partial_bytes_and_all_deferral_evidence() {
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(include_bytes!(
            "../../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
        ))
        .unwrap();
    let legacy = engine.export_cxf().unwrap();
    let receipt = engine.export_cxf_with_receipt().unwrap();
    assert_eq!(receipt.report().bytes, legacy.bytes);
    assert_eq!(receipt.report().warnings.len(), 2);
    let before = keys(receipt.diagnostics());
    assert_eq!(before.len(), 2);
    assert!(
        before
            .iter()
            .all(|key| key.stage() == DiagnosticStage::Export)
    );
    let (mut report, diagnostics) = receipt.into_parts();
    report.warnings.reverse();
    report.warnings.clear();
    report.bytes.clear();
    assert_eq!(keys(&diagnostics), before);
}

#[test]
fn legacy_and_receipt_loads_keep_identical_state_and_legacy_diagnostic_order() {
    let mut old = Engine::in_memory();
    let mut new = Engine::in_memory();
    let old_report = old.load_cxf(DISPLAY_WARNING.as_bytes()).unwrap();
    let new_report = new
        .load_cxf_with_receipt(DISPLAY_WARNING.as_bytes())
        .unwrap();
    let signature = |warnings: &[Diagnostic]| {
        warnings
            .iter()
            .map(|warning| {
                json!([
                    warning.code.as_str(),
                    warning.severity.as_str(),
                    warning.subject,
                    warning.message
                ])
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        signature(&old_report.warnings),
        signature(&new_report.report().warnings)
    );
    let inputs = [("http://example.org#U.uIn", crate::Value::Real(0.0))];
    let prepared = old.prepare_frame(0.0, &inputs).unwrap();
    old.execute_frame(prepared).unwrap();
    let prepared = new.prepare_frame(0.0, &inputs).unwrap();
    new.execute_frame(prepared).unwrap();
    assert_eq!(
        old.state_snapshot().unwrap().into_bytes(),
        new.state_snapshot().unwrap().into_bytes()
    );
}

fn members(paths: &[&str]) -> Vec<oce_graph::ConnectorPath> {
    paths
        .iter()
        .map(|path| oce_graph::ConnectorPath((*path).to_owned()))
        .collect()
}

#[test]
fn algebraic_loop_code_strings_never_change() {
    // Hosts compare these strings; renaming either is a contract change, not a refactor.
    assert_eq!(DiagCode::AlgebraicLoop.as_str(), "algebraic-loop");
    assert_eq!(
        DiagCode::BlockAlgebraicLoop.as_str(),
        "block-algebraic-loop"
    );
}

#[test]
fn block_granularity_cycle_yields_one_bounded_record_per_member_block() {
    let error = oce_graph::BuildError::BlockAlgebraicLoop {
        members: members(&["urn:m#x", "urn:m#p"]),
    };
    let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Schedule);
    let refused = capture.build_refusal(error.clone());
    assert!(matches!(refused, OcError::Build(_)));
    let receipt = capture.finish();
    let observed: Vec<_> = receipt
        .records()
        .iter()
        .map(|record| {
            (
                record.key().stage(),
                record.key().subject().clone(),
                record.key().code().to_owned(),
                record.key().severity(),
                record.message().to_owned(),
            )
        })
        .collect();
    // A block-granularity cycle is not a CDL §7.16 violation, so the text must not claim one: it
    // says the model may be CDL-legal and leads with splitting the block (GitHub issue #347).
    let cause = "The model may be CDL-legal: no connector-level cycle exists; a block fires all its \
                 outputs at once. Remedy: split that block so the looping output and the fed-back \
                 input are on separate blocks, or insert a delay where acceptable.";
    assert_eq!(
        observed,
        [
            (
                DiagnosticStage::Schedule,
                DiagnosticSubject::Opaque("urn:m#p".into()),
                "block-algebraic-loop".to_owned(),
                DiagnosticSeverity::Error,
                format!(
                    "block-granularity algebraic loop member 2 of 2; next member: urn:m#x. {cause}"
                ),
            ),
            (
                DiagnosticStage::Schedule,
                DiagnosticSubject::Opaque("urn:m#x".into()),
                "block-algebraic-loop".to_owned(),
                DiagnosticSeverity::Error,
                format!(
                    "block-granularity algebraic loop member 1 of 2; next member: urn:m#p. {cause}"
                ),
            ),
        ]
    );
}

#[test]
fn one_member_cycle_names_itself_as_the_next_member() {
    let error = oce_graph::BuildError::BlockAlgebraicLoop {
        members: members(&["urn:m#solo"]),
    };
    let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Schedule);
    let _ = capture.build_refusal(error);
    let receipt = capture.finish();
    assert_eq!(receipt.records().len(), 1);
    assert!(
        receipt.records()[0].message().starts_with(
            "block-granularity algebraic loop member 1 of 1; next member: urn:m#solo. "
        ),
        "{}",
        receipt.records()[0].message()
    );
}

#[test]
fn only_connector_cycles_are_reported_as_cdl_violations() {
    // A connector-level cycle breaks CDL §7.16; a block-granularity cycle exists only because the
    // engine fires each block atomically, so its record must not send the author hunting for a
    // §7.16 loop. Both stay bounded: the cause text is fixed, whatever the counts and next member.
    let connector = cycle_member_message(DiagCode::AlgebraicLoop, 1, 2, "urn:m#a.y");
    let block = cycle_member_message(DiagCode::BlockAlgebraicLoop, 1, 2, "urn:m#p");
    assert!(
        connector.contains("CDL §7.16 forbids algebraic loops"),
        "{connector}"
    );
    assert!(!connector.contains("CDL-legal"), "{connector}");
    assert!(!block.contains("§7.16"), "{block}");
    assert!(!block.contains("forbids"), "{block}");
    for needle in [
        "may be CDL-legal",
        "no connector-level cycle exists",
        "fires all its outputs at once",
        "split that block",
        "insert a delay",
    ] {
        assert!(block.contains(needle), "missing {needle:?}: {block}");
    }
    let wide = cycle_member_message(DiagCode::BlockAlgebraicLoop, 999_999, 1_000_000, "");
    let narrow = cycle_member_message(DiagCode::BlockAlgebraicLoop, 1, 1, "");
    assert_eq!(
        wide.len() - narrow.len(),
        "999999".len() - 1 + "1000000".len() - 1
    );
    // 290 fixed bytes plus the two one-digit counts; the receipt tests' per-record overhead bound
    // (`MESSAGE_OVERHEAD_BYTES` in `algebraic_loop_diagnostics.rs`) is sized from this length.
    assert_eq!(narrow.len(), 290 + 2, "{narrow}");
}

#[test]
fn loop_refusal_without_receipt_capture_keeps_no_evidence() {
    let error = oce_graph::BuildError::AlgebraicLoop {
        members: members(&["urn:m#a.y", "urn:m#a.u2"]),
    };
    let mut capture = DiagnosticCapture::new(false, DiagnosticStage::Schedule);
    let refused = capture.build_refusal(error.clone());
    assert!(matches!(refused, OcError::Build(_)));
    assert!(capture.records.is_none());
    assert!(capture.finish().records().is_empty());
}

#[test]
fn connector_loop_records_sort_by_subject_not_by_cycle_order() {
    let error = oce_graph::BuildError::AlgebraicLoop {
        members: members(&["urn:m#a.y", "urn:m#a.u2"]),
    };
    let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Schedule);
    let refused = capture.build_refusal(error.clone());
    assert!(matches!(refused, OcError::Build(_)));
    let receipt = capture.finish();
    let subjects: Vec<_> = receipt
        .records()
        .iter()
        .map(|record| (record.key().stage(), record.key().subject().clone()))
        .collect();
    assert_eq!(
        subjects,
        [
            (
                DiagnosticStage::Schedule,
                DiagnosticSubject::Opaque("urn:m#a.u2".into())
            ),
            (
                DiagnosticStage::Schedule,
                DiagnosticSubject::Opaque("urn:m#a.y".into())
            ),
        ]
    );
}
