//! Fail-closed recovery tests for malformed WAL and snapshot files.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use oce_reference_wal_adapter::ReferenceWalStore;
use oce_store::StoreError;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let idx = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "oce-reference-wal-adapter-recovery-{}-{}-{name}",
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

fn assert_open_backend_error(path: &Path, expected: &str) {
    match ReferenceWalStore::open(path) {
        Err(StoreError::Backend(actual)) => assert!(
            actual.contains(expected),
            "expected Backend containing {expected:?}, got {actual:?}"
        ),
        Err(other) => panic!("expected Backend containing {expected:?}, got {other:?}"),
        Ok(_) => panic!("expected Backend containing {expected:?}, got successful open"),
    }
}

fn valid_wal_line(key: &str, value: i64, at_unix_nanos: u64) -> String {
    let template = r#"{"format_version":1,"record":{"kind":"point_write","durability":"Critical","key":"__KEY__","sample":{"value":{"type":"int","value":__VALUE__},"status":"Ok","at_unix_nanos":__AT__}}}"#;
    template
        .replace("__KEY__", key)
        .replace("__VALUE__", &value.to_string())
        .replace("__AT__", &at_unix_nanos.to_string())
}

#[test]
fn mid_stream_corrupt_wal_line_fails_closed_instead_of_skipping() {
    let dir = TestDir::new("mid-stream-corrupt-wal");
    let wal = format!(
        "{}\n{{not valid json}}\n{}\n",
        valid_wal_line("point:before", 1, 10),
        valid_wal_line("point:after", 2, 20)
    );
    fs::write(dir.path().join("points.wal"), wal).expect("write corrupt WAL");

    assert_open_backend_error(dir.path(), "key must be a string");
}

#[test]
fn unsupported_wal_format_version_is_backend_error() {
    let dir = TestDir::new("wal-version");
    let wal = r#"{"format_version":2,"record":{"kind":"point_write","durability":"Critical","key":"point:bad-version","sample":{"value":{"type":"int","value":1},"status":"Ok","at_unix_nanos":10}}}"#;
    fs::write(dir.path().join("points.wal"), format!("{wal}\n")).expect("write WAL");

    assert_open_backend_error(dir.path(), "unsupported WAL format version 2");
}

#[test]
fn invalid_snapshot_json_is_backend_error() {
    let dir = TestDir::new("invalid-snapshot-json");
    fs::write(dir.path().join("snapshot.json"), b"{not valid json")
        .expect("write invalid snapshot");

    assert_open_backend_error(dir.path(), "key must be a string");
}

#[test]
fn unsupported_snapshot_format_version_is_backend_error() {
    let dir = TestDir::new("snapshot-version");
    fs::write(
        dir.path().join("snapshot.json"),
        r#"{"format_version":2,"models":[],"point_keys":[],"point_samples":[]}"#,
    )
    .expect("write snapshot");

    assert_open_backend_error(dir.path(), "unsupported snapshot format version 2");
}

#[test]
fn snapshot_samples_without_keys_are_rejected_before_indexing() {
    let dir = TestDir::new("snapshot-sample-without-key");
    fs::write(
        dir.path().join("snapshot.json"),
        r#"{"format_version":1,"models":[],"point_keys":[],"point_samples":[{"value":{"type":"int","value":1},"status":"Ok","at_unix_nanos":10}]}"#,
    )
    .expect("write snapshot");

    assert_open_backend_error(
        dir.path(),
        "snapshot point sample references missing point key",
    );
}
