//! Hand-assembled canonical receipts from public facts, not executable identity or numeric oracles.

use oce_api::{
    CatalogContentId, CompatibilityDescriptor, CompatibilityMismatch, CompleteExportContentId,
    ContentIdError, ContractDomain, Engine, Value, contract_descriptors,
};

const MINIMAL: &[u8] = include_bytes!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const DEFERRED: &[u8] = include_bytes!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);

#[test]
fn canonical_public_facts_match_the_hand_assembled_golden_and_repeat() {
    // Fields/order come from the descriptor contract. Catalog ID is the existing independently
    // checked catalog golden; revisions and package version are public source facts.
    let expected = include_str!("fixtures/compatibility.txt");
    for _ in 0..3 {
        assert_eq!(
            CompatibilityDescriptor::current(None).unwrap().to_string(),
            expected
        );
    }
}

#[test]
fn partial_exports_refuse_instead_of_becoming_absent_or_complete_content() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(DEFERRED).unwrap();
    let mut report = engine.export_cxf().unwrap();
    assert_eq!(report.warnings.len(), 2);
    for count in [2, 1] {
        report.warnings.truncate(count);
        assert!(matches!(
            CompatibilityDescriptor::current(Some(&report)),
            Err(ContentIdError::Incomplete { warning_count, .. }) if warning_count == count
        ));
    }
}

#[test]
fn typed_fields_project_only_the_existing_public_contract_facts() {
    fn owned_thread_safe<T: Clone + Send + Sync + 'static>() {}
    owned_thread_safe::<CompatibilityDescriptor>();
    owned_thread_safe::<CatalogContentId>();
    owned_thread_safe::<CompleteExportContentId>();
    let descriptor = CompatibilityDescriptor::current(None).unwrap();
    let revision = |domain| {
        contract_descriptors()
            .iter()
            .find(|d| d.domain == domain)
            .unwrap()
            .revision
    };
    assert_eq!(descriptor.revision(), 1);
    assert_eq!(
        descriptor.catalog_schema_revision(),
        revision(ContractDomain::Catalog)
    );
    assert_eq!(
        descriptor.io_schema_revision(),
        revision(ContractDomain::Io)
    );
    assert_eq!(
        descriptor.value_schema_revision(),
        revision(ContractDomain::Values)
    );
    assert_eq!(
        descriptor.parameter_schema_revision(),
        revision(ContractDomain::Parameters)
    );
    assert_eq!(
        descriptor.execution_profile_schema_revision(),
        revision(ContractDomain::ExecutionProfile)
    );
    assert_eq!(descriptor.execution_profile(), "HostTick-v1");
    assert_eq!(descriptor.oce_api_version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(
        descriptor.catalog_content_id().as_str(),
        oce_api::catalog_content_id(oce_api::catalog())
    );
    assert_eq!(
        descriptor.catalog_content_id().to_string(),
        descriptor.catalog_content_id().as_str()
    );
    assert_eq!(descriptor.export_content_id(), None);
    assert_eq!(
        contract_descriptors().len(),
        7,
        "no new domain or revised old artifact"
    );
}

#[test]
fn complete_export_receipts_match_the_existing_content_golden_and_repeat() {
    // Export ID is the unchanged independently verified minimal-loop golden in export_cxf.rs.
    let expected = include_str!("fixtures/compatibility_export.txt");
    for _ in 0..3 {
        let mut engine = Engine::in_memory();
        engine.load_cxf(MINIMAL).unwrap();
        let report = engine.export_cxf().unwrap();
        let descriptor = CompatibilityDescriptor::current(Some(&report)).unwrap();
        assert_eq!(descriptor.to_string(), expected);
        let id: &CompleteExportContentId = descriptor.export_content_id().unwrap();
        assert_eq!(id.as_str(), report.content_id_complete().unwrap());
        assert_eq!(id.to_string(), id.as_str());
        assert_eq!(descriptor.check_compatible(&descriptor.clone()), Ok(()));
        assert_eq!(expected.lines().count(), 10);
        assert!(
            expected.len() < 320,
            "compact artifact, not the catalog/export itself"
        );
        assert!(!expected.contains('\r'));
        assert!(!expected.contains("fingerprint"));
        assert!(!expected.contains("generation"));
    }
}

#[test]
fn absent_present_and_changed_exports_have_distinct_typed_outcomes() {
    use CompatibilityMismatch::{ExportContent, ExportPresence};
    let absent = CompatibilityDescriptor::current(None).unwrap();
    let mut engine = Engine::in_memory();
    let load = engine.load_cxf(MINIMAL).unwrap();
    let report = engine.export_cxf().unwrap();
    let before = CompatibilityDescriptor::current(Some(&report)).unwrap();
    assert_eq!(absent.check_compatible(&before), Err(ExportPresence));
    assert_eq!(before.check_compatible(&absent), Err(ExportPresence));
    assert_eq!(
        absent.check_compatible(&CompatibilityDescriptor::current(None).unwrap()),
        Ok(())
    );
    engine.halt().unwrap();
    engine
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .unwrap();
    engine.resume().unwrap();
    let after = CompatibilityDescriptor::current(Some(&engine.export_cxf().unwrap())).unwrap();
    assert_eq!(before.check_compatible(&after), Err(ExportContent));
    assert_eq!(after.check_compatible(&before), Err(ExportContent));
    assert_ne!(before.to_string(), after.to_string());
    assert_eq!(before.catalog_content_id(), after.catalog_content_id());
    assert_eq!(load.model_id.as_str(), "http://example.org#MinLoop");
}

#[test]
fn descriptor_capture_preserves_state_and_survives_report_and_engine_lifetimes() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MINIMAL).unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    let mut report = engine.export_cxf().unwrap();
    let captured = CompatibilityDescriptor::current(Some(&report)).unwrap();
    let canonical = captured.to_string();
    assert_eq!(
        engine.state_snapshot().unwrap().into_bytes(),
        snapshot.into_bytes()
    );
    let plan = engine
        .prepare_frame(
            0.0,
            &[("http://example.org#MinLoop.uSet", Value::Real(1.0))],
        )
        .unwrap();
    engine.execute_frame(plan).unwrap();
    let after_frame =
        CompatibilityDescriptor::current(Some(&engine.export_cxf().unwrap())).unwrap();
    assert_eq!(
        captured.check_compatible(&after_frame),
        Ok(()),
        "state and sequence are excluded"
    );
    engine.load_cxf(MINIMAL).unwrap();
    let after_reload =
        CompatibilityDescriptor::current(Some(&engine.export_cxf().unwrap())).unwrap();
    assert_eq!(
        captured.check_compatible(&after_reload),
        Ok(()),
        "incarnation is excluded"
    );
    report.bytes.clear();
    drop(report);
    drop(engine);
    assert_eq!(captured.to_string(), canonical);
    assert_eq!(captured.check_compatible(&captured.clone()), Ok(()));
}

#[test]
fn report_bytes_are_hashed_exactly_without_claiming_validation_or_provenance() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MINIMAL).unwrap();
    let mut report = engine.export_cxf().unwrap();
    // Standard FNV-1a-128 offset (empty input) and independently documented abc vector.
    // A host can edit ExportReport; the new descriptor deliberately inherits that boundary.
    for (bytes, expected) in [
        (
            b"".as_slice(),
            "cxf:fnv1a128:6c62272e07bb014262b821756295c58d",
        ),
        (
            b"abc".as_slice(),
            "cxf:fnv1a128:a68d622cec8b5822836dbc7977af7f3b",
        ),
    ] {
        report.bytes = bytes.to_vec();
        let descriptor = CompatibilityDescriptor::current(Some(&report)).unwrap();
        assert_eq!(descriptor.export_content_id().unwrap().as_str(), expected);
        assert!(
            descriptor
                .to_string()
                .ends_with(&format!("export:{expected}\n"))
        );
    }
    let reference = CompatibilityDescriptor::current(Some(&report)).unwrap();
    for bytes in [b"abc\n".as_slice(), b"\0abc", b"abd", b"abc\xff", b"ab"] {
        report.bytes = bytes.to_vec();
        let changed = CompatibilityDescriptor::current(Some(&report)).unwrap();
        assert_eq!(
            reference.check_compatible(&changed),
            Err(CompatibilityMismatch::ExportContent)
        );
    }
}
