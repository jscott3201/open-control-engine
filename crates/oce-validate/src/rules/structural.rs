//! Rule implementations and deterministic diagnostic ordering for the deep load gate.
//!
//! Every check here is **total and panic-free** on *any* [`ModelGraph`] — including a
//! structurally-malformed, hand-built one whose ids are out of range or whose [`Attrs`] variant
//! violates the R5 tag invariant (a public struct literal can do this, bypassing the checked
//! [`oce_model::Connector::with_attrs`] constructor). Every block/connector/connection index is
//! bounds-checked before use and every `Real`-attribute read goes through
//! [`oce_model::Attrs::as_real`] (which yields `None` on a mismatched tag) rather than an unwrap.
//! A malformed graph yields a [`DiagCode::MalformedDocument`] diagnostic, never an abort
//! (`08` R-ERR-1 / the safety-critical testing standard).

use std::collections::HashSet;
use std::sync::Arc;

use oce_blocks::{PortKind, lookup};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ConnectorId, Dir, ModelGraph, ValueType};

// ---- shared helpers -------------------------------------------------------------------------

use super::{in_degrees, subject_of};

/// Map an `oce-blocks` [`PortKind`] to the model [`ValueType`] a conforming connector must carry.
fn port_value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

// ---- Rule 0: arena id integrity -------------------------------------------------------------

/// Every block and connector must reference the dense arenas the executor indexes by raw id:
///
/// - `model.blocks[i].id.0 == i` for every block (dense + unique [`oce_model::BlockId`] space).
/// - `connector.block.0 < model.blocks.len()` for every connector.
/// - every `BlockInstance.inputs` / `BlockInstance.outputs` [`ConnectorId`] is in range.
///
/// These are malformed-document errors because `oce-graph` intentionally keeps BUILD/tick arena
/// indexing lean and assumes validation has already proven these invariants.
pub(crate) fn check_arena_ids(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let block_count = model.blocks.len();
    let connector_count = model.connectors.len();

    for (index, blk) in model.blocks.iter().enumerate() {
        let id = blk.id.0 as usize;
        if id != index {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block id invariant violated: block at arena index {index} has BlockId({}); \
                         expected a dense unique BlockId equal to its arena index and < blocks={block_count}",
                        blk.id.0
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
        check_block_port_ids(blk, "input", &blk.inputs, connector_count, diags);
        check_block_port_ids(blk, "output", &blk.outputs, connector_count, diags);
    }

    for c in &model.connectors {
        if c.block.0 as usize >= block_count {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "connector id {} references an out-of-range block id {} (blocks={block_count})",
                        c.id.0, c.block.0
                    ),
                )
                .with_subject(subject_of(c)),
            );
        }
    }
}

fn check_block_port_ids(
    blk: &oce_model::BlockInstance,
    label: &str,
    ports: &[ConnectorId],
    connector_count: usize,
    diags: &mut Vec<Diagnostic>,
) {
    for (port_idx, cid) in ports.iter().enumerate() {
        if cid.0 as usize >= connector_count {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block id {} {label} port {port_idx} references an out-of-range \
                         connector id {} (connectors={connector_count})",
                        blk.id.0, cid.0
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
    }
}

// ---- Rule 1: boundary-aware single assignment (§7.10 / §9.1.5) ------------------------------

/// Every `In` connector must have in-degree **exactly 1**, *except* a declared external boundary
/// input (`ModelGraph::external_inputs`, the AD-2 boundary-input elision), which legally has
/// in-degree 0. In-degree ≥ 2 is always a `shall`-error — even for a connector that *is* in
/// `external_inputs` (an external input driven from inside is doubly-assigned).
pub(crate) fn check_single_assignment(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let deg = in_degrees(model);
    let external: HashSet<u32> = model.external_inputs.iter().map(|c| c.0).collect();
    for c in &model.connectors {
        if c.dir != Dir::In {
            continue;
        }
        let d = deg.get(c.id.0 as usize).copied().unwrap_or(0);
        match d {
            1 => {}
            0 => {
                if !external.contains(&c.id.0) {
                    diags.push(
                        Diagnostic::error(
                            DiagCode::SingleAssignment,
                            "input connector has in-degree 0 and is not a declared external \
                             boundary input (§7.10)",
                        )
                        .with_subject(subject_of(c)),
                    );
                }
            }
            n => {
                diags.push(
                    Diagnostic::error(
                        DiagCode::SingleAssignment,
                        format!(
                            "input connector has in-degree {n}; single assignment requires \
                             exactly 1 (§7.10)"
                        ),
                    )
                    .with_subject(subject_of(c)),
                );
            }
        }
    }
}

// ---- Rule 2: per-connection direction + value-type (§9.1.6) ---------------------------------

/// For every connection: `from` must be an `Out` connector, `to` an `In` connector, and the two
/// must share a [`ValueType`] (CDL forbids implicit coercion, §7.10). An out-of-range endpoint is a
/// [`DiagCode::MalformedDocument`] (and the connection is otherwise skipped — never indexed).
pub(crate) fn check_connections(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let n = model.connectors.len() as u32;
    for (idx, c) in model.connections.iter().enumerate() {
        if c.from.0 >= n || c.to.0 >= n {
            diags.push(Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "connection #{idx} references an out-of-range connector id \
                     (from={}, to={}, connectors={n})",
                    c.from.0, c.to.0
                ),
            ));
            continue;
        }
        let from = &model.connectors[c.from.0 as usize];
        let to = &model.connectors[c.to.0 as usize];
        if from.dir != Dir::Out {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection source is not an output connector (§9.1.6)",
                )
                .with_subject(subject_of(from)),
            );
        }
        if to.dir != Dir::In {
            diags.push(
                Diagnostic::error(
                    DiagCode::DirectionMismatch,
                    "connection target is not an input connector (§9.1.6)",
                )
                .with_subject(subject_of(to)),
            );
        }
        if from.value_type != to.value_type {
            diags.push(
                Diagnostic::error(
                    DiagCode::TypeMismatch,
                    format!(
                        "connection joins connectors of different value types: {:?} → {:?}; \
                         CDL forbids implicit coercion (§9.1.6)",
                        from.value_type, to.value_type
                    ),
                )
                .with_subject(subject_of(to)),
            );
        }
    }
}

// ---- Rule 3: block interface ↔ block-signature agreement (AD-8, §7.8) -----------------------

/// Each block's port arity and connector [`ValueType`] must agree with the native block class's
/// [`oce_blocks::BlockSignature`]. This is the **reason `oce-validate` depends on `oce-blocks`**
/// (AD-8): the resolver derives a connector's value type from the CXF `isOfDataType`,
/// *independently* of the block class, so a document could type an input of `CDL.Reals.Add` as
/// `Boolean` — the resolver would record it, and the `read_real` hot-path reader would then silently
/// coerce it to `0.0` in release (a safety-critical silent wrong value). A hand-built graph can also
/// omit or add ports and would otherwise reach emit/gather by port index. This gate rejects those
/// mismatches at load.
///
/// An **unknown** class path is skipped (it is `oce-api`'s `OcError::Load` to report, R-IMPL-2).
/// Out-of-range port connector ids are reported by [`check_arena_ids`]; this rule still bounds them
/// before indexing so the validator remains panic-free even if rules are refactored later.
pub(crate) fn check_port_types(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    let n = model.connectors.len() as u32;
    for blk in &model.blocks {
        let Some(entry) = lookup(&blk.class_iri) else {
            continue; // unknown class → oce-api OcError::Load owns this
        };
        let probe = (entry.make)(&blk.params);
        let sig = probe.signature();
        let (got_in, got_out) = (blk.inputs.len(), blk.outputs.len());
        let (want_in, want_out) = (sig.inputs.len(), sig.outputs.len());
        if got_in != want_in || got_out != want_out {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "block interface mismatch for `{}`: declared {got_in} input(s)/{got_out} \
                         output(s), class requires {want_in}/{want_out}",
                        blk.class_iri
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
        check_ports_dir(
            model,
            diags,
            n,
            &blk.class_iri,
            &blk.inputs,
            sig.inputs,
            "input",
        );
        check_ports_dir(
            model,
            diags,
            n,
            &blk.class_iri,
            &blk.outputs,
            sig.outputs,
            "output",
        );
    }
}

fn block_subject_of(blk: &oce_model::BlockInstance) -> Arc<str> {
    match &blk.instance_iri {
        Some(iri) => Arc::clone(iri),
        None => Arc::from(format!("block#{}", blk.id.0)),
    }
}

/// Compare one direction's worth of a block's port connectors against the signature port kinds.
fn check_ports_dir(
    model: &ModelGraph,
    diags: &mut Vec<Diagnostic>,
    n: u32,
    class_iri: &str,
    ports: &[ConnectorId],
    kinds: &[PortKind],
    label: &str,
) {
    for (port_idx, cid) in ports.iter().enumerate() {
        if cid.0 >= n {
            continue;
        }
        // Arity diagnostics are shared by resolver and `oce-validate` (AD-8). Once the block-level
        // arity diagnostic is emitted above, extra ports have no signature slot to compare.
        let Some(kind) = kinds.get(port_idx) else {
            continue;
        };
        let c = &model.connectors[cid.0 as usize];
        let want = port_value_type(*kind);
        if c.value_type != want {
            diags.push(
                Diagnostic::error(
                    DiagCode::PortKindMismatch,
                    format!(
                        "{label} port {port_idx} of block class {class_iri}: connector value type \
                         {:?} disagrees with the block-signature port kind {:?} (§7.8/§7.10)",
                        c.value_type, want
                    ),
                )
                .with_subject(subject_of(c)),
            );
        }
    }
}
