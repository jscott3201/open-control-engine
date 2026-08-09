//! Written-batch pins for the resolve-once durable write path (issue #242 slice 1).
//!
//! Three guards over what [`crate::Engine::step_realtime`] actually commits, all sourced from a
//! batch-retaining recording store — never from `IoInventory::durable_columns`, which is the
//! producer under change (a golden derived from it would pass by construction):
//!
//! - a corpus-wide golden pinning, per G36 fixture, the committed row count, an ordered KEY
//!   digest, and an ordered VALUE digest. The two digests are separate fields because the
//!   batch's parallel vectors fail differently: a key mutation reds the key half, while index
//!   drift (a short or misaligned connector vector) leaves keys and count intact and reds only
//!   the value half — a single combined digest could not tell the halves apart;
//! - reload invalidation: loading a second model into the same engine must swap the committed
//!   key set to the new model's (`Engine::load_cxf` promises full replacement);
//! - tune-at-rest: a `halt` → `set_param` → `resume` refold must leave the committed keys and
//!   values identical to a freshly loaded engine's.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use oce_store::{
    DomainKey, Durable, EquipmentDto, ModelStore, OcValue, PointHandle, PointListRow,
    PointSnapshot, PointStore, PointWrite, RelationDto, ResolvedModel, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, StoreResult, TemplatePointReq,
};

use super::common::{Arc, Engine, MemStore};

const EPOCH: u64 = 1_700_000_000_000_000_000;

/// Every `.jsonld` document in the G36 corpus, sorted for deterministic iteration order.
fn corpus_fixtures() -> Vec<PathBuf> {
    let fixture_dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut fixtures = fs::read_dir(fixture_dir)
        .expect("read G36 fixture corpus")
        .map(|entry| entry.expect("fixture directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 47, "G36 corpus size moved");
    fixtures
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../oce-cxf/tests/fixtures/g36/{name}.jsonld",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn load_engine(bytes: &[u8]) -> (Arc<BatchRecordingStore>, Engine<BatchRecordingStore>) {
    let store = Arc::new(BatchRecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    engine
        .load_cxf(bytes)
        .unwrap_or_else(|error| panic!("fixture loads: {error:?}"));
    engine.set_realtime_epoch_unix_nanos(EPOCH);
    (store, engine)
}

/// The ordered key list of one captured batch.
fn batch_keys(batch: &[PointWrite]) -> Vec<String> {
    batch
        .iter()
        .map(|write| write.key.as_str().to_owned())
        .collect()
}

/// The ordered deterministic value renderings of one captured batch.
fn batch_values(batch: &[PointWrite]) -> Vec<String> {
    batch
        .iter()
        .map(|write| render_value(&write.sample.value))
        .collect()
}

/// Deterministic rendering of one committed sample value. `Real` renders as the exact bit
/// pattern (`to_bits` hex) so the digest inherits bit-exact determinism; `Int`/`Bool` render
/// exactly. The remaining carriers cannot appear in a durable batch today (String points are
/// excluded from the inventory; enum outputs commit as `Int`) but render totally, so a
/// regression admitting one becomes a digest mismatch rather than a panic.
fn render_value(value: &OcValue) -> String {
    match value {
        OcValue::Real(v) => format!("R:{:016x}", v.to_bits()),
        OcValue::Int(v) => format!("I:{v}"),
        OcValue::Bool(v) => format!("B:{v}"),
        OcValue::Decimal(v) => format!("D:{v}"),
        OcValue::Enum { type_iri, literal } => format!("E:{type_iri}:{literal}"),
        OcValue::String(v) => format!("S:{v}"),
    }
}

/// FNV-1a-128, restated here rather than imported: the golden must stay byte-identical across
/// library refactors and toolchain bumps, and std's `DefaultHasher` is documented unstable
/// across Rust releases. Constants match `src/stable_hash.rs` (`StableHash`). Each item is
/// length-prefixed so adjacent items cannot alias under concatenation.
struct Fnv128(u128);

impl Fnv128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u128::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn write_item(&mut self, item: &str) {
        self.write_bytes(&(item.len() as u64).to_le_bytes());
        self.write_bytes(item.as_bytes());
    }
}

/// 32-hex-digit FNV-1a-128 digest of an ordered item list.
fn digest<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let mut hash = Fnv128(Fnv128::OFFSET);
    for item in items {
        hash.write_item(item);
    }
    format!("{:032x}", hash.0)
}

/// One golden row: `<fixture> count=<rows> keys=<digest> values=<digest>`.
fn parse_golden_row(line: &str) -> (String, String, String, String) {
    let mut fields = line.split_whitespace();
    let name = fields.next().expect("golden row names its fixture");
    let mut take = |prefix: &str| {
        let field = fields
            .next()
            .unwrap_or_else(|| panic!("golden row for {name} is missing `{prefix}...`"));
        field
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("golden row for {name}: expected `{prefix}...`, got {field}"))
            .to_owned()
    };
    let count = take("count=");
    let keys = take("keys=");
    let values = take("values=");
    (name.to_owned(), count, keys, values)
}

#[test]
fn committed_batches_match_the_corpus_golden_keys_and_values() {
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/tests/fixtures/golden/step_realtime_write_batches.txt");
    let mut actual = String::new();
    for fixture in corpus_fixtures() {
        let name = fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("UTF-8 fixture stem")
            .to_owned();
        let bytes = fs::read(&fixture).expect("read G36 fixture");
        let (store, mut engine) = load_engine(&bytes);
        engine
            .step_realtime(0.0)
            .unwrap_or_else(|error| panic!("{name} steps: {error:?}"));
        let batches = store.batches.lock().unwrap();
        assert_eq!(batches.len(), 1, "{name}: exactly one commit per step");
        let batch = &batches[0];
        assert!(
            !batch.is_empty(),
            "{name}: the golden must not pin an empty batch"
        );
        let keys = batch_keys(batch);
        let values = batch_values(batch);
        actual.push_str(&format!(
            "{name} count={} keys={} values={}\n",
            batch.len(),
            digest(keys.iter().map(String::as_str)),
            digest(values.iter().map(String::as_str)),
        ));
    }
    if oce_bless::enabled("OCE_BLESS") {
        fs::write(&golden_path, &actual).expect("write committed-batch golden");
        return;
    }
    let expected = fs::read_to_string(&golden_path)
        .expect("committed-batch golden missing; regenerate deliberately with OCE_BLESS=1");

    // Field-wise comparison so one run reports every divergent fixture and names which half
    // (count / keys / values) moved, instead of stopping at the first byte difference.
    let actual_rows: Vec<_> = actual.lines().map(parse_golden_row).collect();
    let expected_rows: Vec<_> = expected.lines().map(parse_golden_row).collect();
    let mut mismatches = Vec::new();
    if actual_rows.len() != expected_rows.len() {
        mismatches.push(format!(
            "row count moved: expected {}, got {}",
            expected_rows.len(),
            actual_rows.len()
        ));
    }
    for ((a_name, a_count, a_keys, a_values), (e_name, e_count, e_keys, e_values)) in
        actual_rows.iter().zip(&expected_rows)
    {
        if a_name != e_name {
            mismatches.push(format!(
                "fixture order moved: expected {e_name}, got {a_name}"
            ));
            continue;
        }
        if a_count != e_count {
            mismatches.push(format!("{a_name}: count {e_count} -> {a_count}"));
        }
        if a_keys != e_keys {
            mismatches.push(format!("{a_name}: KEY digest moved"));
        }
        if a_values != e_values {
            mismatches.push(format!("{a_name}: VALUE digest moved"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "committed-batch golden diverged ({} finding(s)):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn reloading_a_second_model_swaps_the_committed_key_set() {
    let first = read_fixture("cooling_only_controller");
    let second = read_fixture("multizone_vav_supply_fan");
    let (store, mut engine) = load_engine(&first);
    engine.step_realtime(0.0).expect("first model steps");
    engine
        .load_cxf(&second)
        .expect("second model loads into the same engine");
    engine.step_realtime(0.0).expect("second model steps");
    let batches = store.batches.lock().unwrap();
    assert_eq!(batches.len(), 2, "one commit per step across the reload");
    let first_keys = batch_keys(&batches[0]);
    let reloaded_keys = batch_keys(&batches[1]);
    let reloaded_values = batch_values(&batches[1]);

    // Fresh-engine oracle: what the second model commits when it is the only model ever loaded.
    let (fresh_store, mut fresh) = load_engine(&second);
    fresh.step_realtime(0.0).expect("fresh second model steps");
    let fresh_batches = fresh_store.batches.lock().unwrap();
    let fresh_keys = batch_keys(&fresh_batches[0]);
    let fresh_values = batch_values(&fresh_batches[0]);

    assert_ne!(
        reloaded_keys, first_keys,
        "control: the two models' key sets must differ for this pin to bite"
    );
    assert_eq!(
        reloaded_keys, fresh_keys,
        "reload must re-mint the durable batch for the new model's key set"
    );
    assert_eq!(
        reloaded_values, fresh_values,
        "reload must commit the new model's values, not a stale batch"
    );
}

#[test]
fn tune_at_rest_refold_commits_the_same_batch_as_a_fresh_load() {
    let bytes = read_fixture("cooling_only_controller");
    let (store, mut engine) = load_engine(&bytes);
    // A value-identical edit: `set_param` marks the params dirty unconditionally, so `resume`
    // runs the full refold (re-instantiate blocks, re-allocate state, rebuild outputs) while
    // the fresh-engine comparison below stays exact.
    engine.halt().expect("halt is infallible");
    let mut edited = false;
    for (path, value, _attrs) in engine.params().to_vec() {
        if engine.set_param(&path, value).is_ok() {
            edited = true;
            break;
        }
    }
    assert!(
        edited,
        "fixture must accept at least one value-identical param edit"
    );
    engine.resume().expect("resume refolds the staged edit");
    engine.step_realtime(0.0).expect("resumed engine steps");
    let batches = store.batches.lock().unwrap();
    let resumed = &batches[0];
    assert!(!resumed.is_empty(), "the refold pin must not be vacuous");

    let (fresh_store, mut fresh) = load_engine(&bytes);
    fresh.step_realtime(0.0).expect("fresh engine steps");
    let fresh_batches = fresh_store.batches.lock().unwrap();
    let fresh_batch = &fresh_batches[0];

    assert_eq!(
        batch_keys(resumed),
        batch_keys(fresh_batch),
        "a halt/set_param/resume cycle must not move the committed key set"
    );
    assert_eq!(
        batch_values(resumed),
        batch_values(fresh_batch),
        "a halt/set_param/resume cycle must commit the same values as a fresh load"
    );
}

/// A `Store` that retains every `write_points` batch verbatim while delegating all behavior to
/// an inner [`MemStore`]. The retained batches are the oracle source for every pin in this
/// module: they capture what the engine actually committed, independent of the identity cache
/// that produced them.
#[derive(Default)]
struct BatchRecordingStore {
    inner: MemStore,
    batches: Mutex<Vec<Vec<PointWrite>>>,
}

impl ModelStore for BatchRecordingStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.inner.save_model(model)
    }
    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.inner.load_model(model_id)
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.inner.list_models()
    }
    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        self.inner.delete_model(model_id)
    }
}

impl PointStore for BatchRecordingStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.inner.resolve_points(keys)
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.inner.snapshot()
    }
    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.batches.lock().unwrap().push(batch.to_vec());
        self.inner.write_points(batch)
    }
}

impl SemanticStore for BatchRecordingStore {
    fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()> {
        self.inner.upsert_equipment(eq)
    }
    fn add_relation(&self, rel: &RelationDto) -> StoreResult<()> {
        self.inner.add_relation(rel)
    }
    fn put_semantic_payload(&self, payload: &SemanticPayloadDto) -> StoreResult<()> {
        self.inner.put_semantic_payload(payload)
    }
    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.inner.get_semantic_payloads(subject)
    }
    fn point_list(&self, device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.inner.point_list(device)
    }
    fn retrieve(&self, query: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.inner.retrieve(query)
    }
    fn match_template(&self, points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.inner.match_template(points)
    }
}

impl Durable for BatchRecordingStore {
    fn commit(&self) -> StoreResult<()> {
        self.inner.commit()
    }
    fn flush(&self) -> StoreResult<()> {
        self.inner.flush()
    }
    fn recover(&self) -> StoreResult<()> {
        self.inner.recover()
    }
}
