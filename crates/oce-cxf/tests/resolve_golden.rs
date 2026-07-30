//! Golden, determinism, and boundary-elision integration tests for the §7.1 resolver.
//!
//! `ModelGraph` is intentionally NOT `Serialize`/`PartialEq` (`oce-model/src/lib.rs`), so the
//! golden is the hand-written, deterministic [`render::render`] string (shared with the RT-2
//! export fixpoint suite) compared against a checked-in snapshot — floats rendered **by their IEEE-754 bits** so a one-ULP drift fails loudly
//! (`TESTING.md` pillar 2). To regenerate the snapshot after an *intentional* change, run:
//!
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test resolve_golden golden_minimal_loop_modelgraph
//! ```
//!
//! and review the diff.

mod bless;
mod render;

use std::path::PathBuf;

use oce_cxf::{ResolveOptions, import_cxf};
use oce_model::{ModelGraph, Value};
use render::render;

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");
const GOLDEN_REL: &str = "tests/fixtures/golden/minimal_loop.modelgraph.txt";
const ATTRS_RICH: &str = include_str!("fixtures/connector_attrs.jsonld");
const ATTRS_GOLDEN_REL: &str = "tests/fixtures/golden/connector_attrs.modelgraph.txt";

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

    if bless::enabled() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &actual).unwrap();
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
    // Two imports must render byte-identically...
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
    // ParamTable through the SAME registry path used by the engine — `oce_blocks::lookup(class_iri)` —
    // and exercise it. This proves (a) `class_iri` is a valid registry key, and (b) the
    // param NAMES + VALUES actually reach the block. If the param key were the dotted path instead
    // of "k", `con` would emit 0.0 (the default) and this test would fail.
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
    let y_start = del_params
        .iter()
        .find(|(name, _)| name.as_ref() == "y_start")
        .expect("UnitDelay y_start parameter");
    assert!(
        y_start.1.bit_eq(&Value::Integer(0)),
        "bare-Int param grounds to Integer, not Real: got {:?}",
        y_start.1
    );
    let sample_period = del_params
        .iter()
        .find(|(name, _)| name.as_ref() == "samplePeriod")
        .expect("UnitDelay samplePeriod parameter");
    assert!(
        sample_period.1.bit_eq(&Value::Real(1.0)),
        "UnitDelay samplePeriod must remain the explicit 1.0 fixture parameter"
    );
}

#[test]
fn golden_connector_attrs_modelgraph() {
    // Bit-exact golden for the parsed §7.4.1 connector attrs: unit/quantity/displayUnit
    // in their three legal JSON-LD wire shapes + numeric bounds by `to_bits()`. Re-bless with
    // `OCE_BLESS=1 cargo test -p oce-cxf --test resolve_golden golden_connector_attrs_modelgraph`.
    let g = import_ok(ATTRS_RICH);
    let actual = render(&g);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ATTRS_GOLDEN_REL);
    if bless::enabled() {
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
    // CXF input. Each typed field is asserted
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
