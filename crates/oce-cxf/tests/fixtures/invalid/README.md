# Invalid CXF fixtures — deep-gate rejection / propagation corpus

These documents are crafted to exercise exactly one load-conformance behavior each (`oce-validate`
is the authoritative deep gate; the `oce-cxf` resolver is the structural fail-fast). They are the
**end-to-end** counterparts to the hand-built `ModelGraph` unit tests in `crates/oce-validate/src/`:
once the loader rejects (or warns / propagates) as expected from CXF bytes, the rule is proven all
the way through, not just on a synthetic graph.

## Drive status

All fixtures are driven. The `unit`/`quantity`/`displayUnit`/`min`/`max` §7.10 fixtures became live
in **M1-PR-11**, which wired the resolver's §7.4.1 attribute extraction (`connector_attrs`) so
declared attrs flow from CXF onto `Connector.attrs` and reach the deep gate.

| Fixture | Behavior (DiagCode) | Driven by |
| --- | --- | --- |
| `double_driven.jsonld` | single-assignment (`single-assignment`) — `add.u1` is the target of two outputs; the resolver's in-degree fail-fast (AD-8) rejects at ingest. | `resolve_errors.rs::double_driven_fixture_is_rejected`; `conformance.rs` |
| `two_undriven.jsonld` | **two** `single-assignment` errors (two undriven inputs) — exercises deterministic diagnostic **ordering** end-to-end. | `conformance.rs::multi_diagnostic_rejection_order_is_deterministic` |
| `non_subset.jsonld` | class-not-found (`class-not-found`) — a child of an unregistered CDL class (PID). | `conformance.rs::non_subset_class_is_rejected_end_to_end` |
| `unit_mismatch.jsonld` | unit/quantity (`unit-quantity-mismatch`) — `con.y` `unit "K"` vs driven `add.u1` `unit "degC"` (§7.10 R13.1). | `conformance.rs::unit_mismatch_is_rejected_end_to_end` |
| `bound_mismatch.jsonld` | bound (`bound-mismatch`) — `con.y` `min 0.0` vs driven `add.u1` `min 5.0` (§7.10 R13.1). | `conformance.rs::bound_mismatch_is_rejected_end_to_end` |
| `one_sided_unit.jsonld` | **no error** — R13.2 propagation: `con.y` `unit "K"`, driven `add.u1` unset ⇒ `"K"` propagates to `add.u1`. A *loads-clean-with-propagation* fixture. | `conformance.rs::one_sided_unit_propagates_to_peer` |
| `display_unit_divergence.jsonld` | display-unit divergence (`display-unit-divergence`, **warning**) — units agree (`K`) but `displayUnit` diverges (`degC` vs `K`); advisory R13.3, never a hard error. | `conformance.rs::display_unit_divergence_is_warn_only` |
| `array_flatten_collision.jsonld` | array-flatten collision (`array-flatten-collision`) — a minted array-element name collides with a sibling parameter (§3.6.1). | `resolve_array.rs` |

> Note: the `connector_attrs.jsonld` fixture (a **valid**, rich-attrs document) lives one level up in
> `tests/fixtures/` — it is the bit-exact golden for the resolver's §7.4.1 attribute parse
> (`resolve_golden.rs`), not a rejection fixture.
