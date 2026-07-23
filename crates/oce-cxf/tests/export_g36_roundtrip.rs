//! R7 — the enum-free flat G36 RT-2 subset: 8 individually-named golden tests (Deliverable C).
//!
//! Each G36 fixture is enum-free (recon-verified: zero `ValueType::Enum` connectors, zero
//! `Value::Enum` parameters), so the R7 deferral pre-pass defers nothing and the whole graph
//! exports. RT-2 then holds for the FULL graph (not just a survivor cone):
//! `G1 = import(fixture); bytes = export(G1); G2 = import(bytes); render(G1) == render(G2)`
//! bit-exact (floats by `to_bits()` via [`render::render`], the same key the resolver goldens use),
//! plus the second-order byte fixpoint `export(G2) == bytes`. The enum-free assertion is a
//! provenance guard: it pins that the fixture is genuinely in-subset so the RT-2 equality is
//! meaningful (a fixture that silently grew an enum would make `render` equality vacuous).
//!
//! The `import_ok`/`export_ok`/`render` helpers are DUPLICATED from `export_roundtrip.rs` rather
//! than factored into a shared module: each integration test file is a separate crate/binary, and
//! the three-line helpers are too thin to justify a `#[path]`-included support module. `render`
//! itself IS shared — both binaries compile `tests/render/mod.rs` via `mod render;`.

mod render;

use oce_cxf::{ResolveOptions, export, import_cxf};
use oce_model::{ModelGraph, Value, ValueType};
use render::render;

fn import_ok(bytes: &[u8]) -> ModelGraph {
    let (g, report) =
        import_cxf(bytes, &ResolveOptions::default()).expect("document must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn export_ok(g: &ModelGraph) -> Vec<u8> {
    export(g).expect("graph is inside the minimal export subset")
}

/// The RT-2 fixpoint for an enum-free G36 fixture: render equality (bit-exact) + the second-order
/// byte fixpoint + the enum-free provenance guard (zero enum connectors/params in the input).
fn g36_rt2(fixture: &str) {
    let g1 = import_ok(fixture.as_bytes());
    let bytes = export_ok(&g1);
    let g2 = import_ok(&bytes);
    assert_eq!(render(&g1), render(&g2));
    assert_eq!(export_ok(&g2), bytes); // second-order byte fixpoint
    assert!(
        g1.connectors
            .iter()
            .all(|c| !matches!(c.value_type, ValueType::Enum(_)))
    );
    assert!(g1.blocks.iter().all(|b| {
        b.params
            .values
            .iter()
            .all(|(_, v)| !matches!(v, Value::Enum { .. }))
    }));
}

#[test]
fn g36_ahu_economizer_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!("fixtures/g36/ahu_economizer.jsonld"));
}

#[test]
fn g36_ahu_supply_air_temp_reset_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!(
        "fixtures/g36/ahu_supply_air_temp_reset.jsonld"
    ));
}

#[test]
fn g36_cooling_only_system_requests_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!(
        "fixtures/g36/cooling_only_system_requests.jsonld"
    ));
}

#[test]
fn g36_generic_time_suppression_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!("fixtures/g36/generic_time_suppression.jsonld"));
}

#[test]
fn g36_multizone_vav_economizer_modulations_reliefs_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!(
        "fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld"
    ));
}

#[test]
fn g36_multizone_vav_economizer_modulations_return_fan_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!(
        "fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld"
    ));
}

#[test]
fn g36_reheat_overrides_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!("fixtures/g36/reheat_overrides.jsonld"));
}

#[test]
fn g36_vav_single_zone_reaches_the_rt2_fixpoint() {
    g36_rt2(include_str!("fixtures/g36/vav_single_zone.jsonld"));
}
