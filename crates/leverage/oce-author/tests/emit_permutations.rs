//! The permutation classes (authoring note, "Determinism tests"; the composite subset's ordering
//! and identity contract): each permutation of the model is emitted, loaded and compared with
//! what the contract says for that surface. Nothing is sorted, so every authored order that
//! bears on identity reaches the bytes.

mod support;

use oce_api::Engine;
use oce_author::{EmitOptions, Sequence, emit};

use support::cxf::{corpus_fixture, identity, load_clean};
use support::every_element;

/// A corpus sequence whose export is complete (so it has a content id), with fan-out sources.
fn base() -> Sequence {
    let bytes = std::fs::read(corpus_fixture("reheat_overrides")).expect("read");
    oce_author::import_cxf(&bytes).expect("import")
}

fn emitted(s: &Sequence) -> Vec<u8> {
    emit(s, &EmitOptions::new()).expect("emit")
}

fn loaded(s: &Sequence) -> (Vec<u8>, Engine) {
    let bytes = emitted(s);
    let engine = load_clean(&bytes);
    (bytes, engine)
}

/// Whether `from`'s state snapshot restores into `into` (the executable-compatibility check).
fn restores(from: &Engine, into: &mut Engine) -> bool {
    let snapshot = from.state_snapshot().expect("snapshot");
    into.restore_state(&snapshot).is_ok()
}

#[test]
fn the_unpermuted_sequence_restores_into_itself() {
    let (_, a) = loaded(&every_element());
    let (_, mut b) = loaded(&every_element());
    assert!(restores(&a, &mut b));
}

#[test]
fn block_order_moves_the_bytes_and_the_identity() {
    let base = base();
    let (base_bytes, base_engine) = loaded(&base);
    let mut swapped = base.clone();
    swapped.blocks.swap(0, 1);
    let (bytes, engine) = loaded(&swapped);
    assert_ne!(bytes, base_bytes);
    assert_ne!(identity(&engine), identity(&base_engine));
}

#[test]
fn boundary_order_moves_the_bytes_and_the_identity() {
    let base = base();
    let (base_bytes, base_engine) = loaded(&base);
    for permute in [
        |s: &mut Sequence| s.inputs.swap(0, 1),
        |s: &mut Sequence| s.outputs.swap(0, 1),
    ] {
        let mut permuted = base.clone();
        permute(&mut permuted);
        let (bytes, engine) = loaded(&permuted);
        assert_ne!(bytes, base_bytes);
        assert_ne!(identity(&engine), identity(&base_engine));
    }
}

#[test]
fn a_direct_target_permutation_moves_the_export_but_not_executable_compatibility() {
    // The first block output with two targets; swapping them permutes that source's target array
    // only. (A boundary input's fan-out is normalized by boundary lowering instead.)
    let base = base();
    let (base_bytes, base_engine) = loaded(&base);
    let (first, second) = (0..base.connections.len())
        .filter(|&i| {
            matches!(
                base.connections[i].source,
                oce_author::Endpoint::Port { .. }
            )
        })
        .find_map(|i| {
            (i + 1..base.connections.len())
                .find(|&j| base.connections[j].source == base.connections[i].source)
                .map(|j| (i, j))
        })
        .expect("a fan-out source");
    let mut permuted = base.clone();
    permuted.connections.swap(first, second);
    let (bytes, mut engine) = loaded(&permuted);
    assert_ne!(bytes, base_bytes);
    assert_ne!(identity(&engine), identity(&base_engine));
    assert!(restores(&base_engine, &mut engine));
}

#[test]
fn sequence_parameter_order_leaves_the_loaded_model_identical() {
    // Rule 5 normalizes a composite's own declarations, so only the bytes move.
    let base = every_element();
    let (base_bytes, base_engine) = loaded(&base);
    let mut permuted = base.clone();
    permuted.params.reverse();
    let (bytes, mut engine) = loaded(&permuted);
    assert_ne!(bytes, base_bytes);
    assert_eq!(identity(&engine), identity(&base_engine));
    assert_eq!(engine.topology(), base_engine.topology());
    assert!(restores(&base_engine, &mut engine));
}

#[test]
fn connection_order_across_sources_does_not_move_the_bytes() {
    // The emitter hangs targets on their source node, so only each source's own target order
    // reaches the bytes; moving a connection past one from another source changes nothing.
    let base = every_element();
    // `staCon.y -> yStage` and `uEna -> assMes.u` are each their source's only connection.
    let mut permuted = base.clone();
    let at = |s: &Sequence, spelled: &str| {
        s.connections
            .iter()
            .position(|c| c.spelling() == spelled)
            .expect(spelled)
    };
    let (a, b) = (
        at(&base, "staCon.y -> yStage"),
        at(&base, "uEna -> assMes.u"),
    );
    permuted.connections.swap(a, b);
    assert_ne!(permuted, base);
    assert!(emitted(&permuted) == emitted(&base));
}

#[test]
fn a_boundary_fanout_permutation_moves_the_bytes_but_not_the_identity() {
    // Boundary lowering normalizes a boundary input's fan-out order (ordering contract).
    let base = base();
    let (base_bytes, base_engine) = loaded(&base);
    let (first, second) = (0..base.connections.len())
        .filter(|&i| {
            matches!(
                base.connections[i].source,
                oce_author::Endpoint::Boundary(_)
            )
        })
        .find_map(|i| {
            (i + 1..base.connections.len())
                .find(|&j| base.connections[j].source == base.connections[i].source)
                .map(|j| (i, j))
        })
        .expect("a boundary fan-out");
    let mut permuted = base.clone();
    permuted.connections.swap(first, second);
    let (bytes, engine) = loaded(&permuted);
    assert_ne!(bytes, base_bytes);
    assert_eq!(identity(&engine), identity(&base_engine));
}
