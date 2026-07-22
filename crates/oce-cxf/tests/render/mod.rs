//! Deterministic, bit-exact `ModelGraph` rendering shared by the resolver goldens
//! (`resolve_golden.rs`) and the RT-2 export fixpoint tests (`export_roundtrip.rs`).
//!
//! `ModelGraph` is intentionally NOT `Serialize`/`PartialEq` (`oce-model/src/lib.rs`), so both
//! suites compare this hand-written render string instead. Vectors are printed in index order
//! (`BlockId.0` / `ConnectorId.0`), exactly the order the resolver builds them; floats are
//! printed by `to_bits()` so the comparison is bit-exact, never `==`/epsilon (`TESTING.md`
//! pillar 2). One copy on purpose: if the golden render and the fixpoint render could drift, a
//! renderer bug could hide an export defect.

use std::fmt::Write as _;

use oce_model::{ModelGraph, Value};

/// Render `g` deterministically and human-diffably; the single comparison key for both the
/// checked-in resolver goldens and `render(G1) == render(G2)` fixpoint assertions.
pub fn render(g: &ModelGraph) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "blocks: {}", g.blocks.len());
    for b in &g.blocks {
        let _ = writeln!(
            s,
            "  B{} decl={} class={} instance_iri={:?}",
            b.id.0,
            b.decl_order,
            b.class_iri,
            b.instance_iri.as_deref()
        );
        let _ = writeln!(
            s,
            "    inputs={:?} outputs={:?}",
            id_list(&b.inputs),
            id_list(&b.outputs)
        );
        for (name, v) in &b.params.values {
            let _ = writeln!(s, "    param {name}={}", render_value(v));
        }
    }
    let _ = writeln!(s, "connectors: {}", g.connectors.len());
    for c in &g.connectors {
        let _ = writeln!(
            s,
            "  C{} block=B{} dir={:?} type={:?} decl={} iri={:?}",
            c.id.0,
            c.block.0,
            c.dir,
            c.value_type,
            c.decl_order,
            c.iri.as_deref()
        );
        // The parsed §7.4.1 attrs are locked bit-exactly: a unit/quantity/displayUnit
        // mis-parse, a dropped bound, or a one-ULP bound drift fails the golden loudly.
        let _ = writeln!(s, "    attrs={}", render_attrs(&c.attrs));
    }
    let _ = writeln!(s, "connections: {}", g.connections.len());
    for c in &g.connections {
        let _ = writeln!(s, "  C{} -> C{}", c.from.0, c.to.0);
    }
    let _ = writeln!(
        s,
        "external_inputs: {:?}",
        g.external_inputs.iter().map(|c| c.0).collect::<Vec<_>>()
    );
    s
}

fn id_list<T: std::fmt::Debug>(ids: &[T]) -> Vec<String> {
    ids.iter().map(|i| format!("{i:?}")).collect()
}

fn render_value(v: &Value) -> String {
    match v {
        // Reals by exact bits — the determinism contract; never `==`/epsilon (TESTING.md).
        Value::Real(r) => format!("Real(0x{:016x})", r.to_bits()),
        Value::Integer(i) => format!("Integer({i})"),
        Value::Boolean(b) => format!("Boolean({b})"),
        Value::String(s) => format!("String({s:?})"),
        Value::Enum { class, ordinal } => format!("Enum(class={},ordinal={})", class.0, ordinal),
    }
}

/// A bit-exact rendering of a connector's parsed [`oce_model::Attrs`]. `Real` bounds are printed by
/// `to_bits()` (a one-ULP drift fails loudly); unit/quantity/displayUnit by their string form.
fn render_attrs(a: &oce_model::Attrs) -> String {
    use oce_model::Attrs;
    match a {
        Attrs::Real(r) => format!(
            "Real(unit={:?} quantity={:?} display_unit={:?} min={} max={} nominal={} unbounded={:?})",
            r.unit.as_deref(),
            r.quantity.as_deref(),
            r.display_unit.as_deref(),
            render_opt_bits(r.min),
            render_opt_bits(r.max),
            render_opt_bits(r.nominal),
            r.unbounded,
        ),
        Attrs::Integer(i) => format!("Integer(min={:?} max={:?})", i.min, i.max),
        Attrs::Boolean(_) => "Boolean".to_owned(),
        Attrs::String(_) => "String".to_owned(),
        Attrs::Enum(_) => "Enum".to_owned(),
    }
}

/// An optional `f64` rendered by its IEEE-754 bits (or `-` when unset) — the determinism contract.
fn render_opt_bits(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("0x{:016x}", x.to_bits()),
        None => "-".to_owned(),
    }
}
