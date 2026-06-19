//! Golden + determinism + boundary-elision integration tests for the §7.1 resolver (M1-PR-5).
//!
//! `ModelGraph` is intentionally NOT `Serialize`/`PartialEq` (`oce-model/src/lib.rs`), so the
//! golden is a hand-written, deterministic [`render`] string compared against a checked-in
//! snapshot — floats rendered **by their IEEE-754 bits** so a one-ULP drift fails loudly
//! (`TESTING.md` pillar 2). To regenerate the snapshot after an *intentional* change, run:
//!
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test resolve_golden golden_minimal_loop_modelgraph
//! ```
//!
//! and review the diff.

use std::fmt::Write as _;
use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");
const GOLDEN_REL: &str = "tests/fixtures/golden/minimal_loop.modelgraph.txt";
const ATTRS_RICH: &str = include_str!("fixtures/connector_attrs.jsonld");
const ATTRS_GOLDEN_REL: &str = "tests/fixtures/golden/connector_attrs.modelgraph.txt";

/// Deterministic, human-diffable, bit-exact rendering of a `ModelGraph`. Vectors are printed in
/// index order (`BlockId.0` / `ConnectorId.0`), exactly the order the resolver builds them; floats
/// are printed by `to_bits()` so the comparison is bit-exact, never `==`/epsilon.
fn render(g: &ModelGraph) -> String {
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
        // The parsed §7.4.1 attrs are locked bit-exactly (M1-PR-11): a unit/quantity/displayUnit
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

fn import_ok(src: &str) -> ModelGraph {
    let (g, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("minimal_loop must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL)
}

#[test]
fn golden_minimal_loop_modelgraph() {
    let g = import_ok(FIXTURE);
    let actual = render(&g);

    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &actual).unwrap();
        // (No print: `clippy::print_stderr` is denied workspace-wide. The file write is the effect.)
        return;
    }
    let expected = std::fs::read_to_string(golden_path())
        .expect("golden snapshot missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "lowered ModelGraph diverged from the checked-in golden"
    );
}

#[test]
fn structural_summary_matches_hand_trace() {
    // A redundant, explicit cross-check of the §C hand-trace independent of the snapshot text.
    let g = import_ok(FIXTURE);
    assert_eq!(g.blocks.len(), 5);
    assert_eq!(g.connectors.len(), 11);
    assert_eq!(g.connections.len(), 5);
    assert_eq!(
        g.external_inputs.iter().map(|c| c.0).collect::<Vec<_>>(),
        vec![9]
    );

    // Connections, in deterministic (source @graph order, then isConnectedTo order) order.
    let edges: Vec<(u32, u32)> = g.connections.iter().map(|c| (c.from.0, c.to.0)).collect();
    assert_eq!(edges, vec![(0, 1), (3, 4), (5, 6), (5, 8), (7, 2)]);

    // Params, compared by bits (Real) / value (Integer).
    let con_k = &g.blocks[0].params.values;
    assert_eq!(con_k.len(), 1);
    assert_eq!(con_k[0].0.as_ref(), "k");
    assert!(con_k[0].1.bit_eq(&Value::Real(2.0)));
    let gain_k = &g.blocks[3].params.values;
    assert!(gain_k[0].1.bit_eq(&Value::Real(0.5)));
}

#[test]
fn boundary_elision_input_and_output() {
    // AD-2: gt.u2 (C9) is the only external input, tagged with the uSet boundary IRI; no connector
    // exists for uSet or yAlarm; no connection is In→In or Out→Out; gt.y (C10) drives nothing
    // (its only edge, to the boundary output yAlarm, was elided).
    let g = import_ok(FIXTURE);

    assert_eq!(g.external_inputs, vec![oce_model::ConnectorId(9)]);
    assert_eq!(
        g.connectors[9].iri.as_deref(),
        Some("http://example.org#MinLoop.uSet"),
        "the elided boundary input's IRI must travel on the driven child input (C9)"
    );

    // No connector carries the boundary-output IRI; exactly one carries the boundary-input IRI.
    let uset_count = g
        .connectors
        .iter()
        .filter(|c| c.iri.as_deref() == Some("http://example.org#MinLoop.uSet"))
        .count();
    assert_eq!(uset_count, 1);
    assert!(
        g.connectors
            .iter()
            .all(|c| c.iri.as_deref() != Some("http://example.org#MinLoop.yAlarm")),
        "the boundary output must be elided — no connector keeps its IRI"
    );

    // Every emitted connection is genuinely output→input (no In→In / Out→Out survived).
    for c in &g.connections {
        assert_eq!(g.connectors[c.from.0 as usize].dir, oce_model::Dir::Out);
        assert_eq!(g.connectors[c.to.0 as usize].dir, oce_model::Dir::In);
    }

    // C10 (gt.y) is the model output: it is a connection `from` zero times.
    assert!(g.connections.iter().all(|c| c.from.0 != 10));
    // C9 has in-degree 0 yet the load succeeds (external, not a SingleAssignment error).
    assert!(g.connections.iter().all(|c| c.to.0 != 9));
}

#[test]
fn resolve_is_byte_identical_across_imports() {
    // Determinism (exit #4): two imports must render byte-identically...
    let r1 = render(&import_ok(FIXTURE));
    let r2 = render(&import_ok(FIXTURE));
    assert_eq!(
        r1, r2,
        "two imports of the same document must be byte-identical"
    );

    // ...and a third import after scrambling JSON *key* order (only @graph ARRAY order is
    // load-bearing) must also match — proving no JSON-key-order / HashMap-iteration dependence.
    let scrambled: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let scrambled = serde_json::to_string(&scrambled).unwrap(); // serde_json::Map sorts keys
    let r3 = render(&import_ok(&scrambled));
    assert_eq!(
        r1, r3,
        "key-order must not affect the lowered graph (only @graph array order)"
    );

    // HashMap-leak sentinel: ids are dense 0..n in build order.
    let g = import_ok(FIXTURE);
    assert!(
        g.connectors
            .iter()
            .enumerate()
            .all(|(i, c)| c.id.0 as usize == i)
    );
    assert!(
        g.blocks
            .iter()
            .enumerate()
            .all(|(i, b)| b.id.0 as usize == i)
    );
}

#[test]
fn param_plumbing_oracle_via_oce_blocks() {
    // Oracle / end-to-end cross-check (TESTING.md pillar 3): construct each block from its resolved
    // ParamTable through the SAME registry path PR-6 will use — `oce_blocks::lookup(class_iri)` —
    // and exercise it. This proves (a) `class_iri` is a valid registry key (C-E), and (b) the
    // param NAMES + VALUES actually reach the block. If the param key were the dotted path instead
    // of "k", `con` would emit 0.0 (the default) and this test would fail — the C1-class tripwire.
    let g = import_ok(FIXTURE);

    // con = Constant(k=2.0): algebraic, emits y = k.
    let con = oce_blocks::lookup(&g.blocks[0].class_iri).expect("Constant must be registered");
    let con = (con.make)(&g.blocks[0].params);
    let diag = oce_blocks::NoopDiagnostics;
    let cx = oce_blocks::Ctx::new(0.0, &diag);
    let mut y = None;
    con.step_algebraic(&cx, &[], &mut |idx, v| {
        if idx == 0 {
            y = Some(v);
        }
    });
    assert!(
        y.expect("Constant emits an output")
            .bit_eq(&Value::Real(2.0)),
        "con.k=2.0 must reach the constructed block (proves param name plumbing)"
    );

    // gain = MultiplyByParameter(k=0.5): emits y = k * u. With u = 2.0 → 1.0.
    let gain = oce_blocks::lookup(&g.blocks[3].class_iri).expect("MultiplyByParameter registered");
    let gain = (gain.make)(&g.blocks[3].params);
    let mut gy = None;
    gain.step_algebraic(&cx, &[Value::Real(2.0)], &mut |idx, v| {
        if idx == 0 {
            gy = Some(v);
        }
    });
    assert!(
        gy.expect("gain emits an output").bit_eq(&Value::Real(1.0)),
        "gain.k=0.5 must reach the constructed block (0.5 * 2.0 == 1.0)"
    );

    // Every block's class_iri resolves in the registry (C-E: bridged class_path, not full IRI).
    for b in &g.blocks {
        assert!(
            oce_blocks::lookup(&b.class_iri).is_some(),
            "class_iri {:?} must be a registry key",
            b.class_iri
        );
    }
}

#[test]
fn unit_delay_bare_int_grounds_to_integer_not_real() {
    // del.y_start is a bare Int 0 with no isOfDataType, so the resolver grounds it literal-natural
    // to Value::Integer(0) (NOT re-typed to Real — that is the block constructor's job). A non-zero
    // bare-Int y_start is then correctly promoted to Real by `oce-blocks` `real_param` (Modelica
    // Int→Real); the promotion is covered by `real_param_promotes_integer_to_real` in oce-blocks,
    // so there is no longer a silent-default hole here.
    let g = import_ok(FIXTURE);
    let del_params = &g.blocks[2].params.values;
    assert_eq!(del_params.len(), 1);
    assert_eq!(del_params[0].0.as_ref(), "y_start");
    assert!(
        del_params[0].1.bit_eq(&Value::Integer(0)),
        "bare-Int param grounds to Integer, not Real: got {:?}",
        del_params[0].1
    );
}

#[test]
fn golden_connector_attrs_modelgraph() {
    // Bit-exact golden for the parsed §7.4.1 connector attrs (M1-PR-11): unit/quantity/displayUnit
    // in their three legal JSON-LD wire shapes + numeric bounds by `to_bits()`. Re-bless with
    // `OCE_BLESS=1 cargo test -p oce-cxf --test resolve_golden golden_connector_attrs_modelgraph`.
    let g = import_ok(ATTRS_RICH);
    let actual = render(&g);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ATTRS_GOLDEN_REL);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .expect("golden snapshot missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        actual, expected,
        "connector_attrs graph diverged from golden"
    );
}

#[test]
fn resolver_carries_declared_connector_attrs() {
    // The resolver PARSES each connector's declared §7.4.1 attributes onto `Connector.attrs`, so
    // oce-validate's §7.10 deep gate has something to unify — without this the gate is dead on real
    // CXF input (regression guard for M1-PR-11). Each of the THREE new typed fields is asserted
    // independently (a regression dropping any one would otherwise pass), across all three legal
    // wire shapes: `unit` bare-string "K", `quantity` typed-literal, `displayUnit` IRI-node — plus
    // the numeric bounds, compared by bits. (Resolver layer: §7.10 is NOT run here, so it resolves
    // cleanly.)
    let g = import_ok(ATTRS_RICH);
    let attrs = g
        .connectors
        .iter()
        .find_map(|c| match &c.attrs {
            oce_model::Attrs::Real(a) if a.unit.is_some() => Some(a.clone()),
            _ => None,
        })
        .expect("the con.y Real connector must carry parsed attrs");
    assert_eq!(attrs.unit.as_deref(), Some("K"), "bare-string unit");
    assert_eq!(
        attrs.quantity.as_deref(),
        Some("ThermodynamicTemperature"),
        "typed-literal quantity carries its @value"
    );
    assert_eq!(
        attrs.display_unit.as_deref(),
        Some("degC"),
        "IRI-node displayUnit carries its @id"
    );
    assert_eq!(
        attrs.min.map(f64::to_bits),
        Some(0.0_f64.to_bits()),
        "S231:min grounds onto RealAttrs.min (bit-exact)"
    );
    assert_eq!(
        attrs.max.map(f64::to_bits),
        Some(350.0_f64.to_bits()),
        "S231:max grounds onto RealAttrs.max (bit-exact)"
    );
}
