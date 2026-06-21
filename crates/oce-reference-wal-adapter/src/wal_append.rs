//! Append-only WAL helpers and recoverable-prefix replay.

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::Path;

use oce_store::{StoreError, StoreResult};

use crate::codec::{FORMAT_VERSION, WalRecord};

pub(crate) const WAL_FILE: &str = "points.wal";

pub(crate) fn append_record_and_sync(root: &Path, record: &WalRecord) -> StoreResult<()> {
    fs::create_dir_all(root).map_err(backend_err)?;
    let line = record.encode_line().map_err(backend_err)?;
    let mut file = open_wal_for_append(root)?;
    file.write_all(&line).map_err(backend_err)?;
    file.sync_all().map_err(durability_err)
}

pub(crate) fn append_records_and_sync(root: &Path, records: &[WalRecord]) -> StoreResult<()> {
    if records.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(root).map_err(backend_err)?;
    let mut file = open_wal_for_append(root)?;
    for record in records {
        let line = record.encode_line().map_err(backend_err)?;
        file.write_all(&line).map_err(backend_err)?;
    }
    file.sync_all().map_err(durability_err)
}

pub(crate) fn append_torn_record(root: &Path, record: &WalRecord) -> StoreResult<()> {
    fs::create_dir_all(root).map_err(backend_err)?;
    let line = record.encode_line().map_err(backend_err)?;
    let torn_len = line.len().saturating_sub(1).max(1) / 2;
    let mut file = open_wal_for_append(root)?;
    file.write_all(&line[..torn_len]).map_err(backend_err)
}

pub(crate) fn replay(
    root: &Path,
    mut apply: impl FnMut(WalRecord) -> StoreResult<()>,
) -> StoreResult<()> {
    let wal_path = root.join(WAL_FILE);
    if !wal_path.exists() {
        return Ok(());
    }

    let bytes = fs::read(&wal_path).map_err(backend_err)?;
    let mut offset = 0usize;
    while offset < bytes.len() {
        let remaining = &bytes[offset..];
        let newline = remaining.iter().position(|b| *b == b'\n');
        let (line, next_offset, is_final_line) = match newline {
            Some(pos) => (&remaining[..pos], offset + pos + 1, false),
            None => (remaining, bytes.len(), true),
        };

        if line.is_empty() {
            offset = next_offset;
            continue;
        }

        match serde_json::from_slice::<WalRecord>(line) {
            Ok(record) => {
                if record.format_version != FORMAT_VERSION {
                    return Err(StoreError::Backend(format!(
                        "unsupported WAL format version {}",
                        record.format_version
                    )));
                }
                apply(record)?;
                offset = next_offset;
            }
            Err(_error) if is_final_line => {
                truncate_to(root, offset)?;
                return Ok(());
            }
            Err(error) => return Err(backend_err(error)),
        }
    }
    Ok(())
}

pub(crate) fn truncate(root: &Path) -> StoreResult<()> {
    fs::create_dir_all(root).map_err(backend_err)?;
    let file = File::create(root.join(WAL_FILE)).map_err(backend_err)?;
    file.sync_all().map_err(durability_err)
}

fn truncate_to(root: &Path, len: usize) -> StoreResult<()> {
    let file = OpenOptions::new()
        .write(true)
        .open(root.join(WAL_FILE))
        .map_err(backend_err)?;
    file.set_len(len as u64).map_err(backend_err)?;
    file.sync_all().map_err(durability_err)
}

fn open_wal_for_append(root: &Path) -> StoreResult<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(WAL_FILE))
        .map_err(backend_err)
}

fn backend_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}

fn durability_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Durability(error.to_string())
}
