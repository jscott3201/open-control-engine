//! The R7 enum tracked-deferral pre-pass: detect enum-carrying blocks, grow the deferred set to
//! its transitive cascade fixpoint, and emit the [`DiagCode::ExportDeferred`] warning triples.
//!
//! This module owns the deferral **diagnostic state** only — the per-phase control-flow skips that
//! actually omit deferred blocks from the emitted document live in `crate::export::plan`, which
//! consumes the `deferred` set returned here. Deferral is block-level plus a transitive cascade
//! (a single enum connector near the front of a chain dooms the downstream cone); port-level
//! deferral is unsound under the arity guard and is out of scope.
//!
//! ## Cascade fixpoint
//! `enum_blocks` is the set of blocks carrying any `ValueType::Enum` connector or `Value::Enum`
//! parameter. `deferred` is the least fixpoint containing `enum_blocks` plus every survivor block
//! whose driven inputs are **all** fed only by deferred blocks (recursively). An input that was
//! already undriven in the original graph (a boundary `external_inputs` entry) does NOT trigger a
//! cascade — only a formerly-driven input whose every driver is now deferred does. The fixpoint
//! terminates because `deferred` only grows and is bounded by `g.blocks.len()`; cycles resolve
//! (both ends enter `deferred`).
//!
//! ## Soundness
//! By the fixpoint, every survivor's driven inputs have at least one surviving driver, so the
//! re-imported graph has no `SingleAssignment` error. Connections among survivors are intact, the
//! arity guard is preserved (every survivor keeps all declared ports), and the single-assignment
//! guard is preserved (every driven survivor input has exactly one surviving driver — original G36
//! graphs are single-assignment). RT-2 holds for the survivor cone.

use std::collections::BTreeSet;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{EnumClassId, ModelGraph, Value, ValueType};

/// A deferral warning: an `ExportDeferred` (Warning, non-aborting) diagnostic mirroring
/// `crate::export::reject` but building a warning rather than an error. The `subject` is the
/// deferred block's `instance_iri` (or a synthetic position tag when the block has none, matching
/// `crate::export::owner_subject`).
fn defer(message: impl Into<String>, subject: &str) -> Diagnostic {
    Diagnostic::warning(DiagCode::ExportDeferred, message).with_subject(subject.to_owned())
}

/// Deferral message for an enumeration-typed connector. Pushed in Phase 1b over `enum_blocks` for
/// each block whose connector list carries a `ValueType::Enum`. The connector arm in the
/// Phase 4 scan pushes only a placeholder (never this warning — the warning is pushed here, once
/// per offending block, in block order).
const MSG_ENUM_DEFER_CONNECTOR: &str = "export subset: deferring block `{subject}` — enumeration-typed connector (class `{class}`) \
     has no CXF literal form; the block and its downstream consumers are omitted from the \
     emitted document so the enum-free remainder can export";

/// Deferral message for an enumeration-valued parameter. Pushed in Phase 1b over `enum_blocks`
/// for each block whose `ParamTable` carries a `Value::Enum`. The `param_binding` `Value::Enum`
/// arm is unreachable for any deferred block (the Phase 2 top-continue skips the whole param
/// loop), so the warning is pushed here, once per offending block, in block order — never
/// retrofitted in place (retrofitting would defer only the param, a subset-escape the cascade
/// cannot catch).
const MSG_ENUM_DEFER_PARAM: &str = "export subset: deferring block `{subject}` — parameter `{name}` is enumeration-valued \
     (class `{class}`); the block and its downstream consumers are omitted from the emitted \
     document so the enum-free remainder can export";

/// Deferral message for a cascade-deferred block: every driver of one of its input connectors was
/// itself deferred upstream. Pushed in Phase 1d over `deferred \ enum_blocks` as the cascade
/// fixpoint grows. `conn` is the input connector's owning-block-relative name.
const MSG_ENUM_DEFER_CASCADE: &str = "export subset: deferring block `{subject}` — all drivers of input connector `{conn}` were \
     deferred (upstream enumeration); the block is omitted from the emitted document so the \
     enum-free remainder can export";

/// The diagnostic subject for block `bi`: its `instance_iri`, else a synthetic `block#bi` tag
/// (mirrors the resolver's convention and `crate::export::owner_subject` for IRI-less blocks).
fn block_subject(g: &ModelGraph, bi: usize) -> String {
    g.blocks
        .get(bi)
        .and_then(|b| b.instance_iri.as_deref())
        .map_or_else(|| format!("block#{bi}"), str::to_owned)
}

/// The owning-block-relative name of connector `c` (`in{k}` / `out{k}` from the owner's port-list
/// position) for the cascade message — the same naming `crate::export::plan` mints for port
/// `@id`s, so a host can navigate from the warning to the deferred input.
fn connector_local_name(g: &ModelGraph, c_idx: usize) -> String {
    let Some(c) = g.connectors.get(c_idx) else {
        return format!("connector#{c_idx}");
    };
    let bi = c.block.0 as usize;
    let Some(b) = g.blocks.get(bi) else {
        return format!("connector#{c_idx}");
    };
    let list = if c.dir == oce_model::Dir::In {
        &b.inputs
    } else {
        &b.outputs
    };
    let k = list
        .iter()
        .position(|cid| cid.0 as usize == c_idx)
        .unwrap_or(0);
    let dir = if c.dir == oce_model::Dir::In {
        "in"
    } else {
        "out"
    };
    format!("{dir}{k}")
}

/// Whether block `bi` carries any enumeration-typed connector, returning the enum class id.
fn has_enum_connector(g: &ModelGraph, bi: usize) -> Option<EnumClassId> {
    let b = g.blocks.get(bi)?;
    b.inputs
        .iter()
        .chain(b.outputs.iter())
        .find_map(|cid| g.connectors.get(cid.0 as usize))
        .and_then(|c| match c.value_type {
            ValueType::Enum(id) => Some(id),
            _ => None,
        })
}

/// Whether block `bi` carries any enumeration-valued parameter, returning the first such
/// `(name, class)` for the warning.
fn has_enum_param(g: &ModelGraph, bi: usize) -> Option<(&str, EnumClassId)> {
    let b = g.blocks.get(bi)?;
    b.params.values.iter().find_map(|(name, v)| match v {
        Value::Enum { class, .. } => Some((name.as_ref(), *class)),
        _ => None,
    })
}

/// The connectors driving input connector `to_idx` (the `from` endpoints of every connection
/// whose `to` is `to_idx`) — the driver set for the cascade fixpoint. An input with no drivers in
/// the original graph returns an empty slice (a boundary `external_inputs` entry is undriven
/// inter-block and does NOT trigger a cascade).
fn drivers_of(g: &ModelGraph, to_idx: usize) -> Vec<usize> {
    g.connections
        .iter()
        .filter_map(|conn| (conn.to.0 as usize == to_idx).then_some(conn.from.0 as usize))
        .collect()
}

/// Compute the deferral set and its warnings: `enum_blocks` detected and warned, then the
/// transitive cascade grown to its fixpoint with a cascade warning per newly-deferred block.
/// Returns `(deferred, warnings)` in block-then-cascade order. The `deferred` set is the set of
/// block **indices** (`bi`) to omit from the emitted document; `warnings` are all
/// `ExportDeferred` (Warning) — never an error, so the three-state gate's Defer row is reachable.
pub(crate) fn deferral_set(g: &ModelGraph) -> (BTreeSet<usize>, Vec<Diagnostic>) {
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let mut deferred: BTreeSet<usize> = BTreeSet::new();

    // Phase 1a: enum_blocks = blocks carrying any enum connector OR any enum param.
    let mut enum_blocks: BTreeSet<usize> = BTreeSet::new();
    for (bi, _b) in g.blocks.iter().enumerate() {
        if has_enum_connector(g, bi).is_some() || has_enum_param(g, bi).is_some() {
            enum_blocks.insert(bi);
        }
    }

    // Phase 1b: warn over enum_blocks in block order. A block carrying BOTH an enum connector and
    // an enum param pushes exactly ONE warning (the connector arm — the first out-of-subset axis
    // found in block order), matching the pre-R7 single-diag-per-offender shape.
    for &bi in &enum_blocks {
        let subject = block_subject(g, bi);
        if let Some(id) = has_enum_connector(g, bi) {
            let class = enum_class_label(id);
            warnings.push(defer(
                MSG_ENUM_DEFER_CONNECTOR
                    .replace("{subject}", &subject)
                    .replace("{class}", class),
                &subject,
            ));
        } else if let Some((name, id)) = has_enum_param(g, bi) {
            let class = enum_class_label(id);
            warnings.push(defer(
                MSG_ENUM_DEFER_PARAM
                    .replace("{subject}", &subject)
                    .replace("{name}", name)
                    .replace("{class}", class),
                &subject,
            ));
        }
        deferred.insert(bi);
    }

    // Phase 1c: transitive cascade fixpoint. Repeat until `deferred` stops growing: for each
    // surviving block, for each input connector that had ≥1 driver in the original graph, if
    // EVERY driver's owning block is in `deferred`, the block is cascade-deferred. An input with
    // zero original drivers (a boundary input) is unaffected.
    let mut grew = true;
    while grew {
        grew = false;
        for (bi, b) in g.blocks.iter().enumerate() {
            if deferred.contains(&bi) {
                continue;
            }
            'inputs: for cid in &b.inputs {
                let cin = cid.0 as usize;
                let drivers = drivers_of(g, cin);
                if drivers.is_empty() {
                    continue; // undriven in the original (boundary) — not a cascade trigger
                }
                for d_idx in &drivers {
                    let Some(d) = g.connectors.get(*d_idx) else {
                        continue 'inputs; // out-of-range driver: do not cascade on a bad edge
                    };
                    if !deferred.contains(&(d.block.0 as usize)) {
                        continue 'inputs; // at least one surviving driver — input still driven
                    }
                }
                // Every driver is deferred → cascade-defer this block.
                deferred.insert(bi);
                grew = true;
                let subject = block_subject(g, bi);
                let conn = connector_local_name(g, cin);
                warnings.push(defer(
                    MSG_ENUM_DEFER_CASCADE
                        .replace("{subject}", &subject)
                        .replace("{conn}", &conn),
                    &subject,
                ));
                break 'inputs;
            }
        }
    }

    (deferred, warnings)
}

/// A short, stable label for an enum connector's class (the `EnumClassId` numeric id), used in the
/// deferral message's `{class}` slot. The minimal exporter deliberately does not build the
/// `EnumClassId → isOfDataType IRI` inverse table (that is a future R8), so the numeric id is the
/// honest class identifier available on the export side.
fn enum_class_label(id: EnumClassId) -> &'static str {
    match id.0 {
        0 => "EnumClass#0",
        1 => "EnumClass#1",
        _ => "EnumClass#N",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three deferral message consts are pinned verbatim — a host greps logs/exports for the
    /// stable `export subset: deferring block` prefix and the `{subject}`/`{class}`/`{name}`/`{conn}`
    /// placeholders are part of the contract. Removing or rewording any const breaks host pinning.
    /// The exact-string assertions below are the per-const pins (mirroring the exact-`message`
    /// pinning style of the rejection tests): a single character drift in any const fails loudly.
    #[test]
    fn deferral_message_consts_are_stable_and_templated() {
        assert_eq!(
            MSG_ENUM_DEFER_CONNECTOR,
            "export subset: deferring block `{subject}` — enumeration-typed connector (class \
             `{class}`) has no CXF literal form; the block and its downstream consumers are \
             omitted from the emitted document so the enum-free remainder can export",
            "connector deferral const must be byte-exact"
        );
        assert_eq!(
            MSG_ENUM_DEFER_PARAM,
            "export subset: deferring block `{subject}` — parameter `{name}` is \
             enumeration-valued (class `{class}`); the block and its downstream consumers are \
             omitted from the emitted document so the enum-free remainder can export",
            "param deferral const must be byte-exact"
        );
        assert_eq!(
            MSG_ENUM_DEFER_CASCADE,
            "export subset: deferring block `{subject}` — all drivers of input connector `{conn}` \
             were deferred (upstream enumeration); the block is omitted from the emitted document \
             so the enum-free remainder can export",
            "cascade deferral const must be byte-exact"
        );
    }
}
