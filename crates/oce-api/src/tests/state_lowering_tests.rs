//! Durable identity coverage for boundary fan-out and reserved pass-through lowering.

use std::collections::BTreeMap;

use crate::state::BlockKey;
use crate::{Engine, EngineStateSnapshot};

const FANOUT: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/boundary_fanout.jsonld");
const PASS_THROUGH: &[u8] =
    include_bytes!("../../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");

#[test]
fn duplicate_host_paths_and_lowered_block_keys_round_trip_without_slot_loss() {
    for (name, fixture) in [("fanout", FANOUT), ("pass-through", PASS_THROUGH)] {
        let mut source = Engine::in_memory();
        source.load_cxf(fixture).unwrap();
        source.tick(0.0).unwrap();
        let snapshot = source.state_snapshot().unwrap();
        assert_eq!(
            snapshot.image.values.len(),
            source.state.values.len(),
            "{name}"
        );
        let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
        let mut target = Engine::in_memory();
        target.load_cxf(fixture).unwrap();
        target.restore_state(&decoded).unwrap();
        assert_eq!(target.state.words, source.state.words, "{name}");
        assert!(
            target
                .state
                .values
                .iter()
                .zip(&source.state.values)
                .all(|(left, right)| left.bit_eq(right)),
            "{name}"
        );

        if name == "fanout" {
            let mut counts = BTreeMap::new();
            for connector in &snapshot.image.manifest.connectors {
                *counts.entry(&connector.path).or_insert(0usize) += 1;
            }
            assert!(counts.values().any(|count| *count > 1));
        } else {
            assert!(
                snapshot
                    .image
                    .manifest
                    .blocks
                    .iter()
                    .any(|block| matches!(block.key, BlockKey::PassThrough { .. }))
            );
        }
    }
}
