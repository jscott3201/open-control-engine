# Facade migration

This intentional **pre-release source break** contracts speculative facade shapes; nothing is
published to crates.io and no stable SemVer or downstream compatibility certification is implied.
The [public surface contract](public-surface-contract.md) and its exact baseline own classification.

## Removed names and working alternatives

| Removed | Host action |
| --- | --- |
| `Engine::load_modelica` | Prepare already-specialized, supported CXF externally, then use `load_cxf`. OCE does not parse Modelica source. |
| `Engine::load_from_semantic`, `TemplateRef` | Remove speculative loader calls. There is no semantic-template loader replacement; an app can prepare supported CXF itself. The active effective-point metadata resolver is unrelated and remains wired. |
| Flat `oce_api::SemanticQuery` | For conditional adapter queries, use `oce_api::oce_store::SemanticQuery` or direct `oce_store::SemanticQuery`. Those types/traits are unchanged; this does not supply a template loader. |
| `InputSource::Csv` | Decode files/tables host-side and supply typed named values via `Constant` or a `Send + Sync` `Closure`. `None` remains available. The private conformance driver's reference-table/CSV replay still works; only `DriverInputReplay::FacadeCsv` is removed. |
| `AssertLevel::Error` | Update exhaustive matches and code that constructed the unimplemented severity. `AssertLevel` now contains only `Warning`; `AssertEvent.level` remains. |

**Default-value change:** `AssertLevel::default()` is now `Warning`, previously `Error`. A signature
baseline cannot detect this value change, so a separate explicit default golden pins it. This is not
new escalation, abort behavior or safety logic. Actual `CDL.Utilities.Assert` CXF tests check true is
silent, false produces repeated Warning events, exact message/source/time bits, continued execution
and repeat-run equality. Expected files are hand-derived from Boolean assertion semantics and the
HostTick reporting contract, not an external solver trace. The emitted source is currently the class
path `CDL.Utilities.Assert`, not an instance identity. Tick/simulation keep the no-op diagnostic sink;
realtime write failure may prevent delivery of the collected report even though the tick applied.

## Inventory and execution boundaries remain

`point_list(None)` keeps its existing signature and returns the engine's own effective inventory.
Device filtering (`Some`, including empty/unknown strings) is outside the supported profile and
returns the **existing `OcError::Load`** directly. No new Unsupported variant exists and no store
query is delegated, even with a capable adapter. This is not an experimental supported feature.
The custom-adapter regression checks the None result, specific refusal, no store calls and no engine
mutation. Existing error detail text is retained for compatibility, not promoted into an API schema.

Load/store side effects still precede engine-field commit; replacement is not transactional.
Simulation still preflights collection/constant/first-closure lists before restart, preserves the
entry connector image, and can leave prior ticks and prefix staging after later failures. Realtime
still validates epoch before tick and writes afterward without rollback. State-wire bytes, behavioral
goldens, catalog identity, numerical semantics and concrete engine thread-safety guards are unchanged.

## Package and panic-inventory boundary

No package/feature/dependency policy changes: 17 members, 12 publishable, five private; all three
supported feature selections retain the same eleven normal OCE dependencies and the mem no-op.
Private `oce-docs` retains its `point_list_html` unimplemented exporter outside the supported closure.
Private `oce-extension` retains its reserved DTO, with no runtime. `oce-flatten` successfully returns
its input model unchanged, and `oce-semantics` actively derives effective point metadata; both remain
wired implementation dependencies. No new feature or replacement placeholder is introduced.

The focused production inventory distinguishes placeholder handlers from invariant checks:
CXF resolver `composite.rs` and `composite_orientation/diagnostics.rs` contain `unreachable!` arms
after exhausting the two traversal-frame variants. Blocks' timetable/controller token mappings
contain invariant arms over repository-owned constants. Snapshot decoding's bounds-checked expects,
debug assertions and test-helper panics are not deferred feature handlers. None were purged. Typed
hostile-input tests bound the exercised paths; there is no universal panic-free, adapter-panic,
cancellation or equipment-safety claim.

## Compatibility evidence limits

The compiler contract checks six absences plus same-dependency positive controls under default,
explicit mem and no-default selections. Baseline reintroduction controls are additional, not a
substitute for compilation; unrelated associated `Error` types remain in the baseline. Only the
facade baseline changes (1408 to 1357 rows); the storage baseline remains byte-identical.

Bounded downstream source inspection found working point_list(None)/CXF and Closure use rather than
the selected retired names. Actual downstream pins are unchanged. Exact-candidate Studio adapter
and Library verifier fixture compilation is separate pre-delivery qualification, not established by
that inspection or by the in-repository compiler controls. No downstream acceptance or pin advance
is claimed here.
