//! Filesystem sync helpers shared by WAL and snapshot persistence.

use std::fs::File;
use std::path::Path;

use oce_store::{StoreError, StoreResult};

pub(crate) fn sync_directory(root: &Path) -> StoreResult<()> {
    sync_directory_with_failure(root, false)
}

pub(crate) fn sync_directory_with_failure(root: &Path, fail_sync: bool) -> StoreResult<()> {
    if fail_sync {
        return Err(StoreError::Durability(
            "simulated directory fsync failure".to_owned(),
        ));
    }

    let dir = File::open(root).map_err(durability_err)?;
    dir.sync_all().map_err(durability_err)
}

fn durability_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Durability(error.to_string())
}
