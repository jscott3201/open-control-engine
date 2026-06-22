//! Direct port-behavior tests for the verification-only reference WAL adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_reference_wal_adapter::{
    ReferenceWalOptions, ReferenceWalStore, TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT,
};
use oce_store::{
    BlockClassDto, BlockInstanceDto, BlockKind, ConnectionDto, DomainKey, Durable, EquipmentDto,
    ModelStore, OcValue, PointDirection, PointDto, PointHandle, PointSample, PointStatus,
    PointStore, PointType, PointValueType, RelationDto, RelationKind, ResolvedModel,
    SemanticPayloadDto, SemanticQuery, SemanticStore, Store, StoreError, TrendInterval,
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-{}-{}-{name}",
            std::process::id(),
            idx
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn sample(value: OcValue, at_unix_nanos: u64) -> PointSample {
    PointSample {
        value,
        status: PointStatus::Ok,
        at_unix_nanos,
    }
}

fn write(key: &str, value: OcValue, durability: oce_store::Durability) -> oce_store::PointWrite {
    oce_store::PointWrite {
        key: DomainKey::new(key),
        sample: sample(value, 100),
        durability,
    }
}

fn read_by_key(store: &ReferenceWalStore, key: &str) -> Option<PointSample> {
    store
        .snapshot()
        .expect("snapshot")
        .read_by_key(&DomainKey::new(key))
}

fn model(id: &str) -> ResolvedModel {
    ResolvedModel {
        model_id: DomainKey::new(id),
        schema_rev: 1,
        classes: vec![BlockClassDto {
            class_iri: "CDL.Reals.PID".to_owned(),
            kind: BlockKind::Elementary,
            is_library: true,
            shape_json: "{}".to_owned(),
        }],
        blocks: vec![BlockInstanceDto {
            key: DomainKey::new("ahu.pid"),
            class_iri: "CDL.Reals.PID".to_owned(),
            params: vec![("k".to_owned(), OcValue::Real(1.0))],
            config_json: "{}".to_owned(),
        }],
        points: vec![
            PointDto {
                key: DomainKey::new("ahu.pid.u"),
                owner_block: DomainKey::new("ahu.pid"),
                direction: PointDirection::In,
                value_type: PointValueType::Real,
                point_type: PointType::Ai,
                hardwired: true,
                trend_interval_s: TrendInterval::OnChange,
                trend_enable: true,
                quantity: Some("Temperature".to_owned()),
                unit: Some("K".to_owned()),
                display_unit: Some("degC".to_owned()),
                description: Some("PID input".to_owned()),
                in_pointlist: true,
                controlled_device: Some("ahu".to_owned()),
                search_text: None,
                tags_json: None,
                embedding: None,
            },
            PointDto {
                key: DomainKey::new("ahu.pid.y"),
                owner_block: DomainKey::new("ahu.pid"),
                direction: PointDirection::Out,
                value_type: PointValueType::Real,
                point_type: PointType::Ao,
                hardwired: false,
                trend_interval_s: TrendInterval::EverySeconds(60),
                trend_enable: true,
                quantity: Some("ControlSignal".to_owned()),
                unit: Some("1".to_owned()),
                display_unit: None,
                description: Some("PID output".to_owned()),
                in_pointlist: true,
                controlled_device: Some("ahu".to_owned()),
                search_text: Some("pid output".to_owned()),
                tags_json: Some(r#"{"cmd":true}"#.to_owned()),
                embedding: None,
            },
        ],
        connections: vec![ConnectionDto {
            from_point: DomainKey::new("ahu.pid.y"),
            to_point: DomainKey::new("ahu.pid.u"),
        }],
        containment: Vec::new(),
    }
}

fn assert_same_model(left: &ResolvedModel, right: &ResolvedModel) {
    let left_json = serde_json::to_string(left).expect("left model json");
    let right_json = serde_json::to_string(right).expect("right model json");
    assert_eq!(left_json, right_json);
}

fn assert_value_eq(actual: &OcValue, expected: &OcValue) {
    match (actual, expected) {
        (OcValue::Real(a), OcValue::Real(e)) => assert_eq!(a.to_bits(), e.to_bits()),
        _ => assert_eq!(actual, expected),
    }
}

fn assert_sample_eq(actual: &PointSample, expected: &PointSample) {
    assert_value_eq(&actual.value, &expected.value);
    assert_eq!(actual.status, expected.status);
    assert_eq!(actual.at_unix_nanos, expected.at_unix_nanos);
}

fn assert_unsupported<T: std::fmt::Debug>(result: Result<T, StoreError>, expected: &'static str) {
    match result {
        Err(StoreError::Unsupported(actual)) => assert_eq!(actual, expected),
        other => panic!("expected Unsupported({expected}), got {other:?}"),
    }
}

fn assert_retrieval_unsupported<T: std::fmt::Debug>(
    result: Result<T, StoreError>,
    expected: &'static str,
) {
    match result {
        Err(StoreError::RetrievalUnsupported(actual)) => assert_eq!(actual, expected),
        other => panic!("expected RetrievalUnsupported({expected}), got {other:?}"),
    }
}

fn _assert_is_store<S: Store>() {}

#[test]
fn reference_wal_store_satisfies_the_store_umbrella() {
    _assert_is_store::<ReferenceWalStore>();
}

#[test]
fn critical_write_survives_reopen_without_commit() {
    let dir = TestDir::new("critical-survives");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let expected = sample(OcValue::Real(42.5), 100);
    store
        .write_points(&[oce_store::PointWrite {
            key: DomainKey::new("point:critical"),
            sample: expected.clone(),
            durability: oce_store::Durability::Critical,
        }])
        .expect("critical write");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    assert_sample_eq(
        &read_by_key(&reopened, "point:critical").expect("critical sample"),
        &expected,
    );
}

#[test]
fn telemetry_is_lost_without_flush_and_survives_flush_or_commit() {
    let dir = TestDir::new("telemetry-tiers");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .write_points(&[write(
            "point:telemetry:lost",
            OcValue::Real(1.0),
            oce_store::Durability::Telemetry,
        )])
        .expect("telemetry write");
    assert_eq!(store.current_ordering_high_water(), 0);
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    assert!(read_by_key(&reopened, "point:telemetry:lost").is_none());
    assert_eq!(reopened.current_ordering_high_water(), 0);

    reopened
        .write_points(&[write(
            "point:telemetry:flushed",
            OcValue::Real(2.0),
            oce_store::Durability::Telemetry,
        )])
        .expect("telemetry write");
    reopened.flush().expect("flush telemetry");
    assert_eq!(reopened.current_ordering_high_water(), 1);
    drop(reopened);

    let committed = ReferenceWalStore::open(dir.path()).expect("reopen after flush");
    assert!(read_by_key(&committed, "point:telemetry:flushed").is_some());
    assert_eq!(committed.current_ordering_high_water(), 1);
    committed
        .write_points(&[write(
            "point:telemetry:committed",
            OcValue::Real(3.0),
            oce_store::Durability::Telemetry,
        )])
        .expect("telemetry write before commit");
    assert_eq!(committed.current_ordering_high_water(), 1);
    committed.commit().expect("commit telemetry");
    assert_eq!(committed.current_ordering_high_water(), 2);
    drop(committed);

    let recovered = ReferenceWalStore::open(dir.path()).expect("reopen after commit");
    assert!(read_by_key(&recovered, "point:telemetry:flushed").is_some());
    assert!(read_by_key(&recovered, "point:telemetry:committed").is_some());
    assert_eq!(recovered.current_ordering_high_water(), 2);
}

#[test]
fn model_round_trip_and_idempotent_delete_are_snapshot_durable() {
    let dir = TestDir::new("model-round-trip");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let expected = model("model:ahu");
    store.save_model(&expected).expect("save model");
    store.commit().expect("commit model");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    assert_same_model(
        &reopened.load_model(&expected.model_id).expect("load model"),
        &expected,
    );
    assert_eq!(
        reopened.list_models().expect("list models"),
        vec![expected.model_id.clone()]
    );
    reopened
        .delete_model(&DomainKey::new("model:missing"))
        .expect("delete missing is idempotent");
    reopened
        .delete_model(&expected.model_id)
        .expect("delete existing model");
    reopened.commit().expect("commit deletion");
    drop(reopened);

    let recovered = ReferenceWalStore::open(dir.path()).expect("reopen after delete");
    assert!(matches!(
        recovered.load_model(&expected.model_id),
        Err(StoreError::ModelNotLoaded(_))
    ));
    recovered
        .delete_model(&expected.model_id)
        .expect("delete absent after reopen remains idempotent");
}

#[test]
fn point_handles_are_stable_across_reopen() {
    let dir = TestDir::new("handle-stability");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let a = DomainKey::new("point:a");
    let b = DomainKey::new("point:b");
    assert_eq!(
        store
            .resolve_points(&[a.clone(), b.clone()])
            .expect("resolve points"),
        vec![PointHandle(0), PointHandle(1)]
    );
    store.commit().expect("commit resolved handles");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    assert_eq!(
        reopened
            .resolve_points(&[b, a])
            .expect("resolve after reopen"),
        vec![PointHandle(1), PointHandle(0)]
    );
}

#[test]
fn snapshot_reads_are_total_and_empty_batches_are_noops() {
    let dir = TestDir::new("snapshot-edges");
    let store = ReferenceWalStore::open(dir.path()).expect("open fresh store");
    store.recover().expect("recover fresh store");
    assert_eq!(store.write_points(&[]).expect("empty write"), 0);

    let snapshot = store.snapshot().expect("snapshot");
    assert!(snapshot.read_resolved(PointHandle(0)).is_none());
    assert!(snapshot.read_resolved(PointHandle(1)).is_none());
    assert!(snapshot.read_resolved(PointHandle(u64::MAX)).is_none());
    assert!(
        snapshot
            .read_by_key(&DomainKey::new("point:unknown"))
            .is_none()
    );
}

#[test]
fn critical_partial_failure_recovers_only_the_durable_prefix() {
    let dir = TestDir::new("partial-critical");
    let store = ReferenceWalStore::open_with_options(
        dir.path(),
        ReferenceWalOptions {
            fail_critical_fsync_after: Some(2),
            ..ReferenceWalOptions::default()
        },
    )
    .expect("open store");

    let result = store.write_points(&[
        write(
            "point:critical:0",
            OcValue::Int(0),
            oce_store::Durability::Critical,
        ),
        write(
            "point:critical:1",
            OcValue::Int(1),
            oce_store::Durability::Critical,
        ),
        write(
            "point:critical:2",
            OcValue::Int(2),
            oce_store::Durability::Critical,
        ),
        write(
            "point:critical:3",
            OcValue::Int(3),
            oce_store::Durability::Critical,
        ),
    ]);
    assert!(matches!(result, Err(StoreError::Durability(_))));
    assert_eq!(store.current_ordering_high_water(), 2);
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover prefix");
    assert_eq!(recovered.current_ordering_high_water(), 2);
    assert_eq!(
        read_by_key(&recovered, "point:critical:0")
            .expect("prefix 0")
            .value,
        OcValue::Int(0)
    );
    assert_eq!(
        read_by_key(&recovered, "point:critical:1")
            .expect("prefix 1")
            .value,
        OcValue::Int(1)
    );
    assert!(read_by_key(&recovered, "point:critical:2").is_none());
    assert!(read_by_key(&recovered, "point:critical:3").is_none());

    let wal = fs::read_to_string(dir.path().join("points.wal")).expect("read WAL");
    assert!(wal.ends_with('\n'));
    assert_eq!(wal.lines().count(), 2);
}

#[test]
fn partial_snapshot_temp_file_does_not_corrupt_prior_snapshot() {
    let dir = TestDir::new("snapshot-atomic");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .write_points(&[write(
            "point:stable",
            OcValue::Real(8.0),
            oce_store::Durability::Telemetry,
        )])
        .expect("telemetry write");
    store.commit().expect("commit stable snapshot");
    fs::write(dir.path().join("snapshot.json.tmp"), b"{not valid json")
        .expect("write interrupted temp snapshot");
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("reopen with temp file");
    assert_eq!(
        read_by_key(&recovered, "point:stable")
            .expect("stable point")
            .value,
        OcValue::Real(8.0)
    );
}

#[test]
fn telemetry_group_commit_window_is_bounded_to_the_documented_count() {
    let dir = TestDir::new("telemetry-window");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let pending: Vec<_> = (0..TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT - 1)
        .map(|idx| {
            write(
                &format!("point:pending:{idx}"),
                OcValue::Int(idx as i64),
                oce_store::Durability::Telemetry,
            )
        })
        .collect();
    store.write_points(&pending).expect("pending telemetry");
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen without auto flush");
    assert!(read_by_key(&reopened, "point:pending:0").is_none());
    let auto_flush: Vec<_> = (0..TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT)
        .map(|idx| {
            write(
                &format!("point:auto:{idx}"),
                OcValue::Int(idx as i64),
                oce_store::Durability::Telemetry,
            )
        })
        .collect();
    reopened
        .write_points(&auto_flush)
        .expect("auto-flushing telemetry");
    drop(reopened);

    let recovered = ReferenceWalStore::open(dir.path()).expect("reopen after auto flush");
    assert!(read_by_key(&recovered, "point:pending:0").is_none());
    assert_eq!(
        read_by_key(&recovered, "point:auto:0")
            .expect("auto-flushed first sample")
            .value,
        OcValue::Int(0)
    );
    assert_eq!(
        read_by_key(
            &recovered,
            &format!("point:auto:{}", TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT - 1)
        )
        .expect("auto-flushed last sample")
        .value,
        OcValue::Int((TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT - 1) as i64)
    );
}

#[test]
fn wal_record_encoding_matches_the_byte_stable_golden() {
    let dir = TestDir::new("wal-golden");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .write_points(&[
            oce_store::PointWrite {
                key: DomainKey::new("point:sat"),
                sample: sample(OcValue::Real(-0.0), 123),
                durability: oce_store::Durability::Critical,
            },
            oce_store::PointWrite {
                key: DomainKey::new("point:subnormal"),
                sample: sample(OcValue::Real(f64::from_bits(0x0000_0000_0000_0001)), 124),
                durability: oce_store::Durability::Critical,
            },
        ])
        .expect("write golden record");
    let actual = fs::read_to_string(dir.path().join("points.wal")).expect("read WAL");
    let expected = include_str!("fixtures/critical_real_record.jsonl");
    assert_eq!(actual, expected);
}

#[test]
fn point_sample_round_trip_preserves_bits_and_value_domains() {
    let dir = TestDir::new("sample-roundtrip");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let writes = vec![
        (
            "point:real:nan",
            sample(OcValue::Real(f64::from_bits(0x7ff8_0000_0000_0001)), 1),
        ),
        (
            "point:real:signaling_nan",
            sample(OcValue::Real(f64::from_bits(0x7ff0_0000_0000_0001)), 2),
        ),
        (
            "point:real:pos_inf",
            sample(OcValue::Real(f64::INFINITY), 3),
        ),
        (
            "point:real:neg_inf",
            sample(OcValue::Real(f64::NEG_INFINITY), 4),
        ),
        ("point:real:neg_zero", sample(OcValue::Real(-0.0), 5)),
        (
            "point:real:min_subnormal",
            sample(OcValue::Real(f64::from_bits(0x0000_0000_0000_0001)), 6),
        ),
        (
            "point:real:max_subnormal",
            sample(OcValue::Real(f64::from_bits(0x000f_ffff_ffff_ffff)), 7),
        ),
        (
            "point:real:min_positive",
            sample(OcValue::Real(f64::from_bits(0x0010_0000_0000_0000)), 8),
        ),
        (
            "point:real:max_finite",
            sample(OcValue::Real(f64::from_bits(0x7fef_ffff_ffff_ffff)), 9),
        ),
        (
            "point:real:most_negative_finite",
            sample(OcValue::Real(f64::from_bits(0xffef_ffff_ffff_ffff)), 10),
        ),
        (
            "point:decimal",
            sample(OcValue::Decimal("1234567890.0000000001".to_owned()), 11),
        ),
        ("point:int:min", sample(OcValue::Int(i64::MIN), 12)),
        ("point:int:max", sample(OcValue::Int(i64::MAX), 13)),
        (
            "point:int:past-two-pow-53",
            sample(OcValue::Int(9_007_199_254_740_993), 14),
        ),
        ("point:bool:true", sample(OcValue::Bool(true), 15)),
        ("point:bool:false", sample(OcValue::Bool(false), 16)),
        (
            "point:enum:unicode",
            sample(
                OcValue::Enum {
                    type_iri: "urn:enum:\"operating-mode\"".to_owned(),
                    literal: "wärme\"betrieb".to_owned(),
                },
                17,
            ),
        ),
        (
            "point:string:newline",
            sample(
                OcValue::String("line one\nline two\\nliteral".to_owned()),
                18,
            ),
        ),
    ];
    let point_writes: Vec<_> = writes
        .iter()
        .map(|(key, sample)| oce_store::PointWrite {
            key: DomainKey::new(*key),
            sample: sample.clone(),
            durability: oce_store::Durability::Critical,
        })
        .collect();
    store.write_points(&point_writes).expect("write samples");
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover samples");
    for (key, expected) in writes {
        assert_sample_eq(
            &read_by_key(&recovered, key).unwrap_or_else(|| panic!("missing {key}")),
            &expected,
        );
    }
}

#[test]
fn semantic_store_methods_return_typed_deferral_errors() {
    let dir = TestDir::new("semantic-deferrals");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    let graph_deferred =
        "ReferenceWalStore: semantic graph operations deferred to pointlist-export";
    let retrieval_deferred = "ReferenceWalStore: semantic retrieval deferred to retrieval-recipes";
    let equipment = EquipmentDto {
        key: DomainKey::new("equip:ahu"),
        subtype: Some("AHU".to_owned()),
        tags_json: "{}".to_owned(),
        embedding: None,
    };
    assert_unsupported(store.upsert_equipment(&equipment), graph_deferred);

    let relation = RelationDto {
        src: DomainKey::new("equip:ahu"),
        dst: DomainKey::new("point:sat"),
        kind: RelationKind::HasPoint,
    };
    assert_unsupported(store.add_relation(&relation), graph_deferred);

    let payload = SemanticPayloadDto {
        subject: DomainKey::new("equip:ahu"),
        language: "Brick".to_owned(),
        mime: "text/turtle".to_owned(),
        version: Some("1.3".to_owned()),
        payload: "@prefix brick: <https://brickschema.org/schema/Brick#> .".to_owned(),
    };
    assert_unsupported(store.put_semantic_payload(&payload), graph_deferred);
    assert_unsupported(
        store.get_semantic_payloads(&payload.subject),
        graph_deferred,
    );
    assert_unsupported(store.point_list(Some("ahu")), graph_deferred);
    assert_retrieval_unsupported(
        store.retrieve(&SemanticQuery::FuzzyText {
            query: "supply air temperature".to_owned(),
            k: 3,
        }),
        retrieval_deferred,
    );
    assert_unsupported(store.match_template(&[]), graph_deferred);
}
