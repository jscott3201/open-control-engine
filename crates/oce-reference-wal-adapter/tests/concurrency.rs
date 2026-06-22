//! Concurrent port-use tests for the reference WAL adapter.
//!
//! Lock-poison mapping is implemented at every lock acquisition by `map_err(backend_err)` in the
//! adapter. The locks are private and no public API intentionally panics while holding one, so there is
//! no portable integration-test path to poison them without adding test-only backdoors.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use oce_reference_wal_adapter::ReferenceWalStore;
use oce_store::{
    DomainKey, Durability, Durable, OcValue, PointSample, PointStatus, PointStore, PointWrite,
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

const THREADS: usize = 8;
const WRITES_PER_THREAD: usize = 16;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-concurrency-{}-{}-{name}",
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

fn sample(value: i64, at_unix_nanos: u64) -> PointSample {
    PointSample {
        value: OcValue::Int(value),
        status: PointStatus::Ok,
        at_unix_nanos,
    }
}

fn write(thread_idx: usize, iter: usize) -> PointWrite {
    let value = (thread_idx as i64 * 1_000) + iter as i64;
    PointWrite {
        key: DomainKey::new(format!("thread.{thread_idx}.point.{iter}")),
        sample: sample(value, value as u64),
        durability: if iter.is_multiple_of(2) {
            Durability::Critical
        } else {
            Durability::Telemetry
        },
    }
}

fn assert_sample(actual: Option<PointSample>, expected: &PointSample, key: &DomainKey) {
    let actual = actual.unwrap_or_else(|| panic!("missing sample for {}", key.as_str()));
    assert_eq!(actual.value, expected.value);
    assert_eq!(actual.status, expected.status);
    assert_eq!(actual.at_unix_nanos, expected.at_unix_nanos);
}

#[test]
fn concurrent_writes_snapshots_and_commits_preserve_accepted_samples() {
    let dir = TestDir::new("mixed-ops");
    let store = Arc::new(ReferenceWalStore::open(dir.path()).expect("open store"));

    let mut handles = Vec::new();
    for thread_idx in 0..THREADS {
        let store = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            let mut accepted = Vec::new();
            for iter in 0..WRITES_PER_THREAD {
                let write = write(thread_idx, iter);
                store
                    .write_points(std::slice::from_ref(&write))
                    .expect("write point");
                if iter.is_multiple_of(4) {
                    let snapshot = store.snapshot().expect("snapshot during concurrent writes");
                    assert_sample(snapshot.read_by_key(&write.key), &write.sample, &write.key);
                }
                if iter.is_multiple_of(7) {
                    store.commit().expect("commit during concurrent writes");
                }
                accepted.push((write.key, write.sample));
            }
            accepted
        }));
    }

    let mut expected = Vec::new();
    for handle in handles {
        expected.extend(handle.join().expect("worker thread"));
    }

    store.commit().expect("final commit");
    let snapshot = store.snapshot().expect("snapshot after joins");
    for (key, sample) in &expected {
        assert_sample(snapshot.read_by_key(key), sample, key);
    }
    drop(snapshot);
    drop(store);

    let reopened = ReferenceWalStore::open(dir.path()).expect("reopen store");
    let recovered = reopened.snapshot().expect("snapshot after reopen");
    for (key, sample) in &expected {
        assert_sample(recovered.read_by_key(key), sample, key);
    }
}
