//! Hand-assembled canonical receipts from public facts, not executable identity or numeric oracles.

use oce_api::{
    CatalogContentId, CompatibilityDescriptor, CompatibilityMismatch, ContentId, ContractDomain,
    Engine, Value, contract_descriptors,
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
        assert_eq!(CompatibilityDescriptor::current(None).to_string(), expected);
    }
}

#[test]
fn a_partial_export_does_not_withhold_the_content_line() {
    // The id names the loaded model, so a model whose export is partial still binds a receipt.
    let mut engine = Engine::new();
    engine.load_cxf(DEFERRED).unwrap();
    assert!(engine.export_cxf().unwrap().complete().is_err());
    let id = engine.content_id().unwrap();
    let descriptor = CompatibilityDescriptor::current(Some(&id));
    assert_eq!(descriptor.content_id(), Some(&id));
    assert!(descriptor.to_string().ends_with(&format!("content:{id}\n")));
}

#[test]
fn typed_fields_project_only_the_existing_public_contract_facts() {
    fn owned_thread_safe<T: Clone + Send + Sync + 'static>() {}
    owned_thread_safe::<CompatibilityDescriptor>();
    owned_thread_safe::<CatalogContentId>();
    owned_thread_safe::<ContentId>();
    let descriptor = CompatibilityDescriptor::current(None);
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
    assert_eq!(descriptor.content_id(), None);
    assert_eq!(
        contract_descriptors().len(),
        7,
        "no new domain or revised old artifact"
    );
}

#[test]
fn content_receipts_match_the_hand_assembled_golden_and_repeat() {
    // The content id is the minimal-loop canonical id pinned in export_cxf.rs.
    let expected = include_str!("fixtures/compatibility_export.txt");
    for _ in 0..3 {
        let mut engine = Engine::new();
        engine.load_cxf(MINIMAL).unwrap();
        let minted = engine.content_id().unwrap();
        let descriptor = CompatibilityDescriptor::current(Some(&minted));
        assert_eq!(descriptor.to_string(), expected);
        let id: &ContentId = descriptor.content_id().unwrap();
        assert_eq!(id, &minted);
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
fn absent_present_and_changed_content_ids_have_distinct_typed_outcomes() {
    use CompatibilityMismatch::{ContentIdMismatch, ContentIdPresence};
    let absent = CompatibilityDescriptor::current(None);
    let mut engine = Engine::new();
    let (load, _) = engine.load_cxf(MINIMAL).unwrap();
    let before = CompatibilityDescriptor::current(Some(&engine.content_id().unwrap()));
    assert_eq!(absent.check_compatible(&before), Err(ContentIdPresence));
    assert_eq!(before.check_compatible(&absent), Err(ContentIdPresence));
    assert_eq!(
        absent.check_compatible(&CompatibilityDescriptor::current(None)),
        Ok(())
    );
    engine.halt().unwrap();
    engine
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .unwrap();
    engine.resume().unwrap();
    let after = CompatibilityDescriptor::current(Some(&engine.content_id().unwrap()));
    assert_eq!(before.check_compatible(&after), Err(ContentIdMismatch));
    assert_eq!(after.check_compatible(&before), Err(ContentIdMismatch));
    assert_ne!(before.to_string(), after.to_string());
    assert_eq!(before.catalog_content_id(), after.catalog_content_id());
    assert_eq!(load.model_id.as_str(), "http://example.org#MinLoop");
}

#[test]
fn descriptor_capture_preserves_state_and_survives_id_and_engine_lifetimes() {
    let mut engine = Engine::new();
    engine.load_cxf(MINIMAL).unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    let id = engine.content_id().unwrap();
    let captured = CompatibilityDescriptor::current(Some(&id));
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
    let after_frame = CompatibilityDescriptor::current(Some(&engine.content_id().unwrap()));
    assert_eq!(
        captured.check_compatible(&after_frame),
        Ok(()),
        "state and sequence are excluded"
    );
    engine.load_cxf(MINIMAL).unwrap();
    let after_reload = CompatibilityDescriptor::current(Some(&engine.content_id().unwrap()));
    assert_eq!(
        captured.check_compatible(&after_reload),
        Ok(()),
        "incarnation is excluded"
    );
    drop(id);
    drop(engine);
    assert_eq!(captured.to_string(), canonical);
    assert_eq!(captured.check_compatible(&captured.clone()), Ok(()));
}

#[test]
fn a_stored_content_line_parses_back_to_the_same_receipt() {
    let mut engine = Engine::new();
    engine.load_cxf(MINIMAL).unwrap();
    let reference = CompatibilityDescriptor::current(Some(&engine.content_id().unwrap()));
    let text = reference.to_string();
    let line = text.lines().last().unwrap();
    let stored = line.strip_prefix("content:").unwrap();
    let parsed = ContentId::parse(stored).unwrap();
    let rebuilt = CompatibilityDescriptor::current(Some(&parsed));
    assert_eq!(rebuilt.to_string(), text);
    assert_eq!(reference.check_compatible(&rebuilt), Ok(()));
    // One flipped digit is another id, and the receipts refuse each other.
    let mut flipped = stored.as_bytes().to_vec();
    let last = flipped.len() - 1;
    flipped[last] = if flipped[last] == b'0' { b'1' } else { b'0' };
    let other = ContentId::parse(std::str::from_utf8(&flipped).unwrap()).unwrap();
    assert_eq!(
        reference.check_compatible(&CompatibilityDescriptor::current(Some(&other))),
        Err(CompatibilityMismatch::ContentIdMismatch)
    );
}
