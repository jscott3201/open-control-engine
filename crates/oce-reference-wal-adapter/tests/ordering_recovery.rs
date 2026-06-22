//! Durable write-order high-water recovery tests for the reference WAL adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_reference_wal_adapter::{ReferenceWalOptions, ReferenceWalStore};
use oce_store::{
    DomainKey, Durable, OcValue, PointSample, PointStatus, PointStore, PointWrite, StoreError,
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-ordering-{}-{}-{name}",
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

fn write(
    key: &str,
    value: OcValue,
    durability: oce_store::Durability,
    at_unix_nanos: u64,
) -> PointWrite {
    PointWrite {
        key: DomainKey::new(key),
        sample: sample(value, at_unix_nanos),
        durability,
    }
}

fn read_by_key(store: &ReferenceWalStore, key: &str) -> Option<PointSample> {
    store
        .snapshot()
        .expect("snapshot")
        .read_by_key(&DomainKey::new(key))
}

fn assert_durability_exhausted<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::Durability(message)) => assert!(
            message.contains("exhausted"),
            "expected exhausted Durability error, got {message:?}"
        ),
        other => panic!("expected exhausted Durability error, got {other:?}"),
    }
}

fn write_max_high_water_snapshot(dir: &TestDir) {
    fs::write(
        dir.path().join("snapshot.json"),
        r#"{"format_version":1,"ordering_high_water":18446744073709551615,"models":[],"point_keys":["point:seed"],"point_samples":[null]}"#,
    )
    .expect("write max high-water snapshot");
}

#[test]
fn critical_ordering_high_water_survives_kill_recover_and_next_stamp_increases() {
    let dir = TestDir::new("critical-kill-recover");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .write_points(&[write(
            "point:setpoint",
            OcValue::Int(11),
            oce_store::Durability::Critical,
            10,
        )])
        .expect("critical write");
    let pre_kill_stamp = store.current_ordering_high_water();
    assert_eq!(pre_kill_stamp, 1);
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover killed store");
    assert_eq!(
        read_by_key(&recovered, "point:setpoint")
            .expect("critical sample survived")
            .value,
        OcValue::Int(11)
    );
    assert_eq!(recovered.current_ordering_high_water(), pre_kill_stamp);

    recovered
        .write_points(&[write(
            "point:setpoint:after-recover",
            OcValue::Int(12),
            oce_store::Durability::Critical,
            20,
        )])
        .expect("post-recover critical write");
    let post_recover_stamp = recovered.current_ordering_high_water();
    assert!(post_recover_stamp > pre_kill_stamp);
    assert_eq!(post_recover_stamp, 2);
}

#[test]
fn uncommitted_telemetry_does_not_advance_recovered_high_water() {
    let dir = TestDir::new("telemetry-dropped-high-water");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .write_points(&[write(
            "point:critical",
            OcValue::Int(1),
            oce_store::Durability::Critical,
            10,
        )])
        .expect("critical write");
    assert_eq!(store.current_ordering_high_water(), 1);
    store
        .write_points(&[write(
            "point:telemetry:dropped",
            OcValue::Int(2),
            oce_store::Durability::Telemetry,
            20,
        )])
        .expect("pending telemetry write");
    assert!(read_by_key(&store, "point:telemetry:dropped").is_some());
    assert_eq!(store.current_ordering_high_water(), 1);
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover after telemetry drop");
    assert!(read_by_key(&recovered, "point:telemetry:dropped").is_none());
    assert_eq!(recovered.current_ordering_high_water(), 1);

    recovered
        .write_points(&[write(
            "point:telemetry:flushed",
            OcValue::Int(3),
            oce_store::Durability::Telemetry,
            30,
        )])
        .expect("pending telemetry write");
    assert_eq!(recovered.current_ordering_high_water(), 1);
    recovered.flush().expect("flush telemetry");
    assert_eq!(recovered.current_ordering_high_water(), 2);
    drop(recovered);

    let flushed = ReferenceWalStore::open(dir.path()).expect("recover after flush");
    assert_eq!(flushed.current_ordering_high_water(), 2);
    assert_eq!(
        read_by_key(&flushed, "point:telemetry:flushed")
            .expect("flushed telemetry survived")
            .value,
        OcValue::Int(3)
    );
}

#[test]
fn commit_snapshot_persists_ordering_high_water() {
    let dir = TestDir::new("snapshot-high-water");
    let store = ReferenceWalStore::open(dir.path()).expect("open store");
    store
        .write_points(&[
            write(
                "point:critical",
                OcValue::Int(1),
                oce_store::Durability::Critical,
                10,
            ),
            write(
                "point:telemetry",
                OcValue::Int(2),
                oce_store::Durability::Telemetry,
                20,
            ),
        ])
        .expect("write critical and telemetry");
    store.commit().expect("commit snapshot");
    assert_eq!(store.current_ordering_high_water(), 2);
    let snapshot = fs::read_to_string(dir.path().join("snapshot.json")).expect("read snapshot");
    assert_eq!(
        snapshot,
        r#"{"format_version":1,"ordering_high_water":2,"models":[],"point_keys":["point:critical","point:telemetry"],"point_samples":[{"value":{"type":"int","value":1},"status":"Ok","at_unix_nanos":10},{"value":{"type":"int","value":2},"status":"Ok","at_unix_nanos":20}]}"#
    );
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover snapshot");
    assert_eq!(recovered.current_ordering_high_water(), 2);
    recovered
        .write_points(&[write(
            "point:after-snapshot",
            OcValue::Int(3),
            oce_store::Durability::Critical,
            30,
        )])
        .expect("post-snapshot critical write");
    assert_eq!(recovered.current_ordering_high_water(), 3);
}

#[test]
fn failed_torn_critical_write_does_not_advance_high_water() {
    let dir = TestDir::new("failed-critical-high-water");
    let store = ReferenceWalStore::open_with_options(
        dir.path(),
        ReferenceWalOptions {
            fail_critical_fsync_after: Some(1),
            ..ReferenceWalOptions::default()
        },
    )
    .expect("open store");
    store
        .write_points(&[write(
            "point:critical:durable",
            OcValue::Int(1),
            oce_store::Durability::Critical,
            10,
        )])
        .expect("durable critical write");
    assert_eq!(store.current_ordering_high_water(), 1);

    let failed = store.write_points(&[write(
        "point:critical:failed",
        OcValue::Int(2),
        oce_store::Durability::Critical,
        20,
    )]);
    assert!(matches!(failed, Err(StoreError::Durability(_))));
    assert_eq!(store.current_ordering_high_water(), 1);
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover durable prefix");
    assert_eq!(recovered.current_ordering_high_water(), 1);
    assert!(read_by_key(&recovered, "point:critical:failed").is_none());
}

#[test]
fn critical_ordering_overflow_is_a_typed_durability_error() {
    let dir = TestDir::new("critical-overflow");
    write_max_high_water_snapshot(&dir);
    let store = ReferenceWalStore::open(dir.path()).expect("open max high-water store");
    assert_eq!(store.current_ordering_high_water(), u64::MAX);

    assert_durability_exhausted(store.write_points(&[write(
        "point:critical:overflow",
        OcValue::Int(1),
        oce_store::Durability::Critical,
        10,
    )]));
    assert_eq!(store.current_ordering_high_water(), u64::MAX);
    assert!(read_by_key(&store, "point:critical:overflow").is_none());
}

#[test]
fn telemetry_flush_ordering_overflow_is_a_typed_durability_error() {
    let dir = TestDir::new("telemetry-overflow");
    write_max_high_water_snapshot(&dir);
    let store = ReferenceWalStore::open(dir.path()).expect("open max high-water store");
    assert_eq!(store.current_ordering_high_water(), u64::MAX);

    store
        .write_points(&[write(
            "point:telemetry:overflow",
            OcValue::Int(2),
            oce_store::Durability::Telemetry,
            20,
        )])
        .expect("pending telemetry write");
    assert_eq!(store.current_ordering_high_water(), u64::MAX);
    assert_durability_exhausted(store.flush());
    assert_eq!(store.current_ordering_high_water(), u64::MAX);
}
