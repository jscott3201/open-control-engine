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

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, Connector, ModelGraph};

// ---- shared helpers -------------------------------------------------------------------------

use super::{in_degrees, subject_of};

// ---- §7.10 attribute unification (doc 02 §9 R13.1–R13.4) ------------------------------------

/// A `Real` attribute unified by §7.10 (doc 02 §9 R13.1/R13.2 lists exactly these four:
/// `quantity`, `unit`, `min`, `max`).
#[derive(Copy, Clone)]
enum RealAttr {
    Unit,
    Quantity,
    Min,
    Max,
}

impl RealAttr {
    fn label(self) -> &'static str {
        match self {
            RealAttr::Unit => "unit",
            RealAttr::Quantity => "quantity",
            RealAttr::Min => "min",
            RealAttr::Max => "max",
        }
    }

    /// `unit`/`quantity` conflicts are [`DiagCode::UnitQuantityMismatch`]; `min`/`max` (numeric
    /// bounds) are the analogous [`DiagCode::BoundMismatch`].
    fn conflict_code(self) -> DiagCode {
        match self {
            RealAttr::Unit | RealAttr::Quantity => DiagCode::UnitQuantityMismatch,
            RealAttr::Min | RealAttr::Max => DiagCode::BoundMismatch,
        }
    }
}

/// An `Integer` bound unified by §7.10 (Integer carries no unit/quantity, §3.1 matrix — only
/// `min`/`max`).
#[derive(Copy, Clone)]
enum IntAttr {
    Min,
    Max,
}

impl IntAttr {
    fn label(self) -> &'static str {
        match self {
            IntAttr::Min => "min",
            IntAttr::Max => "max",
        }
    }
}

/// A declared §7.10 attribute value, normalized so one gather-then-decide algorithm handles both
/// the string-valued attributes (`unit`/`quantity`) and the numeric ones (`min`/`max`). Equality is
/// **exact**: text by byte content (no trim/normalize/casefold), `Real` bounds **by bit pattern**
/// (`to_bits`, the crate float-equality convention — never `==`/ε).
#[derive(Clone)]
enum AttrVal {
    Text(Arc<str>),
    Real(f64),
    Int(i64),
}

impl AttrVal {
    fn same(&self, other: &AttrVal) -> bool {
        match (self, other) {
            (AttrVal::Text(a), AttrVal::Text(b)) => a.as_ref() == b.as_ref(),
            (AttrVal::Real(a), AttrVal::Real(b)) => a.to_bits() == b.to_bits(),
            (AttrVal::Int(a), AttrVal::Int(b)) => a == b,
            _ => false,
        }
    }

    fn display(&self) -> String {
        match self {
            AttrVal::Text(s) => s.to_string(),
            AttrVal::Real(x) => format!("{x:?}"),
            AttrVal::Int(i) => i.to_string(),
        }
    }
}

/// Read a connector's `displayUnit` (R13.3) — `None` if not `Real` or unset.
fn read_display_unit(c: &Connector) -> Option<Arc<str>> {
    c.attrs.as_real().and_then(|ra| ra.display_unit.clone())
}

/// §7.10 attribute unification (doc 02 §9 R13.1–R13.4), as a **gather-then-decide cluster
/// algorithm** — always **deterministic**, and **order-independent for every structurally-valid
/// graph**.
///
/// Under single assignment every `In` connector has in-degree ≤ 1, and in a *structurally-valid*
/// graph an `In` connector is never a connection *source* (using one as a `from` is a
/// [`check_connections`] `DirectionMismatch` that fails the load). So a connected component is a
/// disjoint **star**: one output connector and the inputs it drives, and no connector belongs to two
/// clusters — the result is then genuinely order-independent. (We cluster only inputs whose
/// in-degree is exactly 1; a multiply-driven input is a [`check_single_assignment`] error, excluded
/// so a cluster never double-*writes* a connector.) For each star, each `Real` attribute in
/// `{unit, quantity, min, max}` and each `Integer` bound in `{min, max}` is *gathered* (the set of
/// declared, non-default values across all members) then *decided*:
/// - **R13.1** ≥ 2 distinct values → a `shall`-error ([`DiagCode::UnitQuantityMismatch`] for
///   unit/quantity, [`DiagCode::BoundMismatch`] for min/max); no mutation.
/// - **R13.2** exactly 1 distinct value → propagate it to every member whose attribute is unset.
/// - 0 declared → nothing to do.
///
/// A naive single-sweep that *wrote mid-traversal* would be **non-confluent** (the blame for a
/// three-way conflict would depend on connection order); deciding on the gathered set removes that.
/// A *malformed* chained graph (`out → In → …`, where an `In` connector is wrongly reused as a
/// source — itself a load-failing `DirectionMismatch`) can place one connector in two clusters; even
/// there the `is_none` no-overwrite guard prevents any double-write and the pinned ascending
/// `roots.sort_unstable()` keeps the outcome **deterministic** (the transitive propagation is then
/// sort-ordered rather than confluent, but such a graph never passes [`validate`](crate::validate)).
/// Equality is **exact** (text by content, `Real` bounds by `to_bits`) — a deliberate tripwire
/// (`"K"` ≠ `"K "`). `displayUnit` divergence is the advisory R13.3 warning only (never an error,
/// never propagated). `nominal`/`unbounded` are **not** unified (R13.4) — those are the *only* two
/// §7.10 attributes excluded; `min`/`max` are NOT in R13.4 and ARE unified here.
pub(crate) fn unify_clusters(model: &mut ModelGraph, diags: &mut Vec<Diagnostic>) {
    let deg = in_degrees(model);
    let n = model.connectors.len() as u32;

    // Accumulate star clusters: output-connector id → driven (single-assignment) input ids.
    // HashMap is used only to accumulate; iteration is over a sorted key list so the emission is
    // deterministic regardless of graph shape (order-independent outright for the valid disjoint-star
    // case; see the type-doc above for the malformed-chain caveat).
    let mut clusters: HashMap<u32, Vec<u32>> = HashMap::new();
    for c in &model.connections {
        if c.from.0 >= n || c.to.0 >= n {
            continue; // out-of-range — check_connections reports it
        }
        if deg.get(c.to.0 as usize).copied().unwrap_or(0) != 1 {
            continue; // multiply-driven input — excluded for confluence (check_single_assignment errors)
        }
        clusters.entry(c.from.0).or_default().push(c.to.0);
    }
    let mut roots: Vec<u32> = clusters.keys().copied().collect();
    roots.sort_unstable();

    for root in roots {
        let mut members: Vec<u32> = Vec::with_capacity(clusters[&root].len() + 1);
        members.push(root);
        members.extend(clusters[&root].iter().copied());
        for attr in [
            RealAttr::Unit,
            RealAttr::Quantity,
            RealAttr::Min,
            RealAttr::Max,
        ] {
            unify_real(model, &members, attr, diags);
        }
        for attr in [IntAttr::Min, IntAttr::Max] {
            unify_int(model, &members, attr, diags);
        }
        warn_display_unit_divergence(model, &members, diags);
    }
}

/// Read a `Real` attribute off a connector — `None` if the connector is not a `Real` (the
/// `as_real()` guard also tolerates a tag-invariant-violating hand-built connector) or it is unset.
fn read_real(c: &Connector, attr: RealAttr) -> Option<AttrVal> {
    let ra = c.attrs.as_real()?;
    match attr {
        RealAttr::Unit => ra.unit.clone().map(AttrVal::Text),
        RealAttr::Quantity => ra.quantity.clone().map(AttrVal::Text),
        RealAttr::Min => ra.min.map(AttrVal::Real),
        RealAttr::Max => ra.max.map(AttrVal::Real),
    }
}

/// Write a unified `Real` attribute into a connector, **only if** its slot is currently unset.
fn write_real(c: &mut Connector, attr: RealAttr, value: &AttrVal) {
    let Attrs::Real(ra) = &mut c.attrs else {
        return;
    };
    match (attr, value) {
        (RealAttr::Unit, AttrVal::Text(s)) if ra.unit.is_none() => ra.unit = Some(Arc::clone(s)),
        (RealAttr::Quantity, AttrVal::Text(s)) if ra.quantity.is_none() => {
            ra.quantity = Some(Arc::clone(s));
        }
        (RealAttr::Min, AttrVal::Real(x)) if ra.min.is_none() => ra.min = Some(*x),
        (RealAttr::Max, AttrVal::Real(x)) if ra.max.is_none() => ra.max = Some(*x),
        _ => {}
    }
}

/// Gather-then-decide one `Real` attribute over one star cluster (R13.1 conflict / R13.2 propagate).
fn unify_real(
    model: &mut ModelGraph,
    members: &[u32],
    attr: RealAttr,
    diags: &mut Vec<Diagnostic>,
) {
    let mut declared: Vec<AttrVal> = Vec::new();
    for &m in members {
        if let Some(c) = model.connectors.get(m as usize)
            && let Some(v) = read_real(c, attr)
        {
            declared.push(v);
        }
    }
    decide_and_apply(
        model,
        members,
        &declared,
        attr.label(),
        attr.conflict_code(),
        diags,
        |c, v| write_real(c, attr, v),
    );
}

/// Gather-then-decide one `Integer` bound over one star cluster.
fn unify_int(model: &mut ModelGraph, members: &[u32], attr: IntAttr, diags: &mut Vec<Diagnostic>) {
    let mut declared: Vec<AttrVal> = Vec::new();
    for &m in members {
        if let Some(c) = model.connectors.get(m as usize)
            && let Some(ia) = c.attrs.as_integer()
        {
            let v = match attr {
                IntAttr::Min => ia.min,
                IntAttr::Max => ia.max,
            };
            if let Some(i) = v {
                declared.push(AttrVal::Int(i));
            }
        }
    }
    decide_and_apply(
        model,
        members,
        &declared,
        attr.label(),
        DiagCode::BoundMismatch,
        diags,
        |c, value| {
            if let (Attrs::Integer(ia), AttrVal::Int(i)) = (&mut c.attrs, value) {
                match attr {
                    IntAttr::Min if ia.min.is_none() => ia.min = Some(*i),
                    IntAttr::Max if ia.max.is_none() => ia.max = Some(*i),
                    _ => {}
                }
            }
        },
    );
}

/// The shared R13.1/R13.2 decision: ≥ 2 distinct declared → conflict error (no mutation); exactly 1
/// → propagate to every member via `apply` (which itself no-ops on already-set slots).
fn decide_and_apply(
    model: &mut ModelGraph,
    members: &[u32],
    declared: &[AttrVal],
    label: &str,
    conflict_code: DiagCode,
    diags: &mut Vec<Diagnostic>,
    apply: impl Fn(&mut Connector, &AttrVal),
) {
    let Some(first) = declared.first() else {
        return; // 0 declared — nothing to unify
    };
    if declared.iter().all(|v| v.same(first)) {
        // R13.2: propagate the single agreed value to every unset member. Order-independent — the
        // value is identical regardless of which member it came from.
        let value = first.clone();
        for &m in members {
            if let Some(c) = model.connectors.get_mut(m as usize) {
                apply(c, &value);
            }
        }
    } else {
        // R13.1: ≥ 2 distinct declared values → hard error. Subject = the cluster's output
        // connector (the common driver, deterministic). Values are sorted for a stable message.
        let mut distinct: Vec<String> = declared.iter().map(AttrVal::display).collect();
        distinct.sort();
        distinct.dedup();
        let mut d = Diagnostic::error(
            conflict_code,
            format!(
                "connected connectors declare conflicting {label} values {distinct:?}; \
                 §7.10 (R13.1) requires agreement"
            ),
        );
        if let Some(c) = members
            .first()
            .and_then(|&r| model.connectors.get(r as usize))
        {
            d = d.with_subject(subject_of(c));
        }
        diags.push(d);
    }
}

/// R13.3: divergent `displayUnit`s across a star are an advisory warning — non-computational, never
/// an error, never propagated.
fn warn_display_unit_divergence(model: &ModelGraph, members: &[u32], diags: &mut Vec<Diagnostic>) {
    let mut declared: Vec<Arc<str>> = Vec::new();
    for &m in members {
        if let Some(c) = model.connectors.get(m as usize)
            && let Some(du) = read_display_unit(c)
        {
            declared.push(du);
        }
    }
    if declared.len() < 2 || declared.iter().all(|v| v.as_ref() == declared[0].as_ref()) {
        return;
    }
    let mut distinct: Vec<String> = declared.iter().map(|v| v.to_string()).collect();
    distinct.sort();
    distinct.dedup();
    let mut d = Diagnostic::warning(
        DiagCode::DisplayUnitDivergence,
        format!(
            "connected connectors declare divergent displayUnit values {distinct:?}; \
             non-computational, advisory only (§7.17, R13.3)"
        ),
    );
    if let Some(c) = members
        .first()
        .and_then(|&r| model.connectors.get(r as usize))
    {
        d = d.with_subject(subject_of(c));
    }
    diags.push(d);
}
