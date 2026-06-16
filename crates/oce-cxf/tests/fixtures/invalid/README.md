# Invalid CXF fixtures — deep-gate (M1-PR-8) rejection corpus

These documents are crafted to violate exactly one deep-gate rule each (`oce-validate`, the
authoritative load-conformance gate). They are the **end-to-end** counterparts to the hand-built
`ModelGraph` unit tests in `crates/oce-validate/src/tests.rs`: once the loader rejects them with the
expected `DiagCode`, the rule is proven from CXF bytes all the way through, not just on a synthetic
graph.

## Drive status

| Fixture | Rule (DiagCode) | Driven now? | Why |
| --- | --- | --- | --- |
| `double_driven.jsonld` | single-assignment (`single-assignment`) | **yes** | `Bad.add.u1` is the target of two outputs (`con1.y`, `con2.y`). The `oce-cxf` resolver's structural fail-fast (AD-8) catches in-degree ≥ 2 at ingest, so `import_cxf` already rejects it. Driven by `resolve_errors.rs::double_driven_fixture_is_rejected`. |
| `unit_mismatch.jsonld` | unit/quantity (`unit-quantity-mismatch`) | **deferred** | `con.y` declares `unit "K"` / `quantity "ThermodynamicTemperature"`; the driven `add.u1` declares `unit "degC"` — a §7.10 R13.1 conflict. |
| `one_sided_unit.jsonld` | (no error — R13.2 propagation) | **deferred** | `con.y` declares `unit "K"`, the driven `add.u1` is unset — §7.10 R13.2 should propagate `"K"` to `add.u1`. A *should-load-clean-with-propagation* fixture, not a rejection. |
| `display_unit_divergence.jsonld` | display-unit divergence (`display-unit-divergence`, **warning**) | **deferred** | `con.y` and `add.u1` agree on `unit "K"` but declare divergent `displayUnit` (`degC` vs `K`) — an advisory R13.3 warning, never a hard error. |

## Why three are deferred (TODO)

The §7.10 attribute rules can only fire end-to-end once the **resolver extracts** `unit` / `quantity`
/ `displayUnit` off the CXF connector nodes into `RealAttrs`. As of M1-PR-8 the resolver emits
*default* (empty) attributes — by design, to keep PR-8 focused on the gate logic and its hand-built
test matrix — so unification is a no-op via `load_cxf` today and driving these three now would
**false-pass** (`unit_mismatch`/`display_unit_divergence`) or be a trivial no-op (`one_sided_unit`).

The `S231:unit` / `S231:quantity` / `S231:displayUnit` predicate names used here are the expected CDL
connector-attribute predicates; the resolver-extraction PR must confirm them against the S231P
vocabulary and then wire these three fixtures into `resolve_errors.rs` (assert the
`unit-quantity-mismatch` error / the propagated `RealAttrs.unit` / the `display-unit-divergence`
warning respectively). **TODO(resolver-attr-extraction):** drive these three.
