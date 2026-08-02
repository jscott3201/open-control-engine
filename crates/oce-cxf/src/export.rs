//! Minimal CXF exporter (#141): emit a flat, ground, single-root, scalar-parameter
//! [`ModelGraph`] as a CXF JSON-LD document, carrying the in-subset §7.4.1 connector attributes
//! under the **Bare-Scalar Canonical** wire shape.
//!
//! The contract is the RT-2 fixpoint, not source recovery: for a graph `G1` produced by
//! [`import_cxf`](crate::import_cxf), `import(export(G1))` lowers to a graph that renders
//! bit-identically (floats compared by IEEE-754 bits) to `G1` — or, when enum deferral fires, to
//! `G1` restricted to its surviving blocks, since the deferred ones are never emitted (see
//! Rejection surface below). Cosmetic source content (labels, layout, line numbers) is not in
//! `ModelGraph`, so none of it comes back.
//!
//! ## Naming model
//! - The original root `@id` is not recorded in `ModelGraph`; the fixed synthetic
//!   [`EXPORT_ROOT_IRI`] stands in for it — re-import needs *a* root, not the original.
//! - Block node `@id`s are [`oce_model::BlockInstance::instance_iri`] verbatim (absent →
//!   rejected).
//! - Parameter node `@id`s are `<instance_iri>.<name>`; the importer recovers the parameter name
//!   as the segment after the last `.`, so names containing `.` are rejected.
//! - Ordinary imported connectors retain their authored IRI, which is emitted verbatim so port
//!   identity survives an RT-2 round trip. A port `@id` is minted deterministically as
//!   `<instance_iri>.in<k>` / `<instance_iri>.out<k>` only when the connector has no usable own
//!   IRI, including a boundary-driven child whose stored IRI belongs to the boundary node.
//! - Each `external_inputs` connector's stored boundary IRI is emitted verbatim as a boundary
//!   node listed under the root's `hasInput` and wired `isConnectedTo` → the child
//!   port. The boundary node carries the driven child connector's `isOfDataType`; fan-out is
//!   rejected if the driven connector types disagree. Re-import then re-elides the boundary
//!   (AD-2), restoring both the `external_inputs` entry and the child connector's boundary IRI.
//!   The boundary `@id` never appears in the owning block's own `hasInput` list — sharing one
//!   `@id` between the root's and a child's port list re-imports as a rejection.
//!
//! ## §7.4.1 connector attributes (Bare-Scalar Canonical)
//! The five in-subset §7.4.1 attrs live on the **child port node** (never the shared
//! boundary node), each emitted as a **bare JSON scalar/string** — `S231:unit`/`quantity`/
//! `displayUnit` as bare strings ([`crate::dto::TermAttr::Bare`], Real only) and `S231:min`/`max` as bare
//! numbers ([`CxfValue::Float`] for Real, [`CxfValue::Int`] for Integer). Bare is the unique
//! shape that survives the importer's `as_term()` collapse and reproduces itself, so the RT-2
//! render fixpoint holds on an imported `G`. Attributes are emitted iff `Some` (the stored
//! field default is `None`); `None` fields are omitted, so an all-default connector emits zero
//! attr keys — byte-identical to an attr-free port node. `nominal`/`unbounded` (the importer
//! hardcodes `None`) and non-finite Real `min`/`max` (which serialize as JSON `null`) are
//! rejected, not emitted — see [`classify_attrs`].
//!
//! ## Determinism
//! Emission order derives from the `ModelGraph` vectors alone: root, then blocks in `BlockId`
//! order (each followed by its parameter nodes in `ParamTable` order), then one port node per
//! connector in `ConnectorId` order, then boundary nodes in `external_inputs` first-occurrence
//! order. No map iteration feeds any ordering, and `write_document` serializes struct fields in
//! declaration order, so repeated exports are byte-identical.
//!
//! ## Value shape
//! Parameter bindings are bare JSON literals. `Real`s always serialize with a fractional part
//! (serde_json formats every finite `f64` with a `.` or exponent), so a whole-number Real never
//! re-grounds as an Integer; `Integer`s serialize bare, so they never come back as Reals.
//! Non-finite Reals would serialize as JSON `null` and are rejected instead. Strings are emitted
//! as quoted CDL string-literal expressions (`oce-expr` grounds them back to `Value::String`).
//!
//! ## Rejection surface
//! Everything outside the subset is a typed [`CxfError::Validation`] carrying
//! [`oce_diag::DiagCode::ExportUnsupported`] error diagnostics whose `subject` is the owning
//! block's `instance_iri` (connectors have none) — never a panic.
//!
//! Two of those rejections are not about the subset at all but about bytes that would not come
//! back: a connector listed twice in `external_inputs` (which re-imports one entry short) and an
//! input driven by more than one surviving output (which fails §7.10 single assignment and does
//! not re-import at all). Both are judged over the **survivor cone**, so a defect sitting entirely
//! inside a deferred block never aborts an export whose document omits that block.
//!
//! Enum deferral is the one non-aborting axis: a deferred block contributes no error diagnostics
//! at all (not from its connectors' attributes, not from its boundary entries), so a partly
//! enum-bearing graph exports its enum-free remainder with `ExportDeferred` **warnings**. The
//! exception is a non-empty graph with zero emitted runtime blocks after deferred and reserved
//! lowering-only blocks are omitted: that would be an unloadable root-only shell, so it is an
//! error ([`MSG_TOTAL_DEFERRAL`]).

use std::collections::{BTreeMap, BTreeSet};

use oce_diag::{DiagCode, Diagnostic, has_errors};
use oce_model::{Attrs, Connector, Dir, ModelGraph, Value, ValueType};

use crate::dto::{Context, CxfDocument, CxfValue, IriRef, Node, OneOrMany};
use crate::export_attrs::{PortAttrs, emit_port_attrs};
use crate::export_defer::deferral_set;
use crate::export_pass_through::has_valid_shape;
use crate::{CxfError, bridge};

/// `@id` of the synthesized root composite. `ModelGraph` does not record the source document's
/// root IRI, so every export uses this fixed, collision-improbable URN. A (contrived) block whose
/// `instance_iri` equals it would surface as a loud `DuplicateId` on re-import, never silently.
const EXPORT_ROOT_IRI: &str = "urn:open-control:cxf-export:root";

/// The `S231` prefix binding emitted in `@context` (matches the import fixtures).
const S231_CONTEXT_IRI: &str = "http://data.ashrae.org/S231P#";

/// Base IRI for emitted instance `@type`s: `<base>Buildings.Controls.OBC.<class_path>` — the
/// inverse of [`bridge::class_path_of`], using the same base the import fixtures use. The
/// planner verifies each emitted `@type` re-bridges to the identical `class_iri` before
/// accepting the block.
const CLASS_IRI_BASE: &str = "http://example.org#";

/// Whole-operation rejection for a zero-block graph: a root with no `containsBlock` is not a
/// runtime composite, and a root-only document re-imports as a zero-candidate-root
/// `MalformedDocument` — there is nothing warning-free to emit.
const MSG_EMPTY: &str =
    "CXF export requires at least one block: an empty ModelGraph has no runtime composite to emit";
/// Whole-operation rejection when no emitted runtime block remains after deferred and reserved
/// lowering-only blocks are omitted. In that state `build()` would emit the same unloadable
/// root-only document `MSG_EMPTY` names, despite receiving a non-empty input graph.
const MSG_TOTAL_DEFERRAL: &str = "CXF export requires at least one emitted runtime block: all blocks \
     were deferred or reserved lowering-only, leaving no runtime composite to emit";
/// A block without an `instance_iri` cannot name its CXF node (hand-built graphs only; every
/// imported block carries one).
const MSG_NO_INSTANCE_IRI: &str = "export subset: block has no instance_iri to name its CXF node";
/// The class path cannot round-trip through the class-IRI bridge: a `#` in it changes the
/// re-imported fragment (a silent identity flip), and a path already carrying the OBC prefix
/// re-bridges verbatim but can never name a registry class, so its bytes would always fail
/// re-import — both are rejected under this message.
const MSG_CLASS_BRIDGE: &str =
    "export subset: class path does not survive the class-IRI bridge round-trip";
/// Block/connector cross-references disagree (non-dense ids, wrong owner or direction, a
/// connector claimed twice or never, a connection endpoint out of range or not output→input).
const MSG_STRUCTURE: &str = "export subset: block/connector wiring is structurally inconsistent";
/// The connector carries a non-default `nominal` attribute (the importer hardcodes
/// `nominal: None`, so any `Some` is outside the canonical export subset and would be silently
/// dropped rather than round-tripped).
const MSG_ATTR_NOMINAL: &str = "export subset: connector carries a non-default §7.4.1 nominal attribute, \
     which is outside the canonical (bare-scalar) export subset";
/// The connector carries a non-default `unbounded` attribute (the importer hardcodes
/// `unbounded: None`, so any `Some` is outside the canonical export subset).
const MSG_ATTR_UNBOUNDED: &str = "export subset: connector carries a non-default §7.4.1 unbounded attribute, \
     which is outside the canonical (bare-scalar) export subset";
/// A Real `min`/`max` bound is non-finite (`NaN`/`INFINITY`/`NEG_INFINITY`). `serde_json`
/// serializes a non-finite `f64` as JSON `null`, which re-imports as `None`, silently breaking
/// the RT-2 render fixpoint — so the bound is rejected instead of emitted.
const MSG_ATTR_NONFINITE_BOUND: &str = "export subset: connector carries a non-finite §7.4.1 min/max bound, \
     which is outside the canonical (bare-scalar) export subset";
/// String connectors have no importable CXF form (§7.8 forbids them).
const MSG_STRING_CONNECTOR: &str =
    "export subset: String connectors are not permitted in CXF (§7.8)";
/// Enumeration connectors need an enum-class IRI inverse mapping the minimal exporter lacks.
const MSG_ENUM_CONNECTOR: &str =
    "export subset: enumeration-typed connectors are outside the minimal export subset";
/// An `external_inputs` entry whose connector has no stored boundary IRI cannot rebuild the
/// root's `hasInput` (hand-built graphs only; the resolver always stores it).
const MSG_EXTERNAL_IRI: &str =
    "export subset: external input carries no boundary IRI to rebuild the root hasInput";
/// The same connector is listed twice in `external_inputs`. Phase 6 groups boundary targets by
/// IRI, so the repeat only re-pushes an `isConnectedTo` target the node already carries, and the
/// importer deduplicates it away (`resolve`'s `external_inputs.contains` guard). The export would
/// be `Ok` with bytes that come back one entry short — silent round-trip loss, which this exporter
/// rejects rather than emits. Hand-built graphs only: the resolver dedupes on the way in, so no
/// imported graph can reach this.
const MSG_DUPLICATE_EXTERNAL_INPUT: &str = "export subset: a connector is listed more than once in external_inputs, and re-import \
     deduplicates the repeat away";
/// One boundary IRI groups multiple driven child inputs whose value types differ. A single
/// boundary node can carry only one datatype, and any choice would fail the importer's boundary
/// elision type check for at least one target.
const MSG_BOUNDARY_TYPE_MISMATCH: &str =
    "export subset: one boundary input drives child inputs with different value types";
/// A surviving input connector is driven by more than one **surviving** output. The emitted
/// document carries every one of those `isConnectedTo` targets, so re-import counts an in-degree
/// above 1 and fails the §7.10 single-assignment law — the export would be `Ok` with bytes that do
/// not load at all, a strictly worse silent loss than [`MSG_DUPLICATE_EXTERNAL_INPUT`]'s one
/// dropped entry. Deliberately not a reuse of [`MSG_STRUCTURE`]: a multiply-driven input is
/// internally *consistent* (ids in range, owners correct, every edge output→input) and needs a
/// different fix from the host than a cross-reference disagreement does. Hand-built graphs only —
/// `resolve`'s single-assignment pre-check rejects in-degree ≥ 2 on the way in, so no imported
/// graph can reach this.
const MSG_MULTIPLY_DRIVEN: &str = "export subset: input connector is driven by more than one surviving output, and the emitted \
     document fails re-import with a single-assignment error (§7.10)";
/// Two emitted nodes would share one `@id` — duplicate block `instance_iri`s, a parameter named
/// like a minted port (`out0`), a boundary IRI reusing another node's id. The document would
/// fail re-import with `DuplicateId`; reject at plan time with the owner named instead.
const MSG_DUPLICATE_ID: &str =
    "export subset: emitted node @id collides with another emitted node @id";

/// Everything [`build`] needs, fully validated: no `Option` left to unwrap, no index that can
/// miss. Produced by [`plan`] only when the subset checks all pass.
struct Plan {
    blocks: Vec<PlannedBlock>,
    ports: Vec<PlannedPort>,
    boundaries: Vec<PlannedBoundary>,
    boundary_outputs: Vec<PlannedBoundaryOutput>,
}

/// One block node plus its parameter nodes, with every `@id` already minted.
struct PlannedBlock {
    iri: String,
    type_iri: String,
    /// `(node @id, binding)` in `ParamTable` order.
    params: Vec<(String, CxfValue)>,
    input_ports: Vec<String>,
    output_ports: Vec<String>,
}

/// One port node: minted `@id`, `isOfDataType` term, `isConnectedTo` targets (outputs only),
/// and the classified §7.4.1 attributes to emit (Bare-Scalar Canonical).
struct PlannedPort {
    iri: String,
    datatype: &'static str,
    targets: Vec<String>,
    attrs: PortAttrs,
}

/// One boundary-input node: stored boundary IRI, datatype, and minted child ports it drives.
struct PlannedBoundary {
    iri: String,
    datatype: &'static str,
    targets: Vec<String>,
}

/// One boundary-output node emitted only for a reserved pass-through pair.
struct PlannedBoundaryOutput {
    iri: String,
    datatype: &'static str,
}

fn is_pass_through_class(class_path: &str) -> bool {
    matches!(
        class_path,
        "urn:oce:lowering#PassThrough.Real"
            | "urn:oce:lowering#PassThrough.Integer"
            | "urn:oce:lowering#PassThrough.Boolean"
    )
}

/// Export `model` as a CXF document plus the deferral warnings (empty for a fully-in-subset
/// graph). The warnings are `ExportDeferred` (Warning, non-aborting) — they surface via
/// [`crate::export_with_report`] and are discarded by `crate::export`.
///
/// # Errors
/// [`CxfError::Validation`] when any **error**-severity subset check fails (the three-state gate
/// treats `ExportDeferred` warnings as non-aborting); see the module docs for the surface.
pub(crate) fn document(model: &ModelGraph) -> Result<(CxfDocument, Vec<Diagnostic>), CxfError> {
    plan(model)
        .map(|(plan, warnings)| (build(plan), warnings))
        .map_err(CxfError::Validation)
}

/// An `ExportUnsupported` error diagnostic with a subject.
fn reject(message: impl Into<String>, subject: &str) -> Diagnostic {
    Diagnostic::error(DiagCode::ExportUnsupported, message).with_subject(subject.to_owned())
}

/// Record an `@id` the document will emit. A repeat is rejected at plan time with the owning
/// block as subject — the emitted document would otherwise fail re-import with `DuplicateId`
/// (or worse, silently merge two nodes in a permissive consumer).
fn claim_emitted_id(
    seen: &mut BTreeSet<String>,
    id: &str,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) {
    if !seen.insert(id.to_owned()) {
        diags.push(reject(MSG_DUPLICATE_ID, subject));
    }
}

/// The diagnostic subject for a connector: its owning block's `instance_iri`, else a synthetic
/// position tag (mirrors the resolver's `connector#N` convention for IRI-less connectors).
fn owner_subject(g: &ModelGraph, c: &Connector, position: usize) -> String {
    g.blocks
        .get(c.block.0 as usize)
        .and_then(|b| b.instance_iri.as_deref())
        .map_or_else(|| format!("connector#{position}"), str::to_owned)
}

/// Validate the export subset and mint every `@id`. Returns either the plan plus the deferral
/// warnings (empty for a fully-in-subset graph), or the full **error**-severity diagnostic list
/// (all offenders, in block-then-connector-then-connection scan order) when any error-severity
/// check fails. `ExportDeferred` warnings are non-aborting (the three-state gate). All ordering
/// derives from the `ModelGraph` vectors.
fn plan(g: &ModelGraph) -> Result<(Plan, Vec<Diagnostic>), Vec<Diagnostic>> {
    if g.blocks.is_empty() {
        return Err(vec![Diagnostic::error(
            DiagCode::ExportUnsupported,
            MSG_EMPTY,
        )]);
    }
    // Dense-id invariant: every position-based index below (and re-import itself) assumes
    // `BlockId.0`/`ConnectorId.0` equal vector position. The resolver guarantees it; a
    // hand-built graph that breaks it is rejected wholesale rather than exported shifted.
    let dense = g
        .blocks
        .iter()
        .enumerate()
        .all(|(i, b)| b.id.0 as usize == i)
        && g.connectors
            .iter()
            .enumerate()
            .all(|(i, c)| c.id.0 as usize == i);
    if !dense {
        return Err(vec![Diagnostic::error(
            DiagCode::ExportUnsupported,
            MSG_STRUCTURE,
        )]);
    }

    // Phase 1 — DEFERRAL PRE-PASS: detect enum-carrying blocks, grow `deferred` to its transitive
    // cascade fixpoint, and emit the `ExportDeferred` warnings (Warning, non-aborting). A
    // deferred block contributes ZERO entries to the plan below (no `@id` claimed, no param
    // diag, no `PlannedBlock`); its connectors are placeholder-indexed but never emitted.
    let (deferred, warnings) = deferral_set(g);
    let mut diags: Vec<Diagnostic> = warnings;
    let mut port_iri: Vec<Option<String>> = vec![None; g.connectors.len()];
    let mut blocks: Vec<PlannedBlock> = Vec::with_capacity(g.blocks.len());
    // Every @id the document will carry, for duplicate detection only — membership checks,
    // never iteration, so emission order still derives from the ModelGraph vectors alone.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    seen.insert(EXPORT_ROOT_IRI.to_owned());

    // Phase 2 — per-block plan loop. The `deferred.contains(bi)` continue is the FIRST statement
    // of the loop body, BEFORE the subject mint, `claim_emitted_id`, the class-bridge check, the
    // param loop, and `blocks.push`. Placing it later leaks the deferred block's `@id` into
    // `seen` (poisoning duplicate detection for survivors) and/or pushes a `PlannedBlock` for a
    // deferred block (subset-escape). The `param_binding` `Value::Enum` arm is thus unreachable
    // for any deferred block (the whole param loop is skipped).
    for (bi, b) in g.blocks.iter().enumerate() {
        if deferred.contains(&bi) {
            continue;
        }
        if is_pass_through_class(&b.class_iri) {
            continue;
        }
        let subject: String = match b.instance_iri.as_deref() {
            Some(iri) => iri.to_owned(),
            None => {
                let synthetic = format!("block#{bi}");
                diags.push(reject(MSG_NO_INSTANCE_IRI, &synthetic));
                synthetic
            }
        };
        claim_emitted_id(&mut seen, &subject, &subject, &mut diags);
        let type_iri = format!("{CLASS_IRI_BASE}{}{}", bridge::OBC_PREFIX, b.class_iri);
        // Self-oracle: run the actual import bridge over the @type we are about to emit — anything
        // that would re-import under a different class identity is rejected here, loudly. An
        // already-OBC-prefixed class path passes that round-trip (the bridge strips exactly the
        // one prefix we just added) yet can never name a registry class, so it is rejected
        // explicitly rather than emitted as bytes that always fail re-import.
        if b.class_iri.starts_with(bridge::OBC_PREFIX)
            || bridge::class_path_of(&type_iri) != b.class_iri.as_ref()
        {
            diags.push(reject(MSG_CLASS_BRIDGE, &subject));
        }

        let mut params = Vec::with_capacity(b.params.values.len());
        for (name, value) in &b.params.values {
            if name.is_empty() || name.contains('.') {
                diags.push(reject(
                    format!(
                        "export subset: parameter name `{name}` is not a bare member name \
                         (re-import recovers the name after the last `.`)"
                    ),
                    &subject,
                ));
                continue;
            }
            if let Some(binding) = param_binding(name, value, &subject, &mut diags) {
                let param_id = format!("{subject}.{name}");
                claim_emitted_id(&mut seen, &param_id, &subject, &mut diags);
                params.push((param_id, binding));
            }
        }

        let mut input_ports = Vec::with_capacity(b.inputs.len());
        for (k, cid) in b.inputs.iter().enumerate() {
            // Boundary-input elision intentionally replaces the child input's IRI with the
            // boundary IRI. Keep synthesising that child port so the boundary node and child port
            // remain distinct; all other imported connectors retain their authored identity.
            let minted = if g.external_inputs.contains(cid) {
                format!("{subject}.in{k}")
            } else {
                g.connectors[cid.0 as usize]
                    .iri
                    .as_deref()
                    .map_or_else(|| format!("{subject}.in{k}"), str::to_owned)
            };
            if claim_port(
                g,
                bi,
                cid.0,
                Dir::In,
                &minted,
                &mut port_iri,
                &subject,
                &mut diags,
            ) {
                claim_emitted_id(&mut seen, &minted, &subject, &mut diags);
                input_ports.push(minted);
            }
        }
        let mut output_ports = Vec::with_capacity(b.outputs.len());
        for (k, cid) in b.outputs.iter().enumerate() {
            let minted = g.connectors[cid.0 as usize]
                .iri
                .as_deref()
                .map_or_else(|| format!("{subject}.out{k}"), str::to_owned);
            if claim_port(
                g,
                bi,
                cid.0,
                Dir::Out,
                &minted,
                &mut port_iri,
                &subject,
                &mut diags,
            ) {
                claim_emitted_id(&mut seen, &minted, &subject, &mut diags);
                output_ports.push(minted);
            }
        }

        blocks.push(PlannedBlock {
            iri: subject,
            type_iri,
            params,
            input_ports,
            output_ports,
        });
    }

    // Phase 2b — TOTAL-DEFERRAL gate. Phase 2 pushed one `PlannedBlock` per survivor, so an empty
    // `blocks` here means the cascade swallowed every block of a non-empty graph (the `is_empty`
    // guard at the top of `plan` sees only the INPUT graph, which is why this second check is
    // needed). Left alone, `build()` emits a root with no `containsBlock` and the exporter returns
    // `Ok` with bytes that fail re-import as `MalformedDocument`. An error-severity diagnostic is
    // the only honest result: total deferral has nothing to emit. PARTIAL deferral is untouched —
    // it still returns `Ok` with `ExportDeferred` warnings. Pushed rather than returned early so
    // the deferral warnings (already in `diags`) and every other offender still reach the caller.
    if blocks.is_empty() {
        diags.push(Diagnostic::error(
            DiagCode::ExportUnsupported,
            MSG_TOTAL_DEFERRAL,
        ));
    }

    // Phase 3 — reserved lowering shape and orphan scan. Host-built pass-through blocks must have
    // the same single typed external In/Out pair as resolver-produced blocks.
    for (bi, block) in g.blocks.iter().enumerate() {
        if !is_pass_through_class(&block.class_iri) {
            continue;
        }
        if !has_valid_shape(g, bi) {
            let subject = block
                .instance_iri
                .as_deref()
                .map_or_else(|| format!("block#{bi}"), str::to_owned);
            diags.push(reject(MSG_STRUCTURE, &subject));
        }
    }

    // SKIP connectors whose owning block is deferred: a deferred block's
    // connectors are never claimed (Phase 2 skipped `claim_port`), so `port_iri[i]` is `None`.
    // Without this skip the orphan scan would inject `MSG_STRUCTURE` Errors for every deferred
    // connector → `has_errors` true → the Defer state (warnings-only) would be unreachable. The
    // gate flip alone is insufficient; this skip is load-bearing.
    for (i, c) in g.connectors.iter().enumerate() {
        if deferred.contains(&(c.block.0 as usize)) {
            continue;
        }
        if g.blocks
            .get(c.block.0 as usize)
            .is_some_and(|block| is_pass_through_class(&block.class_iri))
        {
            continue;
        }
        if port_iri[i].is_none() {
            diags.push(reject(MSG_STRUCTURE, &owner_subject(g, c, i)));
        }
    }

    // Phase 4 — connector-level subset checks. DO NOT top-of-loop `continue` on deferred
    // connectors: `datatypes`/`classified` are PUSH-built and `plan.ports` indexes them
    // POSITIONALLY (by connector position), so a `continue` would shrink them and shift the
    // index. Instead push a PLACEHOLDER for a deferred connector (`S231:Real` + `PortAttrs::None`)
    // and skip ONLY the reject — every out-of-subset arm here (String as well as Enum) is silent
    // on a deferred block, because the document never carries that connector. A surviving block's
    // String connector still rejects; an enum connector's deferral was warned in Phase 1b.
    let mut datatypes: Vec<&'static str> = Vec::with_capacity(g.connectors.len());
    let mut classified: Vec<PortAttrs> = Vec::with_capacity(g.connectors.len());
    for (i, c) in g.connectors.iter().enumerate() {
        let subject = owner_subject(g, c, i);
        let is_deferred = deferred.contains(&(c.block.0 as usize));
        datatypes.push(match c.value_type {
            ValueType::Real => "S231:Real",
            ValueType::Integer => "S231:Integer",
            ValueType::Boolean => "S231:Boolean",
            ValueType::String => {
                // Same deferral rule as the enum arm below: a block the document omits cannot
                // leak a String datatype onto the wire, so aborting the whole export over it
                // would sink a document whose only defect is in bytes nobody emits. No warning
                // either — the block's own deferral warning was already pushed in Phase 1b.
                if !is_deferred {
                    diags.push(reject(MSG_STRING_CONNECTOR, &subject));
                }
                "S231:Real" // placeholder; the error gate discards the plan
            }
            ValueType::Enum(_) => {
                // Deferred (warned in Phase 1b), not rejected. The placeholder is never read —
                // Phase 7 skips the connector — but the slot must exist to preserve the index.
                if !is_deferred {
                    diags.push(reject(MSG_ENUM_CONNECTOR, &subject));
                }
                "S231:Real"
            }
        });
        // The attr scan obeys the same deferral rule as the datatype scan above: a deferred
        // block's connectors contribute NO error diagnostics. The block is omitted from the
        // document, so a tag mismatch or an out-of-subset `nominal`/`unbounded`/non-finite bound
        // on it can never reach the wire — letting it abort the export would sink the whole
        // document over a defect in bytes nobody emits. The slot is STILL pushed (`plan.ports`
        // indexes `classified` positionally by connector position; shrinking it would shift a
        // survivor's attributes onto the wrong port), as the `PortAttrs::None` placeholder Phase 7
        // never reads.
        classified.push(if is_deferred {
            PortAttrs::None
        } else if c.attrs.matches(c.value_type) {
            classify_attrs(&c.attrs, &subject, &mut diags)
        } else {
            diags.push(reject(MSG_STRUCTURE, &subject));
            PortAttrs::None
        });
    }

    // Phase 5 — connections. The orphan guard `port_iri[t].is_none()` drops any edge whose TARGET
    // endpoint owner is deferred (a deferred block's connectors are unclaimed). The source-side
    // `continue` drops an edge whose SOURCE endpoint owner is deferred.
    //
    // For the emitted DOCUMENT either guard alone suffices with Phase 2 in place: a deferred
    // source's port node is never emitted, so its `targets` entry could not reach the wire. For
    // `in_deg` below both are load-bearing, and in opposite directions — the source guard is what
    // keeps a deferred driver from counting toward a survivor's in-degree, and the target guard is
    // what keeps a multiply-driven input on a deferred block from being counted at all. Move the
    // increment above either one and the count stops describing the emitted document.
    let mut targets: Vec<Vec<String>> = vec![Vec::new(); g.connectors.len()];
    let mut boundary_outputs: Vec<PlannedBoundaryOutput> = Vec::new();
    // Surviving drivers per connector, for the §7.10 single-assignment check after this loop, as
    // two flags rather than a count. Recorded at the `targets` push site so they describe exactly
    // the edges the document will carry.
    //
    // Two `bool`s and no arithmetic is deliberate. The predicate is "more than one", so a third
    // driver carries no information a counter could add — while an integer counter over
    // `g.connections`, a `Vec` bounded by neither the connector count nor `u32::MAX`, has an
    // overflow case: panicking in debug and, far worse, WRAPPING in release, where a count that
    // wrapped to 0 or 1 would slip past the check and re-open the exact hole this phase closes.
    // Saturating the add would fix that but leaves a one-token reversion no test can catch (the
    // boundary needs tens of gigabytes of edges to reach). With no integer there is nothing to
    // overflow: `multiply_driven` is set the second time an edge arrives and never cleared, which
    // is the predicate itself.
    let mut driven: Vec<bool> = vec![false; g.connectors.len()];
    let mut multiply_driven: Vec<bool> = vec![false; g.connectors.len()];
    for conn in &g.connections {
        let (f, t) = (conn.from.0 as usize, conn.to.0 as usize);
        let output_to_input = g.connectors.get(f).is_some_and(|c| c.dir == Dir::Out)
            && g.connectors.get(t).is_some_and(|c| c.dir == Dir::In);
        if !output_to_input {
            // Name the source endpoint's owner when it is in range (else the target's); only a
            // fully dangling edge — both endpoints out of range — stays subjectless.
            let subject = g
                .connectors
                .get(f)
                .map(|c| owner_subject(g, c, f))
                .or_else(|| g.connectors.get(t).map(|c| owner_subject(g, c, t)));
            diags.push(match &subject {
                Some(s) => reject(MSG_STRUCTURE, s),
                None => Diagnostic::error(DiagCode::ExportUnsupported, MSG_STRUCTURE),
            });
            continue;
        }
        // Defense-in-depth: drop edges whose source owner is deferred.
        if g.connectors
            .get(f)
            .is_some_and(|c| deferred.contains(&(c.block.0 as usize)))
        {
            continue;
        }
        // Orphan guard: an unclaimed target (including any deferred-target endpoint) is dropped.
        if let Some(to_iri) = port_iri[t].as_ref() {
            targets[f].push(to_iri.clone());
            // Second arrival at this connector ⇒ multiply driven. Endpoint OWNERSHIP is
            // irrelevant here: a block wired to itself twice is as multiply driven as two blocks
            // wired to one input, and re-import counts both the same way.
            if driven[t] {
                multiply_driven[t] = true;
            } else {
                driven[t] = true;
            }
        }
    }

    for output in &g.boundary_outputs {
        let idx = output.source.0 as usize;
        let Some(source) = g.connectors.get(idx) else {
            diags.push(Diagnostic::error(
                DiagCode::ExportUnsupported,
                MSG_STRUCTURE,
            ));
            continue;
        };
        if source.dir != Dir::Out || deferred.contains(&(source.block.0 as usize)) {
            if source.dir != Dir::Out {
                diags.push(reject(MSG_STRUCTURE, &owner_subject(g, source, idx)));
            }
            continue;
        }
        let Some(_) = port_iri[idx] else {
            continue;
        };
        let iri = output.iri.as_ref();
        claim_emitted_id(&mut seen, iri, &owner_subject(g, source, idx), &mut diags);
        targets[idx].push(iri.to_owned());
        boundary_outputs.push(PlannedBoundaryOutput {
            iri: iri.to_owned(),
            datatype: datatypes[idx],
        });
    }

    // Phase 5b — §7.10 single assignment over the survivor cone. Every edge counted above is one
    // the document emits, so an input carrying two of them re-imports with a `SingleAssignment`
    // error and the bytes do not load at all. That is the outcome class this exporter refuses:
    // `MSG_DUPLICATE_EXTERNAL_INPUT` already rejects bytes that come back one entry short, and
    // these come back as nothing. Rejecting here also names the offender better than re-import
    // can — the owning block's `instance_iri`, against re-import's positional `connector#N`.
    //
    // One diagnostic per offending connector, in connector-position order (a post-loop scan, not a
    // reject at the push site), so a triple-driven input yields one entry rather than two and the
    // ordering matches the Phase 3 orphan scan. No `Dir::In` test is needed: the `output_to_input`
    // guard above is the only path that sets either flag, so both are false for every output.
    for (i, c) in g.connectors.iter().enumerate() {
        if multiply_driven[i] {
            diags.push(reject(MSG_MULTIPLY_DRIVEN, &owner_subject(g, c, i)));
        }
    }

    // Phase 6 — boundary inputs. SKIP any `external_inputs` entry whose target owner is deferred.
    // Against a DANGLING boundary node (one whose `isConnectedTo` target Phase 7 never emitted,
    // which re-imports as `UnresolvedReference`) this skip is defense-in-depth, not the load-bearing
    // guard: `port_iri[idx]` is `None` for every deferred connector, so the orphan `let-else` below
    // already drops the entry. What the skip does uniquely is suppress the ERROR-severity checks
    // between here and there — a deferred block's boundary entry with the wrong direction or no
    // stored boundary IRI would otherwise push `MSG_STRUCTURE`/`MSG_EXTERNAL_IRI` and abort an
    // export whose only defect sits in a block the document omits. Same rule as the Phase 4 attr
    // scan: an omitted block contributes no errors. That arm is pinned by
    // `tests/export_deferral.rs::boundary_entry_for_a_deferred_block_is_dropped_not_rejected`.
    let mut boundaries: Vec<PlannedBoundary> = Vec::new();
    // Survivor `external_inputs` connectors already seen, for the duplicate-entry rejection below.
    // Scoped to survivors on purpose: the `deferred.contains` skip runs first, so a duplicate on a
    // deferred block never reaches the check and never aborts an export whose document omits it.
    let mut seen_external: BTreeSet<usize> = BTreeSet::new();
    for cid in &g.external_inputs {
        let idx = cid.0 as usize;
        let Some(c) = g.connectors.get(idx) else {
            // Out of range: there is no connector, so no owner to name — genuinely subjectless
            // (like the whole-graph dense-id rejection, and unlike every per-node rejection).
            diags.push(Diagnostic::error(
                DiagCode::ExportUnsupported,
                MSG_STRUCTURE,
            ));
            continue;
        };
        if deferred.contains(&(c.block.0 as usize)) {
            continue; // boundary drives a deferred block's child — drop the whole entry
        }
        let subject = owner_subject(g, c, idx);
        // Reject a repeat of the SAME connector. Boundary fan-out — one boundary IRI driving
        // several DISTINCT child inputs — is untouched: those are different connectors, grouped
        // by the `find(|pb| pb.iri == boundary_iri)` arm below, and only a second listing of one
        // connector is lost on re-import.
        if !seen_external.insert(idx) {
            diags.push(reject(MSG_DUPLICATE_EXTERNAL_INPUT, &subject));
            continue;
        }
        if c.dir != Dir::In {
            diags.push(reject(MSG_STRUCTURE, &subject));
            continue;
        }
        let Some(boundary_iri) = c.iri.as_deref() else {
            diags.push(reject(MSG_EXTERNAL_IRI, &subject));
            continue;
        };
        let datatype = datatypes[idx];
        let pass_through_output = g
            .blocks
            .get(c.block.0 as usize)
            .filter(|block| is_pass_through_class(&block.class_iri))
            .and_then(|block| {
                (block.inputs.as_slice() == [*cid] && block.outputs.len() == 1)
                    .then_some(block.outputs[0])
            });
        let target = if let Some(output_id) = pass_through_output {
            let output_idx = output_id.0 as usize;
            let Some(output) = g.connectors.get(output_idx) else {
                diags.push(reject(MSG_STRUCTURE, &subject));
                continue;
            };
            if output.dir != Dir::Out
                || output.block != c.block
                || datatypes.get(output_idx).copied() != Some(datatype)
            {
                diags.push(reject(MSG_STRUCTURE, &subject));
                continue;
            }
            let Some(output_iri) = output.iri.as_deref() else {
                diags.push(reject(MSG_EXTERNAL_IRI, &subject));
                continue;
            };
            claim_emitted_id(&mut seen, output_iri, &subject, &mut diags);
            boundary_outputs.push(PlannedBoundaryOutput {
                iri: output_iri.to_owned(),
                datatype,
            });
            output_iri.to_owned()
        } else {
            let Some(target) = port_iri[idx].as_ref() else {
                continue; // orphan connector: already rejected above
            };
            target.clone()
        };
        match boundaries.iter_mut().find(|pb| pb.iri == boundary_iri) {
            Some(pb) if pb.datatype != datatype => {
                diags.push(reject(MSG_BOUNDARY_TYPE_MISMATCH, &subject));
            }
            Some(pb) => pb.targets.push(target.clone()),
            None => {
                // First occurrence mints the boundary node, so its @id joins the emitted set;
                // later same-IRI entries reuse the node (fan-out grouping), not a duplicate.
                claim_emitted_id(&mut seen, boundary_iri, &subject, &mut diags);
                boundaries.push(PlannedBoundary {
                    iri: boundary_iri.to_owned(),
                    datatype,
                    targets: vec![target],
                });
            }
        }
    }

    // Phase 8 — three-state gate. Abort ONLY on error-severity diagnostics (the
    // `ExportDeferred` warnings are non-aborting). The gate flip alone is insufficient — the
    // Phase 3 orphan-scan skip is load-bearing to keep deferred connectors from injecting
    // `MSG_STRUCTURE` Errors.
    if has_errors(&diags) {
        return Err(diags);
    }

    // All error checks passed. `port_iri` may still hold `None` for deferred connectors (never
    // claimed); Phase 7 skips them, so the `let Some(iri) = minted` arm below never hits its
    // fallback for a deferred connector. Every survivor connector is claimed exactly once.
    let mut ports = Vec::with_capacity(g.connectors.len());
    for (i, minted) in port_iri.into_iter().enumerate() {
        // Phase 7 — SKIP connectors whose owner is deferred. The placeholder `datatypes[i]`/
        // `classified[i]` from Phase 4 is never read for a deferred connector. Forgetting this
        // skip emits a deferred enum connector as a `S231:Real` port (silent type-mutation,
        // subset-escape), NOT a loud arity failure.
        let Some(c) = g.connectors.get(i) else {
            continue;
        };
        if deferred.contains(&(c.block.0 as usize)) {
            continue;
        }
        if g.blocks
            .get(c.block.0 as usize)
            .is_some_and(|block| is_pass_through_class(&block.class_iri))
        {
            continue;
        }
        let Some(iri) = minted else {
            // Unreachable for a survivor after the orphan scan; kept total (never a panic).
            return Err(vec![Diagnostic::error(
                DiagCode::ExportUnsupported,
                MSG_STRUCTURE,
            )]);
        };
        ports.push(PlannedPort {
            iri,
            datatype: datatypes[i],
            targets: std::mem::take(&mut targets[i]),
            attrs: classified[i].clone(),
        });
    }

    Ok((
        Plan {
            blocks,
            ports,
            boundaries,
            boundary_outputs,
        },
        diags,
    ))
}

/// Claim connector `raw_cid` for owner block `bi` under `dir`, recording `minted` as its port
/// `@id`. Returns `false` (with a structure diagnostic) when the connector is out of range, owned
/// by a different block, points the other way, or was already claimed.
#[allow(clippy::too_many_arguments)] // A claim needs the graph, the claim key, and the two
// accumulators; bundling them into a one-shot struct would only rename the arguments.
fn claim_port(
    g: &ModelGraph,
    bi: usize,
    raw_cid: u32,
    dir: Dir,
    minted: &str,
    port_iri: &mut [Option<String>],
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let idx = raw_cid as usize;
    let claimable = g
        .connectors
        .get(idx)
        .is_some_and(|c| c.block.0 as usize == bi && c.dir == dir)
        && port_iri.get(idx).is_some_and(Option::is_none);
    if claimable {
        port_iri[idx] = Some(minted.to_owned());
    } else {
        diags.push(reject(MSG_STRUCTURE, subject));
    }
    claimable
}

/// Convert one ground parameter value to its canonical CXF binding, or reject it (enumeration
/// values have no literal form here; non-finite Reals would serialize as JSON `null`).
fn param_binding(
    name: &str,
    value: &Value,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<CxfValue> {
    match value {
        Value::Real(r) if r.is_finite() => Some(CxfValue::Float(*r)),
        Value::Real(_) => {
            diags.push(reject(
                format!(
                    "export subset: parameter `{name}` is a non-finite Real, which cannot \
                     round-trip through JSON"
                ),
                subject,
            ));
            None
        }
        Value::Integer(i) => Some(CxfValue::Int(*i)),
        Value::Boolean(b) => Some(CxfValue::Bool(*b)),
        Value::String(s) => Some(CxfValue::Expr(string_literal(s))),
        Value::Enum { .. } => {
            diags.push(reject(
                format!(
                    "export subset: parameter `{name}` is enumeration-valued, which has no CXF \
                     literal form in the minimal export subset"
                ),
                subject,
            ));
            None
        }
    }
}

/// Classify a type-matched connector's [`Attrs`] into the [`PortAttrs`] emit subset, rejecting
/// `nominal`/`unbounded`/non-finite bounds (outside the canonical subset) into `diags`. A safe,
/// total `match attrs` — no `as_real().unwrap()`, so no `plan()`-reachable panic. The importer
/// hardcodes `nominal: None` and `unbounded: None` (`resolve/attrs.rs`), so any `Some` is outside
/// the imported-graph contract; a non-finite Real bound would serialize as JSON `null` and
/// re-import as `None`, silently breaking the RT-2 render fixpoint, so it is rejected too (R6-C).
fn classify_attrs(attrs: &Attrs, subject: &str, diags: &mut Vec<Diagnostic>) -> PortAttrs {
    match attrs {
        Attrs::Real(a) => {
            if a.nominal.is_some() {
                diags.push(reject(MSG_ATTR_NOMINAL, subject));
            }
            if a.unbounded.is_some() {
                diags.push(reject(MSG_ATTR_UNBOUNDED, subject));
            }
            let min = finite_real_bound(a.min, subject, diags);
            let max = finite_real_bound(a.max, subject, diags);
            PortAttrs::Real {
                unit: a.unit.clone(),
                quantity: a.quantity.clone(),
                display_unit: a.display_unit.clone(),
                min,
                max,
            }
        }
        Attrs::Integer(a) => PortAttrs::Integer {
            min: a.min,
            max: a.max,
        },
        Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => PortAttrs::None,
    }
}

/// Emit a finite Real bound, or reject a non-finite one (R6-C). Mirrors the `is_finite()` guard on
/// `param_binding`'s Real arm: `serde_json` serializes a non-finite `f64` as JSON `null`, which
/// `Option<CxfValue>` deserializes back to `None`, so emitting `Some(NaN)` would silently break
/// `render(G1) == render(G2)`. The importer's `real_connector_bound` stores `Some(r)` with no
/// `is_finite()` check, so `Some(NaN)` IS import-reachable — this guard is required for RT-2 on
/// imported graphs, not merely defense-in-depth.
fn finite_real_bound(v: Option<f64>, subject: &str, diags: &mut Vec<Diagnostic>) -> Option<f64> {
    match v {
        Some(x) if x.is_finite() => Some(x),
        Some(_) => {
            diags.push(reject(MSG_ATTR_NONFINITE_BOUND, subject));
            None
        }
        None => None,
    }
}

/// Quote a `Value::String` payload as a CDL string-literal expression. Only `\` and `"` need
/// escaping (the `oce-expr` lexer resolves both; every other character — including raw control
/// characters — passes through the JSON string layer verbatim).
fn string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Render a validated [`Plan`] as the CXF document. Infallible: every `@id` and binding was
/// minted by [`plan`], and the emission order is fixed by the plan's vectors.
fn build(plan: Plan) -> CxfDocument {
    let mut graph = Vec::new();

    let mut root = blank_node(EXPORT_ROOT_IRI.to_owned());
    root.r#type = Some(OneOrMany::One("S231:Block".to_owned()));
    root.has_input = refs(plan.boundaries.iter().map(|b| b.iri.clone()).collect());
    root.has_output = refs(
        plan.boundary_outputs
            .iter()
            .map(|output| output.iri.clone())
            .collect(),
    );
    root.contains_block = refs(plan.blocks.iter().map(|b| b.iri.clone()).collect());
    graph.push(root);

    for b in plan.blocks {
        let mut node = blank_node(b.iri);
        node.r#type = Some(OneOrMany::One(b.type_iri));
        node.has_input = refs(b.input_ports);
        node.has_output = refs(b.output_ports);
        node.has_parameter = refs(b.params.iter().map(|(id, _)| id.clone()).collect());
        graph.push(node);
        for (id, binding) in b.params {
            let mut pnode = blank_node(id);
            pnode.value = Some(binding);
            graph.push(pnode);
        }
    }

    for port in plan.ports {
        let mut node = blank_node(port.iri);
        node.is_of_data_type = Some(iri_ref(port.datatype.to_owned()));
        node.is_connected_to = refs(port.targets);
        emit_port_attrs(&mut node, &port.attrs);
        graph.push(node);
    }

    for boundary in plan.boundaries {
        let mut node = blank_node(boundary.iri);
        node.is_of_data_type = Some(iri_ref(boundary.datatype.to_owned()));
        node.is_connected_to = refs(boundary.targets);
        graph.push(node);
    }

    for output in plan.boundary_outputs {
        let mut node = blank_node(output.iri);
        node.is_of_data_type = Some(iri_ref(output.datatype.to_owned()));
        graph.push(node);
    }

    let mut context = BTreeMap::new();
    context.insert(
        "S231".to_owned(),
        serde_json::Value::String(S231_CONTEXT_IRI.to_owned()),
    );
    CxfDocument {
        context: Context::Map(context),
        graph,
        other: BTreeMap::new(),
    }
}

/// A [`Node`] with only its `@id` set — every other field absent/empty.
fn blank_node(id: String) -> Node {
    Node {
        id,
        r#type: None,
        has_input: OneOrMany::None,
        has_output: OneOrMany::None,
        has_parameter: OneOrMany::None,
        has_constant: OneOrMany::None,
        contains_block: OneOrMany::None,
        has_instance: OneOrMany::None,
        is_connected_to: OneOrMany::None,
        is_of_data_type: None,
        label: None,
        access: None,
        value: None,
        min: None,
        max: None,
        unit: None,
        quantity: None,
        display_unit: None,
        is_final: None,
        is_array: None,
        n_dims: None,
        size_dims: None,
        fmu_path: None,
        is_conditional: None,
        cond_expr: None,
        is_replaceable: None,
        other: BTreeMap::new(),
    }
}

/// A bare `{"@id": …}` cross-reference.
fn iri_ref(id: String) -> IriRef {
    IriRef {
        id,
        other: BTreeMap::new(),
    }
}

/// Wrap `@id`s as a [`OneOrMany`] edge: absent for zero, a bare object for one (the producer
/// byte style [`OneOrMany::One`] serializes to), an array otherwise.
fn refs(ids: Vec<String>) -> OneOrMany<IriRef> {
    let mut ids = ids;
    match ids.len() {
        0 => OneOrMany::None,
        1 => OneOrMany::One(iri_ref(ids.remove(0))),
        _ => OneOrMany::Many(ids.into_iter().map(iri_ref).collect()),
    }
}
