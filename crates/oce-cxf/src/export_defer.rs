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
//! By the fixpoint, every survivor's driven inputs have at least one surviving driver, so no
//! survivor input loses its driver to deferral. Connections among survivors are intact and the
//! arity guard is preserved (every survivor keeps all declared ports). RT-2 holds for the survivor
//! cone.
//!
//! The fixpoint gives *at least* one surviving driver; it does not give *at most* one, and cannot
//! — deferral only ever removes edges, so it can lower an input's in-degree but never raise it. A
//! graph handed to `export` already carrying two drivers on one input still carries them after
//! deferral. That half of §7.10 is enforced in `crate::export::plan`, which counts surviving
//! drivers per connector and rejects an in-degree above 1. Earlier revisions of this paragraph
//! asserted single assignment as a property of the *input* ("original G36 graphs are
//! single-assignment"), which was an assumption about the caller stated as a fact about the
//! algorithm; nothing checked it, and a hand-built graph that broke it exported `Ok` with bytes
//! that failed re-import.

use std::collections::{BTreeSet, HashMap};

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{EnumClassId, ModelGraph, Value, ValueType};

/// A deferral warning: an `ExportDeferred` (Warning, non-aborting) diagnostic mirroring
/// `crate::export::reject` but building a warning rather than an error. The `subject` is the
/// deferred block's `instance_iri` (or a synthetic position tag when the block has none, matching
/// `crate::export::owner_subject`).
fn defer(message: impl Into<String>, subject: &str) -> Diagnostic {
    Diagnostic::warning(DiagCode::ExportDeferred, message).with_subject(subject.to_owned())
}

// Every deferral message is rendered by ONE `format!`. That is not a style choice: these messages
// interpolate host-supplied text — a block's `instance_iri`, a parameter's name — and a deferred
// block bypasses the survivor-side parameter-name validation, so both arrive unscreened from
// public `ModelGraph` state. Rendering by a chain of `str::replace` over a placeholder template
// re-scans what the previous step inserted, so a *value* shaped like a placeholder is rewritten by
// a later step: a parameter literally named `{class}` used to render as its own class label, and
// an `instance_iri` containing `{name}` or `{conn}` corrupted every later slot in all three shapes.
// `format!` interpolates its arguments without reparsing them, so no value can be read as
// template; it also makes a missing slot a compile error rather than a literal `{name}` shipped to
// an operator.

/// Deferral message for an enumeration-typed connector. Pushed in Phase 1b over `enum_blocks` for
/// each block whose connector list carries a `ValueType::Enum`. The connector arm in the
/// Phase 4 scan pushes only a placeholder (never this warning — the warning is pushed here, once
/// per offending block, in block order).
fn msg_enum_defer_connector(subject: &str, class: &str) -> String {
    format!(
        "export subset: deferring block `{subject}` — enumeration-typed connector (class \
         `{class}`) has no CXF literal form; the block and its downstream consumers are omitted \
         from the emitted document so the enum-free remainder can export"
    )
}

/// Deferral message for an enumeration-valued parameter. Pushed in Phase 1b over `enum_blocks`
/// for each block whose `ParamTable` carries a `Value::Enum`. The `param_binding` `Value::Enum`
/// arm is unreachable for any deferred block (the Phase 2 top-continue skips the whole param
/// loop), so the warning is pushed here, once per offending block, in block order — never
/// retrofitted in place (retrofitting would defer only the param, a subset-escape the cascade
/// cannot catch).
fn msg_enum_defer_param(subject: &str, name: &str, class: &str) -> String {
    format!(
        "export subset: deferring block `{subject}` — parameter `{name}` is enumeration-valued \
         (class `{class}`); the block and its downstream consumers are omitted from the emitted \
         document so the enum-free remainder can export"
    )
}

/// Deferral message for a cascade-deferred block: every driver of one of its input connectors was
/// itself deferred upstream. Pushed in Phase 1d over `deferred \ enum_blocks` as the cascade
/// fixpoint grows.
///
/// `conn` is `in{k}` for the triggering input's position in the DEFERRED BLOCK'S OWN `inputs`
/// vector — the same position `crate::export::plan` mints into that block's port `@id`. Not the
/// connector's "owning-block-relative" name: that phrasing belonged to a deleted helper which
/// resolved the list from `Connector::block`, the owner a connector *claims*, and so could name a
/// port in a different block entirely. The caveat on reading the pair as a navigable reference is
/// the `instance_iri`, not the position: `crate::export::plan` claims duplicate `@id`s only among
/// SURVIVORS, so two blocks sharing an `instance_iri` — one deferred, one not — leave the subject
/// ambiguous and `k` indexed against whichever block the reader picks.
fn msg_enum_defer_cascade(subject: &str, conn: &str) -> String {
    format!(
        "export subset: deferring block `{subject}` — all drivers of input connector `{conn}` \
         were deferred (upstream enumeration); the block is omitted from the emitted document so \
         the enum-free remainder can export"
    )
}

/// The diagnostic subject for block `bi`: its `instance_iri`, else a synthetic `block#bi` tag
/// (mirrors the resolver's convention and `crate::export::owner_subject` for IRI-less blocks).
fn block_subject(g: &ModelGraph, bi: usize) -> String {
    g.blocks
        .get(bi)
        .and_then(|b| b.instance_iri.as_deref())
        .map_or_else(|| format!("block#{bi}"), str::to_owned)
}

/// Whether block `bi` carries any enumeration-typed connector, returning the class id of the
/// FIRST one in `inputs`-then-`outputs` port-list order (the order the Phase 1b warning text
/// pins). The `find_map` closure searches the *enum predicate*, not merely the resolution: an
/// out-of-range connector id is skipped and the scan continues, and a block whose enum port sits
/// at any position — not just position 0 — is detected.
fn has_enum_connector(g: &ModelGraph, bi: usize) -> Option<EnumClassId> {
    let b = g.blocks.get(bi)?;
    b.inputs.iter().chain(b.outputs.iter()).find_map(|cid| {
        match g.connectors.get(cid.0 as usize)?.value_type {
            ValueType::Enum(id) => Some(id),
            _ => None,
        }
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
            warnings.push(defer(msg_enum_defer_connector(&subject, &class), &subject));
        } else if let Some((name, id)) = has_enum_param(g, bi) {
            let class = enum_class_label(id);
            warnings.push(defer(
                msg_enum_defer_param(&subject, name, &class),
                &subject,
            ));
        }
        deferred.insert(bi);
    }

    // Phase 1c: transitive cascade fixpoint. Index each input's drivers once, then decrement its
    // surviving-driver count when an owning block becomes deferred. `current_pass` and
    // `next_pass` preserve the former repeated ascending scan: a newly ready higher-index block is
    // still visited in this pass, while a lower-index block waits for the next one. This keeps the
    // warning order stable without rescanning every connection for every input on every pass.
    #[derive(Clone, Copy, Default)]
    struct DriverState {
        original: usize,
        remaining: usize,
        blocked: bool,
    }

    let mut input_uses: HashMap<u32, Vec<(usize, usize)>> = HashMap::new();
    let mut states = g
        .blocks
        .iter()
        .enumerate()
        .map(|(bi, block)| {
            for (k, input) in block.inputs.iter().enumerate() {
                input_uses.entry(input.0).or_default().push((bi, k));
            }
            vec![DriverState::default(); block.inputs.len()]
        })
        .collect::<Vec<_>>();
    let mut dependents = vec![Vec::<(usize, usize)>::new(); g.blocks.len()];
    for connection in &g.connections {
        let Some(uses) = input_uses.get(&connection.to.0) else {
            continue;
        };
        for &(bi, k) in uses {
            let state = &mut states[bi][k];
            state.original += 1;
            let Some(driver) = g.connectors.get(connection.from.0 as usize) else {
                state.blocked = true;
                continue;
            };
            let owner = driver.block.0 as usize;
            if !deferred.contains(&owner) {
                state.remaining += 1;
                if let Some(entries) = dependents.get_mut(owner) {
                    entries.push((bi, k));
                }
            }
        }
    }

    let ready = |state: &DriverState| state.original != 0 && state.remaining == 0 && !state.blocked;
    let mut current_pass = states
        .iter()
        .enumerate()
        .filter_map(|(bi, inputs)| {
            (!deferred.contains(&bi) && inputs.iter().any(&ready)).then_some(bi)
        })
        .collect::<BTreeSet<_>>();
    let mut next_pass = BTreeSet::new();
    while let Some(bi) = current_pass.pop_first() {
        if deferred.contains(&bi) {
            continue;
        }
        let Some(k) = states[bi].iter().position(&ready) else {
            continue;
        };
        deferred.insert(bi);
        let subject = block_subject(g, bi);
        let conn = format!("in{k}");
        warnings.push(defer(msg_enum_defer_cascade(&subject, &conn), &subject));

        for &(target_bi, target_k) in &dependents[bi] {
            if deferred.contains(&target_bi) {
                continue;
            }
            let state = &mut states[target_bi][target_k];
            let became_ready = state.remaining == 1 && state.original != 0 && !state.blocked;
            state.remaining = state.remaining.saturating_sub(1);
            if became_ready {
                if target_bi > bi {
                    current_pass.insert(target_bi);
                } else {
                    next_pass.insert(target_bi);
                }
            }
        }
        if current_pass.is_empty() {
            std::mem::swap(&mut current_pass, &mut next_pass);
        }
    }

    (deferred, warnings)
}

/// A short, stable label for an enum connector's class (the `EnumClassId` numeric id), used in the
/// deferral message's `{class}` slot. The minimal exporter deliberately does not build the
/// `EnumClassId → isOfDataType IRI` inverse table (that is a future R8), so the numeric id is the
/// honest class identifier available on the export side.
///
/// The label is injective in `id`: distinct classes never render the same text. That matters
/// because the id space is dense at both ends — `oce-model` pins `1..=4` for the CDL `Types`
/// classes and `101..=110` for the G36 ones — so any label that collapsed a tail range would make
/// most of a G36 model's enum classes indistinguishable in a warning a host shows a technician.
fn enum_class_label(id: EnumClassId) -> String {
    format!("EnumClass#{}", id.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use oce_model::{BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ParamTable};

    fn repeated_scan_reference(g: &ModelGraph) -> (BTreeSet<usize>, Vec<Diagnostic>) {
        let mut warnings = Vec::new();
        let enum_blocks = g
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(bi, _)| {
                (has_enum_connector(g, bi).is_some() || has_enum_param(g, bi).is_some())
                    .then_some(bi)
            })
            .collect::<BTreeSet<_>>();
        let mut deferred = enum_blocks.clone();
        for &bi in &enum_blocks {
            let subject = block_subject(g, bi);
            if let Some(id) = has_enum_connector(g, bi) {
                warnings.push(defer(
                    msg_enum_defer_connector(&subject, &enum_class_label(id)),
                    &subject,
                ));
            } else if let Some((name, id)) = has_enum_param(g, bi) {
                warnings.push(defer(
                    msg_enum_defer_param(&subject, name, &enum_class_label(id)),
                    &subject,
                ));
            }
        }

        let mut grew = true;
        while grew {
            grew = false;
            for (bi, block) in g.blocks.iter().enumerate() {
                if deferred.contains(&bi) {
                    continue;
                }
                'inputs: for (k, input) in block.inputs.iter().enumerate() {
                    let drivers = g.connections.iter().filter_map(|connection| {
                        (connection.to == *input).then_some(connection.from)
                    });
                    let mut found = false;
                    for driver_id in drivers {
                        found = true;
                        let Some(driver) = g.connectors.get(driver_id.0 as usize) else {
                            continue 'inputs;
                        };
                        if !deferred.contains(&(driver.block.0 as usize)) {
                            continue 'inputs;
                        }
                    }
                    if found {
                        deferred.insert(bi);
                        grew = true;
                        let subject = block_subject(g, bi);
                        warnings.push(defer(
                            msg_enum_defer_cascade(&subject, &format!("in{k}")),
                            &subject,
                        ));
                        break 'inputs;
                    }
                }
            }
        }
        (deferred, warnings)
    }

    fn chain_graph(block_count: usize, reverse: bool) -> ModelGraph {
        let enum_block = if reverse { block_count - 1 } else { 0 };
        let mut graph = ModelGraph::default();
        for bi in 0..block_count {
            let block_id = BlockId(bi as u32);
            let input_id = ConnectorId((bi * 2) as u32);
            let output_id = ConnectorId(input_id.0 + 1);
            graph.connectors.push(Connector::new(
                input_id,
                block_id,
                Dir::In,
                ValueType::Real,
                input_id.0,
            ));
            graph.connectors.push(Connector::new(
                output_id,
                block_id,
                Dir::Out,
                if bi == enum_block {
                    ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER)
                } else {
                    ValueType::Real
                },
                output_id.0,
            ));
            graph.blocks.push(BlockInstance {
                id: block_id,
                class_iri: Arc::from("CDL.Reals.Add"),
                inputs: vec![input_id],
                outputs: vec![output_id],
                params: ParamTable::default(),
                decl_order: bi as u32,
                instance_iri: Some(Arc::from(format!("block{bi}"))),
            });
        }
        for bi in 0..block_count - 1 {
            let (driver, target) = if reverse { (bi + 1, bi) } else { (bi, bi + 1) };
            graph.connections.push(Connection {
                from: ConnectorId((driver * 2 + 1) as u32),
                to: ConnectorId((target * 2) as u32),
            });
        }
        graph
    }

    #[test]
    fn ordered_worklist_matches_repeated_scans_on_both_chain_directions() {
        for block_count in [2, 3, 8, 32, 128] {
            for reverse in [false, true] {
                let graph = chain_graph(block_count, reverse);
                assert_eq!(
                    deferral_set(&graph),
                    repeated_scan_reference(&graph),
                    "block_count={block_count}, reverse={reverse}"
                );
            }
        }
    }

    #[test]
    fn ordered_worklist_matches_every_three_block_topology() {
        for enum_mask in 0_u8..8 {
            for edge_mask in 0_u8..64 {
                let mut graph = chain_graph(3, false);
                graph.connections.clear();
                for bi in 0..3 {
                    graph.connectors[bi * 2 + 1].value_type = if enum_mask & (1 << bi) != 0 {
                        ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER)
                    } else {
                        ValueType::Real
                    };
                }
                let mut edge_bit = 0;
                for driver in 0..3 {
                    for target in 0..3 {
                        if driver == target {
                            continue;
                        }
                        if edge_mask & (1 << edge_bit) != 0 {
                            graph.connections.push(Connection {
                                from: ConnectorId((driver * 2 + 1) as u32),
                                to: ConnectorId((target * 2) as u32),
                            });
                        }
                        edge_bit += 1;
                    }
                }
                assert_eq!(
                    deferral_set(&graph),
                    repeated_scan_reference(&graph),
                    "enum_mask={enum_mask:03b}, edge_mask={edge_mask:06b}"
                );
            }
        }
    }

    /// The three rendered messages are pinned verbatim — a host greps logs/exports for the stable
    /// `export subset: deferring block` prefix and the sentence shape around it. A single
    /// character of drift fails loudly.
    ///
    /// These replace the template pins that stood here while the messages were consts rendered by
    /// a `str::replace` chain. Pinning the rendered output is the stronger property: a template
    /// can be byte-perfect and still reach an operator wrong, which is exactly what the
    /// substitution defect did.
    #[test]
    fn rendered_deferral_messages_are_byte_exact() {
        assert_eq!(
            msg_enum_defer_connector("blk", "EnumClass#2"),
            "export subset: deferring block `blk` — enumeration-typed connector (class \
             `EnumClass#2`) has no CXF literal form; the block and its downstream consumers are \
             omitted from the emitted document so the enum-free remainder can export"
        );
        assert_eq!(
            msg_enum_defer_param("blk", "controllerType", "EnumClass#1"),
            "export subset: deferring block `blk` — parameter `controllerType` is \
             enumeration-valued (class `EnumClass#1`); the block and its downstream consumers are \
             omitted from the emitted document so the enum-free remainder can export"
        );
        assert_eq!(
            msg_enum_defer_cascade("blk", "in1"),
            "export subset: deferring block `blk` — all drivers of input connector `in1` were \
             deferred (upstream enumeration); the block is omitted from the emitted document so \
             the enum-free remainder can export"
        );
    }

    /// A value shaped like a placeholder is data, not template. Under the `str::replace` chain
    /// each step re-scanned what the previous step inserted, so a parameter named `{class}` came
    /// out as its own class label and an `instance_iri` carrying `{name}`/`{conn}`/`{class}`
    /// rewrote every later slot. Rendering in one pass makes each argument land verbatim.
    ///
    /// Every argument is given a placeholder-shaped value naming a *different* slot than the one
    /// it occupies, so a renderer that re-scanned would visibly swap them.
    #[test]
    fn a_placeholder_shaped_value_renders_literally() {
        let connector = msg_enum_defer_connector("blk{class}", "EnumClass#2");
        assert!(
            connector.contains("block `blk{class}`") && connector.contains("class `EnumClass#2`"),
            "the subject must keep its literal braces: {connector}"
        );

        let param = msg_enum_defer_param("blk{name}", "{class}", "EnumClass#3");
        assert!(
            param.contains("block `blk{name}`")
                && param.contains("parameter `{class}`")
                && param.contains("class `EnumClass#3`"),
            "subject, name, and class must each land verbatim: {param}"
        );

        let cascade = msg_enum_defer_cascade("blk{conn}", "in0");
        assert!(
            cascade.contains("block `blk{conn}`")
                && cascade.contains("input connector `in0`")
                && !cascade.contains("blkin0"),
            "the subject's braces must not be consumed by the connector slot: {cascade}"
        );
    }

    /// No slot ordering can corrupt another slot's value — the property the test above only
    /// half-proves.
    ///
    /// A sequential renderer corrupts slot `A` when `A` is filled *before* `B` and `A`'s value
    /// contains `B`'s marker. The case above gives every value a marker naming a *later* slot, so
    /// it kills the original `subject → name → class` order and nothing else: a review swapped in
    /// a `class → name → subject` chain and the whole suite stayed green while a parameter named
    /// `{subject}` rendered as the block IRI.
    ///
    /// So each argument here carries the markers of *both* other slots, which leaves no order to
    /// be lucky in — whichever slot a chain fills first, its value carries a marker a later step
    /// would rewrite. Only the class and connector slots are exempt, and by construction rather
    /// than by luck: `enum_class_label` renders `EnumClass#<digits>` and the cascade's connector
    /// slot is `format!("in{k}")` over a port-list index, so neither can carry a brace to begin
    /// with. Both are derived from numbers, never from host-supplied text.
    #[test]
    fn no_slot_ordering_can_corrupt_another_slots_value() {
        let param = msg_enum_defer_param("blk{name}{class}", "{subject}{class}", "EnumClass#3");
        assert!(
            param.contains("block `blk{name}{class}`"),
            "a later step must not rewrite the subject's markers: {param}"
        );
        assert!(
            param.contains("parameter `{subject}{class}`"),
            "a later step must not rewrite the name's markers: {param}"
        );
        assert!(
            param.contains("class `EnumClass#3`"),
            "the class slot must still be filled: {param}"
        );

        let connector = msg_enum_defer_connector("blk{class}{subject}", "EnumClass#2");
        assert!(
            connector.contains("block `blk{class}{subject}`")
                && connector.contains("class `EnumClass#2`"),
            "a self-naming marker in the subject must survive too: {connector}"
        );

        // `in2`, not `out2`: the cascade loop iterates `b.inputs` only, so `in{k}` is the sole
        // shape the connector slot can ever carry. An `out{k}` literal here would pin a string
        // the code cannot produce.
        let cascade = msg_enum_defer_cascade("blk{conn}{subject}", "in2");
        assert!(
            cascade.contains("block `blk{conn}{subject}`")
                && cascade.contains("input connector `in2`"),
            "the cascade subject's markers must survive both orders: {cascade}"
        );
    }

    /// Every enum class `oce-model` pins gets its own label. The previous label folded every id
    /// of 2 or more into the literal `EnumClass#N`, so ten of the fourteen pinned classes — the
    /// whole G36 block, `101..=110` — read identically in a deferral warning, and a technician
    /// told "class `EnumClass#N`" learned nothing about which type stopped the export.
    #[test]
    fn every_pinned_enum_class_gets_a_distinct_label() {
        let pinned = [
            EnumClassId::SIMPLE_CONTROLLER,
            EnumClassId::SMOOTHNESS,
            EnumClassId::EXTRAPOLATION,
            EnumClassId::ZERO_TIME,
            EnumClassId::G36_ASHRAE_CLIMATE_ZONE,
            EnumClassId::G36_CONTROL_ECONOMIZER,
            EnumClassId::G36_COOLING_COIL,
            EnumClassId::G36_ENERGY_STANDARD,
            EnumClassId::G36_FREEZE_STAT,
            EnumClassId::G36_HEATING_COIL,
            EnumClassId::G36_OUTDOOR_AIR_SECTION,
            EnumClassId::G36_PRESSURE_CONTROL,
            EnumClassId::G36_TITLE24_CLIMATE_ZONE,
            EnumClassId::G36_VENTILATION_STANDARD,
        ];
        let labels: BTreeSet<String> = pinned.iter().copied().map(enum_class_label).collect();
        assert_eq!(
            labels.len(),
            pinned.len(),
            "each pinned class needs its own label, got: {labels:?}"
        );
    }

    /// The label shape itself, pinned: `EnumClass#` plus the decimal `EnumClassId` — what the
    /// function's rustdoc promises and what a host greps for. Covers the id that used to be the
    /// fold's first casualty (`SMOOTHNESS` = 2) and one from the G36 range.
    #[test]
    fn the_label_renders_the_decimal_class_id() {
        assert_eq!(enum_class_label(EnumClassId::SMOOTHNESS), "EnumClass#2");
        assert_eq!(enum_class_label(EnumClassId::EXTRAPOLATION), "EnumClass#3");
        assert_eq!(
            enum_class_label(EnumClassId::G36_VENTILATION_STANDARD),
            "EnumClass#110"
        );
        assert_eq!(enum_class_label(EnumClassId(0)), "EnumClass#0");
    }
}
