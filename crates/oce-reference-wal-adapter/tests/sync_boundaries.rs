//! Sync-boundary durability tests for Critical WAL writes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_reference_wal_adapter::{ReferenceWalOptions, ReferenceWalStore};
use oce_store::{DomainKey, OcValue, PointSample, PointStatus, PointStore, PointWrite, StoreError};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-sync-{}-{}-{name}",
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

fn sample(value: OcValue) -> PointSample {
    PointSample {
        value,
        status: PointStatus::Ok,
        at_unix_nanos: 100,
    }
}

fn critical_write(key: &str, value: OcValue) -> PointWrite {
    PointWrite {
        key: DomainKey::new(key),
        sample: sample(value),
        durability: oce_store::Durability::Critical,
    }
}

fn read_by_key(store: &ReferenceWalStore, key: &str) -> Option<PointSample> {
    store
        .snapshot()
        .expect("snapshot")
        .read_by_key(&DomainKey::new(key))
}

fn assert_durability<T: std::fmt::Debug>(result: Result<T, StoreError>, expected: &str) {
    match result {
        Err(StoreError::Durability(actual)) => assert!(
            actual.contains(expected),
            "expected Durability containing {expected:?}, got {actual:?}"
        ),
        other => panic!("expected Durability containing {expected:?}, got {other:?}"),
    }
}

#[test]
fn first_critical_write_fsyncs_wal_directory_entry() {
    let dir = TestDir::new("critical-dir-fsync");
    let store = ReferenceWalStore::open_with_options(
        dir.path(),
        ReferenceWalOptions {
            fail_critical_directory_sync_after: Some(0),
            ..ReferenceWalOptions::default()
        },
    )
    .expect("open store");

    let result = store.write_points(&[critical_write("point:critical:first", OcValue::Int(7))]);
    assert_durability(result, "simulated directory fsync failure");
    assert_eq!(store.current_ordering_high_water(), 0);
    assert!(read_by_key(&store, "point:critical:first").is_none());
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover after dir fsync failure");
    assert_eq!(recovered.current_ordering_high_water(), 0);
    assert!(read_by_key(&recovered, "point:critical:first").is_none());
    let wal_len = fs::metadata(dir.path().join("points.wal"))
        .expect("WAL file remains after truncating failed first record")
        .len();
    assert_eq!(wal_len, 0);
}

#[test]
fn critical_file_sync_failure_truncates_failed_record_before_recovery() {
    let dir = TestDir::new("critical-file-sync");
    let store = ReferenceWalStore::open_with_options(
        dir.path(),
        ReferenceWalOptions {
            fail_critical_file_sync_after: Some(2),
            ..ReferenceWalOptions::default()
        },
    )
    .expect("open store");

    let result = store.write_points(&[
        critical_write("point:critical:0", OcValue::Int(0)),
        critical_write("point:critical:1", OcValue::Int(1)),
        critical_write("point:critical:2", OcValue::Int(2)),
        critical_write("point:critical:3", OcValue::Int(3)),
    ]);
    assert_durability(result, "simulated file fsync failure");
    assert_eq!(store.current_ordering_high_water(), 2);
    assert!(read_by_key(&store, "point:critical:2").is_none());
    drop(store);

    let recovered = ReferenceWalStore::open(dir.path()).expect("recover file-sync prefix");
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
