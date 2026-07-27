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
/// fixpoint grows. `conn` is the input connector's owning-block-relative name.
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
                warnings.push(defer(msg_enum_defer_cascade(&subject, &conn), &subject));
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
    /// than by luck: `enum_class_label` and `connector_local_name` both derive their text
    /// (`EnumClass#<digits>`, `in<k>`/`out<k>`), so neither can carry a brace to begin with.
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

        let cascade = msg_enum_defer_cascade("blk{conn}{subject}", "out2");
        assert!(
            cascade.contains("block `blk{conn}{subject}`")
                && cascade.contains("input connector `out2`"),
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
