//! Serialized admission boundary controls. Expected sizes are independently byte-counted;
//! parser allocation is measured using the existing thread-local allocator seam.

mod support;

use std::{error::Error as _, sync::Arc};

use allocation_counter::measure;
use oce_api::{DiagnosticStage, Engine, MAX_CXF_BYTES, OcError};
use support::recording_store::{RecordingStore, StoreCallSnapshot};

const LIMIT: usize = 8 * 1024 * 1024;
const MODEL: &[u8] = include_bytes!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

#[test]
fn oversized_valid_document_refuses_without_parser_allocation_or_store_calls() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    store.reset_calls();
    let mut bytes = MODEL.to_vec();
    bytes.resize(LIMIT + 1, b' ');
    let mut result = None;
    let allocations = measure(|| result = Some(engine.load_cxf(&bytes)));
    let error = result
        .unwrap()
        .expect_err("oversize admission must refuse before parsing");
    assert_oversize(&error, LIMIT + 1, LIMIT);
    assert_eq!(allocations.count_total, 0, "{allocations:?}");
    assert_eq!(store.calls(), StoreCallSnapshot::default());
}

fn assert_oversize(error: &OcError, actual: usize, limit: usize) {
    assert!(
        matches!(error, OcError::CxfTooLarge { actual_bytes, limit_bytes }
        if *actual_bytes == actual && *limit_bytes == limit),
        "{error:?}"
    );
    assert!(error.source().is_none());
    assert!(error.diagnostics().is_empty());
    assert_eq!(error.all_diagnostics().count(), 0);
}

#[test]
fn inclusive_default_and_stricter_boundaries_preserve_legacy_acceptance() {
    assert_eq!(MAX_CXF_BYTES, LIMIT);
    for limit in [LIMIT, MODEL.len() + 1] {
        for size in [limit - 1, limit, limit + 1] {
            let mut bytes = MODEL.to_vec();
            bytes.resize(size, b' ');
            for receipt in [false, true] {
                let mut engine = Engine::in_memory();
                assert_eq!(engine.cxf_byte_limit(), LIMIT);
                engine.set_cxf_byte_limit(limit).unwrap();
                let result = if receipt {
                    engine
                        .load_cxf_with_receipt(&bytes)
                        .map(|_| ())
                        .map_err(|failure| {
                            assert_eq!(failure.stage(), DiagnosticStage::Import);
                            assert!(failure.diagnostics().records().is_empty());
                            failure.into_error()
                        })
                } else {
                    engine.load_cxf(&bytes).map(|_| ())
                };
                if size > limit {
                    assert_oversize(&result.unwrap_err(), size, limit);
                } else {
                    result.unwrap();
                    assert_eq!(engine.export_cxf().unwrap().bytes, {
                        let mut reference = Engine::in_memory();
                        reference.load_cxf(MODEL).unwrap();
                        reference.export_cxf().unwrap().bytes
                    });
                }
                assert_eq!(engine.cxf_byte_limit(), limit);
            }
        }
    }
}

#[test]
fn invalid_configuration_is_typed_and_never_silently_widens() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    engine.set_cxf_byte_limit(17).unwrap();
    for actual in [LIMIT + 1, usize::MAX] {
        let error = engine.set_cxf_byte_limit(actual).unwrap_err();
        assert!(
            matches!(error, OcError::CxfByteLimitTooLarge { actual_bytes, limit_bytes }
            if actual_bytes == actual && limit_bytes == LIMIT)
        );
        assert!(error.source().is_none());
        assert_eq!(error.all_diagnostics().count(), 0);
        assert_eq!(engine.cxf_byte_limit(), 17);
    }
    engine.set_cxf_byte_limit(0).unwrap();
    assert_oversize(&engine.load_cxf(b"x").unwrap_err(), 1, 0);
    assert!(matches!(engine.load_cxf(b""), Err(OcError::Cxf(_))));
    assert_eq!(store.calls(), StoreCallSnapshot::default());
}

#[test]
fn oversize_receipts_allocate_only_the_error_box_and_are_deterministic() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    // An allocating malformed JSON prefix distinguishes admission from parser refusal.
    let mut bytes = b"{\"@graph\":[{\"@id\":\"untrusted-payload\"},".to_vec();
    bytes.resize(LIMIT + 1, b' ');
    store.arm_hot_path_guard();
    for _ in 0..2 {
        let mut result = None;
        let allocations = measure(|| result = Some(engine.load_cxf_with_receipt(&bytes)));
        let failure = result.unwrap().unwrap_err();
        // Existing OperationFailure owns one Box<OcError>; it is not parser traffic.
        assert_eq!(allocations.count_total, 1, "{allocations:?}");
        assert_eq!(
            allocations.bytes_total,
            std::mem::size_of::<OcError>() as u64
        );
        assert_oversize(failure.error(), bytes.len(), LIMIT);
        assert_oversize(
            failure.source().unwrap().downcast_ref::<OcError>().unwrap(),
            bytes.len(),
            LIMIT,
        );
        assert_eq!(failure.stage(), DiagnosticStage::Import);
        assert!(failure.diagnostics().records().is_empty());
        assert_eq!(
            format!("{}\n", failure.error()),
            include_str!("fixtures/admission/refusal.txt")
        );
        assert_eq!(store.calls(), StoreCallSnapshot::default());
    }
    // Direct parser control over the same prefix (bounded to a few bytes), not a shipping hook.
    let allocations = measure(|| {
        assert!(matches!(
            oce_cxf::parse_document(&bytes[..40]),
            Err(oce_cxf::CxfError::Json(_))
        ));
    });
    assert!(allocations.count_total > 0 && allocations.bytes_total > 0);
}

#[test]
fn empty_and_hostile_corpus_below_cap_keep_json_errors_and_import_stage() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    let corpus = include_str!("fixtures/admission/malformed.txt");
    for input in std::iter::once("").chain(corpus.lines()) {
        let error = engine.load_cxf(input.as_bytes()).unwrap_err();
        assert!(
            matches!(&error, OcError::Cxf(oce_cxf::CxfError::Json(_))),
            "{error:?}"
        );
        assert!(error.source().is_some());
        let failure = engine.load_cxf_with_receipt(input.as_bytes()).unwrap_err();
        assert_eq!(failure.stage(), DiagnosticStage::Import);
        assert!(matches!(
            failure.error(),
            OcError::Cxf(oce_cxf::CxfError::Json(_))
        ));
        assert!(failure.diagnostics().records().is_empty());
    }
    assert_eq!(store.calls(), StoreCallSnapshot::default());
}

/// Bounded manual observation, not a timing assertion or a resource-safety certification.
#[test]
#[ignore = "explicit bounded parser resource observation"]
#[allow(clippy::print_stdout)]
fn parser_resource_observation() {
    let mut padded = MODEL.to_vec();
    padded.resize(LIMIT, b' ');
    let mut payload = b"{\"@context\":{},\"@graph\":[],\"description\":\"".to_vec();
    payload.resize(LIMIT - 2, b'x');
    payload.extend_from_slice(b"\"}");
    let mut nodes = b"{\"@context\":{},\"@graph\":[".to_vec();
    for n in 0..4096 {
        if n != 0 {
            nodes.push(b',');
        }
        nodes.extend_from_slice(format!("{{\"@id\":\"test:node-{n}\"}}").as_bytes());
    }
    nodes.extend_from_slice(b"]}");
    nodes.resize(LIMIT, b' ');
    let mut malformed = payload.clone();
    malformed[LIMIT - 1] = b'!';
    for (name, bytes, valid) in [
        ("representative", MODEL, true),
        ("cap_whitespace", padded.as_slice(), true),
        ("cap_string", payload.as_slice(), true),
        ("cap_nodes", nodes.as_slice(), true),
        ("cap_malformed", malformed.as_slice(), false),
    ] {
        let stats = measure(|| {
            let result = oce_cxf::parse_document(bytes);
            assert_eq!(result.is_ok(), valid, "{name}");
        });
        // Only this explicitly selected observational test emits measurements.
        println!("{name}: bytes={} {stats:?}", bytes.len());
    }
}
