# Changelog

Notable changes to the Open Control Engine. Nothing has been released yet, so every entry
sits under `Unreleased`: the package is not on crates.io and there is no semver promise.

Entries are grouped by area rather than by date, and each names the PR so the change can be
read in full.

An entry is expected from every PR that changes behaviour, the public surface, or a published
claim — added in that PR, not batched later. Nothing enforces this: an entry is a judgement about
what mattered, so no check can derive one, and a check that merely required *some* text would pass
on a placeholder. It has therefore fallen behind four times: #215 recovered 64 commits, #228
recovered seven PRs, #259 recovered ten, and #264 recovered one PR and one missing citation before
the next promotion.

The third recovery discredits the check the second one wrote down here. That check was
`git log main..development -- CHANGELOG.md`, on the reading that returning nothing means the
release is undocumented — but the converse does not hold, and the converse is how it was used.
Run against the ten-PR gap, it returned three commits and passed: three earlier PRs had each
added an entry, which is all the predicate ever asked. It answers "was this file touched in the
range", never "is it current". Reading currency off it is what let the gap reach ten.

What actually establishes currency is a comparison, run at promotion: list the PRs merged into
`development` since the last release, and check that each number appears below.
`gh pr list --state merged --base development` against a grep of this file is enough.
Deliberately not a CI gate — judging whether a change was notable is the part no check can do,
and a gate that accepted any text would restore exactly the false assurance described above.

## Unreleased

### CXF import and export

- **Export exists.** Flat, ground, scalar model graphs serialize back to CXF at the RT-2
  round-trip fixpoint (#175), and where the subset does not apply the former panic is now a
  typed `ExportUnsupported` rejection (#174). Connector §7.4.1 attributes export under the
  bare-scalar canonical subset (#176).
- **Arrays export flattened, and ordinary enum-carrying blocks defer with warnings rather
  than failing** (#177). Reserved pass-through blocks still reject enum parameters because
  omission would erase hidden state. `export_with_report` surfaces deferral warnings; plain
  `export` discards them — so an integrator who needs to know whether an export was *complete*
  must use the reporting form.
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
- **JSON-LD `@import` fails closed** (#269). An `@import` entry inside an inline context map
  previously fell through the generic keyword skip, so the engine ignored identity bindings the
  document required. It now refuses as `non-subset-construct` before any identity slot is expanded,
  for every payload shape and for each occurrence in a context list.
- **Node-scoped JSON-LD contexts fail closed** (#282). A direct `@context` on an `@graph` node,
  followed identity/type reference, or modeled value/term object was retained while expansion
  applied only the document context, so an author and the engine could resolve the same spelling to
  different IRIs or datatypes. Ingest now refuses the scoped context before identity expansion and
  indexing; Layer-A parse/serialize remains lossless.
- **An identity token must be an absolute IRI on both verbatim arms** (#234, #238). `expand_token`
  had two arms returning a token unchanged without checking it was absolute, so a malformed `@id`
  like `2024:MinLoop.con.y`, `:x` or `1st://x` became a durable point key with zero diagnostics —
  while the identical spelling was refused as a `@context` prefix binding. Both arms now classify a
  non-absolute token as `Expansion::Relative` and refuse it through `DiagCode::RelativeIri`,
  symmetric with the binding check; `is_absolute_iri` additionally requires a nonempty,
  whitespace-free remainder after the scheme, because `ab:` and `ab:c d` are legal URI syntax that
  carry no name to key a durable identity on. Declared-prefix rescue of compact tokens is
  untouched — term names are legitimately not scheme-checked, per JSON-LD 1.1 compact IRIs — and a
  `'//'` token is never CURIE-split, so `2024://x` refuses rather than minting `<bound-iri>//x`.
  This is an acceptance change: what newly refuses is invalid JSON-LD no conforming emitter writes.
  Each arm was swept by execution before landing — 142 CXF-shaped documents for the first, 290 for
  the second, spanning the checked-in corpus and the research reference set — and neither sweep
  found a document that would newly refuse.
- **Member value references resolve enclosing-first** (#241). Step-7 leaf grounding was
  latest-wins, so a member expression naming an identifier bound *both* in the enclosing scope and
  by a sibling member took whichever the document happened to push later: member array order
  decided a Hysteresis threshold, the two orders diverging in sign with no diagnostic either way.
  Value lookups now search the enclosing region before the sibling region, and sibling-only names
  still resolve against earlier-declared siblings. Dimension references (`sizeOfDimensions`)
  deliberately keep the undivided latest-wins view, so a name bound in both regions is read twice —
  sibling drives shape, enclosing drives values. An element-count divergence between the two
  readings refuses with both counts in the message; a value divergence at a matching count stays
  silent, like the scalar path. Pinned by
  `hysteresis_thresholds_are_invariant_under_member_array_order`, and the split is stated in
  `docs/cxf-composite-subset.md` Rule 5.
- **A block's own declarations are one order-independent mutual scope** (#247). A block's
  `hasParameter`-then-`hasConstant` chain is evaluated as a single mutual scope by
  dependency-ordered topological evaluation rather than in array order. An own name masks a
  same-named enclosing binding whether or not it was declared first; a forward reference to a later
  sibling grounds; a declaration cycle — including a self-reference that previously ground silently
  against an enclosing binding — refuses as `composite/declaration-cycle` naming its participant
  ring; and duplicate local names in one scope refuse as `composite/duplicate-declaration`. For an
  emitter this means permuting a declaration array can no longer change a loading document's
  imported model, and a refusing document keeps its rule ids and participant sets under permutation
  though diagnostic subjects may relocate. Measured across the change: twelve of the 44 vendored
  modelica-json documents shed 48 `grounding-failed` diagnostics in the two ruled classes — forward
  sibling references that now ground, and specialization-pass generic machinery that went
  non-emitting — and no document among the 44 flipped between accepting and refusing. Most of the
  48 are the second class, so attributing them to sibling references alone reads the change
  backwards; `docs/cxf-composite-subset.md` states the same split.
- **String literal contents do not create declaration dependencies** (#268). The own-scope census
  previously scanned identifier-shaped text inside quotes as code, so `a = "a"` formed a false
  self-cycle and `a = "b"; b = a` formed a false mutual cycle. Quoted bodies and escaped quotes are
  now opaque to dependency analysis. Malformed literals refuse through the expression parser's
  `grounding-failed` diagnostic where grounding verdicts emit; a pruned chain reached only during
  specialization stays silent, matching the existing specialize-pass policy. This widens acceptance
  for malformed documents that previously refused only because of the fabricated tagged cycle.
- **Child-instance interfaces derive from `S231:hasInstance`** (#251). A child node declaring no
  `hasInput`/`hasOutput` of its own now takes its interface from the instance declaration —
  fallback-only, so a node that declares its own interface keeps it, and scalar-only for this
  slice, so a width-parameterized member list refuses by rule id on the derivation path instead of
  deriving a wrong width. Synthesised connectors mint into one total `ConnectorId` order; an output
  short of its declaration is padded and an input short of its declaration refuses; a node carrying
  both an own interface and a conflicting derived one raises the new
  `ConflictingInterfaceDeclaration`. The 44 vendored modelica-json translations gained a
  per-document characterization capture that asserts the corpus size before comparing anything, so
  a document appearing or disappearing fails the test rather than quietly moving the baseline.
- **Declared boundary outputs carry and export their authored §7.4.1 attributes** (#245). A root
  `S231:hasOutput` node's `{unit, quantity, displayUnit, min, max}` were dropped *symmetrically* —
  ingest never read them, because a root boundary node is never in `conn_nodes`, and export never
  emitted them — so every existing round-trip guard stayed green while the metadata vanished. This
  is the #227 failure shape repeating: a loss identical on both sides of a comparison is invisible
  to that comparison. A per-node authored-vs-exported comparator now runs key by key over the G36
  corpus, pinning the population from its own counters at 97 surviving declared outputs of which 61
  carry attributes (`export_declared_output_attrs.rs:283`). Boundary inputs were deferred to #243.
  Two consequences are worth knowing before you emit: reusing the connector-attr
  path makes six refusal shapes reachable from a declared output node that were previously
  reachable only from an instance port — each pinned with its own rejected fixture and exact
  message, none occurring in the G36 corpus.
- **Declared boundary inputs carry and export their own §7.4.1 attributes** (#284, fixes #243). The
  resolver previously back-filled only the boundary IRI onto each child target, so all five fields
  vanished before `ModelGraph` existed. One declaration may fan out, and child ports retain their
  own metadata; `ModelGraph.boundary_inputs` therefore stores declaration attrs separately from
  `external_inputs` and `Connector.attrs`. The current 47-document sweep measures 184 authored
  root inputs, 170 surviving exports, and 100 attr-bearing survivors; every scoped value now
  compares to the authored node key by key. A second oracle removes only those five fields from the
  new exports and recovers all 47 pre-change canonical byte streams. `@type`, undriven declarations,
  and boundary-input §7.10 unification remain out of scope. Two distinct boundary IRIs claiming one
  child input now refuse instead of silently overwriting the first identity, and a boundary input
  cannot reuse an instance connector's IRI. Adding the sidecar is source-breaking for exhaustive
  `oce-model::ModelGraph` literals; the frozen `oce-api` surface is unchanged.
- **Declared boundary-output attributes now unify with their source connector** (#274, fixes #273). The
  `BoundaryOutput.attrs` alias previously sat outside every §7.10 cluster, so a declared output
  claiming `unit: "Pa"` over a `unit: "K"` driver loaded and exported both contradictory contracts.
  Boundary aliases now join the existing deterministic gather-then-decide cluster rooted at their
  source. Conflicting unit, quantity, and bounds refuse; one-sided values propagate to unset
  connector and alias members; `displayUnit` divergence remains advisory. A conflict rolls back all
  propagation, permuted aliases produce identical diagnostics, and the full validator refuses
  malformed hand-built aliases without a panic. Across the 47 swept CXF documents — 46 G36 catalog
  fixtures plus one resolver contract — 33 exported byte streams change and none flips between
  accepting and refusing. The 27 complete exports among those changes receive new
  `content_id_complete` values; the other six remain incomplete before and after the change. A
  declared alias can now supply a previously unset driver connector's unit, quantity, or bounds, so
  `IoInventory` and `point_list(None)` metadata can change for an unchanged input document. Unit and
  quantity also reach the durable `PointDto`; `IoSummary` remains unchanged because it contains
  counts rather than point metadata.
- **Export refuses extra aliases, connections, and undeclared connectors involving reserved
  pass-through connectors** (#277). Reserved lowering connectors have no emitted child-port node;
  the exporter previously skipped those host-built graph entries. The affected hand-built graphs
  now fail with `export-unsupported` instead of losing graph state. Resolver-produced pass-through
  graphs remain representable and unchanged.
- **Export is total for malformed block ports and rejects hidden reserved-block state** (#280,
  fixes #278 and #279). Out-of-range connector IDs in a block's input or output list now return a
  structural `export-unsupported` diagnostic instead of panicking while reading an authored port
  IRI. Reserved pass-through blocks must have no authored instance identity, parameters, or input
  attributes and must carry the scalar type named by their class; violations now reject instead of
  dropping state or re-importing as another reserved class. Deferred reserved blocks also reject
  connector attributes, undeclared owned connectors, and duplicate or misdirected external-input
  membership instead of hiding those defects through cascade omission. Resolver-produced
  pass-through graphs and their exported bytes are unchanged.
- **Context bindings and declaration dependencies no longer admit false identities or cycles**
  (#281). A context term whose value is a compact IRI through another active prefix now refuses as
  non-subset; retaining that spelling made the term and its expanded twin key different points.
  Composite declaration dependencies now come from the parsed expression AST rather than a raw
  identifier scan, so comprehension iterators shadow their bodies and qualified enum references do
  not become sibling edges.

### Host facade

- **HostTick v1 names and pins the engine's existing transition profile** (#301). Every successful
  `Engine::tick` call advances state exactly once, including repeated timestamps; the evaluator
  performs no hidden same-time event iteration or convergence search. `CDL.Logical.Pre` emits its
  call-entry memory and latches current input for the next successful call. Facade tests pin
  initialization, equal-time transitions, non-convergent feedback, host output views, and snapshot
  continuation. Verification accounting now separates 390 source-semantics signal references from
  20 exact HostTick-profile references across three `Pre`-dependent G36 fixtures. Neither those
  profile references nor the `Pre` tests claim Modelica/OpenModelica same-time event equivalence.
- **Engine state can be checkpointed, persisted, and restored**
  (#283, fixes [issue #143](https://github.com/jscott3201/open-control-engine/issues/143)).
  `EngineCheckpoint` is an
  opaque process-local image that may rewind a compatible running engine. `EngineStateSnapshot`
  carries canonical, integrity-checked bytes for durable continuation into a freshly loaded engine;
  its decoder is capped at 64 MiB and rejects malformed or non-canonical input with typed
  `EngineStateError` variants. Compatibility is derived from the executable manifest rather than
  the diagnostic model id, including stable block and connector identities, parameters, port
  bindings, schedule, state-slot revisions, and enum descriptors. Restore validates the complete
  image before mutating the target and never calls the `Store` port. Persistence, authentication,
  generation fencing, real-time epoch configuration, and actuator ownership remain host-owned.
  Sampled-time blocks now refuse finite model times that their integer clock state cannot represent,
  before any engine or store mutation; sampled periods and `SampleTrigger` shifts must also be
  finite at load.
- **A simulation preflight refusal preserves the prior run** (#266, fixes #260). `simulate` now
  resolves fixed inputs and the first list returned by an input closure before clearing the run
  clock or re-seeding state words. An unknown or wrong-typed preflight input therefore leaves model
  time, connector values, outputs, state words and the monotonic-time guard unchanged. The first
  closure result is cached and staged without invoking the closure twice. Later closure failures
  remain partial-run errors: completed ticks and valid pairs before the failing pair stay in effect.
  Store-backed input staging remains inside the tick and is not part of this guarantee. Simulation
  `wall_nanos` now includes input preflight and the state re-seed; recorded-column resolution remains
  outside that interval.
- **A refused load says why, through `oce-api` alone** (#263). `OcError::diagnostics()` returns the
  structured diagnostics behind a failure — stable code, severity, subject, message — where before
  a consumer depending only on this crate could read them off one of the two rejection seams and
  not the other. `OcError::Validate`'s payload is a struct field, which Rust reaches through a type
  the caller cannot name; `OcError::Cxf`'s sits in a tuple variant, which needs the variant path and
  so a dependency on `oce-cxf`. That asymmetry was an accident of how the two errors are shaped, and
  it fell on the wrong side: of the 198 checked-in CXF documents 97 are refused, and 90 of those
  take the resolver seam, including all 40 composite-contract rejection fixtures. Its `Display`
  renders a count, so an unresolved reference and a duplicate `@id` printed the same sentence.
  Nothing new is computed and no diagnostic changed — the data was always on the error.
  **The two seams filter differently.** `Validate` is errors-only; `Cxf` carries the whole finalized
  stream in its pinned order, so its first element can be a warning with the causing errors behind
  it. Filter on `is_error()` rather than indexing. An empty slice means the failure carried no
  diagnostics (malformed JSON, a build failure, host misuse), not that it passed; both
  diagnostic-bearing variants are built only from a non-empty vector. `DiagCode` is deliberately
  still not re-exported: `code.as_str()` already resolves without naming the type, and the enum is
  `#[non_exhaustive]`, so matching it exhaustively is impossible anyway. Two gaps found alongside it
  were filed separately rather than folded in — #261 (resolver warnings discarded when a later
  stage fails, fixed below) and #262 (a composite rejection's rule id is reachable only as a
  message-text prefix).
- **Failed loads retain diagnostics from completed stages** (#292, fixes #261). `Engine::load_cxf` now
  carries resolver, validation, and semantic diagnostics across a later validation, build, or store
  failure in an opaque `OcError::LoadContext`. The allocation-free `OcError::all_diagnostics()`
  iterator returns prior-stage diagnostics followed by the terminal stream without cloning the
  terminal payload; `diagnostics()` keeps its existing terminal-error semantics. `Display` remains
  the terminal message and `Error::source()` exposes the terminal `OcError`. Loads with no prior
  diagnostics retain their original top-level variant. `oce-conformance` keeps contextual
  diagnostics on the failed Tier 0 report without allowing warning-only context to downgrade a
  build or store failure to an advisory.
- **`simulate` is a run restart, and was not one** (#257, fixes #256). It cleared the run clock and
  nothing else, so a reused engine started a horizon from the state words the previous run left
  behind, falsifying the method's own rustdoc and R-SIM-2. Entry now re-seeds those words. The
  reset stops at `words`: connector values, including anything staged through `set_input`, are left
  alone. Two behaviour changes ride with it, both in `docs/host-responsibilities.md` — chunking one
  horizon across two calls no longer continues the trajectory, and a what-if interleaved into a
  live run now resets that engine's stateful blocks.

- **The durable point path is an authored `@id`** (#229). Every facade surface — `point_list`,
  topology block ports and edges, `external_inputs`, pass-through pairs, `Outputs::to_map` keys —
  and the durable `PointDto` projection now name a point by the authored `@id`, as written in the
  source CXF document, of its host-visible identity node: the declared boundary input's node for a
  composite-boundary-driven connector, the connector's own node otherwise. The `@id` was not
  `@context`-expanded when this landed, so a document re-serialized between compact and expanded
  spellings renamed its points; #230 below closed that gap. The positional `conn#<N>` form
  survives only as the fallback for hand-built, IRI-less models, which no public API can
  construct: JSON-LD `@graph` is an unordered set, so a semantically identical document could
  renumber every point, and a store keyed on `conn#4` could graft one point's samples onto a
  different point's history with no error. Migration note for hosts: histories persisted under
  `conn#<N>` keys are disposable, not migratable — an index is not traceable to an authored
  connector once the document changes.
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
- `Engine::get_output` and `CollectSpec::Named` resolve **output** identities only — output
  connectors, and since #236 the declared boundary-output names that alias them; naming an input
  point returns `OcError::UnknownPoint` rather than reading the staged input value.
- **Declared boundary outputs are addressable facade point identities** (#236). A CXF root's
  `S231:hasOutput` declares the composite's output contract, but no facade surface exposed those
  names, so hosts addressed outputs by internal driver IRIs. `watch`, `get_output` and
  `simulate(CollectSpec::Named)` now accept a declared boundary-output IRI as a read alias for its
  driving connector's slot, and `Topology` gains
  `boundary_outputs: Vec<DeclaredOutput { path, driver_path }>` enumerating the declared interface
  in an order that is deterministic per document but is **not** the authored `hasOutput` array
  order — elided entries follow their declared nodes' `@graph` positions, then pass-throughs are
  appended after them — so key by name, never by index. `set_input` refuses declared names:
  the alias space is output-only. Two diagnostics arrive with it: `UndrivenBoundaryOutput`
  (Warning — a declared-but-undriven output previously imported silently and then vanished from
  re-export, the #227 class one level down) and `BoundaryOutputShadowsConnector` (Error — a
  document double-booking one IRI as both declared output and live connector previously loaded
  clean, carrying two conflicting truths about one name). The change is additive: `point_list`,
  `Outputs::to_map`, `CollectSpec::All`, `StepReport.written` and every pre-existing point key are
  bit-identical, and no durable key moves. It is one of two changes in this range that move the
  `oce-api` public-API baseline: this change adds 40 lines, of which the load-bearing lines are the
  `DeclaredOutput` type, its two `String` fields, and `Topology::boundary_outputs`; the remainder
  are derived and blanket impls the baseline enumerates in full. #263 later adds two lines for
  `OcError::diagnostics` and the type's inherent implementation row.
- **`Engine::step_realtime` resolves its durable batch once at load** (#246). It re-derived the
  batch's key identity on every step — one path-`String` clone per output point, plus two `Vec`
  collections — where `simulate` had always resolved its identity once before ticking. The batch is
  now minted at load and refreshed in place, taking a warm `step_realtime` down to the pre-existing
  snapshot-`Box` allocation floor (`warm_step_realtime_allocates_only_the_snapshot_box`, which
  asserts against the harness's own floor snapshot rather than a literal). No public API change.
  The guard matters more than the saving: a cache never invalidated on reload passed the entire
  workspace suite undetected, because no test had loaded a second model into one `Engine`.
  `reloading_a_second_model_swaps_the_committed_key_set` closes that, alongside a corpus-wide
  golden over captured `write_points` batches that records count, ordered key digest and ordered
  value digest as three separate fields — a combined digest cannot distinguish index drift from a
  key rename.
- **Input staging is all-or-nothing, and `InputSource::Constant` names resolve once per run**
  (#255). `simulate` resolves a `Constant` list into connector ids once before ticking instead of
  hashing the names every step; the write itself still happens every step, at the same cadence as
  before. `InputSource::Closure` is untouched — a closure may name different points at each `t`, so
  its names cannot be pre-resolved. The acceptance change rides along and is host-visible: a bad
  name or wrong-typed value now refuses without writing the pairs that precede it, where per-name
  staging wrote them first. That matches what the sibling collect path already documented for
  itself, and is characterized by test rather than left to a reader. Separately, `Outputs::get`
  binary-searches rather than scanning linearly; the in-tree caller count is one and it is a test,
  so no in-tree win is claimed — the point is that a `ConnectorId`-keyed read a host may call in a
  loop should not be O(n). The ascending-order dependency is pinned under both build profiles.
- **Connector IDs must equal their arena positions before a model reaches BUILD** (#281).
  `ConnectorId` is documented and consumed as a dense arena index, but the structural gate enforced
  that invariant only for blocks. An in-crate hand-built graph could therefore reach `Outputs::get`
  with an unsorted snapshot and miss an existing output under binary search; current public loaders
  already mint dense ids. The validated in-crate load tail now returns a typed `malformed-document`
  diagnostic before allocating state or building the snapshot, protecting future non-CXF loaders
  at the same seam.
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

- **Human-adjudicated conformance discrepancies have a bounded evidence register** (#285). The
  register is test-only and initially empty; a separate nonempty synthetic record exercises its
  closed schema, lifecycle, repository containment, and evidence digests on every PR. Register
  membership cannot alter discrepancies, tolerances, comparison results, tier status, goldens, or
  test outcomes, and the register adds no public or runtime surface.
- **One exhaustive OpenModelica differential now exists for `CDL.Logical.Nand`** (#286). Two sandboxed
  native-arm64 OMC 1.25.1 runs produced byte-identical raw CSV for all four Boolean input pairs; a
  strict keep-last projection feeds the existing facade-bound exact harness. The evidence set binds
  the image, source trees, raw and canonical bytes, logs, OCI metadata, and semantic/projection
  mutation controls. This is scoped Tier-3 evidence for one stateless Boolean class. The global
  Tier-3 report remains skipped, and the result says nothing about sequences, stateful behavior,
  numeric tolerances, or cross-architecture OMC identity.
- **One stateful OpenModelica differential now exists for `CDL.Logical.Toggle`** (#295).
  Two sandboxed native-arm64 OMC 1.25.1 runs produced byte-identical event traces for an initially
  true input, repeated rises, clear-only reset, simultaneous rise with clear priority, and clear
  release. The facade runs at the exact emitted timestamp bits and is checked against an independent
  recurrence. A one-token Latch substitution, a live keep-first projection mutation, and four
  independently accumulated wrong recurrences fail at pinned rows. Separate controls catch a missed
  clear-only reset and lost clear priority on a simultaneous input rise. This is scoped Tier-3
  evidence for one stateful Boolean schedule;
  global Tier 3 remains skipped, with no sequence-wide, numeric, or cross-architecture claim.
- **Nextest policy is versioned and shared across codegen modes** (#267). CI pins 0.9.143 and the
  repository config refuses older runners. Debug, release-codegen, and public-API profiles inherit
  zero retries, timeout and leak failures, and Jenkins-format JUnit reports; CI uploads the reports
  for 14 days after clearing any cached copies.
- **A registry-wide tick-allocation census runs per-PR**, with a permanent positive control
  (#195). The same change made `Log`/`Log10` warnings static strings and made `Sort`
  stack-backed through 64 inputs, so the tick allocates for fewer reasons than before —
  though not zero, and the census is what keeps that claim honest.
- **The local gate now runs CI's standalone Quickstart execution check** (#258).
  `check-quickstart-runs.sh` had run only as a separate required CI step, so
  `bash .agents/gate.sh` could pass locally while CI still ran more. A narrow coverage check now
  requires a textual path reference in the gate for each `*.sh` under `.github/scripts/`; it does
  not claim general parity with inline workflow steps or scripts elsewhere.
- **The registry-wide allocation census now measures the evaluator thread without probabilistic
  retries** (#271). The process-global meter could attribute unrelated thread traffic to whichever
  block was ticking (#231); its replacement counts only the measured thread. Each block now runs
  separate initial and post-warm-up eight-tick windows over signed zero, finite, NaN, and infinite
  inputs. Controls pin first-tick-only and periodic allocation detection, the known wide `Sort`
  allocation, and exclusion of off-thread traffic. Current catalog blocks do not delegate work to
  worker threads; a future block that does needs a companion worker-allocation guard.
- **Facade allocation guards now measure only the calling thread** (#275, fixes #272). The previous
  process-global allocator could charge libtest or worker-thread traffic to a tick or simulation
  region despite the integration test's mutex. The replacement preserves the manual-tick,
  snapshot-floor, realtime-step, and fixed-per-run simulation contracts; watch remains the positive
  meter control, and a synchronized worker allocation proves off-thread traffic is excluded. The
  guards cover the calling thread; future facade worker delegation needs its own allocation guard.
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
- **Eighteen public methods are pinned by a compile-time guard** (#255). The two `cargo public-api`
  baselines run in `release-gate.yml` only, never on a PR into `development`, so a signature drift
  in a method the frozen set does not otherwise reach could ride a fully green PR. `guards.rs` is a
  non-test module, so a drift there now fails the ordinary `cargo build` that per-PR CI does run.
  Two pins are narrower than the method they cover and say so rather than overclaim.
  `ExportReport::content_id` is deliberately excluded with the reason recorded beside the
  `content_id_complete` pin: it is `#[deprecated]`, so naming it in a non-test module is a hard
  error under `-D warnings`, and an `#[allow]` would be worse than the gap — the only change left
  to make to a deprecated method is deleting it, and a deletion is public-surface removal, which
  the release-gate baseline is the right review to adjudicate.

### Hardening

- **Composite boundary lowering is iterative and resource-bounded** (#290, #298, #299). A path may enter at most 64
  non-top `isConnectedTo` boundary nodes, and one document may cause at most 65,536 target
  examinations or 8 MiB of aggregate target-IRI bytes within boundary walks. Attempting the next
  hop, examination, or byte returns a deterministic `malformed-document` diagnostic before a
  partial graph is built; resource-limit diagnostics omit the attempted target subject to avoid an
  additional attacker-controlled IRI copy at refusal. Direct leaf wiring is unchanged. The walk
  remains path-local and preserves canonical order and duplicate multiplicity, so multiple drives
  remain visible to single-assignment validation. An active unorientable relation now rejects after
  the bounded walk when boundary elision would otherwise erase it, whether the boundary is its
  authored source or target. Active boundary-source relations to inactive targets remain loud.
  Listed node-less and omitted padded outputs can drive through a re-anchored boundary edge. These
  synthesized drivers remain charged as authored targets under the byte bound. Deferred diagnostics
  omit attacker-controlled subjects, and boundary resource errors retain precedence. Expanded edges
  that repeat one missing endpoint emit one unresolved-reference diagnostic across ordinary and
  boundary-specific orientation, preventing fanout from multiplying the same subject allocation.
  These limits are engine acceptance bounds, not CDL semantics.
- **Ingest recursion and AST growth are bounded with typed diagnostics** (#194). Expression
  nesting is capped at 64 and AST size at 4096 nodes, enforced at parser entry, again on the
  completed AST, and again in `eval()`. Composite nesting is capped at 64.
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

- **Release records and contributor gate prose were reconciled before promotion** (#287). State
  snapshot, known-divergence, and scoped OpenModelica entries now cite their implementing PRs, and
  the contributor guide and gate comment name the three test packages that run per PR.
- Measured tick throughput is recorded per run, with the commit, host and method that
  produced it (#205), with a later correction to the load figure, which was a cold-process
  artifact (#207). The file moved from `BENCHMARKS.md` to
  [`docs/benchmarks.md`](docs/benchmarks.md) in the documentation restructure.
- **Documentation restructured.** The README became a front door rather than the 366-line dossier
  it had grown into, with its deep material extracted into `docs/` behind an index; a new
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
- **A documentation currency pass corrected twelve measured drifts** (#244), the first under the
  standing currency directive. The substantive one: the README claimed all 410 signal goldens were
  compared bit-exactly, and 21 transcendental, psychrometric and solar Real goldens in fact run
  under the documented 1e-12 aligned-tolerance band. `docs/verification-evidence.md` — the page the
  README cites *for* the honest accounting — carried the same false claim in four more places.
  Both now state the measured split, and `TESTING.md`'s "never epsilon" rule names its one
  exception, scoped to those 21 goldens rather than left as a general license. Three tracked
  scripts were committed `100644` while `scripts/install-hooks.sh` chmods them to `755`, so the
  documented onboarding step left every contributor with three spurious mode diffs; the committed
  modes now match, verified blob-identical on both sides. Nine stale line citations across five
  `docs/` pages were refreshed — each was exact when written and shifted under #217–#241, which is
  the ordinary failure mode of citing a line number at all.
- A read-only revendor reporter sits behind the pin-advance policy (#203).
- The gate is single-sourced in `.agents/gate.sh` (#178), and CI runs that script as a
  coverage backstop rather than a parity check (#180).
- Ten false or stale claims across published documentation were corrected (#214) — including
  a README statement that untrusted expression input could exhaust the thread stack, a
  hazard closed back in #194, and a `CONTRIBUTING.md` statement that CI would not catch a
  new allocating block, which it does on every PR.
