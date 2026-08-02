# Changelog

Notable changes to the Open Control Engine. Nothing has been released yet, so every entry
sits under `Unreleased`: the package is not on crates.io and there is no semver promise.

Entries are grouped by area rather than by date, and each names the PR so the change can be
read in full.

An entry is expected from every PR that changes behaviour, the public surface, or a published
claim — added in that PR, not batched later. Nothing enforces this: an entry is a judgement about
what mattered, so no check can derive one, and a check that merely required *some* text would pass
on a placeholder. It has therefore fallen behind twice (#215 recovered 64 commits; this file
recovered seven PRs). The cheapest place to notice is a `development` → `main` promotion, where
`git log main..development -- CHANGELOG.md` returning nothing means the release is undocumented.

## Unreleased

### CXF import and export

- **Export exists.** Flat, ground, scalar model graphs serialize back to CXF at the RT-2
  round-trip fixpoint (#175), and where the subset does not apply the former panic is now a
  typed `ExportUnsupported` rejection (#174). Connector §7.4.1 attributes export under the
  bare-scalar canonical subset (#176).
- **Arrays export flattened, and enum-carrying blocks defer with warnings rather than
  failing** (#177). `export_with_report` surfaces those warnings; plain `export` discards
  them — so an integrator who needs to know whether an export was *complete* must use the
  reporting form.
- **Ports bind by declared CDL name, not array position** (#185), with the port-order table
  derived from vendored upstream CDL source instead of a hand-maintained JSON file (#184).
  Fixture port order is gated against upstream declaration order (#183).
- **`isConnectedTo` is symmetric.** Either endpoint may be the subject, per CXF Table 8.2
  (#186), and `external_inputs` ordering no longer follows a boundary port's array spelling
  (#187).
- **Direct boundary input→output connections** lower to reserved native pass-through
  identities on import and elide exactly on export (#191, #188).
- **Nested composite boundaries canonicalize by role and containment** (#199). A fallback
  that fabricated an orientation when it could not determine one was deleted rather than
  repaired.
- **Array-valued connector and instance nodes are rejected with tagged rule ids** (#198),
  and composite-subset rejections carry stable rule ids naming every offender (#172). The
  normative composite-subset contract is published with its own conformance corpus (#173) —
  the document to read if you emit CXF for this engine.
- A multiply-driven input is rejected rather than exported as bytes that cannot re-import
  (#179).
- **Authored connector identities survive import and export** (#226). Ingest previously discarded
  every connector's authored `@id` and minted port identities positionally on the way out, so a
  port's name did not round-trip. A missing `@id` is now a load error and a duplicate is rejected
  naming both offenders. Host-visible point paths were deliberately left unchanged there; #229
  retires them.
- **Boundary outputs are represented in the model, so export stops dropping the authored output
  contract** (#227). Exporting `ahu_economizer` produced a root with no `S231:hasOutput` at all
  where the source declares four names, and the driving edge went with it — 130 authored boundary
  output names across the G36 corpus existed in no engine artifact. The RT-2 round trip could not
  see it: the fixpoint is asserted over `ModelGraph`, which had `external_inputs` and no output
  counterpart, so the loss was identical on both sides of the comparison and the test passed
  because of the defect. `ModelGraph` now carries `boundary_outputs`, and export restores both the
  nodes and the edges. `minimal_loop`'s content identity moved as a result — a migration event for
  anyone persisting one, not a golden refresh. Composite-level `hasParameter`/`hasConstant` remain
  absent from exports by design: flattening resolves them into child block values, which are
  emitted.
- **Identity tokens expand against `@context` at ingest** (#230), closing the gap #229 named.
  Compact and expanded spellings of the same subject IRI now name the same point, block, model,
  and datatype: expansion runs as one pre-resolve pass over the document's `@id`s, followed
  structural references, `@type`s, and `isOfDataType`, before any identity map is built — so a
  compact and an absolute spelling of one subject collide as `DuplicateId` instead of loading as
  two nodes, and a document re-serialized between spellings keeps its point paths. A relative
  `@id` no context can canonicalize is refused with the new `relative-iri` diagnostic. Safety
  consequence: a compact `isOfDataType` previously *disabled* G36 closed-world enum checking —
  a wrong-class enum literal loaded with zero diagnostics; expansion closes that hole for every
  expandable spelling. Diagnostics that name an expandable token as their subject now carry the
  canonical expanded IRI, not the authored compact spelling. Unit/quantity/displayUnit terms are
  deliberately untouched: lexical terms, not graph identities — permanently outside expansion.
  The supported `@context` form is an inline prefix map (a single map, or a list of maps merged
  in order, later bindings winning); a remote context reference, `@base`, `@vocab`, and prefix
  bindings that are not absolute IRIs are refused at load as non-subset constructs rather than
  silently ignored, so the same-identity guarantee holds for every document that loads at all.

### Host facade

- **The durable point path is an authored `@id`** (#229). Every facade surface — `point_list`,
  topology block ports and edges, `external_inputs`, pass-through pairs, `Outputs::to_map` keys —
  and the durable `PointDto` projection now name a point by the authored `@id`, as written in the
  source CXF document, of its host-visible identity node: the declared boundary input's node for a
  composite-boundary-driven connector, the connector's own node otherwise. The `@id` is not
  `@context`-expanded — a known gap, so a document re-serialized between compact and expanded
  spellings renames its points until expansion lands. The positional `conn#<N>` form survives only
  as the fallback for hand-built, IRI-less models, which no public API can construct: JSON-LD
  `@graph` is an unordered set, so a semantically identical document could renumber every point,
  and a store keyed on `conn#4` could graft one point's samples onto a different point's history
  with no error. Migration note for hosts: histories persisted under `conn#<N>` keys are
  disposable, not migratable — an index is not traceable to an authored connector once the
  document changes.
- **`Engine::step_realtime` commits computed outputs through the `PointStore` port** (#212).
  It previously advanced a tick and then wrote a hardcoded *empty* batch, while its own
  rustdoc claimed it wrote point state through the store. Sample timestamps come from a
  host-supplied epoch: the seam never invents time, so the epoch is required, and an instant
  that is not exactly representable is a typed error rather than a silent clamp.
- **`Engine::watch`** — key-selected, stateless, deterministic reads of output connector
  values (#201, prose #202).
- **CXF export, a content id, and a read-only topology view** are public on the facade (#192):
  `Engine::export_cxf()`,
  `ExportReport::content_id()` (`cxf:fnv1a128:`), and `Engine::topology()`.
  Catalog introspection is available separately through `oce_blocks::catalog()` and is not
  re-exported by `oce-api`.
- `Engine::get_output` and `CollectSpec::Named` resolve **output** connectors only; naming
  an input point returns `OcError::UnknownPoint` rather than reading the staged input value.
- **Export completeness is enforceable** (#217). `ExportReport::content_id()` would mint a
  well-formed `cxf:fnv1a128:…` identity for a *partially* exported document. Its rustdoc already
  said hosts must require an empty warning list; nothing made them, and because deferral warnings
  are `Warning` severity by design a partial export returns `Ok`. The G36 corpus pins real deferral
  cones of 83/213 and 63/226, so an automated caller would have received a valid-looking identity
  naming a fragment. `content_id_complete()` refuses when warnings exist and reports the count;
  `content_id()` is deprecated in its favour.

### Binding expressions

- 1-D array literals and ranges (#166), array reductions and shape built-ins (#167), 1-D
  indexing (#169), and single-iterator comprehensions with sum-reduction sugar (#170).
- Array-expression parameter values are grounded through the `oce-expr` evaluator (#168).

### ASHRAE Guideline 36 sequences

Ten runtime sequences landed: TerminalUnits CoolingOnly ActiveAirFlow (#154), Reheat
Overrides (#155), CoolingOnly SystemRequests (#156), Dampers (#157) and Alarms (#158),
Generic TimeSuppression (#159), ThermalZones ZoneStates (#160) and ControlLoops (#161),
VentilationZones ASHRAE62_1 Setpoints (#162), and the CoolingOnly Controller (#163).

### Block library

- `CDL.Reals.Limiter` (and the PID-family output clamp, which upstream wires through a
  Limiter instance) follows the upstream comparison chain exactly: a NaN input passes
  through to `y` fail-visible (canonicalized) instead of being absorbed into `uMin`, and
  boundary-equal inputs return their own bits, including zero sign.
- Parameters that upstream declares with no default are required at load time (`Round.n`,
  `AddParameter.p`, `MultiplyByParameter.k`, `Sources.Constant.k` for Real, Integer and
  Logical, `Integers.AddParameter.p`, `Limiter.uMax`/`uMin`, `Hysteresis.uLow`/`uHigh`,
  `LimitSlewRate.raisingSlewRate`, `MovingAverage.delta`, `Logical.TrueDelay.delayTime`,
  `Logical.TrueFalseHold.trueHoldDuration`, `Utilities.Assert.message`). Omitting one
  previously fell through to a silent engine default.
- PID/PIDWithReset range validation matches the upstream `min=100*Constants.eps` annotations
  on `k`, `Ti`, `Td`, `r`, `Ni`, `Nd` (inclusive floor), and the `yMin`/`yMax` pair is
  validated like `Limiter` bounds — error on inversion, warning on equality. `Hysteresis`
  validates `uLow <= uHigh` the same way. The equal-bounds warning is block-agnostic: it
  reports that the bounded interval collapses to a single value rather than always naming
  `Limiter`.
- Unsafe block parameter values are rejected at load time: missing required
  `SampleTrigger.period`, non-positive timing/window parameters, and inverted
  `Reals.Limiter` bounds fail validation.
- Registry-derived static parameter bounds are surfaced through `ParamAttrs`, and
  out-of-range tune-at-rest edits are rejected through `Engine::set_param`.
- All three TimeTable classes publish their authored parameter defaults from single-source
  constants (#200).
- **A required parameter declares its kind, so a wrong-kind value cannot execute as a silent
  fallback** (#225). All 49 `ParamRule::Required` declarations named a parameter and not its type,
  so a model supplying (say) a Boolean where a Real was required loaded clean and ran the
  constructor's own default instead. `Required` now carries an `oce_model::ValueType` and
  validation rejects the mismatch. Integer values still satisfy a Real requirement, which is
  widening, not coercion.
- A machine-readable registry manifest is published with a regenerate-and-diff guard (#171).

### Verification

- **A registry-wide tick-allocation census runs per-PR**, with a permanent positive control
  (#195). The same change made `Log`/`Log10` warnings static strings and made `Sort`
  stack-backed through 64 inputs, so the tick allocates for fewer reasons than before —
  though not zero, and the census is what keeps that claim honest.
- **G36 provenance records are bound to their golden bytes by content digest** (#204). The
  previous `engine_rev` field was deleted as unverifiable: CI checks out at depth 1, so no
  history-based check can run there at all.
- **The CXF structural oracle is vendored and fixtures are gated against it** (#189), with
  Tier-A goldens added for Nand and LimitSlewRate and honest provenance recorded for the 26
  classes that share a kernel (#196).
- **A byte-level hash manifest gates the vendored third-party tree** (#197), anchored to a
  hand-edited subtree SHA, alongside a pinned modelica-json constant.
- A populated Tier 0–4 conformance report is assembled from a real G36 run (#152), and the
  G36 suites route through the L1 funnel band (#149, #150, #151).
- **One Tier-A oracle was audited clean-room** (#218). `Nand`'s provenance record asserted
  `"independent re-derivation"` in its own `source` field, and `Nand` was zero-oracle until
  shortly before that claim was written — so whether its expected values were derived or
  transcribed was an open question nothing could answer. This is **not** Tier 3: this repository
  defines Tier 3 as cross-implementation differential testing, and analytical re-derivation adds
  no independence. What it shows is that clean-room adjudication is executable here at all.

### Hardening

- **Ingest recursion and AST growth are bounded with typed diagnostics** (#194). Expression
  nesting is capped at 64 and AST size at 4096 nodes, enforced at parser entry, again on the
  completed AST, and again in `eval()`. Composite nesting is capped at 64. One gap remains,
  stated rather than assumed: composite boundary resolution recurses per `isConnectedTo` hop
  and is not depth-bounded.
- **Environment-variable switches use truthiness, not presence** (#206, #210, #211, #213).
  `OCE_BLESS`, `UPDATE_EXPECT` and `OCE_SKIP_HOOKS` each treated `0` as "on", so setting one
  to zero armed golden regeneration or disabled the git hooks. Empty, `0` and `false` now
  disable; every other value enables. The policy has a single definition.
- **`.gitattributes` pins line endings repo-wide** (#213). Every checked-in golden is LF and
  every digest is LF-derived, so a contributor with `core.autocrlf` would previously have
  red the byte-level gates — for a reason CI, being ubuntu-only, can never observe.

### Toolchain

- Rust pinned to 1.97.1 with the MSRV raised to match (#208), then the MSRV lowered to
  1.97.0 (#209) when 1.97.1 broke both release-gate surface gates: the gate-only nightly is
  `1.97.0-nightly`, and all 15 per-PR checks stayed green while the release gate did not.

### Documentation and tooling

- Measured tick throughput is recorded per run, with the commit, host and method that
  produced it (#205), with a later correction to the load figure, which was a cold-process
  artifact (#207). The file moved from `BENCHMARKS.md` to
  [`docs/benchmarks.md`](docs/benchmarks.md) in the documentation restructure.
- **Documentation restructured.** The README is a front door of ~220 lines rather than a
  366-line dossier, with its deep material extracted into `docs/` behind an index; a new
  `SECURITY.md` states the threat model and names the one known hardening limit; and the
  README's Quickstart is now a compiled example (`crates/oce-api/examples/quickstart.rs`)
  with a drift guard, so it cannot rot silently.
- **The documentation is published as a site** at
  [jscott3201.github.io/open-control-engine](https://jscott3201.github.io/open-control-engine/)
  (#219). mdBook builds a staged copy whose paths and content bytes are hashed before and after
  staging, so the published pages cannot diverge from the tracked `docs/` corpus. Deployment runs
  on pushes to `main` only, which means the site trails `development` by a release — the README
  says so at the point where it links the site.
- **The README Quickstart is executed by CI, not merely compiled** (#221). It had been a compiled
  example with a byte-level drift guard since #216 — and it **errored on line one**, because
  nothing ever ran it. Compiling proves a snippet type-checks; it says nothing about whether the
  model loads. A new gate step runs it and fails on a non-zero exit. That step rides the existing
  required `gate (light)` job, so it adds no branch-protection context.
- **The README points at the published site and says where it lags** (#222). Merging is not
  publishing: the docs site deploys only on pushes to `main`, so the live Quickstart page was still
  serving the pre-#221 example — the exact version that errored — while `development` carried the
  fix. Found by fetching the page and diffing it, not by anything going red. `TESTING.md` and
  `SECURITY.md` are repository-only and the README now says so, because the site's page list is a
  hardcoded summary that does not include them.
- A read-only revendor reporter sits behind the pin-advance policy (#203).
- The gate is single-sourced in `.agents/gate.sh` (#178), and CI runs that script as a
  coverage backstop rather than a parity check (#180).
- Ten false or stale claims across published documentation were corrected (#214) — including
  a README statement that untrusted expression input could exhaust the thread stack, a
  hazard closed back in #194, and a `CONTRIBUTING.md` statement that CI would not catch a
  new allocating block, which it does on every PR.
