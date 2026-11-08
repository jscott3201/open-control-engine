//! Recovery ordering of the host-side post-load sequence against a minimal write-ahead-log store
//! that follows the normative `Durable` contract: `Critical` point writes are durable on return,
//! `Telemetry` at the next `flush`, model saves at the next `commit`, and `recover` discards the
//! uncommitted tail and rebuilds live state from the log. Against that contract the order is
//! load-bearing: a save made before `recover` is discarded with the tail, and a handle resolved
//! before `recover` indexes state that recovery replaces. Both tests fail if the helper reorders.
#![cfg(feature = "mem")]

use std::sync::{Arc, Mutex, RwLock};

use oce_store::host::persist_loaded_model;
use oce_store::*;

/// What survives a crash: the append-only log, shared between successive "process lifetimes".
type Log = Arc<Mutex<Vec<Record>>>;

#[derive(Clone)]
enum Record {
    Model(ResolvedModel),
    Point(PointWrite),
}

/// A volatile `MemStore` in front of a durable log; dropping it is a crash.
struct WalStore {
    log: Log,
    live: RwLock<MemStore>,
    pending: Mutex<Vec<Record>>,
}

impl WalStore {
    fn open(log: &Log) -> Self {
        let (log, live, pending) = (Arc::clone(log), RwLock::default(), Mutex::default());
        Self { log, live, pending }
    }
    fn live(&self) -> std::sync::RwLockReadGuard<'_, MemStore> {
        self.live.read().unwrap()
    }
    fn move_pending(&self, keep: impl Fn(&Record) -> bool) {
        let mut pending = self.pending.lock().unwrap();
        let (durable, rest) = pending.drain(..).partition(|r| !keep(r));
        self.log.lock().unwrap().extend::<Vec<_>>(durable);
        *pending = rest;
    }
}

impl Durable for WalStore {
    fn commit(&self) -> StoreResult<()> {
        self.move_pending(|_| false);
        Ok(())
    }
    fn flush(&self) -> StoreResult<()> {
        self.move_pending(|r| matches!(r, Record::Model(_)));
        Ok(())
    }
    fn recover(&self) -> StoreResult<()> {
        self.pending.lock().unwrap().clear();
        let fresh = MemStore::new();
        for r in self.log.lock().unwrap().iter().cloned() {
            match r {
                Record::Model(m) => fresh.save_model(&m)?,
                Record::Point(w) => drop(fresh.write_points(&[w])?),
            }
        }
        *self.live.write().unwrap() = fresh;
        Ok(())
    }
}

impl ModelStore for WalStore {
    fn save_model(&self, m: &ResolvedModel) -> StoreResult<()> {
        self.pending.lock().unwrap().push(Record::Model(m.clone()));
        self.live().save_model(m)
    }
    fn load_model(&self, id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.live().load_model(id)
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.live().list_models()
    }
    fn delete_model(&self, id: &DomainKey) -> StoreResult<()> {
        self.live().delete_model(id)
    }
}

impl PointStore for WalStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.live().resolve_points(keys)
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.live().snapshot()
    }
    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        for w in batch.iter().cloned() {
            match w.durability {
                Durability::Critical => self.log.lock().unwrap().push(Record::Point(w)),
                Durability::Telemetry => self.pending.lock().unwrap().push(Record::Point(w)),
            }
        }
        self.live().write_points(batch)
    }
}

impl SemanticStore for WalStore {
    fn upsert_equipment(&self, e: &EquipmentDto) -> StoreResult<()> {
        self.live().upsert_equipment(e)
    }
    fn add_relation(&self, r: &RelationDto) -> StoreResult<()> {
        self.live().add_relation(r)
    }
    fn put_semantic_payload(&self, p: &SemanticPayloadDto) -> StoreResult<()> {
        self.live().put_semantic_payload(p)
    }
    fn get_semantic_payloads(&self, s: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.live().get_semantic_payloads(s)
    }
    fn point_list(&self, d: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.live().point_list(d)
    }
    fn retrieve(&self, q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.live().retrieve(q)
    }
    fn match_template(&self, r: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.live().match_template(r)
    }
}

fn model(rev: u32) -> ResolvedModel {
    ResolvedModel {
        model_id: DomainKey::new("seq:ahu"),
        schema_rev: rev,
        classes: Vec::new(),
        blocks: Vec::new(),
        points: Vec::new(),
        connections: Vec::new(),
        containment: Vec::new(),
    }
}

fn write(key: &str, v: f64, durability: Durability) -> PointWrite {
    let sample = PointSample {
        value: OcValue::Real(v),
        status: PointStatus::Ok,
        at_unix_nanos: 7,
    };
    let key = DomainKey::new(key);
    PointWrite {
        key,
        sample,
        durability,
    }
}

/// `TOut` first: recovery replays `TZon` into slot 0, so a resolve before `recover` would hand
/// `TZon` a slot the recovered state does not have.
const INPUTS: [(&str, PointValueType); 2] = [
    ("ahu.TOut", PointValueType::Real),
    ("ahu.TZon", PointValueType::Real),
];

#[test]
fn recovered_model_is_replaced_by_the_fresh_save_not_the_reverse() {
    let log = Log::default();
    let first = WalStore::open(&log);
    persist_loaded_model(&first, &model(1), INPUTS).unwrap();
    first.commit().unwrap();
    drop(first); // Crash, then restart with a newer model of the same id.
    let second = WalStore::open(&log);
    persist_loaded_model(&second, &model(2), INPUTS).unwrap();
    let id = DomainKey::new("seq:ahu");
    assert_eq!(second.load_model(&id).unwrap().schema_rev, 2);
    second.commit().unwrap();
    drop(second); // A further crash replays the log in order; the newest commit wins.
    let third = WalStore::open(&log);
    third.recover().unwrap();
    assert_eq!(third.load_model(&id).unwrap().schema_rev, 2);
}

#[test]
fn handles_returned_after_recovery_read_only_durable_point_state() {
    let log = Log::default();
    let first = WalStore::open(&log);
    persist_loaded_model(&first, &model(1), INPUTS).unwrap();
    let batch = [
        write("ahu.TZon", 21.5, Durability::Critical),
        write("ahu.TOut", 3.0, Durability::Telemetry),
    ];
    first.write_points(&batch).unwrap();
    drop(first); // Crash before any flush: the telemetry sample is lost.

    let second = WalStore::open(&log);
    let handles = persist_loaded_model(&second, &model(1), INPUTS).unwrap();
    let snap = second.snapshot().unwrap();
    let zone = snap.read_resolved(handles[1]).expect("critical survived");
    assert_eq!(zone.value, OcValue::Real(21.5));
    assert!(snap.read_resolved(handles[0]).is_none());

    second
        .write_points(&[write("ahu.TOut", 4.0, Durability::Telemetry)])
        .unwrap();
    second.flush().unwrap();
    drop(second); // Crash after the flush: the flushed sample survives.
    let third = WalStore::open(&log);
    let handles = persist_loaded_model(&third, &model(1), INPUTS).unwrap();
    let out = third.snapshot().unwrap().read_resolved(handles[0]).unwrap();
    assert_eq!(out.value, OcValue::Real(4.0));
}
