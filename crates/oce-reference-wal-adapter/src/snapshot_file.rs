//! Snapshot-file persistence for the reference WAL adapter.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use oce_store::{StoreError, StoreResult};

use crate::codec::{FORMAT_VERSION, SnapshotFile};
use crate::fs_sync::sync_directory;

pub(crate) const SNAPSHOT_FILE: &str = "snapshot.json";
pub(crate) const SNAPSHOT_TEMP_FILE: &str = "snapshot.json.tmp";

pub(crate) fn load(root: &Path) -> StoreResult<Option<SnapshotFile>> {
    let snapshot_path = root.join(SNAPSHOT_FILE);
    if !snapshot_path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(&snapshot_path).map_err(backend_err)?;
    let snapshot: SnapshotFile = serde_json::from_slice(&bytes).map_err(backend_err)?;
    if snapshot.format_version != FORMAT_VERSION {
        return Err(StoreError::Backend(format!(
            "unsupported snapshot format version {}",
            snapshot.format_version
        )));
    }
    Ok(Some(snapshot))
}

pub(crate) fn write_atomic(root: &Path, snapshot: &SnapshotFile) -> StoreResult<()> {
    fs::create_dir_all(root).map_err(backend_err)?;
    let temp_path = root.join(SNAPSHOT_TEMP_FILE);
    let snapshot_path = root.join(SNAPSHOT_FILE);
    let bytes = serde_json::to_vec(snapshot).map_err(backend_err)?;

    {
        let mut file = File::create(&temp_path).map_err(backend_err)?;
        file.write_all(&bytes).map_err(backend_err)?;
        file.sync_all().map_err(durability_err)?;
    }

    fs::rename(&temp_path, &snapshot_path).map_err(backend_err)?;
    sync_directory(root)?;
    Ok(())
}

fn backend_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}

fn durability_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Durability(error.to_string())
}
