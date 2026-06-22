#![forbid(unsafe_code)]
//! Verification-only reference WAL adapter for the `oce-store` port.
//!
//! This crate is a non-shipped `publish = false` workspace member used to prove the frozen
//! `oce-store` seam can support real durability without adding a first-party database backend to
//! Open Control Engine. It is deliberately DB-free and async-free: persistence is implemented with
//! `std::fs`, newline-delimited JSON WAL records, and an atomic snapshot file. Production
//! applications remain responsible for their own durable/queryable adapters behind the port
//! (R-SEAM-2 / D-OWNER-1).
//!
//! Status: verification-only adapter for M3 durability port tests.

mod codec;
mod fs_sync;
mod snapshot_file;
mod wal_append;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use codec::{SnapshotFile, StoredPointSample, WalRecord, WalRecordKind};
use oce_store::{
    DomainKey, Durability, Durable, EquipmentDto, ModelStore, PointHandle, PointListRow,
    PointSample, PointSnapshot, PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, StoreError, StoreResult, TemplatePointReq,
};

/// Maximum number of Telemetry samples accepted before this adapter group-commits them.
///
/// This is the documented bounded lost-write window for [`Durability::Telemetry`]: before an
/// explicit [`Durable::flush`] or [`Durable::commit`], at most this many minus one recently
/// accepted Telemetry samples may be lost on process death. Critical writes are not subject to
/// this window; each Critical record is appended and fsync'd before `write_points` returns `Ok`.
pub const TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT: usize = 1024;

const SEMANTIC_GRAPH_DEFERRED: &str =
    "ReferenceWalStore: semantic graph operations deferred to pointlist-export";
const SEMANTIC_RETRIEVAL_DEFERRED: &str =
    "ReferenceWalStore: semantic retrieval deferred to retrieval-recipes";

/// Construction options for [`ReferenceWalStore`].
#[derive(Clone, Debug, Default)]
pub struct ReferenceWalOptions {
    /// Test-oriented failure injection: after this many successful Critical record fsyncs, the next
    /// Critical write appends a deliberately torn final WAL record and returns
    /// [`StoreError::Durability`]. `None` disables injection.
    pub fail_critical_fsync_after: Option<usize>,
    /// Test-oriented failure injection: after this many successful Critical record fsyncs, the next
    /// Critical write appends a complete WAL record, fails the file fsync boundary, truncates the
    /// unsynced record, and returns [`StoreError::Durability`]. `None` disables injection.
    pub fail_critical_file_sync_after: Option<usize>,
    /// Test-oriented failure injection: after this many successful Critical record fsyncs, the next
    /// Critical write appends and fsyncs the WAL file, fails the parent-directory fsync boundary,
    /// truncates the unsynced record, and returns [`StoreError::Durability`]. `None` disables
    /// injection.
    pub fail_critical_directory_sync_after: Option<usize>,
}

/// Verification-only file-backed implementation of all `oce-store` ports.
pub struct ReferenceWalStore {
    root: PathBuf,
    state: RwLock<AdapterState>,
    fsync_failure: Mutex<FsyncFailure>,
}

#[derive(Clone, Default)]
struct AdapterState {
    models: HashMap<DomainKey, ResolvedModel>,
    points: PointState,
    pending_telemetry: Vec<PendingPointWrite>,
}

#[derive(Clone, Default)]
struct PointState {
    by_handle: Vec<Option<PointSample>>,
    by_key: HashMap<DomainKey, u64>,
    keys_by_handle: Vec<DomainKey>,
}

#[derive(Clone)]
struct PendingPointWrite {
    key: DomainKey,
    sample: PointSample,
}

#[derive(Default)]
struct FsyncFailure {
    torn_tail_after: Option<usize>,
    file_sync_after: Option<usize>,
    directory_sync_after: Option<usize>,
    successful_critical_fsyncs: usize,
}

struct ReferenceWalSnapshot {
    by_handle: Vec<Option<PointSample>>,
    by_key: HashMap<DomainKey, u64>,
}

impl ReferenceWalStore {
    /// Open or create a reference WAL store rooted at `path`, recovering persisted state before
    /// returning.
    ///
    /// # Errors
    /// Returns [`StoreError::Backend`] when the directory, snapshot, or WAL cannot be read, and
    /// [`StoreError::Durability`] when fsync/truncation needed during recovery fails.
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        Self::open_with_options(path, ReferenceWalOptions::default())
    }

    /// Open or create a reference WAL store with explicit options.
    ///
    /// The options exist so direct port tests can simulate the recoverable-prefix Critical
    /// partial-failure contract without modifying the frozen `oce-store` traits.
    ///
    /// # Errors
    /// Returns [`StoreError::Backend`] when the directory, snapshot, or WAL cannot be read, and
    /// [`StoreError::Durability`] when fsync/truncation needed during recovery fails.
    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: ReferenceWalOptions,
    ) -> StoreResult<Self> {
        let store = Self {
            root: path.as_ref().to_path_buf(),
            state: RwLock::new(AdapterState::default()),
            fsync_failure: Mutex::new(FsyncFailure {
                torn_tail_after: options.fail_critical_fsync_after,
                file_sync_after: options.fail_critical_file_sync_after,
                directory_sync_after: options.fail_critical_directory_sync_after,
                successful_critical_fsyncs: 0,
            }),
        };
        std::fs::create_dir_all(&store.root).map_err(backend_err)?;
        store.recover()?;
        Ok(store)
    }
}

impl PointState {
    fn handle_for(&mut self, key: &DomainKey) -> u64 {
        if let Some(&idx) = self.by_key.get(key) {
            return idx;
        }

        let idx = self.by_handle.len() as u64;
        self.by_handle.push(None);
        self.by_key.insert(key.clone(), idx);
        self.keys_by_handle.push(key.clone());
        idx
    }

    fn set_sample(&mut self, key: &DomainKey, sample: PointSample) {
        let idx = self.handle_for(key) as usize;
        self.by_handle[idx] = Some(sample);
    }
}

impl AdapterState {
    fn snapshot_file(&self) -> SnapshotFile {
        let mut models: Vec<_> = self.models.values().cloned().collect();
        models.sort_by(|a, b| a.model_id.as_str().cmp(b.model_id.as_str()));
        let point_samples = self
            .points
            .by_handle
            .iter()
            .map(|sample| sample.as_ref().map(StoredPointSample::from))
            .collect();
        SnapshotFile::new(models, self.points.keys_by_handle.clone(), point_samples)
    }

    fn from_snapshot(snapshot: Option<SnapshotFile>) -> StoreResult<Self> {
        let mut state = Self::default();
        let Some(snapshot) = snapshot else {
            return Ok(state);
        };

        for model in snapshot.models {
            state.models.insert(model.model_id.clone(), model);
        }

        for key in snapshot.point_keys {
            state.points.handle_for(&key);
        }

        for (idx, sample) in snapshot.point_samples.into_iter().enumerate() {
            if idx >= state.points.by_handle.len() {
                return Err(StoreError::Backend(
                    "snapshot point sample references missing point key".to_owned(),
                ));
            }
            state.points.by_handle[idx] = sample.map(|stored| stored.to_sample());
        }
        Ok(state)
    }

    fn apply_wal_record(&mut self, record: WalRecord) -> StoreResult<()> {
        match record.record {
            WalRecordKind::PointWrite { key, sample, .. } => {
                self.points.set_sample(&key, sample.to_sample());
                Ok(())
            }
        }
    }
}

impl FsyncFailure {
    fn should_write_torn_tail(&self) -> bool {
        self.torn_tail_after
            .is_some_and(|limit| self.successful_critical_fsyncs >= limit)
    }

    fn sync_options(&self) -> wal_append::WalSyncOptions {
        wal_append::WalSyncOptions {
            fail_file_sync: self
                .file_sync_after
                .is_some_and(|limit| self.successful_critical_fsyncs >= limit),
            fail_directory_sync: self
                .directory_sync_after
                .is_some_and(|limit| self.successful_critical_fsyncs >= limit),
        }
    }

    fn record_critical_success(&mut self) {
        self.successful_critical_fsyncs += 1;
    }
}

impl PointSnapshot for ReferenceWalSnapshot {
    fn read_resolved(&self, handle: PointHandle) -> Option<PointSample> {
        let idx = usize::try_from(handle.0).ok()?;
        self.by_handle.get(idx).cloned().flatten()
    }

    fn read_by_key(&self, key: &DomainKey) -> Option<PointSample> {
        let idx = *self.by_key.get(key)?;
        self.by_handle.get(idx as usize).cloned().flatten()
    }
}

impl ModelStore for ReferenceWalStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.state
            .write()
            .map_err(backend_err)?
            .models
            .insert(model.model_id.clone(), model.clone());
        Ok(())
    }

    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.state
            .read()
            .map_err(backend_err)?
            .models
            .get(model_id)
            .cloned()
            .ok_or_else(|| StoreError::ModelNotLoaded(model_id.clone()))
    }

    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        let mut keys: Vec<_> = self
            .state
            .read()
            .map_err(backend_err)?
            .models
            .keys()
            .cloned()
            .collect();
        keys.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(keys)
    }

    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        self.state
            .write()
            .map_err(backend_err)?
            .models
            .remove(model_id);
        Ok(())
    }
}

impl PointStore for ReferenceWalStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        let mut state = self.state.write().map_err(backend_err)?;
        Ok(keys
            .iter()
            .map(|key| PointHandle(state.points.handle_for(key)))
            .collect())
    }

    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        let state = self.state.read().map_err(backend_err)?;
        Ok(Box::new(ReferenceWalSnapshot {
            by_handle: state.points.by_handle.clone(),
            by_key: state.points.by_key.clone(),
        }))
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        let mut state = self.state.write().map_err(backend_err)?;
        let mut accepted = 0usize;

        for write in batch
            .iter()
            .filter(|write| write.durability == Durability::Critical)
        {
            let record =
                WalRecord::point_write(Durability::Critical, write.key.clone(), &write.sample);
            let mut failure = self.fsync_failure.lock().map_err(backend_err)?;
            if failure.should_write_torn_tail() {
                wal_append::append_torn_record(&self.root, &record)?;
                return Err(StoreError::Durability(
                    "simulated Critical fsync failure after durable prefix".to_owned(),
                ));
            }
            wal_append::append_record_and_sync(&self.root, &record, failure.sync_options())?;
            failure.record_critical_success();
            state.points.set_sample(&write.key, write.sample.clone());
            accepted += 1;
        }

        for write in batch
            .iter()
            .filter(|write| write.durability == Durability::Telemetry)
        {
            state.points.set_sample(&write.key, write.sample.clone());
            state.pending_telemetry.push(PendingPointWrite {
                key: write.key.clone(),
                sample: write.sample.clone(),
            });
            accepted += 1;
        }

        if state.pending_telemetry.len() >= TELEMETRY_GROUP_COMMIT_SAMPLE_COUNT {
            flush_telemetry(&self.root, &mut state)?;
        }

        Ok(accepted)
    }
}

impl SemanticStore for ReferenceWalStore {
    fn upsert_equipment(&self, _eq: &EquipmentDto) -> StoreResult<()> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }

    fn add_relation(&self, _rel: &RelationDto) -> StoreResult<()> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }

    fn put_semantic_payload(&self, _p: &SemanticPayloadDto) -> StoreResult<()> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }

    fn get_semantic_payloads(&self, _subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }

    fn point_list(&self, _controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }

    fn retrieve(&self, _q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        Err(StoreError::RetrievalUnsupported(
            SEMANTIC_RETRIEVAL_DEFERRED,
        ))
    }

    fn match_template(&self, _required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        Err(StoreError::Unsupported(SEMANTIC_GRAPH_DEFERRED))
    }
}

impl Durable for ReferenceWalStore {
    fn commit(&self) -> StoreResult<()> {
        let mut state = self.state.write().map_err(backend_err)?;
        flush_telemetry(&self.root, &mut state)?;
        let snapshot = state.snapshot_file();
        snapshot_file::write_atomic(&self.root, &snapshot)?;
        wal_append::truncate(&self.root)
    }

    fn flush(&self) -> StoreResult<()> {
        let mut state = self.state.write().map_err(backend_err)?;
        flush_telemetry(&self.root, &mut state)
    }

    fn recover(&self) -> StoreResult<()> {
        let snapshot = snapshot_file::load(&self.root)?;
        let mut recovered = AdapterState::from_snapshot(snapshot)?;
        wal_append::replay(&self.root, |record| recovered.apply_wal_record(record))?;
        *self.state.write().map_err(backend_err)? = recovered;
        Ok(())
    }
}

fn flush_telemetry(root: &Path, state: &mut AdapterState) -> StoreResult<()> {
    if state.pending_telemetry.is_empty() {
        return Ok(());
    }

    let records: Vec<_> = state
        .pending_telemetry
        .iter()
        .map(|write| {
            WalRecord::point_write(Durability::Telemetry, write.key.clone(), &write.sample)
        })
        .collect();
    wal_append::append_records_and_sync(root, &records)?;
    state.pending_telemetry.clear();
    Ok(())
}

fn backend_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}

/// Compile-time proof the reference adapter satisfies the full store umbrella.
#[allow(dead_code)]
fn _assert_is_store<S: oce_store::Store>() {}
