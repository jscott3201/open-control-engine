//! What a pass-through key costs a load, in heap allocations and never in elapsed time
//! (`TESTING.md`: cost is asserted with deterministic measures).
//!
//! Every key of every `@graph` node is classified by the loader: a spelling of a modeled property,
//! a refused near miss of one, or a key it does not read. A node may carry tens of thousands of
//! keys it does not read, so classifying one must cost no allocation of its own. That holds in
//! particular for the check that refuses a `hasUnit` key in a near miss of a QUDT namespace
//! (issue #675): a key that does not end in `hasUnit` is ruled out by one comparison, and one that
//! does and names another vocabulary's property is compared against four namespaces, with nothing
//! allocated either way.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread, on a
//! document of one node carrying 50,000 such keys and on a sixteenth of it:
//! - **A ceiling for each size**, with a fixed headroom over what was measured (in brackets),
//!   never a part of an allocation per key: one more allocation per key fails it.
//! - **Equality between two documents of the same bytes but for the keys' last seven
//!   characters**, one set ending in `hasUnit` and one not: the near-miss comparisons allocate
//!   nothing, so the full check costs a pass-through key what the early exit costs it.
//!
//! The counts are a function of the input, the loader, and the `serde_json` and `std` that
//! `Cargo.lock` and the pinned toolchain fix; a change to any of them that moves a count is
//! re-measured. What this cannot see: work that allocates nothing. The number of near-miss checks
//! is counted in `oce-cxf`'s own tests (`unread_qudt_has_unit.rs`).

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The node of [`MODEL`] the keys are written on: the first block instance.
const NODE: &str = "\"@id\": \"http://example.org#MinLoop.con\",";

/// [`MODEL`] with `keys` keys on one node, each a full IRI in a vocabulary the loader does not
/// read, numbered and ending in `ending`, with the value `0`.
fn document(keys: usize, ending: &str) -> Vec<u8> {
    assert_eq!(MODEL.matches(NODE).count(), 1, "the node is found once");
    let written: String = (0..keys)
        .map(|i| format!("\"http://example.org/v#k{i:05}{ending}\": 0, "))
        .collect();
    MODEL
        .replacen(NODE, &format!("{NODE} {written}"), 1)
        .into_bytes()
}

/// The allocations of one load of `bytes`, which must load without a warning. The unedited model
/// is loaded once first, so what a process sets up on its first load is not counted.
fn load(bytes: &[u8]) -> AllocationInfo {
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    let (report, _) = result
        .expect("the load ran")
        .unwrap_or_else(|error| panic!("expected a load: {error}"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    info
}

#[test]
fn a_pass_through_key_costs_the_same_allocations_whether_or_not_it_ends_in_has_unit() {
    for keys in [3_125, 50_000] {
        // Seven characters each, so the two documents are the same length key for key.
        let ruled_out = load(&document(keys, "notUnit"));
        let compared = load(&document(keys, "hasUnit"));
        assert_eq!(
            (
                compared.count_total,
                compared.bytes_total,
                compared.bytes_max
            ),
            (
                ruled_out.count_total,
                ruled_out.bytes_total,
                ruled_out.bytes_max
            ),
            "{keys} keys"
        );
    }
}

/// What a ceiling allows over the measured count: a fixed amount, far under one allocation a key
/// at either size.
const HEADROOM: u64 = 64;

#[test]
fn fifty_thousand_pass_through_keys_on_one_node_stay_within_their_measured_allocations() {
    // `(keys, the allocations measured for one load)`: about 5.7 a key, for the key's two copies
    // (the parsed node and the authored-key scan), its expansion, and the maps' nodes.
    for (keys, measured) in [(3_125, 19_273_u64), (50_000, 284_908_u64)] {
        let info = load(&document(keys, "notUnit"));
        assert!(
            info.count_total <= measured + HEADROOM,
            "{keys} keys: {} allocations, over the {measured} measured and the headroom",
            info.count_total
        );
        // Peak memory is a fixed amount a key [measured: 461 and 428 bytes], about ten times
        // the bytes the key takes in the document.
        assert!(info.bytes_max <= 512 * keys as u64, "{keys} keys: {info:?}");
    }
}
