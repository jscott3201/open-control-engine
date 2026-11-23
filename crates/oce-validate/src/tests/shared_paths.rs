//! A path many connectors carry (every input a boundary input drives holds the boundary's IRI) is
//! grouped and looked up as the text it spells, whether the connectors share one allocation of it
//! or each hold their own (issue #811).
//!
//! The rules that group connectors by path find a repeat of an allocation by its address before
//! they read its text, so a boundary's fan-out costs its path once. These tests hold that the
//! address never decides an answer: the shared and the separate graphs give the same diagnostics,
//! in the same order, stated here by hand.

use super::common::*;
use oce_model::BoundaryInput;

use crate::rules::conn_of_iri;

/// Abs blocks 0..6, their inputs (connectors 0, 2, 4, …) driven by the boundary inputs `urn:b:c`
/// and `urn:b:a` in turn, so the two groups interleave in arena order and in the order opposite
/// to their paths. The first input of each path admits `[5, 10]`, the others `[0, 3]`, and each
/// declaration `[0, 10]`: only the first input and a later one together leave the domain empty, so
/// a group that lost one of its inputs would not be refused. With `shared`, each path is one
/// allocation all its inputs hold.
fn interleaved(shared: bool) -> ModelGraph {
    let paths = ["urn:b:c", "urn:b:a"].map(Arc::<str>::from);
    let mut blocks = Vec::new();
    let mut connectors = Vec::new();
    let mut external_inputs = Vec::new();
    for b in 0..6_u32 {
        blocks.push(block(b, "CDL.Reals.Abs", &[2 * b], &[2 * b + 1]));
        let (min, max) = if b < 2 { (5.0, 10.0) } else { (0.0, 3.0) };
        let mut input = real_conn(
            2 * b,
            b,
            Dir::In,
            RealAttrs {
                min: Some(min),
                max: Some(max),
                ..RealAttrs::default()
            },
        );
        let path = &paths[b as usize % 2];
        input.iri = Some(if shared {
            Arc::clone(path)
        } else {
            Arc::from(&**path)
        });
        connectors.push(input);
        connectors.push(real_conn(2 * b + 1, b, Dir::Out, RealAttrs::default()));
        external_inputs.push(ConnectorId(2 * b));
    }
    let boundary_inputs = paths
        .iter()
        .map(|path| BoundaryInput {
            iri: Arc::from(&**path),
            attrs: Attrs::Real(RealAttrs {
                min: Some(0.0),
                max: Some(10.0),
                ..RealAttrs::default()
            }),
        })
        .collect();
    ModelGraph {
        blocks,
        connectors,
        external_inputs,
        boundary_inputs,
        ..ModelGraph::new()
    }
}

#[test]
fn each_boundarys_combined_domain_is_refused_once_whatever_its_allocations() {
    let combined = "the combined S231:min (5) exceeds the combined S231:max (3) across the Real \
                    boundary input and the child inputs it drives; no input value satisfies them \
                    all";
    for shared in [true, false] {
        let mut model = interleaved(shared);
        let error = unify_and_validate(&mut model).expect_err("both domains are empty");
        let rendered: Vec<(DiagCode, &str, &str)> = error
            .diagnostics
            .iter()
            .map(|d| {
                (
                    d.code,
                    d.subject.as_deref().unwrap_or_default(),
                    &*d.message,
                )
            })
            .collect();
        // The rule raises them in path order; the pass then sorts connector subjects by the
        // connector each path resolves to, the last carrying it: 8 for `urn:b:c`, 10 for `urn:b:a`.
        assert_eq!(
            rendered,
            [
                (DiagCode::CrossedBounds, "urn:b:c", combined),
                (DiagCode::CrossedBounds, "urn:b:a", combined),
            ],
            "shared: {shared}"
        );
    }
}

#[test]
fn a_path_several_connectors_carry_resolves_to_the_last_of_them() {
    // The diagnostic sort resolves a subject to a connector through this map; the connector it
    // finds is the last in arena order carrying the path, as when each overwrote the one before.
    for shared in [true, false] {
        let model = interleaved(shared);
        let map = conn_of_iri(&model);
        assert_eq!(map.len(), 2, "shared: {shared}");
        assert_eq!(map["urn:b:a"], ConnectorId(10), "shared: {shared}");
        assert_eq!(map["urn:b:c"], ConnectorId(8), "shared: {shared}");
    }
    // A later connector holding the text in an allocation of its own still wins over a shared one.
    let mut model = interleaved(true);
    let mut last = real_conn(12, 0, Dir::In, RealAttrs::default());
    last.iri = Some(Arc::from("urn:b:c"));
    model.connectors.push(last);
    assert_eq!(conn_of_iri(&model)["urn:b:c"], ConnectorId(12));
}
