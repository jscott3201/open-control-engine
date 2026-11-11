# Changelog

Notable changes to the Open Control Engine. Releases are git tags; nothing is on crates.io yet
and there is no semver promise. `v0.1.0` was a git pin cut for one external consumer before this
file tracked releases, so it has no section of its own: every entry from the start of the project
through 0.2.0 sits under [0.2.0](#020).

Entries are grouped by area rather than by date, and each names the PR so the change can be
read in full.

An entry is expected from every PR that changes behaviour, the public surface, or a published
claim — added in that PR, not batched later. Nothing enforces this: an entry is a judgement about
what mattered, so no check can derive one, and a check that merely required *some* text would pass
on a placeholder. It has therefore fallen behind eight times: #215 recovered 64 commits, #228
recovered seven PRs, #259 recovered ten, #264 recovered one PR and one missing citation, #287
recovered one missing record and two missing citations, #288 recovered #287, #302 recovered
#288, #296, #300, and #301 before the next promotion, and the 0.2.0 release preparation recovered
the 22 PRs #308 through #331 (#308 had added its entry under its issue number, #307).

The third recovery discredits the check the second one wrote down here. That check was
`git log main..development -- CHANGELOG.md`, on the reading that returning nothing means the
release is undocumented — but the converse does not hold, and the converse is how it was used.
Run against the ten-PR gap, it returned three commits and passed: three earlier PRs had each
added an entry, which is all the predicate ever asked. It answers "was this file touched in the
range", never "is it current". Reading currency off it is what let the gap reach ten.

What actually establishes currency is a comparison, run at promotion: list the PRs merged into
`development` since the last release, and check that each number appears below.
Every squash-merge subject on `development` ends in `(#N)`, so
`git log --oneline origin/main..origin/development` against a grep of this file is enough.

GitHub PR numbers collide with the retired Forgejo numbering, so PRs merged on GitHub are cited
as `(GitHub #N)`; bare `(#N)` citations predate the move, and the Forgejo CI port is cited as
`(Forgejo #332)` to keep it apart from GitHub #332.
Deliberately not a CI gate — judging whether a change was notable is the part no check can do,
and a gate that accepted any text would restore exactly the false assurance described above.

## Unreleased

### Facade

- **`oce_api::ENGINE_VERSION` names the engine version this build stamps and accepts** (GitHub
  #592, issue #536). It re-exports `oce_graph::blob::ENGINE_VERSION`, an `EngineVersion` holding
  the major, minor and patch of the workspace version, with any pre-release or build suffix
  dropped. Every compiled-schedule blob and state snapshot this build writes carries it in its
  header, and either `EngineVersionMismatch` names it as `expected`. A host's storage envelope,
  an `oce-verify` receipt or an `oce-replay` envelope can now record "written by engine X" from a
  named facade item, without a direct `oce-graph` dependency and without parsing a header, an
  error or a version string. It is a release version, not a build identity. It is not the
  compatibility descriptor's `oce-api-version`, which keeps the suffix. Tests in
  `tests/state_engine_version.rs` hold it equal to `oce-api`'s own package version and to the
  bytes of a freshly written blob and snapshot, after frames and straight after load. They also
  check that both refusals name it as `expected`. The facade-contract check's positive control now
  compiles a host-side use of it, and fails with E0425 without the re-export. Documented in the
  [identity glossary](docs/facade-contracts.md#identity-glossary) and in
  [host responsibilities](docs/host-responsibilities.md#persist-engine-state-outside-the-engine).
  No behaviour, format or golden changes.
- **A state snapshot names the engine version that wrote it, and only that version restores it**
  (GitHub #540, issue #530; owner rulings of 2026-11-09). The fixed header gains six bytes after
  the execution-state ABI revision: major, minor and patch as little-endian `u16`, from the constant
  a compiled-schedule blob records (`oce_graph::blob::ENGINE_VERSION`; neither records a
  pre-release suffix). The header is now 30 bytes and every snapshot is six bytes longer.
  `EngineStateSnapshot::from_bytes` refuses any other version with the new
  `EngineStateError::EngineVersionMismatch { found, expected }`, checked after the magic and the
  format revision and before the body length, the trailer or the body is read, so
  `Engine::restore_state` never receives such a snapshot. The execution fingerprint now covers
  the version bytes, as the trailer already covered the header: a snapshot edited to claim this
  build's version fails its trailer, and with the trailer recomputed, its fingerprint.
  **Breaking, in place:** the format and ABI revisions stay 1 and there is no reader for a header
  without the field. `development` wrote that header between GitHub #518 and this change and no
  release did, so no version bump accompanies it; one such snapshot is kept as a refused fixture
  (`fixtures/state_unstamped.hex`, `tests/state_old_layout.rs`), and the older empty-model
  vector, and the release snapshot once its numbers are edited, are now refused at the version
  field as well. The three inline state pins are re-blessed and from here on move with a
  workspace version bump; blob goldens, canonical content ids and
  `tools/golden-gen/goldens` are unchanged. The engine version is not a build token: the host
  envelope still binds a snapshot to its model, build and deployment
  ([state continuation](docs/state-compatibility.md#wire-revision-and-integrity)).
- **Output types in slot order** (GitHub #525, issue #460). `FrameSlots::output_types()` returns
  one `ValueType` per output slot, parallel to `outputs()`: the declared type of the connector that
  drives each boundary output (for a pass-through declared output, the type of the input it
  copies), known at load. Until now `outputs()` gave paths only and an output's type existed only
  on the first accepted frame; `oce-verify` types every expected value and compares a mutant's
  boundary with the subject's before any frame runs (`docs/design/vectors.md`, decisions 7 and 27).
  It is the type alone, as `InputDefinition::value_type` is for an input, and an Enum output is
  `ValueType::Enum` with its exact class identity. For every frame accepted in the load the map
  was built from, through either entry, the `j`-th output value has the type of entry `j`. The slice is borrowed (no allocation
  on read), is empty for an executable with no boundary output, and belongs to the map's load
  incarnation like the paths: a superseded map keeps its types and is still refused with
  `StalePreparedFrame`. Additive only: no existing signature, refusal, error variant or behaviour
  changes, `execute_slots` and the frame driver are untouched, `oce-replay` still fixes its output
  columns from the first accepted frame, and every golden is byte-identical. The `is_current`
  staleness probe that shared this issue is split off to M6.
  Evidence (`tests/slot_output_types.rs`): over the 46 G36 fixtures and the two library rules, each
  of the 134 output slots (102 Real, 18 Integer, 14 Boolean) is typed before any frame exactly as
  its CXF document node declares it, by a `serde_json` walk that shares no engine helper, with a
  transposition control on the ten mixed-type boundaries; all 21,440 output values of the shared
  160-frame schedule and the named path's first `CompletedFrame` have their slot's type; an empty
  boundary, Real, Integer and Boolean pass-through outputs copied bit for bit, a reload of
  identical bytes (a new map, the same types, the old map refused), a dirty parameter resume, a
  reload of a different model, a failed load and an allocation-free read with a positive control.
  An Enum and a String boundary output, which no CXF can declare, are hand-built in
  `frame_slots_tests.rs`: seven declared outputs over every value kind and two enumeration
  classes, typed in lexical slot order with the exact class, on both entries. The signature guard
  (`guards.rs`) and the facade compiler contract's positive control name the accessor;
  `docs/complete-frame-contract.md`, `docs/facade-contracts.md` and the README are updated, and
  `docs/design/vectors.md` records that the accessor landed.

- **Enumeration and constant-package table** (GitHub #488, issue #468). `oce_api::enumerations()`
  lists, beside `class_metadata()`, every named type the engine knows as one `&'static` table of
  `EnumerationClass { class_path, qualified_path, kind, id, members, values }`: the four CDL
  enumerations, the ten G36 enumerations (`ASHRAEClimateZone`, `ControlEconomizer`, `CoolingCoil`,
  `EnergyStandard`, `FreezeStat`, `HeatingCoil`, `OutdoorAirSection`, `PressureControl`,
  `Title24ClimateZone`, `VentilationStandard`) with their members in source order, and the four G36
  Integer constant packages (`DemandLimitLevels`, `FreezeProtectionStages`, `OperationModes`,
  `ZoneStates`) with each constant's value. `class_path` has one form for every row, relative to
  `Buildings.Controls.OBC.` (`CDL.Types.SimpleController`, `ASHRAE.G36.Types.OperationModes`); each
  row is an `EnumerationKind::Enumeration` (a parameter and connector type, carrying its
  `EnumClassId`) or an `EnumerationKind::ConstantPackage` (named Integer constants, never a
  parameter type). `enumeration(class_path)`, `enumeration_by_id(id)`,
  `enumeration_class_path(id)`, `enumeration_member_value(class_path, member)` and
  `enumeration_types()` look rows up without allocating, and `EnumerationClass::member_value` gives
  a member's ordinal or Integer (`MemberValue`); `EnumClassId` is re-exported.
  `enumeration_class_path` is the relative authoring form; the durable spelling snapshots, blobs,
  the replay wire and class hashes record stays `EnumDescriptor::class_path` (fully qualified for
  G36), and the rustdoc on both says so. `CATALOG_JSON`,
  `catalog_content_id`, every class signature hash and the `class_ports` golden are unchanged. Tests
  (`crates/oce-api/tests/enumerations.rs`) hold the table to every `EnumMembers` rule, `Required`
  Enum kind, enum default and Enum-typed parameter the catalog publishes (members in order), and to
  every Enum parameter a load of the G36 corpus keeps. #465 (recording an Enum column's class in
  `oce-replay` traces) stays open; a trace must record the durable `enum_descriptor` spelling, not
  `enumeration_class_path`.

- **Canonical content id** (GitHub #474, issue #470; design note GitHub #471).
  `Engine::content_id()` mints one identity for a loaded sequence, `oce:canon1:` and the 64 hex
  digits of the SHA-256 (via `sha2` 0.10, `default-features = false`, on `oce-api` only) of the
  `OCECANON` version 1 encoding of the loaded flat graph, exactly as `docs/design/content-id.md`
  specifies: boundary inputs (name, type, `quantity`/`unit`/`displayUnit`, effective domain,
  fan-out), boundary outputs including pass-through outputs (driven by their input), non-reserved
  blocks (name, class path, class signature hash, bound parameters sorted by name) and connections
  by port index, every name stripped of its namespace with the whole-IRI fallback. It refuses
  before a load and while parameter edits are pending (`compile_blob`'s preconditions) and is a
  typed `OcError::ContentId` for a hand-built model with an unnamed block. `ContentId` has
  `Display`, a checked `parse`/`FromStr` of the exact text (refusing with `ContentIdParseError`)
  and `digest()`; it does not convert from
  a string, `CatalogContentId` or `DomainKey`. `Engine::canonical_content_bytes()` returns the
  encoded bytes, versioned with the id text (`canon1`), for recomputing the digest or diagnostics;
  no decoder is provided or promised. The order-bearing export tag is removed:
  `ExportReport::content_id_complete()`, the deprecated `ExportReport::content_id()`,
  `CompleteExportContentId` and `ContentIdError::Incomplete` are gone; `ExportReport::complete()
  -> Result<(), ExportIncomplete>` keeps the completeness check, and `ContentIdError` now names the
  minting refusals (`UnnamedBlock`, `UnregisteredClass`, `UnknownEnumClass`, `MalformedModel`,
  `TooLarge`, carried as the new `OcError::ContentId`). The compatibility descriptor is revision 2: its tenth line is `content:none` or
  the `oce:canon1:` id, `CompatibilityDescriptor::current` takes an `Option<&ContentId>` and is
  infallible, `export_content_id()` is `content_id()`, `check_compatible` requires revision 2, and
  `ExportPresence`/`ExportContent` are `ContentIdPresence`/`ContentIdMismatch`; the replay record's
  descriptor grammar follows (its wire size grows by one byte, `export:none` to `content:none`).
  Evidence: the note's two worked examples reproduced byte for byte from hand-assembled vectors
  (`offset_gain` 394 bytes `fbbea1ef…`, `TrueDelay` 216/233 bytes `aff7775c…`/`05b0f40f…`); 13
  same-id rewrites of all 47 corpus documents (`@graph` reversed and rotated, relation arrays
  reversed, keys reversed, pretty-printed, compact IRIs, two other namespaces) and a leaf
  port-list reversal that keeps the id exactly when the binding is unchanged; load → export →
  load canonical-byte equality for the 34 complete exports; every parameter's one-bit, ±1,
  flipped or other-member edit (837 edits, 120 of them `+0.0` to `-0.0`) and block, boundary and
  sequence renames, rewires, class swaps, boundary label changes, tightened bounds and removed
  bindings each moving the id with no two variants sharing one; the 160-frame schedule over every
  document and four permuted variants giving identical outputs, per-block state and refusals; the
  pass-through case, name collisions, refusals, the FIPS 180-4 vectors and the largest encoding
  (64,491 bytes) pinned. `tests/fixtures/content_id/restamp.tsv` maps all 47 documents' retired
  `cxf:fnv1a128:` tags (`-` for the 13 partial exports) to their new ids, in the format the
  library's re-stamp uses. Re-blessed: `compatibility.txt` and `compatibility_export.txt` (revision
  and `content` line); `oce-cxf`'s `fixture_corpus_import.txt` gains one line for each of the four
  new `tests/fixtures/content_id/*.jsonld` documents (the tracked corpus is 222 with #477's
  four), every existing line unchanged. `g36_export_content.txt`, the export bytes and every other golden are
  byte-identical. PC-018, PC-021 and PC-036 are reworded and PC-041 names a format-2 blob (product
  contract revision 23). `oce-author`'s `validate_cost` ratio test fails on `development` as well on
  a local arm64 debug build; it is tracked as #478, untouched here.
  `oce-author`'s test helpers (merged first in GitHub #473) move to the new id: `identity` is
  `Engine::content_id`, and `executable` zeroes header bytes 20..52 and the CRC. The order tests
  invert, because order no longer moves the id: `block_order_moves_the_bytes_but_not_the_identity`,
  `boundary_order_moves_the_bytes_but_not_the_identity`, the direct-target permutation, and
  `node_order_alone_moves_the_sumzone_export_but_not_its_identity`, which now pins the export-byte
  difference. The kelvin/celsius semantic-seam test is unchanged. `fixture_corpus_import.txt`'s
  corpus pin is 222: GitHub #477 added four fixtures, and this PR adds four more.

- **Slot-indexed execute entry** (GitHub #459, issue #444). `Engine::frame_slots()` resolves the
  executable boundary once into an owned `FrameSlots` map (input slots in `input_definitions()`
  order, output slots in `CompletedFrame::outputs()` order, both the compiled blob's I/O order),
  and `Engine::execute_slots(&slots, t, &values)` commits one complete frame from one value per
  input slot, returning a `SlotFrame` that borrows the outputs in slot order and the frame's
  warnings. It runs the same core driver (`oce_graph::frame::FrameDriver`) as `execute_frame`, with
  the same refusal order, error variants and atomicity: a shorter vector is `FrameMissingInput` for
  the first absent slot, and the one new variant, `OcError::FrameExtraInputs { expected, found }`,
  refuses a longer one before any value is read. The map carries the prepared-plan incarnation
  fence, so a reload or dirty resume makes it `StalePreparedFrame` instead of rebinding positions;
  that variant's rustdoc and `Display` text now name the slot map too ("prepared input frame or slot
  map is stale for this executable context").
  Once the map exists a slot frame allocates nothing; warning records are reused in place, so only
  a frame that emits more or longer warnings than any before it allocates (census: 0 allocations
  on 7,357 of the 7,360 corpus frames; the other three each grow a warning record). New evidence:
  dual-entry equivalence on all 46 G36 fixtures (`tests/slot_frames.rs`, and the slot entry as a
  fourth caller in `blob_dual_path_tests.rs`), refusal parity for every class on every fixture
  (`tests/slot_frame_refusals.rs`, String values in every slot included), Enum- and String-slot
  type and domain parity on detached probes plus private preflight and transition-count controls
  (`frame_slots_tests.rs`), a whitespace-tolerant, path-exact check that `frame.rs` and
  `frame_slots.rs` are the only `FrameDriver` constructors and that every excluded test source
  compiles only under `#[cfg(test)]` (`tests/frame_driver_ownership.rs`), and the slot path in the
  facade compiler contract's positive control.
  `docs/complete-frame-contract.md`, `docs/facade-contracts.md`, `docs/execution-profile.md`,
  `docs/host-responsibilities.md`, PC-035 in `docs/product-contract.md` and the README name the
  slot entry beside the named path. `oce-graph` and every golden are unchanged.

- **Resolved port names, class descriptions and parameter metadata** (GitHub #461, issue #440;
  authoring-layer design note decisions 15 and 18). `oce_api::class_ports(class_path, params)`
  names every port of a class as configured, in port index order: scalar connectors keep their CDL
  name, a vector connector `u[n]` is `u1`…`un` (the corpus's spelling, bound by position by the
  import), a matrix `y[r, c]` is `y1_1`…`yr_c` row-major; a `TimeTable` source has `y1`…`yn` for an
  `n`-value-column table. The parameters are first checked with the load gate's own rules through
  the new `oce_validate::validate_block_parameters`, so a table a load would refuse is
  `ClassPortsError::InvalidParameters` (carrying the load diagnostics) and an unregistered class is
  `ClassPortsError::UnknownClass`; nothing panics. `oce_api::DiagCode` is now re-exported (the type
  of `Diagnostic::code`), so a host matches those refusals without `oce-diag`; `as_str()` is the
  stable key. `oce_api::class_metadata()` gives every class in catalog order a one-sentence
  description (the vendored Modelica class description string, checked against `third_party/`, or
  an engine-written sentence for `TrueHoldWithReset` and the three reserved identities) and its
  parameters, each with `value_type`, `default` (the catalog's `CatalogDefault`: a literal,
  `Derived` with its formula, or `Required`) and `settable`; the `TimeTable` sources' rule-only
  `table_<row>_<col>` is listed as `Required`. Per the
  [#440 amendment](https://github.com/jscott3201/open-control-engine/issues/440#issuecomment-6730952664),
  `settable` means "an author may assign this parameter" and is false only for the reserved
  identities: the twelve derived defaults (`MatrixGain.K`, `PIDWithReset.y_reset`, …) are plain
  overridable CDL parameters, and the M2 design note's `parameter-not-settable` row is narrowed to
  the reserved classes. `oce_api::param_element_name` expands a catalog template (`k_<i>`,
  `K_<row>_<col>`) to the names the CXF import mints, and the placeholders are exported as
  `PARAM_INDEX_PLACEHOLDER`, `PARAM_ROW_PLACEHOLDER` and `PARAM_COL_PLACEHOLDER`. Underneath,
  `oce_blocks::resolved_ports` synthesizes the names over the registry constructor's
  `resolved_signature()` and refuses a layout that does not cover it exactly;
  `oce_blocks::class_description`, `class_parameters` and `ParamNameTemplate` carry the metadata.
  All four new `oce-blocks` modules compile only with the `std` feature, so the bare-metal firmware
  image is byte-for-byte the size it was on `development` (`thumbv7em` `.text` 144,428 B, flash
  206,264 B; `core::fmt` 21 and 22 functions). No constructor, tick behaviour or `ParamRule`
  changes, and the metadata stays outside the revision-1 catalog: `CATALOG_JSON`,
  `catalog_content_id`, `class_signatures.tsv` and every golden under `tools/golden-gen/goldens`
  are byte-identical. A new golden, `crates/oce-blocks/tests/fixtures/class_ports.golden.txt`
  (bless switch `OCE_BLESS_CLASS_PORTS`), pins the names of every class at its empty table and of
  every width-driven class at widths 1, 2 and the maximum, every description, and every
  parameter's type, default and settable flag. Cross-checks run against the CXF import: every G36
  document's bound connectors, and hand-written documents for the replicators, matrix reductions,
  `Sort`, the extractors, filters and extract signals, `Integers.MultiSum` and the three
  `TimeTable` sources, must equal `class_ports`; preserved arrays import under exactly the
  expanded template names. The naming seam's every refusal is reached with hand-built signatures,
  names are checked against the constructed blocks' feedthrough under every flag spelling, and
  `validate_block_parameters` is held equal to whole-model validation over the constructor fuzz's
  hostile tables for every class (the generator moved to
  `crates/oce-blocks/tests/support/hostile_tables.rs` so both crates share it). The fuzz also
  names every constructible table. PC-041's evidence line for
  `every_constructor_survives_hostile_parameter_tables` in `docs/product-contract.md` moves to
  where that test now starts. `docs/cdl-coverage.md` states the naming contract.

### Execution core

- **A compiled-schedule blob of another engine version is refused by its version, whatever else
  is wrong with it** (GitHub #579, issue #535). `oce_graph::blob::decode` now checks the size
  limit, that a header and a trailer fit, the magic, the format, then the engine version, and only
  then the total length, the CRC-32C trailer, the profile and the flags. It used to check the
  length before the format and the version after the CRC and the profile, so a blob another
  version wrote could report `LengthMismatch` or `IntegrityMismatch` ("corrupt") before its
  version was read. The state snapshot decoder has used the new order since issue #530, for the
  same reason: formats stay at revision 1 and change in place, so another version may lay out its
  length, trailer and body differently. **Behaviour change:** a blob wrong in two of these ways
  can report a different refusal than before. Another engine version with a wrong length, a wrong
  CRC or another profile now reports `EngineVersionMismatch`, where it reported
  `LengthMismatch`, `IntegrityMismatch` or `UnsupportedProfile`. So a flipped bit in the version
  field reports the version it now spells, not an integrity failure. A wrong format with a wrong
  length reports `UnsupportedFormat`, not `LengthMismatch`. A blob with one defect reports what
  it did before. `BlobError` lists `EngineVersionMismatch` straight after
  `UnsupportedFormat`, matching the order. No byte of the format changes: the 46 blob goldens,
  the frame digests and `tools/golden-gen/goldens` are unchanged. One rule now covers both
  artefacts ([which refusal wins](docs/compiled-schedule-blob.md#which-refusal-wins)). It is
  pinned by `tests/blob_refusal_precedence.rs` and two new tests in
  `src/tests/state_codec_tests.rs`, which put every pair of defects into each artefact and check
  that the earlier check wins, and by a test that the release-0.4.0 blob is still refused by its
  version when its length, trailer, profile or body is damaged.
- **Every format and schema revision is 1 until 1.0.0, and `development` carries version 0.5.0**
  (GitHub #518, issue #509; owner rulings of 2026-11-09). There is no backwards compatibility
  before 1.0.0: a format changes in place and its fixtures and goldens are re-blessed, with no
  importer, mapping table or dual-read path. Six numbers that had moved are reset to 1, with no
  layout change: the `OCEB` blob format (`oce_graph::blob::FORMAT_VERSION`, was 2), the state
  snapshot format and execution-state ABI revisions (were 2), the compatibility descriptor
  (`oce-compatibility:1`, was 2), and the diagnostics (`DIAGNOSTIC_SCHEMA_REVISION`, was 3),
  assertions and execution-profile contract revisions (were 2;
  `execution-profile-schema:1` in the descriptor). A wrong number is still refused with the same
  typed errors (`BlobError::UnsupportedFormat`, `EngineStateError::UnsupportedFormat`,
  `CompatibilityMismatch::DescriptorRevision`).
  **The workspace version moves to 0.5.0 in the same change, without a release or a tag.** With
  the numbers reset and the version still 0.4.0, every one of release 0.4.0's 46 blobs decoded and
  built an executable (same sections, class hashes and a valid CRC), and 33 of them presented the
  retired `cxf:fnv1a128:` export tag as a canonical content id. The engine version in the blob
  header and the descriptor's `oce-api-version` line now separate the layouts: a release-0.4.0
  blob is refused with `BlobError::EngineVersionMismatch` (`tests/blob_old_layout.rs`, one such
  blob kept unblessed as `fixtures/blob_legacy.oceb`). The rule that follows is in `AGENTS.md`:
  the first PR after a release tag that changes a format's bytes or meaning bumps the workspace
  version.
  A state snapshot header had no engine version at this change; the entry under Facade above
  (GitHub #540, issue #530) adds it. Releases 0.2.0 to 0.4.0 numbered their snapshots 2, so a
  populated snapshot from them is refused by number (`tests/state_old_layout.rs`,
  `fixtures/state_revision_two.hex`). The same snapshot with its numbers edited to 1, and the
  empty-model vector of the layout before those releases (`fixtures/state_legacy.hex`), which
  shares today's numbers, are refused for the version field they lack
  ([state continuation](docs/state-compatibility.md#wire-revision-and-integrity)).
  Re-blessed: the 46 blob goldens (format field, engine-version bytes and CRC only; 123,329 bytes
  in all, unchanged), `compatibility.txt` and `compatibility_export.txt`, the two pinned state
  vectors in `state_tests.rs` and `state_format_golden_tests.rs`, and `restamp.tsv`, which loses
  its retired-id column and becomes a plain pin of the 47 canonical content ids (all 47 unchanged).
  The catalog content id, every class signature hash, the run digests, the determinism state
  vectors' sizes and `tools/golden-gen/goldens` do not move. `DiagnosticStage::rank` and the
  three contract schema files change only their revision number; the product contract advances to
  revision 25.

- **One enumeration table in `oce-model`** (GitHub #488, issue #468). The enum facts that lived in
  hand-written `match` arms and member constants in `oce-model/src/types.rs` are now one static
  table, `oce_model::enumerations` (`no_std`, plain `&'static` data, linear lookups with no
  allocation and no hashing). `enum_class_id`, `enum_descriptor`, `enum_descriptor_by_path`,
  `enum_member_ordinal`, `g36_enum_class_id`, `g36_enum_literals`,
  `is_g36_integer_constant_package`, `g36_integer_constant` and `g36_integer_constant_in_package`
  read it, so the four G36 constant packages have a list a test can enumerate for the first time.
  No behaviour change and no golden moved: the descriptor keeps its frozen durable class paths
  (relative for CDL, fully qualified for G36), and `crates/oce-model/tests/enumeration_table.rs`
  checks every table-backed function against a verbatim copy of the old `match` arms on every class
  path and member, and on hostile spellings (wrong case, trailing dot, prefixes, the full
  `Buildings.Controls.OBC.` form, IRIs, empty), plus the table's invariants (unique ids, paths and
  last segments; one value per constant) and the G36 members against the vendored `.mo` sources in
  order. `tests/enumeration_corpus_references.rs` checks that every `CDL.Types` / `G36.Types` class,
  member and constant the CXF corpus names is a row or member, grounding to the row's value.
  Provenance: the 10 G36 enumerations are checked against the vendored `.mo` sources in order; the 4
  CDL enumerations are not vendored and are held to the registry's `EnumMembers` lists and the typed
  `SimpleController` / `ZeroTime` parsers; the 4 constant packages are not vendored either and carry
  exactly the members and values the resolver always used, pinned only by the differential test
  against the old `match` arms (the corpus test cannot catch a wrong value there).
  `DemandLimitLevels`' member order (`cooling0..3`, then `heating0..3`) is a choice, not an upstream
  order. A test pins that every row lies in `CDL.Types` or `ASHRAE.G36.Types`, the two packages
  whose durable class-path form is decided.

- **Blob format 2: the content-id field holds the canonical digest** (GitHub #474, issue #470).
  `oce_graph::blob::FORMAT_VERSION` is 2 and the decoder accepts only 2: the 32-byte header field
  now holds the SHA-256 digest of the canonical content id in output order, present for all 46
  fixtures including the 13 partial exports, and 32 zero bytes only for a hand-built model with an
  unnamed block. Any other failure to mint the id refuses `compile_blob` with `OcError::ContentId`,
  so a blob from CXF never ships without its id.
  `content_id_field` takes the digest bytes; `CompiledBlob::content_id` is `Option<ContentId>`.
  The decoder still carries the field verbatim, so `sha2` never enters the `no_std` core. The 46
  blob goldens are re-blessed and differ from format 1 in exactly three places: byte 4 (format
  version 1 → 2), bytes 20 to 51 (the content id) and the 4-byte CRC-32C trailer.
  `blob_frame_digests.tsv` and the firmware image's expected run digests are unchanged. Both blob
  docs say format 2; the design note's dump is regenerated from the golden with the real class
  hash, id and CRC.

- **The load gate refuses non-canonical array and matrix element names** (GitHub #474, issue
  #470). A bound parameter that spells an element of one of its class's array templates (`k_<i>`,
  `K_<row>_<col>`, `table_<row>_<col>`, `offset_<i>`) with a sign or a leading zero (`table_01_2`,
  `table_+1_2`, `offset_+1`, `k_01`) is refused with the new
  `DiagCode::NonCanonicalElementName` (`non-canonical-element-name`) before `make`. `TimeTable`
  parsed indices with `usize::parse`, so an aliased spelling silently replaced a cell and the later
  one in table order won; with one spelling per element, sorting parameters by name in the content
  id loses nothing. This is the spelling half of the facade gap GitHub #467 (names outside the
  declared extent), which stays open.

### CXF import and export

- **A parameter's `min`, `max`, `unit`, `quantity` and `displayUnit` are shape-checked on both
  readers** (GitHub #594, issue #559; owner rulings of 2026-11-11). Nothing read them on a
  parameter, so a malformed one loaded in the engine and imported through `oce-author` unjudged:
  `"S231:max": ["30", "…#double"]`, the list of a typed literal's values, among them. Their values
  are still not used for control. The engine (`oce-cxf`) now checks them where a parameter is read
  (a leaf's `hasParameter`, `hasConstant` and classified `hasInstance` parameter members, and every
  declaration of an active composite in the lowering pass), once per parameter node however often
  it is read. A bound is one number, typed literal or string, the last the expression form
  modelica-json writes for a bound over other parameters (`"yMin"`, `"100*1e-15"`); a term is a
  string, typed literal or IRI node, as on a connector; and no string among them (the attribute, a
  typed literal's `@value`, an IRI node's `@id`) may be empty or whitespace-only. A list, boolean
  or blank bound, a typed literal with a blank `@value`, and every malformed term are refused as
  `malformed-document`, subject the parameter; JSON `null` reads as absent, and a pruned or
  unreferenced parameter is not checked. A bound that is an object but no typed literal (`{}`,
  `{"@id": …}`, `{"@value": "30"}`, `{"@list": …}`), or a list holding one, gets no such
  diagnostic: it matches no shape the document reader knows, so the whole document fails to parse
  (`CxfError::Json`, no code or subject) wherever the parameter is, as it already did. The authoring
  importer (`oce_author::import_cxf`) checks the same shapes before it drops the attributes, in its
  own strict reading (a typed literal is exactly `@value` and `@type`, an IRI node exactly `@id`,
  and `null` is refused), with `wrong-shape` naming the key; the `DROPPED_ATTRIBUTES` exception of
  its closed-shapes sweep is gone, so a list at those positions is held like a list anywhere else.
  The importer note tabulates where the two readers differ. **Breaking, in place:** a document
  carrying such a shape loaded or imported before. No tracked document does: a probe over every
  tracked `.jsonld` (vendored translations and `CXF-Core.jsonld` included) and the current
  library's 142 CXF documents moved no outcome, none carries a blank value in these attributes on
  any node, and `fixture_corpus_import.txt` is byte-identical. A stricter rule that also refused a
  string bound would have added a refusal to three vendored translations that are already refused
  for other reasons (`Limits/Common`, `SetPoints/SupplyFan`, `Generic/TrimAndRespond`, whose bounds
  are written `"yMin"`, `"minSpe"` and `"100*1e-15"`), so the expression string is a legal shape.
- **A CDL extension block is refused at load with its own code, `extension-block-unsupported`,
  and the dead `missing-fmu-path` code is removed** (GitHub #582, issue #537). An extension
  block's behaviour is a Functional Mockup Unit (OBC §7.13 and §8.7), and FMI is out of scope
  before 1.0. Before composite lowering, the resolver now refuses every active node typed
  `S231:ExtensionBlock` (at any position of its `@type` list), carrying `S231:hasFmuPath` (whose
  domain is `ExtensionBlock`), or whose `@type` names such a node of the same document, wherever
  it sits: the root, a child at any depth, an extension block that lists children of its own, and
  a node no `containsBlock` edge reaches. The class is read the way a modeled key is (owner ruling
  on GitHub #582): its type must expand to `http://data.ashrae.org/S231#ExtensionBlock` (that IRI,
  `S231:ExtensionBlock` under the published binding or under none, another prefix bound to the
  namespace, or a `@context` term expanding to it); a class of that name in any other namespace,
  or a bare `ExtensionBlock` that no term expands, is an ordinary class, and the class in the
  `https:` spelling of either S231 namespace is refused as `vocabulary-namespace-mismatch`, as a
  modeled key there is. Each extension block gets one error under the new
  `DiagCode::ExtensionBlockUnsupported`, subject its `@id`, and the resolve ends there, so nothing
  lowering or a later stage would find is reported for the document; the declaration findings
  the specialize pass withheld still are. A node pruned by a false conditional guard is not
  refused; a class definition the document carries is, even when every instance of it is pruned.
  **Breaking, in place:**
  four shapes loaded before. An extension block that listed children, as the root or nested, was
  lowered as a composite and its children ran in place of its FMU; one that no `containsBlock`
  edge reached was ignored; `@type: [<registered class>, S231:ExtensionBlock]` ran the registered
  class; and `S231:hasFmuPath` on a registered class was never read. A leaf extension block was
  `class-not-found` and an extension-block root without children `composite/root-count`; the
  `https:` spellings of the class were read as an ordinary class, and are now refused. An
  instance of an extension-block class whose definition the document does not carry is still
  `class-not-found`. `DiagCode::MissingFmuPath` (`missing-fmu-path`), an advisory that nothing
  ever constructed, is removed (the workspace builds without the variant), the `ClassNotFound`
  rustdoc no longer describes an `ExtensionBlock` check that no code made, and `Node::fmu_path`
  gains its one reader. Two fixtures under `crates/oce-cxf/tests/fixtures/invalid/` and
  `tests/resolve_extension_blocks.rs` pin each position, and `oce-api`'s conformance harness drives
  both fixtures through `Engine::load_cxf`. Re-blessed: `fixture_corpus_import.txt` gains the two
  fixtures' lines and its pinned count goes from 223 to 225; none of its existing 223 lines moved.
  `tools/golden-gen/goldens`, the blob goldens and the content-id pin are unchanged.
- **CXF is read and written in one vocabulary namespace, `http://data.ashrae.org/S231#`, and
  every exported document names the software that wrote it** (GitHub #543, issue #527; owner
  rulings of 2026-11-09 and 2026-11-10). The evidence for the namespace is public: the
  OpenBuildingControl (OBC) specification lists it (OBC §11.3) and modelica-json's `CXF-Core.jsonld`
  binds it; this project has not read the CXF clause of ANSI/ASHRAE Standard 231-2026. The
  exporter (`oce-cxf`) and the authoring emitter (`oce-author`) bind `S231` to that IRI.
  **Breaking, in place:** the importer reads the vocabulary in no other namespace. `S231` bound to
  any other IRI (the `https:` spelling included), any prefix bound to
  `http://data.ashrae.org/S231P#` (the IRI the engine wrote through release 0.4.0), a modeled
  property keyed by its full IRI in that namespace or in the `https:` spelling of either, and a
  `@type` or `isOfDataType` value in that namespace are each refused with the new diagnostic code
  `vocabulary-namespace-mismatch` (`DiagCode::VocabularyNamespaceMismatch`), whose message names
  the IRI found and the IRI expected. A foreign-namespace key was `non-subset-construct` before;
  a document that bound `S231` anywhere else used to load. The authoring importer refuses the same
  bindings with the new kind `ImportErrorKind::NamespaceMismatch { term, found, expected }`
  (`namespace-mismatch`, the thirtieth code). A document with no `S231` binding still loads, its
  compact keys read in the published namespace; another prefix bound to the published IRI is still
  accepted by the engine importer; in a list of context maps only the binding in force is judged.
  There is no reader, alias or importer for the earlier spelling: two documents exactly as
  release 0.4.0 carried them are kept unblessed to prove the refusal
  (`crates/oce-cxf/tests/fixtures/previous_layout/`,
  `crates/leverage/oce-author/tests/fixtures/previous_layout/`). The workspace was already at
  0.5.0, so no version bump accompanies the change.
  **The label.** Both writers put `S231:translationSoftware` (`open-control-engine`) and
  `S231:translationSoftwareVersion` (the workspace version) on the root node, as two JSON
  strings; the spelling is `CXF-Core.jsonld`'s, the properties are OBC §8.2 Table 8.2, and OBC
  §8.4 makes them optional. No importer reads them back, in any spelling, on any node, with any
  value, so they are in no content id, blob or snapshot; the authoring importer admits the two
  keys on the root and on a block node and treats them as unknown keys elsewhere. Exported and
  emitted bytes therefore change with the workspace version (the pins are listed in
  `.agents/project-facts.md`).
  **What moved.** No canonical content id, no blob golden, no frame digest and nothing under
  `tools/golden-gen/goldens`. The `S231` context IRI was rebound mechanically in 175 input
  fixtures and in five copies of documents from other repositories, whose provenance records now
  give the source digest beside the new one (`tests/fixtures/v0/PROVENANCE.md`,
  `tests/fixtures/library_rules/README.md`). Regenerated by their own bless switches: the nine
  export goldens, the allowed-delta and export-content expectation files (47 records each), two
  of the 223 lines of `fixture_corpus_import.txt` (both rejected fixtures whose message quotes the
  IRI; every vendored translation's verdict is unchanged), the six emit goldens and two CLI
  transcripts; the three inline export digests in `tests/export_cxf.rs` were copied from the
  failing assertions and the first cross-checked against the regenerated golden. Each
  regenerated document differs from its predecessor in the IRI and the label and nothing else.
  **Strict-bit inventory.** The conformance harness builds each case's CXF with the binding in
  its `@context`, and the corpus pins each document's SHA-256, so the 21 `cxf_sha256` values of
  `corpus.json` were rewritten from a fresh capture; no other field changed. The retained native
  archives are byte-identical and their `cxf_sha256` values are no longer compared with the
  corpus (as GitHub #392 did for their source maps), so both recorded matrix digests still
  reconstruct; a fresh collection and the per-PR captures keep the comparison.
  Rustdoc, `AGENTS.md` and the `Cargo.toml` header now name ANSI/ASHRAE Standard 231-2026 and
  cite the OBC specification's sections as `OBC §N`.

- **Composite lowering costs time and memory linear in the document, not fan-out × IRI length**
  (GitHub #490, issue #480). The lowering pass classified both ends of every authored
  `isConnectedTo` edge by string content and keyed its duplicate counts, crossed drivers and
  adjacency on IRI strings, so one long authored source shared by a whole fan-out cost fan-out ×
  IRI length: a leaf port with a 1 MiB `@id` wired 65,535 times to a top boundary port (a 10 MB
  document) took more than three minutes of release CPU before any diagnostic, and the swapped
  variant (a leaf input naming a top input) copied the IRI into the lowered list once per edge,
  about 64 GiB. Lowering now interns each distinct endpoint once
  (`resolve/composite_orientation/endpoints.rs`) and runs orientation, duplicate suppression, the
  boundary walk and the erased-relation check on dense ids, with expansion checks memoized per
  start; owners, composites and parents are `@graph` positions rather than copied IRIs; and the
  lowered edge list lives beside the lowered document with one shared allocation per distinct
  IRI. The passes after it keep the same cost: boundary-type derivation classifies a source once
  per node and skips a repeated shared target, Step 9 memoizes a run of one shared target as it
  already did a source, and the diagnostic sort keys each subject allocation once instead of
  hashing a long subject on every comparison. That document now imports to its ordinary
  diagnostics in under 0.1 s in release (about 1 s in debug). Every diagnostic, its order and its
  text are unchanged, and no bound changed; no golden moved. Regression tests pin the bound (1 MiB
  × 65,535 for a kept, a contradictory and a swapped fan-out, with a short-source control) and
  determinism (a repeated import, lowering one document twice, and shared versus separate
  allocations for lowered targets, boundary-type derivation and the diagnostic sort).
  `SECURITY.md` no longer lists the path as not yet bounded.

- **Resolve Step 9 costs time linear in the edge list, not fan-out × source-IRI length**
  (GitHub #481, issue #479). Connection collection classified both ends of every
  `isConnectedTo` edge by string content, hashing the source IRI about ten times per edge, so one
  long source shared by a whole fan-out cost fan-out × IRI length: a 10 MB document with a 1 MiB
  derivation-skipped re-anchored source and 65,535 targets took about 71 s of release CPU before it
  was refused, breaking the bound-CPU-at-load rule. Step 9 now interns each distinct endpoint once
  (`resolve/connection_endpoints.rs`, the loop moved to `resolve/connections.rs`), memoizes an edge
  group's shared source by slice identity, keys every report-once set, the pass-through pair set
  and the boundary-input assignment tracker on endpoint ids, and mints one shared subject
  allocation per endpoint, so a per-edge diagnostic no longer copies a long IRI. The unused-input
  warning reads the boundary inputs Step 9 interned instead of walking the edges again. That
  document is now refused in about 75 ms in release (0.9 s in debug), and the three
  `skipped_synthesized_*` tests drop from 55–86 s to about 1 s in debug, which closes #469 with
  no test-side change. Every diagnostic, its order and its text are unchanged; no golden moved.
  Regression tests pin the bound (1 MiB × 65,535 at and one past the target-examination limit for
  all three target roles, a short-source control) and determinism (a repeated import, and shared
  versus separate source allocations collecting identically). A long *authored* source fanned out
  in the lowering pass remains bilinear — more than 180 s of release CPU at 1 MiB × 65,535 — and
  is tracked in #480; `SECURITY.md` lists that path as not yet bounded.

- **A block input driven by both a block output and a root boundary input is refused**
  (GitHub #477, issue #472). CDL §7.10 gives every input exactly one driver, and the resolver
  refused two of the three double-driver shapes (two block outputs; two boundary inputs) but
  loaded the mixed one warning-free: boundary-driven assignments were counted by the
  boundary-input elision pass and wire in-degree by the later single-assignment check, and that
  check deliberately accepted an `external_inputs` connector with one wire. At run time the wire
  won (`build_driver_of` points the input at the driving output's slot) and the frame value the
  host staged for the boundary input was discarded without a trace. The boundary-input pass now
  counts both kinds of driver and refuses the mixed shape with `single-assignment` at the input's
  authored IRI, message `input is driven by a block output and a boundary input`; the wire-only
  check leaves every frame-driven input to it. A wire from an input connector is not counted (it
  is the existing direction refusal). The exporter refuses the same shape on a hand-built graph
  (`ExportUnsupported`, owning block named) instead of emitting bytes that no longer re-import.
  New rejected-corpus fixtures pin all three shapes and the authoring note's `econ_enable`
  example with `TOut -> hys.u` added, through `import_cxf`, `Engine::load_cxf` and
  `load_cxf_with_receipt`, with hostile spellings (reversed edges, a repeated boundary target,
  a nested pass-through composite on either driver, a parameter wired to the input) and a
  sweep proving every one of the 47 G36 fixture documents gives each input exactly one driver.
  No golden moved: the 46 fixture goldens, the 422 provenance goldens, the blob goldens and
  `class_signatures.tsv` are byte-identical. The vendored-corpus characterization moved
  deliberately: `single-assignment` 57 → 68, eleven mixed inputs in three documents
  (`Limits/SeparateWithDP`, `SetPoints/FreezeProtection`, `Generic/AirEconomizerHighLimits`)
  that were already refused for unresolvable conditional guards, so no document changed outcome
  (`fixture_corpus_import.txt` re-hashes those three error lines, and gains four new lines for
  the new fixtures, corpus 214 → 218). Three existing tests that
  had carried an unintended mixed driver were corrected, and the export-side test that pinned
  the mixed shape as round-trippable now pins its refusal.

### Leverage

- **Time unit suffixes scale from the vocabulary, and a suffix must fit its parameter**
  (GitHub #586, issue #501; vocabulary v1, part 3 of 3; [vocabulary note](docs/design/vocabulary.md),
  "Units", decision 14 and rulings 5 and 6; authoring decision 13 amended).
  - **Scaled suffixes.** A suffix that is exactly `min`, `h` or `d` (`oce_vocab::SCALE_FACTORS`)
    scales the number to seconds with one rounding: an Integer by its exact product, a Real from
    the binary64 it parses to, through its shortest round-trip decimal (the spelling `fmt`
    prints), not from the digits as typed. So `1.1 h` is `3960.0` (multiplying the parsed `f64`
    gives `3960.0000000000005`), and `fmt` never moves a value. Two consequences: a Real typed
    with more digits than binary64 holds can scale to about an ulp (more for a subnormal) from the
    exact product of its typed digits (`1.10000000000000009 h` is `3960.0`), and an Integer and a Real spelling past
    2^53 scale differently (`9007199254740993 min` is `540431955284459584.0` s,
    `9007199254740993.0 min` is `540431955284459520.0` s). A scaled Integer past 2^53 is
    accepted, though the same Integer unscaled on a Real parameter is `parameter-out-of-range`. The
    model's value is the base-unit Real, which is all the wire form, the emitter and the engine
    see; the new text-only `Assignment::scaled_from` and `SequenceParameter::scaled_from` keep
    the written number beside the suffix, so `fmt` prints `1 h` again and never moves a value. A
    scaled value past binary64 is `non-finite-number`.
  - **Still refused** (`unit-not-base`): `degC`/`degF`, `%`, prefixed SI units, any other symbol,
    and a scaled symbol inside a compound (`m3/h`, `1/h`, `h2`). `1h` stays `malformed-number`.
  - **`unit-suffix-mismatch`**, a new author-only stage-1 code (`STAGE_ONE_CODES` is
    twenty-six): a suffix that does not reduce to the block parameter's declared unit from
    `oce-vocab`, spelled alike (`delayTime = 5 K`, `delayTime = 1 Hz`), and a scaled suffix on a
    parameter that declares no unit or on a sequence parameter. A base-unit suffix where no unit
    is declared stays an annotation. The parity test pins it as silent in the engine.
  - **A disagreeing annotation is caught.** A model built in code whose suffix, written number
    and value disagree (a tool that set `delayTime` to `1.0` and left `1 h` in place), or whose
    suffix is longer than the 256-byte token bound, is `unit-suffix-mismatch` in stage 1, so
    `emit` refuses it, and the printer refuses it too (`PrintError::Unspellable`) through the same
    shared check: no suffix stops a model that validates clean from printing, and one that prints
    parses back equal.
  - **Dependencies:** `oce-author` now depends on the leverage crate `oce-vocab` (the table and
    the declared units), which moves `Cargo.lock`.
  - **Cost.** The declared-unit lookup is an index over the embedded vocabulary, bounded by the
    catalog, not the document; `src/counted.rs` lists it, with the scale-factor table and the
    agreement check, among stage 1's uncounted bounded lookups. The counted tests of GitHub #584
    gain three shapes, a chain of `n` blocks whose suffixes fit, do not fit, or disagree with
    their values, held to the same `24 * n * bits(n)`; a suffix adds no counted comparison (the
    fitting chain costs exactly what the plain chain does).
  - **`oce vocab`** says how the text form reads the table, in its heading ("it scales the number,
    rounding once") and in the meanings of `unit-not-base` and the new `unit-suffix-mismatch`.
    Re-blessed: `vocab_text` and `vocab_json` (those three lines and the new code's row). New golden: `tests/fixtures/text/fmt/scaled_suffixes.out.oce`
    beside its hand-written input. No emit golden, schema, wire fixture or other transcript moves.
  - **Docs.** The authoring note (decision 13 amended, the unit-literal paragraph and suffix
    production, the stage-1 list, the finding-code table, out of scope, a ruling), the vocabulary
    note (four implementation readings; step 4 landed), the `oce-author`, `oce-cli` and
    `oce-vocab` READMEs and the root README's status.

- **`oce-author`'s validation cost is pinned in key comparisons, not in elapsed time** (GitHub
  #584, issue #478).
  - **What was wrong.** `validate_cost`'s ratio test failed on `development` because it timed two
    document sizes and compared them. Stage 1 was never superlinear: run alone, 1,875 to 30,000
    blocks (sixteen times) costs 16.6 times at the commit the issue names (debug codegen) and, on
    `development` before this change, 16.7 times in debug and 17.9 times in release. Under sixteen
    concurrent copies of the issue commit's test binary the smallest run still took 8.7 ms and the
    largest 310 to 473 ms instead of 145 ms: the
    best of several short runs escapes a busy machine and a long run does not, so the ratio
    measured the host.
  - **What replaces it.** Every index stage 1 and the text placement (`SourceMap::apply`) build
    over the document is keyed by a crate-private `Counted` wrapper, which orders as the value
    it wraps and, in the library's own test build only, counts each comparison on the comparing
    thread; the sort of the findings counts through the same counter. The unit tests of
    `src/counted.rs` hold stage 1 (a clean chain, a chain whose every block is a finding, one
    block of `n` ports resolved, misnamed and with `n` assignments, and `n` referenced sequence
    parameters, at 1,875 and 30,000) and the text placement (a
    finding per block and one name repeated by every block, at 4,096 and 65,536) to
    `24 * n * bits(n)` comparisons [measured: at most 19.9], and a control counts the scan per
    lookup the indexes replaced: `n * (n + 1)`, seven times the bound at 1,875 blocks and eighty
    times at 30,000.
  - **Behaviour.** Unchanged: every finding, its order, its subject and its location. The port
    sets and the text subject index now borrow their names where they copied them. The two
    wall-clock ratio tests are removed from `tests/validate_cost.rs` and `tests/text_cost.rs`;
    the tests there that validate a document at the wire bound and at the statement cap stay.
  - **Not covered.** A lookup that compares names without `Counted` is invisible to the counter;
    the ones stage 1 has today (the class index, a class's parameters, the named types,
    `STAGE_ONE_CODES`, `class_ports`) are bounded by the catalog, not the document. The cost of
    parsing text is no longer pinned by any test: the removed ratio test timed it with the rest,
    and the parser compares no counted key (#589). Two more follow-ups are filed: stage 2 maps each engine diagnostic by scanning the model, a cost
    of diagnostics times blocks inside the 8 MiB emission bound (#577), and
    `import_hostile.rs` still asserts a 60 s ceiling on elapsed time (#578).

- **The authoring provenance statement is `derived_from`, no longer `extends`** (GitHub #580,
  issue #529; [authoring note](docs/design/authoring-layer.md), decision 5 as amended; owner
  ruling of 2026-11-09). CDL uses `extends` for inheritance of a composite block with
  modifications (OBC specification §7.15), which M7's composite instantiation needs, so the word
  is kept for that meaning.
  - **Renamed in place.** `Sequence::derived_from`, `SequenceBuilder::derived_from`,
    `ImportOptions::derived_from` and `ImportOptions::with_derived_from`; the text statement
    `derived_from none` or `derived_from <provenance id>`; the wire key `"derived_from"`; the
    subject `derived_from` of `invalid-provenance-id`; the parse code `missing-derived-from`
    (`MISSING_DERIVED_FROM`). The format stays at revision 1, with no reader, alias or hint for the
    earlier word.
  - **The earlier spelling is refused.** A text file that says `extends` is `missing-derived-from`
    in the header and `unknown-keyword` after it; a wire document with an `extends` key is
    `WireError::UnknownKey` (`unknown-key` from `oce`), in place of the new key, beside it or after
    every other key. Every text fixture rewritten to the earlier spelling is refused at that
    statement, and `oce validate`, `emit` and `fmt` refuse both forms.
  - **Names.** `derived_from` joins `TEXT_FORM_KEYWORDS`, so it is no longer a declared name
    (`invalid-name`). `extends` stays refused as a Modelica reserved word; it is no longer a
    statement word, so the printer now spells a connection whose source is named `extends` (a
    model only the wire form or the builder can hold, which reads back equal).
  - **No emitted byte moves.** The provenance id is never emitted: the six emit goldens, every
    content id and every blob golden are unchanged.
  - **Re-blessed:** the wire schema (`schema/sequence.schema.json`, the property and its
    `required` entry), the canonical text goldens (`tests/fixtures/text/*.oce` and the three `fmt`
    outputs), the three v0 `*.import.txt` goldens and `findings/defects.tsv`, and the `oce`
    transcripts `fmt_text`, `fmt_wire_to_text`, `validate_wire_findings`, `vocab_text` and
    `vocab_json`. Edited by hand: the four wire bundles, the three `fmt` inputs, the note's two
    copied blocks and the four `oce-cli` inputs.
  - **Docs.** The authoring note (decision 5's amendment, the grammar, the worked example, the wire
    block and a ruling), the diff note (`derived-from-changed`, `SequenceRef`), the importer and
    content-id notes, and the `oce-author` README.

- **Vectors, part 4 of 4: the `oce verify` verb and the in-repo replay gate** (GitHub #573, issue
  #522; [vectors note](docs/design/vectors.md), "CLI", decisions 19, 26, 30, 31, 34 and 36, the
  owner's rulings of 2026-11-11 in "Part 4's readings").
  - **The verb.**
    `oce verify <VECTORS> --subject <FILE> [--subject-form cxf|text|wire] [--out <DIR>]
    [--require-mutants-caught] [--deny-warnings] [--timings]` takes one vector set and one subject
    per run over `oce-verify`, which `oce-cli` now depends on.
  - **Forms.** A subject named `.jsonld` or `.cxf.json` is CXF, tested before `.json` is wire; `.oce`
    is text. A text or wire subject is validated and emitted with the default options first.
    Anything else is the new code `unknown-subject-form`.
  - **Reads.** Every size is checked from metadata before a byte is read: the vector file at 4 MiB,
    a CXF subject and each mutant at 8 MiB, a text or wire subject at its form's 4 MiB (owner
    ruling), and the mutants together at 64 MiB.
  - **Mutant paths, per the owner's ruling.** Each path is resolved against the vector file's
    directory, following links. A path that leaves it is the new `mutant-outside-directory`, and
    two listings that are one file are `duplicate-mutant`. One file is one (device, inode) on Unix
    and one canonical path on Windows. Each mutant must be a regular file by its metadata before
    it is opened, so a named pipe or a socket is `not-a-file` and never blocks the run (review of
    GitHub #573). It is then opened once, the handle must be the file checked, and its size and
    bytes come from that handle.
  - **Output.** The receipt is stdout for a verdict, exit 0 or 1: the CLI's one exception to "exit
    1 prints the report". Exit 2 prints the `oce-report/1` report. `--out DIR` writes
    `DIR/receipt.json` atomically and prints the report with `passed`, `status`, `receipt_id` and
    `output`; on exit 2 nothing is written.
  - **Report errors** gain the optional keys `at` (the place), `path`, `candidates`, `cause` and
    `diagnostic_codes`, written only with a value. The schema is changed in place and stays
    `oce-report/1`. The place was first built as `subject`; the owner ruled `at`, because a
    `verify` report's top-level `subject` is the subject file.
  - **Exit-only flags.** `--require-mutants-caught` and `--deny-warnings` flip the exit status over
    the same receipt bytes. By owner ruling, `--require-mutants-caught` also fails a set that
    lists no mutant (no `mutants` key, or an empty list).
  - **`--timings`** fills `load_ms` (reading and emitting) and `run_ms` (the whole `run_vectors`
    call). `compare_ms` stays 0, and `receipt_id` never moves.
  - **Vocabulary.** `oce vocab` lists `oce-verify`'s 35 load codes and 30 bind and run codes, pinned
    to its lists, and the two new CLI codes.
  - **The in-repo replay gate.** `crates/leverage/oce-cli/tests/replay_gate.rs` runs the built
    binary over both in-repo library rules in place and requires exit 0 and stdout byte-equal to
    `oce-verify`'s golden receipts. It runs in the leverage lane, and an edited expectation fails
    it.
  - **Transcripts.** New: 23 `verify_*` transcripts and `help_verify`. Re-blessed: `help_oce` and
    `usage_no_arguments` (the new subcommand line), `vocab_text` and `vocab_json` (the new tables,
    and the widened exit and `input-too-large` meanings). No `oce-verify` golden moves.
  - **Docs.**
    - `docs/design/vectors.md`: part 4's rulings and readings, decision 19's gate, the CLI codes.
    - `docs/design/authoring-layer.md`: `oce check` stays deferred, cut as #574.
    - `docs/ci-and-the-gate.md`: the gate as the M3 "In-repo rules replay" evidence.
    - The crate README, `docs/architecture.md`, `crates/leverage/README.md` and
      `.agents/project-facts.md`.
    - The root README's M3 status.
    - Tracking #456 item 7 now says `oce verify --commission`.
- **Binding resolves names through an index and holds set-level edits once** (GitHub #572, issue
  #571; [work-budget note](docs/design/vector-work-budget.md), "What the budgets leave bounded
  elsewhere"). The review of GitHub #570 found two costs at bind that come before the work budgets.
  Both are removed, and no resolved name, refusal or receipt byte changes.
  - **`oce_replay::PathIndex`** (new public type) answers the one name rule, `resolve_path`, for
    many names at once. It sorts a path list once by its bytes read backwards, so the paths ending
    with a name at a `.` boundary are found by one binary search; a shared last segment costs
    nothing extra. `resolve_path` stays as it was. `InputMap::resolve` resolves its column
    targets and policy overrides through the index, and finds columns by name in an ordered map;
    every `MapError` is unchanged. A generated corpus pins every answer against `resolve_path`,
    including exact, suffix, ambiguous (candidates in list order) and absent names, dots,
    brackets, `#`, multi-byte characters and an out-of-order list. A comparison count pins the
    cost: about 30 comparisons a key for 51,200 suffix keys against 8,000 paths, where the scan
    makes 409,600,000.
  - **`oce-verify` binds through it**: each of the input, output and parameter lists is indexed
    once. 204,800 suffix keys against 8,000 parameters bound in 15.7 s in `dev` before and 1.4 s
    now, measured once on the shape `tests/name_resolution.rs` binds; that test asserts that every
    key resolves, and no time.
  - **Set-level edits are held once.** An edit is a parameter-table row and its value. Each
    scenario keeps its own map, and the effective list is built for the trial, run or receipt
    that uses it. Distinct lists are found by sorting the scenarios by their merged lists rather
    than by copying a key for each. For 4,096 scenarios under 256 set-level edits, the most held
    before the budget step fell from 72,811,014 bytes in 1,221,928 allocations to 3,177,307 bytes
    in 67,105, the subject's load (`tests/bind_cost.rs`).
  - **Goldens.** The five golden receipts, the schema, the canonical goldens and `oce-replay`'s
    trace goldens are unchanged. The digest `tests/parameter_trials.rs` pinned before #570's
    deduplication still matches.
- **Vectors: four work budgets bound one run, checked at bind** (GitHub #570, issue #561;
  [work-budget note](docs/design/vector-work-budget.md), the owner's rulings of 2026-11-11).
  - **The budgets.** `VerifyLimits` gains three fields beside `max_total_frames` (2^24, kept):
    - `max_total_instants`, 2^28: `(1 + mutants)` times every window's ticks and every
      diagnostic;
    - `max_total_load_bytes`, 2^30: the subject's and every supplied mutant's bytes times
      `1 + e + s` (`e` distinct edit lists, `s` scenarios);
    - `max_total_frame_bytes`, 2^40: `(N + 1) * s` times the same bytes.

    Each refuses with its own bind code, `too-many-instants`, `too-many-load-bytes` and
    `too-many-frame-bytes`, placed after `too-many-frames`; the bind codes go from 27 to 30. Each
    is lower-only, like every limit (decision 35).
  - **One step, before every other load.** The four are checked in that order after the subject's
    first load and its names and values, before the parameter trials and every mutant check. So
    `too-many-frames` now wins over `param-refused`. Each product is an upper bound taken in `u128`
    with checked multiplication, so a hostile count refuses and never wraps or panics.
  - **The subject's parameter trials are deduplicated.** Each distinct effective edit list is
    tried once, in the order of the first scenario that has it, as the mutants' already were. A
    refusal still names the first scenario holding the refused list. A receipt pinned before the
    change is byte-identical after it (`tests/parameter_trials.rs`). The distinct lists are now
    found through an ordered set rather than by comparing every pair.
  - **Tests.** `tests/work_budgets.rs` pins each budget at its bound and one past it, on AHU-0016's
    set with and without its mutants (the receipt at the bound is byte-identical to the default's).
    It also pins the order of the budgets, their precedence over `param-refused` and the mutant
    codes, what the load bytes count, and that a host asking for `u64::MAX` gets the defaults.
    `src/budget_tests.rs` pins the products at the `u64` and `u128` extremes.
    `tests/parameter_trials.rs` pins the catalog's longest cross-parameter rule chain at two (14
    classes), the bound on the passes an edit list takes.
  - **Goldens.** The five golden receipts, the schema and the canonical goldens are unchanged; no
    receipt byte moves. Formats stay at revision 1.
  - **Docs.** `docs/design/vectors.md` is amended as the work-budget note lists: binding steps 4
    to 6, the error-code table, decision 35, the API sketch, part 2's readings 8 and 9, and part
    3's readings 1 and 6. The work-budget note records the build and two corrections: there are
    four cross-parameter rules, not three, and each finds its partner by a scan that is linear in
    the document. The crate README's bounds table, bind order, codes and costs are updated.
- **Vectors, part 3 of 4: mutant mode, the catch score and the in-repo AHU-0016 mutants**
  (GitHub #565, issue #521; [vectors note](docs/design/vectors.md) decisions 27 to 30, the
  owner's ruling of 2026-11-11 and "Part 3's readings").
  - **The format, in place.** `mutants` is a list of `{ "path", "description"? }`, at most 256
    (`too-many-mutants`). A path is `/`-separated with no empty, `.` or `..` segment and no
    backslash (`mutant-path-invalid`), and is unique (`duplicate-mutant`). The canonical writer and
    `spec_id` cover it, and the pinned schema gains the `Mutant` object (re-blessed). The load
    codes go from 32 to 35.
  - **The mutants' bytes.** `VectorInputs::with_mutants` hands them in under their listed paths;
    `oce-verify` still opens no file. `VerifyLimits` gains `max_mutants` (256) and
    `max_mutant_bytes` (64 MiB, all mutants together). By owner ruling, part 4's CLI resolves each
    path, keeps it inside the vector file's directory and refuses two listings that resolve to one
    file; the format's lexical rule stays as it is.
  - **Validation at bind**, before any frame runs and after the frame budget (which counts
    `(N + 1) * scenarios * (1 + mutants)` from the listing, so a set over it loads no mutant;
    owner ruling), the first refusal winning:
    - the bytes given are listed, and given once (`mutant-unlisted`, `duplicate-mutant`);
    - each listed mutant is given (`mutant-missing`);
    - the bytes fit their bound (`too-many-mutant-bytes`);
    - each mutant in file order loads (`mutant-load-failed`, with the engine's codes), has the
      subject's input and output paths and types, input bounds not compared
      (`mutant-boundary-differs`, naming the first port that differs), and has another content id
      than the subject (`mutant-identical`);
    - each mutant admits every distinct effective edit list, each tried once on a fresh engine of
      it in the order of the first scenario that has it (`mutant-param-refused` with the cause
      `param-range`, `param-type`, `param-structural` or `param-path-unknown`, owner ruling).
    - An invalid mutant is an error with no receipt.
  - **Scoring**, only when the subject passes. Each mutant runs the scenarios in file order, as
    the subject did, and stops at the first that fails on it. A frame-class refusal fails it, so a
    mutant that narrows an input bound is caught, not an error.
  - **The receipt** gains the body's `mutants` section, written when the set lists a mutant:
    - `status` (`scored` or `not_run`), `total`, `caught` and `survived`;
    - one entry per mutant with its `path`, its `digest` (`fnv1a128:` of the bytes given), its
      `content_id` and its `status`, plus, when caught, the `scenario` and its first `failure`;
    - the score never changes `passed`. The bind codes go from 19 to 27.
  - **Fixtures** under `crates/leverage/oce-verify/tests/fixtures/AHU-0016/`: AHU-0016's set plus
    the `mutants` list (its clock and scenarios held equal to the facade's copy, which lists none)
    and three hand edits of the rule, named `.cxf.json`. The run agrees with the note's table:
    - `threshold_inclusive` is caught by `threshold_edge`, `value-mismatch` at 900 s;
    - `no_persistence` is caught by `both_open` at 0 s;
    - `late_alarm` survives, as designed.
  - **Goldens:** the fifth golden receipt, `receipts/AHU-0016.mutants.receipt.json`, compared byte
    for byte. The four earlier receipts are byte-identical. `sets/every_field.json` gains a
    `mutants` list, so its canonical golden and `spec_id` are re-derived by the independent Python
    script.
  - **Docs:** the note records the owner's ruling, part 3's readings (for confirmation) and the
    mutant table confirmed by the run. Mutant mode multiplies the loads and frames #561 describes;
    the issue now counts them. The 46 fixture goldens and 422 provenance goldens are
    byte-identical.
- **Vectors, part 2 of 4: bind, replay and compare a vector set, and issue the receipt**
  (GitHub #558, issue #520; [vectors note](docs/design/vectors.md) decisions 3, 6 to 8, 11,
  12, 14, 15, 18 to 26, 31 to 33 and 37). `oce-verify` now depends on `oce-api` and `oce-replay`
  (the leverage rule holds as written) and runs a set: `run_vectors(&set,
  &VectorInputs::new(&cxf), &VerifyOptions)` returns a `VectorRun` with its `Receipt`, or a
  `VerifyError` and no receipt.
  - **Binding**, once per set against a throwaway engine, before any scenario runs: the subject
    loaded (`subject-load-failed` carries the engine's `DiagCode` spellings; load warnings go to
    the receipt, at most 64 with their total), an Enum boundary port refused
    (`enum-port-unsupported`, until #465) and a String one (`unsupported-port-type`), every input,
    output and parameter name resolved by one rule, every value typed by the declared type of what
    it lands on (`InputDefinition::value_type`, `FrameSlots::output_types()`, the parameter's
    declared type): a Real takes an integer token only when a binary64 holds it exactly (`-0` is
    `+0.0`; `9007199254740993` is `value-not-representable`), an Integer refuses `1.0`, a Boolean
    refuses `0` and `1`. A scenario's `params` override the set's by resolved path whatever either
    spelling, two keys of one map on one parameter are `param-path-duplicate`, and the frame
    budget, `(N + 1) * scenarios`, is held to `VerifyLimits::max_total_frames`. Every scenario's
    effective edits are also tried at bind on a freshly loaded engine, so an edit no pass admits is
    `param-refused` before any frame of any scenario runs (owner ruling).
  - **The run**: each scenario on a freshly loaded engine, its parameter edits applied inside one
    halt in passes in ascending path order (lowering both `Reals.Hysteresis` thresholds takes two
    passes; a pass that applies nothing is `param-refused` with the class `param-range`,
    `param-type` or `param-structural`), then one replay on `TimeBase::Grid` with the horizon added
    as an instant and replay's defaults, in chunks of `VerifyOptions::chunk_frames` frames. After
    it, in order: a run-class engine refusal is `run-refused`; any other refused frame fails the
    scenario with one `frame-refused` record (later ticks uncounted, coverage not checked);
    otherwise coverage, then the exact comparison: a Real by its bits, a diagnostic by its class
    path and optional message at its tick. Diagnostic expectations match warnings one to one
    (owner ruling): within a tick, the expectations that name a message pair first, then the
    block-only ones, each taking the earliest free warning it matches, so a second instance's
    equal warning stays unmatched. Unmatched warnings are listed (16, with the total) and never
    fail a scenario. A replay error is `replay-failed` with a cause from a closed table
    (`trace-too-large`, `output-set-changed`, `grid-not-increasing`, `engine-setup`, `source`).
  - **The receipt**: `Receipt { header, body: ReceiptBody }`, `ReceiptBody` closed and
    `#[non_exhaustive]` with its one `Vectors` variant; every field of the note's header, body,
    scenario and failure-record tables, written canonically by `Receipt::to_canonical_json`. A
    `frame-refused` record always keeps the list's last slot, so earlier mismatches cannot push it
    out (owner ruling).
    Values are exact (`-0.0`, a NaN and the infinities as `"0x"` and their 16 hex digits),
    `engine_version` is `CompatibilityDescriptor::oce_api_version`, `timings` are zeros unless the
    caller passes them, and `receipt_id` is `receipt:fnv1a128:` over the canonical receipt with
    `timings` and `receipt_id` removed (`Receipt::computed_receipt_id` recomputes it). A frame
    refusal's class is a kebab-case table held complete by an exhaustive match
    (`ENGINE_REFUSAL_CLASSES`, `engine_refusal_class`).
  - **Errors**: `VerifyError` with a closed `VerifyErrorKind`, a `code()` per kind and
    `VERIFY_ERROR_CODES` (19 codes: the note's bind and run rows without the six mutant rows, which
    arrive with mutant mode, plus `window-uncovered` for a window a run left uncovered), with the
    place, the canonical path, an ambiguous name's candidates and a cause where a kind has one.
    `VerifyLimits` gains `max_recorded_failures` and `max_recorded_warnings` (16 each),
    `max_total_frames` (16,777,216) and `replay` (`ReplayLimits`), lowered and never raised.
  - **`oce-replay`**: the private name rule is the public `resolve_path` over a sorted path list
    (exact path, else the one `.`-boundary suffix, the candidates on ambiguity), and
    `InputMap::resolve` calls it. No behaviour changes; the four canonical trace goldens are
    byte-identical.
  - **Goldens**: the pass receipts of both in-repo library rules (AHU-0016: 217 frames and 216
    instants; AHU-0004: 441 frames), read in place from `crates/oce-api/tests/fixtures/`, a fail
    receipt (`both_open` with its first window ending at 900 s, where the delay fires) and a
    `no_data` receipt, under `crates/leverage/oce-verify/tests/fixtures/receipts/`, compared byte
    for byte and blessed only by `OCE_BLESS_VERIFY`. Each `receipt_id` is checked against a second
    FNV-1a-128 over the golden's text, and is one across two runs, chunk sizes 1, 7 and the whole
    run, and with caller timings. They embed the engine version, so a workspace version bump
    re-blesses them (`.agents/project-facts.md`).
  - The commissioning note's API sketch is amended: `Receipt.body` is `ReceiptBody`. The vectors
    note records part 2's ten readings as ruled ("Part 2's readings"). Follow-ups: #561 (bound a
    run's total work, before part 4) and #563 (record the caps a receipt was written under). The
    46 fixture goldens and 422 provenance goldens are byte-identical.
- **The authoring wire reader refuses a list, or any other type, where an object belongs**
  (GitHub #544). `oce-author`'s `from_wire_str` read a JSON list where the wire form defines an
  object: `serde_json` reads a `serde`-derived struct from a list as well as from an object, taking
  the list positionally as the struct's fields, and the strict scan in front of `serde` checks
  syntax, depth, repeated keys and sizes, not shapes. So `"semantic": ["", "", "K", "", "", "", [],
  false]` loaded as the slot it abbreviates and `oce validate` answered `"ok": true`; on
  `development`, 1,765 of the 1,948 objects the new sweep replaces loaded that way (every root,
  sequence parameter, boundary, semantic slot, block and assignment it reaches; the 183
  `{"ref": …}` and `{"member": …}` values were already refused), and a boundary's `type` read
  `{"Real": null}` as `"Real"`. The typed parse now runs through a closed-shape adapter that wraps
  `serde_json`'s deserializer at every depth (`src/wire/closed.rs`): a struct is read from an
  object only, an enum from a string only, and an option never reads `null` as absent, so a struct
  added to the form later is closed without a change to the reader. Each refusal is
  `WireError::WrongType` (`wrong-type` in `oce`), which gains a `path` field naming the offending
  value (`inputs[0].semantic`, `document` for the root) and prints it
  (`invalid type: sequence, expected a SemanticPoint object at inputs[0].semantic, line 19 column
  18`); a boundary `type` of `null`, a number, a boolean, a list or an object, refused as
  `json-syntax` before, is `wrong-type` too. `null` stays refused at every position, since the
  form has no nullable value; the [authoring note](docs/design/authoring-layer.md#wire-form) now
  says so. New sweeps over every wire fixture, the note's example and the 31 importable corpus
  fixtures in the wire form replace each object by the list of its values and each value by every
  JSON type its position does not take, read the allowed types from the published schema, and
  fail if a schema object appears in no fixture; a source scan allows only `deny_unknown_fields`
  and `rename` on the DTOs, because serde's buffered-content attributes (`untagged`, `tag`,
  `content`) would read a struct past the adapter. The importers (`import_cxf`,
  `import_graph_v0`) read a `serde_json::Value` and never fill a derived struct, so they never had
  the hole; sweeps now hold that, and `oce validate`, `emit` and `fmt` refuse each mutant at its
  path. Breaking for pre-1.0 code that builds `WireError::WrongType`, which gains `path`. The
  format stays at revision 1: no fixture, golden or schema byte moves.

- **Vectors, part 1 of 4: the `oce-verify` crate and the one vector format, `oce/vectors/v1`**
  (GitHub #541, issue #519; [vectors note](docs/design/vectors.md) decisions 1 to 5, 8 to 12,
  16 to 18 and 35). A new leverage crate, `crates/leverage/oce-verify` (`publish = false`;
  `schemars` and `serde_json`, with `oce-bless` and `allocation-counter` as dev-dependencies). It
  holds the format and nothing that runs: it loads no engine, replays nothing and writes no
  receipt, so it names neither `oce-api` nor `oce-replay` yet; binding, the run, receipts,
  mutants and the `oce verify` verb are parts 2 to 4.
  - **The reader**, `VectorSet::from_json_bytes(bytes, &VerifyLimits)`: strict JSON under the
    authoring wire profile (nesting 8, lists 65,536, strings 4,096 bytes, no repeated key, no
    control character, no non-finite number, integer tokens within `i64`), read by the crate's own
    cursor with a line and column on every refusal. Every object is closed: a key the format does
    not define is `unknown-key` and is never skipped, which covers `within`, `of`, `when` and
    `mutants` until the format gains them; `null` is never a value and a list is never read as an
    object's members. A root `schema` other than `oce/vectors/v1` is `unknown-schema` before the
    shape is judged, `cxf-library/vectors/v1` included: there is one format and no reader of
    another. A value is a number or a boolean and keeps the kind it was written as (`Literal`:
    `1` is not `1.0`, `-0` is the integer zero); typing waits for the binder.
  - **The rules a shape cannot say**, checked at load. The clock's cadence is decided by
    multiplication (`Clock`): a time is a tick when some integer `k` gives `k * step_s` exactly,
    found by rounding the quotient and testing its neighbours, never by flooring it, so
    `{ step_s: 0.1, horizon_s: 4.3 }` loads and a horizon one ulp away does not. Step lists start
    at exactly `t = 0`, increase strictly and never pass the horizon. Windows are inclusive, lie
    inside the clock and must hold a tick, decided from tick values (a loaded `Window` carries its
    first and last tick). A diagnostic `at_s` is a tick. An integer token in a time position must
    be within ±2^53 (`MAX_TIME_INTEGER`), where every integer is a binary64, or it is
    `integer-out-of-range`; a larger time is written as a real. No name is empty: `empty-name`
    covers a scenario name, an input name, a parameter path, an output name and a diagnostic's
    `block`. The note is amended with both rules, dated 2026-11-10 (review ruling).
  - **Bounds**: `VerifyLimits` (4 MiB, 16,777,216 ticks, 4,096 scenarios, 4,096 expectations per
    scenario, 256 parameters per map), each a typed refusal one past its value, lowerable to zero
    and never raisable (a value above the maximum is read as the maximum). Nothing is materialised
    per tick, and `tests/load_cost.rs` pins the memory a load holds with `allocation-counter`: at
    most 24 bytes per document byte for the densest shape, 4 for step lists and windows.
  - **The canonical form and `spec_id`**: `to_canonical_json` (keys sorted by UTF-8 bytes at every
    depth, two-space indentation, ASCII only, one final line feed) and
    `spec_id() = spec:fnv1a128:<32 hex>` over it, with FNV-1a-128 implemented in the crate.
    **The number-formatting rule is confirmed and written back into the note**, which had marked
    it as recalled from memory: an integer token as a decimal integer (`-0` as `0`); a real with
    the shortest round-trip digits, in plain decimal for zero and `1e-4 <= |x| < 1e16`, otherwise
    `d[.ddd]e[-]N` (`0.0001` and `9.999999999999999e-5`, `9999999999999998.0` and `1e16`, `-0.0`,
    `5e-324`). The thresholds and the exponent spelling stand as the note had them; the layout is
    the crate's own code, compared with Rust's `{:?}` over more than 23,000 values. The note and
    the rustdoc now also state the tiebreak: of the shortest strings that read back (`3e-324`
    through `7e-324` all read back to the smallest subnormal) the one closest to the exact value
    is written, pinned by goldens at the three smallest subnormals and by an exact-integer check
    over 6,000 values. The canonical goldens
    and the `spec_id` literals are derived by a second implementation in Python
    (`tests/fixtures/derive_canonical.py`), never blessed from the code they check.
  - **Errors**: `VectorError` with a closed, `#[non_exhaustive]` `VectorErrorKind`, a kebab-case
    `code()` per kind and `VECTOR_ERROR_CODES` (32 codes, checked against the kinds at compile
    time and pinned by a test that produces each one from a document). They are the note's load
    rows without the three mutant rows (`too-many-mutants`, `mutant-path-invalid`,
    `duplicate-mutant`), which arrive with the `mutants` key in part 3.
  - **The schema**: `schema/vectors.schema.json` (`VECTORS_JSON_SCHEMA`), derived with `schemars`
    from the reader's shapes, regenerated and compared byte for byte (`OCE_BLESS_VERIFY`), walked
    for keywords outside the strict profile, and held to the reader key by key.
  - **The two in-repo vector files are rewritten in place** to the format:
    `crates/oce-api/tests/fixtures/library_rules/AHU-0016/vectors.json` and
    `AHU-0004/vectors.json` change their `schema` line and nothing else, their two SHA-256 rows in
    that directory's README move, and `library_rule_state_continuation.rs` asserts the new tag and
    resolves names at a `.` boundary only. It runs the same scenarios, windows and restart sweep.
  - Tests: 155 in the crate, among them the strict cursor against `serde_json` as a syntax oracle
    over 20,000 mutated documents, the cadence rule against a search over every tick product, a
    10,000-document byte-mutation sweep through the whole load, and every bound at its limit and
    one past it. One of them reads `docs/design/vectors.md` and holds the note's worked example
    equal to its fixture byte for byte, so `scripts/ci/classify-changes.sh` now lists the note as
    a document a test reads: an edit to it runs the determinism and leverage lanes.
  - The workspace has 18 members (10 publishable, eight private); `release.yml`'s comment and step
    name, its digest and byte golden in `scripts/package_policy/` are re-pinned for the new
    private member. The 46 fixture goldens and 422 provenance goldens are byte-identical.
- **Vocabulary v1, part 2: `oce catalog` serves the block vocabulary and `oce vocab` the unit
  scale-factor table** (GitHub #526, issue #500; vocabulary note decisions 12 and 14). `oce-cli`
  now depends on `oce-vocab` (a leverage crate, so the `oce-api`-only rule holds as written).
  - **`oce catalog CLASS`** adds, to each parameter and each resolved port, the vocabulary's
    `description` and the `unit` and `quantity` the CDL source declares; to each parameter a
    `range`; and to the class its typed `notes` and its upstream `links` (the Modelica source at the
    pinned Buildings commit, and the latest-release help page, labelled as possibly drifting from
    the pin). A port reads its connector's text, so `u3` of a `MultiSum` carries the vector
    connector `u`'s. A note is printed as `kind`, `subject` and `text`: its `evidence` is for
    reviewers and is never printed, in text or JSON, which a test holds over every class. The text
    list of `oce catalog` is still one line per class; `oce catalog --json` gives every class the
    same shape as a single one. The report schema stays `oce-report/1`: the change adds keys
    (`description`, `unit`, `quantity` on ports and parameters, `range` on parameters, `notes` and
    `links` on the class) and removes none. A reserved lowering identity has no vocabulary: empty
    strings and `"links": null`.
  - **A range is rendered from the catalog's rules, never stored.** `oce_vocab::parameter_range`
    (and `ParameterVocab::range` in the joined view) turns `oce_api::CatalogEntry::param_rules`
    into one line of text per value rule: a bound (`Ti >= 1e-13`, `delta > 0.0`,
    `pAtm > 0.0 and finite`, `shift is finite`), a relation listed for both parameters
    (`uLow <= uHigh`, `nout * nin <= 1048576`, `0.001 <= h * n <= 0.5`), a relation that only warns
    (`uLow == uHigh warns`) and an element bound (`1 <= extract_<i> <= nin`). Rules about type,
    array shape and port structure render nothing. All 27 rule kinds the catalog uses are named in
    one total `match` and pinned by a test, and every kind of line is checked against what
    `oce_api::class_ports` and a loaded engine accept, refuse and warn about at the boundary value
    and the next `f64` past it. 101 of the 189 parameters have a range (176 lines).
  - **`oce vocab`** adds `scale_factors`, in text and JSON: `min` 60, `h` 3600 and `d` 86400, each
    an exact multiple of `s`. The table is `oce_vocab::SCALE_FACTORS` (with `scale_factor(symbol)`),
    time only; `degC`, `%` and prefixed SI units are not in it. It states the table: the text form
    still refuses `1 h` (`unit-not-base`) until part 3 (#501) reads it, and the output says so
    where an agent reads it: the text heading reads "not accepted as a unit suffix yet", and the
    meaning of `unit-not-base` under `parse_codes`, in text and JSON, names `min`, `h` and `d` as
    not accepted yet. A test fails when the text form starts accepting one.
  - **The text listing is held to the JSON line for line.** `tests/catalog_text.rs` writes the
    text it expects from each class's JSON report and requires the binary's text to be exactly
    that, for the 133 classes that resolve at a table of plain values, so a `unit:` line dropped
    under a port fails for every port that declares one (21 do).
  - **No output cap.** `oce catalog` builds the whole report in memory, for text as for JSON, and
    the catalog's width rules allow one class about 2^21 ports:
    `CDL.Routing.RealExtractSignal` at `nin=1048576` and `nout=1048576` wrote 416 MB and peaked at
    3.1 GB. The README states it; a cap is a separate change.
  - **Carried in from the review of GitHub #511, in `oce-vocab`:** the curated join no longer
    scans a class once per curated entry. Each curated class indexes its subjects once and every
    description and note is a binary search, so the reviewer's pair (one class with 24,000
    connectors, 32,000 notes naming the last) costs about 0.83 million name comparisons where the
    scan made 498 million; a test holds the join to `n log n` in name comparisons, not wall-clock
    time, and the resolved positions are kept so a view searches nothing. The reader's comment
    and test no longer say the list bound "cannot be reached": that holds for an accepted document only, and
    a compact 95,000-item document under the byte bound is read, checked and then refused
    `NotCanonical` (now pinned). A 1,000,000-deep nest is refused where the closed shape ends, in
    both readers, with a typed `VocabError::Json` (now pinned; an open shape would have scanned the
    whole document). The design note says 189 parameters and gives the regeneration command with
    `-p`.
  - Transcripts: nine new (`catalog_true_delay`, `catalog_true_hold_with_reset` and
    `catalog_dew_point`, each as text and `_json`; `catalog_pid`, `catalog_hysteresis_json`,
    `catalog_multisum`) and eleven re-blessed
    (`catalog_class`, `catalog_class_params_json`, `catalog_enum_param`, `catalog_rule_refusal`,
    `catalog_unresolved`, `vocab_text`, `vocab_json`, `help_oce`, `help_catalog`, `help_vocab`,
    `usage_no_arguments`). `oce-api`, the core four, `oce-cxf`, `oce-author` and both vocabulary
    data files are untouched.

- **Vocabulary v1, part 1: the vendored CXF-Core document and the `oce-vocab` crate** (GitHub
  #511, issue #499; design note GitHub #497).
  `third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld` is modelica-json's
  `-o cxf --elementary --cxfCore -p` output over the Buildings `CDL` package at the two recorded
  pins (682,615 bytes; the vendored README records the command, both pins, the date and the
  SHA-256, and says that byte-identity across operating systems and Node versions is unverified,
  so a pin advance re-vendors and re-blesses). The hash manifest gains a
  `generated-cxf-core` bucket of exactly that one file, the tree-SHA constant and the tracked
  `.jsonld` census (now 223; the document's import outcome is one new golden line, a refusal) move
  with it, and `.agents/revendor-upstream.sh` knows the bucket.
  `crates/leverage/oce-vocab` (`publish = false`; `oce-api`, `serde` and `serde_json`, with
  `oce-bless` and `sha2` as dev-dependencies) holds two strict-JSON files in one canonical spelling
  (closed objects, sorted keys, no `null`; a bounded reader that refuses anything else with a typed
  `VocabError`): `data/generated.json`, a description plus the declared Modelica unit and quantity
  for every connector and parameter of the 133 authorable classes, and `data/curated.json`, typed
  notes (`convention`, `pitfall`, `port-order`, `loop`, `units`) and descriptions only where the
  generated one is empty, every entry with `evidence`. `vocabulary()` merges and checks the pair
  against the catalog; `class_vocab(class_path)` joins one class with `class_metadata()` (the
  one-liner, each parameter's type, default and settable flag) and derives its pinned source link
  and its latest-release help link, labelled as possibly drifting from the pin; a resolved port
  name maps to its connector with `ClassVocab::connector_of_port` (`u3` is element 3 of `u`).
  Defaults and ranges are never copied into the data, and `oce-api` is untouched: `catalog()`,
  `class_metadata()` and `CATALOG_JSON` are byte-identical.
  - **The generator is a regenerate-and-diff test** (`OCE_BLESS=1` rewrites `generated.json`). It
    walks the catalog's own names, looks each up in CXF-Core by `@id`, and inverts modelica-json's
    QUDT unit table through a closed pinned table plus the S231 fallback (`1/s`). It refuses, naming
    the class and the subject, a missing node, a node for something pinned as absent
    (`TrueHoldWithReset`, and `MatrixGain`'s `protected` `nin` and `nout`), a malformed node, an
    unknown unit or quantity identifier, and a quantity kind with two inverses on a subject without
    a pinned resolution (`Pressure` also stands for `PressureDifference`; one parameter,
    `SpecificEnthalpy_TDryBulPhi.pAtm`, is resolved and checked against its `.mo` declaration).
  - **Coverage:** 133 classes (132 upstream, with links), 322 connectors (265 named ports and 57
    vector or positional connectors) and 189 parameters (the note's 186 plus the three `TimeTable`
    sources' rule-only `table`), every one described: 505 descriptions generated and six curated
    (`TrueHoldWithReset`'s `u`, `clr`, `y` and `duration`; `MatrixGain`'s `nout` and `nin`); 73
    declared units and 52 quantities. Curation seed: 13 notes over 11 classes (the prototype's port
    order, `delayOnInit` and loop-breaker rules, and a `Discrete.Sampler` pitfall).
  - **Tests:** the regeneration is byte-stable and fails on a renamed port or parameter, a
    removed or added class and an edited file; coverage against the catalog in both directions,
    with every width-driven and positional class resolved at widths 1, 2 and 1,048,576; every
    generated description, unit and quantity found in its vendored `.mo` declaration, and no
    declared unit dropped; each curation rule refused with its typed error; no string in either
    file with an absolute path, a UUID shape, a `modelica://` link or an HTML tag; and every seeded
    note the engine can decide held against a loaded engine (a self-loop through each class with a
    `loop` note, swapped inputs for each `port-order` note, both `delayOnInit` settings, runs of
    frames through `TrueHoldWithReset` and `Sampler`).
  Workspace 17 members (10 publishable, seven private); `release.yml`'s comment and step name, its
  byte pin and the package validator's golden name the new split. Parts 2 (`oce catalog`, #500)
  and 3 (time-unit suffixes, #501) follow.

- **The importer: the in-subset library CXF and the prototype's v0 graph JSON into the
  authoring model** (GitHub #498, issue #496; the [importer note](docs/design/importer.md); closes
  #475, M2 gate item 1). `oce-author`'s `import_cxf(bytes, &ImportOptions)` now returns
  `Imported { sequence, report }` and reads the full flat library dialect: enumeration-typed
  sequence parameters, boundary `min` / `max` converted exactly as the loader converts them (a
  test pins every corpus input bound to the engine's `InputDefinition` bound bit for bit),
  enumeration members and library constants as full member paths resolved through
  `oce_api::enumerations()`, and a constant-package-typed Integer boundary normalised to the
  Integer the engine reads. **Gate item 1 is met:** 31 of the 34 in-subset G36 fixtures import,
  emit, load warning-free and match the committed fixture's canonical content id (their exports
  byte for byte wherever block order agrees); the other 15 are refused with their kind (4
  `nested-composite`, 7 `conditional-component`, 1 `expression-value`, 3 `port-attribute`).
  `ImportError` becomes `{ source, subject, kind }` with a closed, `#[non_exhaustive]`
  `ImportErrorKind` of 29 kinds, each with a stable kebab-case `code()` (`IMPORT_ERROR_CODES`,
  pinned in-crate and each produced by a test), refused in a fixed precedence (nested composite,
  conditional component, expression value, port attribute, name) whatever the node order; the
  provisional `Unsupported { construct }` and `Malformed` variants are gone. `ImportReport` lists
  every dropped property (the root's `@type`, a differing `S231:label`, a parameter's `unit`,
  `quantity`, `displayUnit`, `min` and `max`), every normalisation and every truncated v0 comment
  line. The new `import_graph_v0` reads the frozen prototype's compact block graph under the wire
  form's bounds: `unit` and `description` become comment lines (split at line feeds, controls to
  spaces, truncated at a character boundary within `MAX_V0_COMMENT_LINE_BYTES`) and report
  entries, never a point unit, and the sequence carries `# imported from v0 graph JSON`; string,
  null, object and list values are refused. The prototype's three golden pairs are vendored
  verbatim under `crates/leverage/oce-author/tests/fixtures/v0/` with a `PROVENANCE.md`
  (repository URL, commit `f951346`, `MIT OR Apache-2.0`, the copyright line), and each graph
  loads to the canonical content id of the CXF the prototype wrote. The hostile suite grows: a
  mutation sweep over all 31 fixtures, every emitted golden and the three graphs; every bound,
  member, datatype and v0 refusal with its exact kind; each out-of-subset construct grafted onto an
  importable fixture. New goldens: `tests/fixtures/v0/*.import.txt`; no other golden moved, and the
  every-element emit golden now re-imports to the same bytes. `import_cxf`'s signature and
  `ImportError`'s shape are breaking for pre-1.0 consumers of `oce-author`; none outside the crate
  calls it. The importer note records the choices the note left open under "As landed".

- **Enumeration-typed sequence parameters, boundary bounds and member literals in the authoring
  model** (GitHub #493, issue #492; the [importer note](docs/design/importer.md)'s model changes).
  `oce-author`'s `SequenceParameter::value_type` is a `ParameterType` (`Real`, `Integer`, `Boolean`
  or `Enum(type_name)`, the enumeration's last segment): wire `"type": "SimpleController"`, text
  `param conTyp : SimpleController = SimpleController.PI`; the pinned schema lists a `type` enum of
  the three scalars and every `oce_api::enumeration_types()` row (17 values, never a constant
  package). `Literal::EnumMember` is renamed `Literal::Member(MemberRef)` and covers library
  constants (`OperationModes.occupied`) beside enumeration members, resolved through
  `oce_api::enumerations()`; a constant fits an Integer parameter and the emitter writes the package
  IRI as its `isOfDataType` (importer note, ruling 9), an enumeration parameter its class IRI and
  full member path, as the corpus does. `Boundary` gains `min` and `max` (`Option<Bound>`, an Integer
  or Real literal): wire `"min": []` or `[<number>]` (a second item is
  `WireError::BoundListTooLong`), text `@min` / `@max` after `@displayUnit`, emitted as `S231:min` /
  `S231:max` (a typed `xsd:double` on a Real boundary, a bare integer on an Integer one). Stage 1
  gains four codes (`STAGE_ONE_CODES` is twenty-five): `unknown-enum-type` (an unknown type name or a
  constant package as a type; the engine's code), `invalid-bound` (author-only: a bound on a Boolean
  boundary, a Real on an Integer boundary, an Integer above 2^53 on a Real one, and an Integer
  bound on an Integer boundary outside the engine's `i32` domain, which the engine would narrow
  without a warning: the importer note's decision 6 is amended, and a parity test pins the
  narrowing), `non-finite-bound`
  and `crossed-bounds` (the engine's codes), each pinned by the parity test; a reference to an
  enumeration parameter type-checks by class identity (`parameter-kind-mismatch` otherwise), and the
  builder refuses a non-finite bound (`BuildError::NonFiniteBound`). Tests generate every enumeration
  member and every constant of the facade table through both forms, the emitter and a warning-free
  load; pin each input's loaded domain to its declared bounds bit for bit (the text spelling
  converted as the loader converts it); show an input bound moves the canonical content id and an
  output bound does not; and extend the refusal matrix, the printer's totality cases, the enumerated
  round-trip models, the schema checker and the hostile sweeps (a new wire mutation sweep over the
  every-element bundle). Re-blessed: `schema/sequence.schema.json`, the every-element wire, text and
  emit goldens, `wire/econ_enable.json`, `wire/note_example.json`, `wire/defects.json` and its
  findings golden; the authoring note's wire example gained `"min": [], "max": []`. The minimal
  importer refuses the every-element golden until the importer PR, which unblocks 27 of #475's 36
  fixtures with these model changes; the authoring note carries the dated amendments. The renames
  (`EnumMember` to `MemberRef`, `Literal::EnumMember` to `Literal::Member`, `value_type:
  ParameterType`) are breaking for pre-1.0 consumers of `oce-author`; the `oce` CLI (GitHub #491)
  is the only one.

- **`oce-author` reads the facade's enumeration table** (GitHub #488, issue #468). The pinned
  four-row `ENUM_TYPES` table is deleted: the validator and emitter look an enum parameter's type up
  with `oce_api::enumeration_by_id`, and `tests/enum_member_oracle.rs` loads each member of the
  facade's `SimpleController` row through the engine. A test round-trips every row's class path
  through the emitter's class base (`http://example.org#Buildings.Controls.OBC.`). Findings and
  emitted bytes are unchanged.

- **`oce-replay`, the first leverage crate** (GitHub #462, issue #445). `crates/leverage/oce-replay`
  implements the historical replay note (`docs/design/replay.md`, decisions 1 to 23) above the
  facade, `publish = false`: an `InputMap` resolves source columns to engine input slots once (exact
  path or unique `.`-boundary suffix; ambiguous, unknown, unmapped, doubled, mistyped and stale maps
  are typed errors), and three sources lend rows by slot with `None` for a missing sample —
  `ColumnarSource` (owned typed columns, NaN and cleared validity missing, seconds or epoch
  nanoseconds), `CsvSource<R: Read>` (RFC 3339 instants with an explicit offset against a
  header-only origin) and `StepSource` (change lists as signals). `TimeBase::Recorded` passes times
  through bit for bit; `TimeBase::Grid { t0, step }` ticks at `t0 + k * step` by multiply with
  zero-order hold, a sample current only for its tick interval. `MissingPolicy` (`Refuse` by
  default, `HoldLast { max_age }` holding only observed samples, per-input `Fallback`, `SkipFrame`),
  `OnRefusal` (`Stop` by default, `RecordAndContinue`) and `DuplicateTime` (`Refuse` by default,
  `Allow`) are recorded frame by frame; `ReplayOptions::non_finite` defaults to `Refuse` and is set
  on the owned engine and recorded. `Replay::advance(max_frames)` is a resumable cursor over
  `Engine::execute_slots` (GitHub #459) and `finish` returns the engine. The `Trace` is columnar
  with raw bits: one row per attempted frame with a status byte, outputs fixed by the first accepted
  frame and checked every frame, a dense substitution mask, opt-in inputs and state snapshots,
  sparse diagnostic and refusal tables, and `Trace::canonical_bytes()`; every limit of decision 19
  is a typed error, accepted at the cap and refused one past.
  As built, against the note's sketch (its new "As built" section lists them): column specs carry a
  `ColumnKind` rather than a `ValueType`; `FrameSource` exposes `map_id`, `kind`, `origin` and
  `ignored_columns` rather than `columns()`, and `&mut S` and `Box<S>` are sources; `TimeOrigin` is
  a parsed struct; `StepSource::with_instants` adds evaluation instants with no input change;
  `Replay::reserve` preallocates rows; `Trace::check_coverage` counts only executed rows;
  `max_frames` with frames remaining is `ReplayError::TooManyFrames` and a grid whose ticks stop
  advancing is `ReplayError::GridNotIncreasing`; a missing input that cannot be substituted refuses
  the frame even when another is `SkipFrame`, and the mask records substitutions on skipped and
  refused rows; a refusal stores a closed `EngineRefusal` class in the canonical bytes (the facade's
  message stays in memory), and a `Run`-class refusal stops the run whatever `OnRefusal` says; Real
  NaN from any source, custom ones included, is missing. Decision 21 as built: the facade has no
  enum class-path lookup, so an Enum input records its source-declared class path as an unverified
  label and the canonical bytes carry an Enum output's ordinal with an empty class path. Decision 22
  as built: `CsvSource` tokenizes with `csv-core` (pinned `=0.1.13`, the allocation-free core of the
  `csv` crate) so the column, cell and byte caps bound memory as the bytes arrive; the `csv`
  crate's reader buffered a whole record first (a 100 MB header line of commas reached 3.2 GB in
  review). `allocation-counter` is a new dev-dependency, for the census.
  Evidence: all 46 G36 fixtures through the columnar and step sources equal the named facade path
  bit for bit, and the controller's minute day equals its conformance golden
  (`g36_traces/cooling_only_controller.csv`); four canonical-byte goldens; chunk sizes 1, 7 and N,
  and a mid-run `reserve`, byte-identical; every policy pair with exact status, mask and refusal
  rows; refusal atomicity against a run that never saw the frames; time, mapping, bounds, a custom
  source, and a hostile CSV corpus including 64 and 100 MB floods refused at small peak memory;
  trace bytes equal `rows × bytes_per_frame` for every fixture (the controller: 89 B a frame,
  1,538,009 B for a day at 5 s); and an allocation census: nothing on any quiet frame after the
  first, once the trace columns are reserved; the first frame allocates the output columns;
  warning, refusal and snapshot frames allocate as documented. The leverage rule now allows
  `oce-bless` as a dev-dependency only (authoring note decision 19): `scripts/ci/leverage-members.sh`
  reads normal and build edges and dev edges separately, with fixtures for each. The workspace has
  14 members (10 publishable, four private); `release.yml`'s comment and step name, its digest and
  the validator's controls are re-pinned.

- **`oce-author`: the authoring model, wire form, pinned schema and stage-1 validator** (GitHub
  #466, issue #463). `crates/leverage/oce-author` (`publish = false`, `oce-api` only) is M2's first
  crate, built to `docs/design/authoring-layer.md`. The model is one flat `Sequence` of sequence
  parameters, boundary inputs and outputs (with `quantity`, `display_unit` and a closed
  `SemanticPoint` slot), leaf blocks with ordered assignments (a tagged literal or a reference to a
  sequence parameter; array parameters one element at a time under `oce_api::param_element_name`)
  and typed connections, all in authored order; `Identifier` and `ProvenanceId` carry the note's
  grammars, and `SequenceBuilder` refuses what it can see without the catalog with a typed
  `BuildError`. The wire form (`from_wire_str`, `from_wire_bytes`, `to_wire_string`) is closed
  serde DTOs with every key required, the five value shapes and `"a.y -> b.u"` connections; a bounded
  lexical pass refuses an oversized document (4 MiB), nesting past 8, a list past 65,536 entries, a
  string past 4,096 bytes, duplicate keys, `NaN`/`Infinity`, integers outside `i64` and control
  characters as typed `WireError`s before the typed parse, and the canonical writer is byte-stable
  (fixed key order, integers bare, reals in the shortest round-trip spelling, ASCII only). The JSON
  Schema is derived with `schemars` 1.2.3 (the only new third-party dependency) with the `class`
  enum from the 133 authorable catalog classes, checked in as `schema/sequence.schema.json`,
  regenerated byte for byte in a test and walked for keywords outside the strict profile.
  `validate` (stage 1, `validate_author_side`; the engine stage arrives with the emitter) reports
  the twenty stage-1 codes with the note's exact strings, sorted by authored position, code and
  detail and deduplicated, using `oce_api::class_metadata` and `oce_api::class_ports` for class,
  parameter, kind, settability (reserved classes only), catalog-rule (the load gate's own checks,
  mapped by `DiagCode::as_str`), arity and port-name judgements. `SimpleController`'s members are
  not published by the facade, so the crate pins the four enumeration types and a test checks the
  table against everything `oce-api` does publish (noted on #455). Goldens: four canonical wire
  bundles (the worked example, an every-element sequence, the note's abridged example and a
  defects bundle) and their finding lists, bless switch `OCE_BLESS_AUTHOR`; a structural schema
  checker agrees with the parser shape by shape, the `SimpleController` table is loaded member by
  member through `Engine::load_cxf`, and a document near the 4 MiB bound validates in near-linear
  time (name indexes built once per run). Every statement carries text-form `Annotations`
  (comments, a blank line before it) and assignments and sequence parameters a unit `suffix`, so
  the text form needs no model change; the wire form and the emitter ignore them, and the model,
  `BuildError`, `Stage`, `Origin`, `Severity` and `Finding` are `#[non_exhaustive]`. Choices the
  note left open: a connection string without exactly one `->` is a `WireError`, a sequence
  parameter's value is a literal (no reference), `-0` reads as `Integer(0)` while `-0.0` keeps its
  sign, a non-finite Real (reachable only through the Rust model) is written `null`, a String value
  the wire cannot carry is `parameter-out-of-range`, and an array element past the configured width
  (`k_3` on `nin = 2`) is not refused until the facade reports unread names (#467). The crate
  relies on the leverage rule from GitHub #462 that allows `oce-bless` as a dev-dependency only.
  The workspace has 15 members (10 publishable, five private); `release.yml`'s comment and step
  name, its digest and the validator's controls are re-pinned.

- **`oce-author`: the emitter, engine-side validation and the library round trip** (GitHub #473,
  issue #464). `emit(&Sequence, &EmitOptions)` writes one flat, single-root CXF document in the
  authoring note's fixed layout and authored order, never sorted: instance IRIs
  `{namespace}{sequence}.{local}` under `urn:oce:author:` or an override that must end in `#`, `/`
  or `:` (the content-id note's last-delimiter rule, GitHub #471), block `@type` under the fixed
  class base `http://example.org#Buildings.Controls.OBC.`, only the assignments the author wrote,
  relation lists always arrays, array parameters in the CXF array form, boundary `unit`, `quantity`
  and `displayUnit` and no semantic record, from a canonical writer; it refuses a stage-1 error
  (`EmitError::Invalid`), a malformed namespace and a document above `MAX_CXF_BYTES` (one byte over
  is refused, the limit itself loads). `validate` now runs stage 2 when stage 1 is clean: it emits,
  loads in a throwaway `Engine::new()` through the receipt path and maps every engine diagnostic
  back through the IRI scheme, every warning an error finding with the engine's code (decision 14),
  `origin = engine`; `connector#N` and other subjects outside the scheme stay on `sequence` with
  the raw subject in `detail`. Two new codes: `document-too-large` and `load-failed`. Stage 1
  gains two rules: a target the connection list repeats is `single-assignment` (pre-empting the
  engine's own refusal, which covers a block output beside a boundary input since GitHub #477),
  and an Integer literal above 2^53 in magnitude on a Real parameter is `parameter-out-of-range`
  (`MAX_EXACT_INTEGER`). A minimal `import_cxf` reads the flat corpus dialect strictly (a key
  written twice or an integer outside `i64` is refused) and refuses everything outside the
  authoring subset with a typed `ImportError`. The parity test emits each stage-1 defect with
  stage 1 bypassed and amends four rows of the note's finding-code table: `duplicate-parameter`
  pre-empts `duplicate-id`, `unknown-enum-literal` pre-empts `grounding-failed`,
  `unknown-parameter` pre-empts nothing (#467) and `unknown-port` pre-empts
  `unresolved-reference`; decision 15 gains the partial array (written element by element under
  the flattened names, no default filled). **Gate item 1 is not yet met:** 4 of the 40 in-subset
  G36 fixtures (6 are nested or conditional, out of subset) import — `cooling_only_system_requests`,
  `generic_time_suppression`, `multizone_vav_relief_fan_group`, `reheat_overrides` — and each
  round-trips to the committed fixture's export identity (`multizone_vav_relief_fan_group` exports
  partially, so it is compared on export bytes plus deferral codes, not a content id); the 36
  others wait on model decisions (boundary or port `min`/`max`, enumeration-typed sequence
  parameters, library enumerations; #475). The comparison re-stamps the corpus namespace
  (`http://example.org#g36.source.`, which ends in `.` and so cannot be an emitter namespace) to
  `urn:oce:library:`; the re-stamp stays after the canonical content id lands, until library
  bundle v2. **#458:** the only flat fixture whose `@graph` order differs from the emitter's
  layout is `multizone_vav_outdoor_airflow_sumzone` (block nodes out of `containsBlock` order
  from the third block), and the four nested fixtures have no flat layout; all five are refused by
  the importer before order matters, so they are excluded with a documented list and a test that
  pins the sumzone mismatch, not compared on loaded models.
  Byte goldens under `tests/fixtures/emit/` (`.cxf.json`, so `oce-cxf`'s `.jsonld` census is
  unchanged; the worked example, the every-element sequence and the four imported fixtures) each
  load warning-free and load to the model's topology; values ground bit for bit through the
  engine (subnormals, `f64::MAX`, `-0.0`, `i64::MIN`/`MAX`, enumeration members, references); the
  permutation classes, the §7.17 seam (only `unit` lines move and the compiled blob is unit-blind
  apart from its content id) and hostile importer input (a bounded mutation sweep over every
  imported document) are tested. Found on the way, and fixed by GitHub #477: a block input driven by
  both a block output and a boundary input loaded without a `single-assignment` refusal (#472);
  the parity test now pins the engine's refusal of it.
- **`oce-author`: the text form, with an idempotent `fmt`** (GitHub #485, issue #482).

  **What it is.** The authoring note's terse line grammar (decisions 1, 3, 12, 13 and 15): a
  hand-written recursive-descent parser and a canonical printer.
  - `parse_text` / `parse_text_bytes` / `fmt_text` return `Result<_, ParseError>`; `print_text`
    returns `Result<String, PrintError>`.
  - `fmt_text` is parse then print, and it is idempotent. It keeps every comment: an end-of-line
    comment moves above its statement, and a comment after the last item of a list or block is
    refused.
  - Parsing is bounded: 4 MiB, 100,000 lines, 8,192-byte lines, 256-byte tokens, 4,096-byte
    strings, 8,189-byte comments, list depth 2, 65,536 statements of one kind.
  - Each malformed construct gets one typed `ParseError { code, location, detail }`, never a
    panic. The thirty-one codes of `PARSE_CODES` are pinned in order, as `STAGE_ONE_CODES` is.
  - Semantic defects stay the validator's.
  - Unit suffixes are base units only: `1 h` is refused as `unit-not-base`, naming vocabulary v1.
  - Lists lower to the catalog's element names.
  - Reals print in the shortest round-trip spelling, and an Integer never becomes a Real.
  - `validate_text` fills `Finding::span` for both stages. A `SourceMap` indexes the subjects once,
    so a document at the statement cap places its findings in linear time. A duplicate is placed
    on the later declaration in model section order; a subject the text does not name keeps
    `None`.

  **The one model change the text form required:** `Sequence::annotations` (header comments) and
  `Sequence::trailing_comments`. The wire form, the schema and the emitter ignore both. The note's
  worked example opens with a header comment, and statement annotations could not hold it.

  **Stage 1 additions** that make every clean model printable:
  - `nan`, `inf` and `infinity` (any case) are refused as declared names (`invalid-name`);
  - a boundary carries at most 16 `@haystack` tags (`MAX_HAYSTACK_TAGS`), so a clean boundary fits
    on one line;
  - strings, `@name` and `@label` with a bidirectional override or isolate are refused.

  **Gate item 2 holds:** wire → text → wire is byte-identical for the three wire bundles and the
  four imported corpus fixtures. The note's worked text example parses to its wire bundle.

  **Deviations from the note and the issue:**
  - The functions are named `parse_text` / `print_text` / `fmt_text`, not `parse` / `print` /
    `fmt`, after `from_wire_str` / `to_wire_string`.
  - The line bound is 8,192 bytes, not 4,096, so the longest string the wire form carries
    (4,096 bytes as written) fits on one line with its assignment.
  - `print_text` is total or refusing. A name, class path, atom, suffix, string, value or
    `extends` id the lexer could not read back, a statement whose printed line would pass 8,192
    bytes, and a document that would pass 4 MiB, 100,000 lines or 65,536 statements of one kind are
    `PrintError::Unspellable`, never text that `parse_text` refuses or reads
    to a different model.
  - `tests/fixtures/wire/defects.json` is excluded from wire → text → wire. It holds a `unit` atom
    with a space and a non-path class, which stage 1 refuses and which `print_text` refuses as
    unspellable.
  - Long arrays print one element or row per line, and a long row one cell per line.
  - Array elements regroup into `[..]` only when the run is every element of the template the block
    assigns: consecutive, from index 1, at least two, a matrix a whole rectangle. Anything else
    prints flattened.
  - `NaN`, `inf` and `Infinity` are refused as values (`non-finite-number`).
  - `-0` reads as the Integer `0`, and a leading `+` or zero is refused rather than normalized
    (stricter than decision 12).
  - `comment-before-close` refuses a comment after the last item of a `(...)` or `[...]`.
  - The empty list is refused (`malformed-list`).
  - The unit-suffix grammar is a Modelica subset: exponents other than zero, a single `/` whose
    right-hand side is the whole denominator (`kg/m.s2` is kg/(m·s²)), no parentheses.
  - Atoms are checked as printable ASCII at parse (`malformed-atom`).
  - Bidirectional overrides and isolates are refused anywhere.
  - A dotted name has no space around its dots.

  **Tests:**
  - canonical bundle texts and `fmt` comment goldens (`OCE_BLESS_AUTHOR`);
  - 48 enumerated models through print and parse;
  - every stage-1-clean fixture and a set of clean models at their field bounds printing within the
    line bound and reading back equal;
  - one test per unspellable value, statements and documents past a bound refused rather than
    printed, and a dirty-model sweep (39 string slots × 26 hostile strings) in which every model
    either refuses or prints text that reads back equal within the bounds;
  - one test per refusal code with its exact location, a mechanical check that every code in
    `PARSE_CODES` is produced by some input, and every bound at its boundary;
  - finding spans and a span-cost test at the statement cap;
  - a deterministic mutation sweep of more than 25,000 mutants, with `catch_unwind` naming each,
    that also requires every mutant that parses to print and parse back unchanged.

  The note's text-form section is amended (dated) with each of these.

- **The `oce` command-line tool** (GitHub #491, issue #489). `crates/leverage/oce-cli` (package
  `oce-cli`, binary `oce`, `publish = false`) is M2 item 4 and the first binary above the facade:
  `docs/architecture.md`'s no-binaries rule now covers the engine crates only (authoring note
  decision 9). `oce emit` validates both stages and writes flat CXF to stdout or atomically to `-o`
  (`--namespace` checked by the emitter), writing nothing on an error finding; `oce validate`
  reports both stages or `--author-side` alone, with spans for text input; `oce fmt` prints the
  canonical text (wire input converted), `--check` answers whether the input is canonical in its own
  form and `--write` rewrites a text file atomically and only on change (both file writes go
  through a uniquely named `create_new` temporary file that never follows a planted link and is
  never more permissive than the target, new files `0600` on Unix, keep the target's permissions, follow a symbolic-link target, and report `changed`, `output` and `bytes`
  only once the write succeeded); `oce catalog` lists the
  classes or describes one with its parameters (type, default, settable) and its ports resolved at
  the default table or at `--param NAME=VALUE`; `oce vocab` prints the keywords, tags, base units and
  every code the tool reports with one line of meaning, and the facade's enumeration and
  constant-package table (`oce_api::enumerations()`, GitHub #488) with each member's value. Arguments are parsed with `clap` (derive,
  pinned `=4.6.7`, no colour), the second of M2's two new dependencies. Input is `.oce` text or
  `.json` wire by extension, `--from text|wire` required for stdin; every read is bounded by the
  form's 4 MiB limit, a file's size checked from its metadata before a byte is read. Stdout carries
  one document per run — the product, or a JSON report (`schema: "oce-report/1"`: `tool`, `command`,
  `input` as given, `ok`, `findings`, `errors`, command keys; sorted keys, LF, no clock, no host
  path) documented in the crate README as the agent-facing contract; a human rendering goes to
  stderr only on a terminal or with `--render always`. Exit codes: `0` yes, `1` no (error findings,
  non-canonical, unresolved ports, unknown class), `2` error (usage, read, parse, size, write). A catalog
  parameter of an enumeration type names its type's class path, and its `--param` value is a member
  name (`PI` or `SimpleController.PI`). Evidence: 59 golden transcripts (exit, stdout, stderr) through the built binary
  for every subcommand × outcome class and every `--help`; a contract test over every report;
  `fmt --check` over every `oce-author` text fixture in place, `fmt` equal to its goldens and
  idempotent; every emitted bundle loads warning-free in a throwaway `Engine` under two namespaces;
  hostile inputs (binary junk, bad UTF-8, a 100 MB sparse file refused from its metadata, an
  over-size stdin, a directory, a missing path, an empty file). 16 workspace members (10
  publishable, 6 private); `release.yml`'s comment and step name re-pinned.
### Verification

- **`rustix` moves from 1.1.4 to 1.1.6 in `Cargo.lock`, clearing RUSTSEC-2026-0486** (GitHub
  #605, issue #598). The advisory (unsound: rustix's xattr functions mark unwritten bytes
  initialized for a zero-length buffer) made `cargo deny check advisories` fail in the daily
  `advisories.yml` run and in `release-gate.yml`'s `cargo-deny (full)` job. The path is a
  dev-dependency only (`rustix` is `oce-cxf`'s Unix dev-dependency), so no shipped build linked it,
  and the per-PR `cargo-deny` job, which checks bans, licenses and sources only, stayed green. Only
  the lockfile moves: rustix's own dependencies and the `Cargo.toml` requirement (`1.1.4`, which
  1.1.6 satisfies) are unchanged, rustix 1.1.6 needs Rust 1.65 against the workspace's 1.97, and no
  ignore entry was added. `cargo deny check bans licenses sources advisories` is clean, with the same
  warnings as before. Not moved: `tools/openmodelica-line-reference/Cargo.lock` and
  `tools/openmodelica-reliefs-reference/Cargo.lock` also pin rustix 1.1.4, but no workflow
  advisory-checks those workspaces, and each lockfile's SHA-256 is bound into recorded OpenModelica
  evidence that only the manual native-arm64 workflows can regenerate (GitHub issue #604).
- **The workflow checker closes five gaps, and the pin-advance row states its real reason**
  (GitHub #564, issue #555; follow-ups from the reviews of GitHub #547 and #557, and an owner
  rulings of 2026-11-10 and 2026-11-11). No workflow or gate command changed; the `full` proof
  row gains two files.
  **Checker.** `check-workflow-gates.sh` now (1) pins the body of `[profile.ci-release-workspace]`
  and of its `.junit` table in `.config/nextest.toml` to the line, so a `default-filter` or any
  other key added to the profile fails instead of narrowing the nightly's one release-codegen
  workspace run; (2) makes each exception in the per-PR/nightly determinism-job comparison
  one-sided: only `pr-gate.yml` may carry the lane filter and only `release-gate.yml` the pinned
  checkout ref, so a per-PR determinism job that pins a ref of its own fails; (3) compares the
  workflow-level `env:` of `pr-gate.yml`, `merge-gate.yml` and `release-gate.yml`, `CHECKOUT_REF`
  aside; (4) end-anchors the presence rules for the determinism cells, their fan-in and the
  Windows cell (new `require_job_line`), so `<command> || true` or a trailing argument added to
  both copies of a shared step fails; (5) pins the release gate's
  `save-if: matrix.arch == 'aarch64'`. It also gains a tripwire, `refuse_failure_masks`, on every
  non-comment line of the three gating workflows: `||` outside a `${{ }}` expression, a line
  ending in `|\` or starting with `|`, a trailing `; true`, `; exit 0` or `&`, `set` with a `+`
  option among its arguments, `shopt -u`, a quoted literal containing `|`, `&`, `;` or `+` inside
  an expression, and a `shell` key in block or flow style that is not the line `shell: bash` (a
  template such as `bash {0}` drops errexit and pipefail). The tripwire is not a proof that no
  step discards a failure: a bare `exit 0`, `cmd && true` on a non-final line, `cmd & wait` and
  `(cmd &)` still pass, and GitHub #566 tracks them. 52 hostile fixtures, each of which the
  checker let through before the rule that refuses it, and 2 positive ones (305 cases).
  **Pin advance.** The `full` row stays. Its stated reason is corrected in `AGENTS.md`,
  `.agents/project-facts.md`, `docs/ci-and-the-gate.md` and `scripts/ci/proof-row.sh`: the
  pin-guard tests already run per PR (`oce-cxf`'s `g36_catalog_tests` in `crate tests`,
  `oce-blocks`'s `registry::catalog_tests` in the determinism subset); what only `full` or the
  nightly gives a pin advance is `oce-conformance`'s `conformance` binary, three of whose modules
  read the vendored tree and the reference catalog, that crate's `clean_room` unit tests and
  `open_modelica_nand` binary, which pin the vendored `Nand.mo` by path, line and equation, and
  the doctests. "The two files that hold a pinned upstream commit" was false and is reworded:
  nine files write one, the row lists two pin-guard files, and `scripts/ci/test-proof-row.sh`
  now holds the full list and fails when it changes.
  **Proof row.** `.config/nextest.toml` (for its `ci-release-workspace` profile) and
  `scripts/ci/install-wasmtime.sh` (for its default destination) join the `full` row in
  `scripts/ci/proof-row.sh`: only the script's `full` block exercises them (owner ruling of
  2026-11-11). `qemu.yml` and `emulate-firmware.sh`, which have a nightly workflow of their own,
  are named in prose under the dispatch rule, and
  `crates/oce-conformance/Cargo.toml` no longer names `merge-gate.yml` as a runner of
  `--test blob_round_trip`.

- **The release-codegen determinism cells run per pull request again, the merge gate keeps the
  Windows cell alone, and no change owes a local release-codegen run** (GitHub #557,
  issue #556; owner decision of 2026-11-10, closing step 3 of #504). Issue #502 had moved the
  release cells out of `pr-gate.yml` when one took 11 to 18 minutes; with the integration tests
  of `oce-api`, `oce-author`, `oce-cxf` and `oce-conformance` built as a few binaries each
  (GitHub #546, #551, #553, #554) they took 174 s (x86_64) and 181 s (aarch64) in `merge-gate.yml`
  run 42948818576. The per-PR gate took 5 min 11 s on this change's own head (run 42953966045),
  against 2.5 to 4.9 minutes for the seven full-lane runs before it.
  **`pr-gate.yml`.** `determinism-matrix` is `codegen: [debug, release]` again, four native cells,
  and `determinism-compare` again compares portable and target-bound snapshot bytes, the 46
  compiled-schedule blobs and the round-trip recordings across debug and release codegen as well
  as across architectures. Both jobs keep the `core` lane filter, and `CI OK` already needs
  them, so a determinism-lane PR is gated on all four cells and every comparison. The steps are
  `release-gate.yml`'s; only the debug cells save the shared `nextest` rust-cache entry.
  **`merge-gate.yml`** runs one job, `std build and nextest (Windows x86_64, debug)`, under
  `Merge gate OK`; nothing else is merge-only. **`release-gate.yml`** keeps every job (comments
  only, one of them the false "the gate script installs nothing").
  **Local proof.** The `determinism` row is gone: the core four, `oce-cxf`, `oce-api`,
  `Cargo.lock`, the root `Cargo.toml`, `rust-toolchain.toml` and `.cargo/config.toml` owe the
  engine row, `bash .agents/gate.sh`. `scripts/ci/proof-row.sh` names only `docs`, `leverage`,
  `light` and `full`. `bash .agents/gate.sh determinism` stays as a mode, for reproducing a red
  release cell on one host. The `full` row (a pin advance, or `.agents/gate.sh`) and the rule to
  dispatch `release-gate.yml` for a change only the nightly executes are unchanged, and the
  documents now record the owner's confirmation of them (2026-11-10) in place of "to be
  confirmed".
  **Checker.** `check-workflow-gates.sh` requires `codegen: [debug, release]`, the release
  subset, round trip and state vectors and every cross-codegen comparison in `pr-gate.yml`;
  requires the determinism cells and fan-in to run the same steps in `pr-gate.yml` and
  `release-gate.yml` (every non-comment line equal apart from the lane filter, the pinned
  checkout ref and `save-if`); pins `save-if: matrix.codegen == 'debug'` per PR; and holds
  `merge-gate.yml` to exactly the Windows cell and its aggregate, byte-identical to the release
  gate's. Twelve per-PR hostile fixtures and three merge-gate ones replace the twelve merge-gate cell fixtures (251 cases, from 249).
  The product contract moves to revision 29, evidence delivery only (PC-041's limitation: both
  byte identities are compared per pull request). No crate source, test, fixture or golden
  changed.

- **The local proof before a merge is matched to the lane a pull request runs, and the heavy
  release gate runs nightly instead of after every merge** (GitHub #547, issue #545; owner
  decisions of 2026-11-10, which amend issue #502's "the release gate runs on every merge" and
  the standing "run `gate.sh full` before the PR"). Measured on `fdd70c0`: a cold
  `bash .agents/gate.sh full` took 30.1 minutes on an idle 10-core arm64 Mac, 21 of them building
  release-codegen test binaries, about three times per PR; the post-merge `release-gate.yml` run
  42767547196 started 43 minutes after its push and then took 1 h 29 min (`gate (full)` 88.7
  minutes, the full test suite 41.8), while its four determinism cells, both comparison legs and
  the Windows cell finished in under 13 minutes.
  **Local proof.** `.agents/gate.sh` has five proof modes, each running everything the one before
  it runs: `docs` (new; a docs-only PR: the always-on checks, no cargo build), `leverage`, the
  default `light` (the engine lane, now in debug codegen only), `determinism` (new; `light`
  plus the determinism subset and the strict-bit step under release codegen, the two steps
  `light` ran until now) and `full`. A PR owes the row its changed paths select: docs only;
  `crates/leverage/**`, docs and the root `README.md`; anything else; the core four (`oce-model`,
  `oce-expr`, `oce-blocks`, `oce-graph`), `oce-cxf`, `oce-api`, `Cargo.lock`, the root
  `Cargo.toml`, `rust-toolchain.toml` or `.cargo/config.toml`, which owe the release-codegen run;
  and a pin advance (`third_party/**`, `tools/reference-catalog/**`, the two files that hold a
  pinned upstream commit) or `.agents/gate.sh` itself, which owe `full`. The first three rows and
  the core four and `oce-cxf` are the owner's table; `oce-api`, the four build files and the
  fifth row are coordinator decisions of 2026-11-10 from the review of this change, which the
  owner confirmed the same day (GitHub #557 then removed the `determinism` row). A change to `release-gate.yml`, or to anything else only `full` or the
  nightly run executes, also dispatches that workflow on its branch and waits for it before
  merging. `scripts/ci/proof-row.sh` (new, with fixtures in the per-PR `gate behavior
  fixtures` job) names the row from changed paths through the lane classifier, fail-closed, and
  every gate run ends with a `LOCAL PROOF` block naming the row it covered and the row the
  checkout needs against `origin/development`; that block is advice and never changes the exit
  status. A rebase with no conflicts needs `CI OK` on the new head and no local re-run; a rebase
  with conflicts or a fix round re-runs the row. No row runs every crate's suite, so the author
  runs the suites of the crates the PR changes. `full` is also what CI executes nightly. A
  determinism-lane merge needs the latest nightly not red, or its failure already being fixed
  forward. `scripts/ci/test-gate-modes.sh` (new, per PR) executes the script in each mode with
  stubbed children and fails unless the modes nest. Every mode now runs the per-PR `docs` job's
  steps (mdBook install, checker contracts, staging, build, links), which were CI-only.
  **Workflows.** New `merge-gate.yml` runs on every push to `development` and on dispatch, on the
  commit the event names, never cancelled in progress: the determinism matrix in all four native
  cells, its fan-in with the debug-against-release and cross-architecture comparisons, and the
  Windows cell, under an aggregate named `Merge gate OK` (not `CI OK`, which is `main`'s required
  status and must not be satisfiable by this short run on a release PR's head commit).
  `release-gate.yml` no longer triggers on a push to `development`; it runs nightly on the
  `development` tip, on dispatch, on release PRs into `main` and on pushes to `main`, with every
  job it had. A dispatched run now checks out the dispatched ref's head (`github.sha`), as a push
  run does, instead of the `development` tip, so a branch can show the heavy set green before it
  merges; a scheduled run still checks out `development`. The release-codegen workspace suite is
  built once per run, not twice: `test-suite` runs the debug suite and the doctests, and
  `gate (full)` runs the release suite through the executed script under a nextest profile of
  its own, `ci-release-workspace` (new in `.config/nextest.toml`), clearing that profile's JUnit
  report before the script and requiring it after; the shared `ci-release` report would not do,
  because the release determinism subset and strict-bit step write it in the same mode. `CI OK` stays the one required status on both
  protected branches; branch-protection settings are unchanged.
  **Checker.** `check-workflow-gates.sh` pins each of the two workflows' events and branch
  filters exactly, their job ids exactly (a heavy job deleted from `release-gate.yml` with its
  `needs:` entry fails; so does a cell, a comparison or the Windows cell leaving `merge-gate.yml`,
  or a heavy job entering it), the three shared job definitions byte for byte between the files,
  the merge gate's aggregate name, checkout ref and concurrency lines, that `branches:` is each
  event's only filter, the nightly cron string, the `gate` job's clear-run-require order, the
  shape of the `full` block of `.agents/gate.sh` (so a workspace step that is dropped, wrapped
  in a condition or prefixed with one fails) and that nothing else names the
  `ci-release-workspace` profile. A required job command that survives only in a comment no
  longer satisfies `require_job_pattern`. Its fixtures gain 79 cases, every one a hostile
  control.
  **wasmtime.** `scripts/ci/install-wasmtime.sh` installs by default into a user cache directory
  (`$OCE_WASMTIME_DIR`, else `${XDG_CACHE_HOME:-$HOME/.cache}/open-control-engine/wasmtime-bin`)
  instead of `target/wasmtime-bin`, which the `cargo clean` at each merge boundary deleted, and
  prints that path for `.agents/gate.sh` (`--print-destination`); the pin and both SHA-256 checks
  are unchanged, and the per-PR wasm32 leg still passes `target/wasmtime-bin` explicitly. The
  script's blob round trip now runs the installer, which re-verifies an installed binary's
  digest, and executes that binary by path, never another `wasmtime` from PATH;
  `scripts/ci/test-install-wasmtime.sh` (new, per PR) holds the destination order.
  The product contract moves to revision 28, evidence delivery only (PC-041's limitation names
  the merge gate). No crate source, test, fixture or golden changed.

- **The root `README.md` has its own lane in the per-PR gate, so a pull request that updates it
  beside leverage or documentation paths no longer runs the determinism lane** (GitHub #507,
  issue #503; owner decision of 2026-11-09). `scripts/ci/classify-changes.sh` emits a fifth output,
  `readme`, true when the root `README.md` changed or `core` is true; the root `README.md` alone
  sets neither `core` nor `leverage` and is not `docs_only`. Nothing else is reclassified:
  `crates/*/README.md` and `third_party/**/README.md` stay `core`, `crates/leverage/**/README.md`
  stays `leverage`, `docs/cxf-composite-subset.md` stays `core`, an unmatched path is `core` and
  empty input runs every lane. The README lane is one job, the existing `facade-contract`
  (`facade contract and Quickstart`), now filtered on `readme` instead of `core` and so still run by
  every engine change. It gains a step that runs `readme_quickstart`, the byte-pin of the README's
  first Rust block against `crates/oce-api/examples/quickstart.rs`, beside the executed Quickstart
  it already ran; before this the byte-pin ran only inside the determinism cells' `oce-api` suite.
  The README's other readers were already in always-on jobs (`scripts/docs/stage.py` and
  `scripts/docs/test_check_links.py` in `docs`, `scripts/product_contract/check.py` and its controls
  in the package/authority/product contract job). `scripts/ci/test-classify-changes.sh` now holds
  that enumeration: it scans the crates, tools, firmware and script trees for sources that name the
  root `README.md` and fails when one is run neither by an always-on job nor by the README-lane job,
  when a recorded job is filtered on another lane or loses the command, and when a recorded
  exception no longer matches; eleven controls (a reader on the `core` lane, three dropped steps,
  an always-on job given a filter, a removed job, four unlisted-reader constructions and a
  non-root README that must not be reported) prove each failure. `ci_ok.py` accepts `readme` as a
  filter lane, requires the output, and refuses `core=true` without `readme=true` and
  `docs_only=true` with it; `check-workflow-gates.sh` accepts the lane name, pins
  `facade-contract=readme`, requires the `readme` output on the `changes` job and requires the
  byte-pin step, its pinned nextest, its report check and the Quickstart execution in that job.
  Their fixtures go from 160 to 170 cases and from 20 to 35 tests: a README-lane job missing from
  `JOB_LANES`, moved back to `core`, made always-on, or stripped of a step each fail. Eight of the
  new `test_ci_ok.py` tests compose the real classifier with `pr-gate.yml`'s own `needs:` and
  `JOB_LANES`, because a pull request that changes CI files runs every lane and so cannot show its
  own filter working: a README-only change runs `facade-contract` alone among the filtered jobs and
  passes `CI OK`, README with a leverage path adds only `leverage-lane`, an engine path runs every
  filtered job, and a failed or unstarted README job fails a README-only change.
  `bash .agents/gate.sh` runs the byte-pin and the executed Quickstart in every mode, `leverage`
  included, so the local mirror of a leverage PR that also edits the README covers what CI runs;
  the facade compiler contract stays in the engine lane there. Before this, GitHub #485 and #498
  (leverage, documentation and `README.md` only) ran the whole determinism lane, about 18 minutes a
  push at the time, where GitHub #493 (no README change) ran in 2. No crate source or golden
  changed.

- **The per-PR gate keeps the debug determinism cells and loses its three long jobs, which move to
  the release gate; the release gate now runs on every merge into `development`** (GitHub #505,
  issue #502; owner decision of 2026-11-09, which amends the plan's "debug and release on every PR"
  for the determinism matrix only). `pr-gate.yml`'s determinism matrix is two native cells, x86_64
  on `ubuntu-latest` and aarch64 on `ubuntu-24.04-arm`, in debug codegen, and its fan-in keeps every
  cross-architecture check: portable snapshot bytes equal, target-bound bytes different, the 46
  compiled-schedule blobs equal and 123329 bytes each, the round-trip recordings equal (92 files
  each), the replay of the x86_64 recording and the refusal of the foreign target-bound snapshot.
  `std-build-windows` and `gate` (`gate (light)`) leave `pr-gate.yml`, `CI OK`'s `needs:` and
  `JOB_LANES`. The strict-bit matrix (four cells) and its eight-capture fan-in, the wasm32 round-trip
  leg, the macOS cell and its comparison, `crate-tests`, the bare-metal cell, the leverage lane and
  every always-on job are unchanged. `gate_scripts` stays a classifier output, informational like
  `docs_only`: a gate-script path still implies `core`, and `ci_ok.py` and
  `check-workflow-gates.sh` now refuse a job filtered on it. `release-gate.yml` gains the
  determinism matrix in all four cells (both architectures, debug and release) with the full
  fan-in — the cross-architecture checks plus portable and target-bound bytes, blobs and round-trip
  recordings equal across debug and release — and the Windows cell, all in its `CI OK`;
  `gate (full)` already covers what `gate (light)` ran. It also triggers on push to `development`.
  Its concurrency group cancels only a superseded release-PR run, so for pushes the run in progress
  finishes and only the newest waiting run is kept, and a push run now checks out the pushed commit
  (`github.sha`) in every job, so the cells and their fan-in build one tree when another merge lands
  meanwhile (a push to `main` therefore tests the pushed commit too, not the branch tip at job
  start). The x86_64 determinism cells there restore the shared `nextest` rust-cache key without
  saving it, so `test-suite`'s superset entry stays the one PRs warm-start from; the arm64 cells
  save. `check-workflow-gates.sh` follows the jobs: `codegen: [debug]` exactly for the per-PR
  determinism matrix, `[debug, release]` for the release gate's and for the strict-bit matrix; the
  release-codegen cell rules, the cross-codegen comparisons and the Windows cell rules apply to
  `release-gate.yml`; the cross-architecture fan-in rules apply to both files and now include the
  blob comparison and its size check; new rules pin the release gate's push branches, the push
  checkout ref, `ref: ${{ env.CHECKOUT_REF }}` on every checkout, both concurrency lines, no
  `continue-on-error` and the two step-condition shapes; the full-suite rules are scoped to the
  `test-suite` job, because the copied cells carry look-alike lines; `pr-gate.yml` still refuses a
  `push` trigger. Its fixtures go from 129 to 160 cases, one or more failing control for each moved
  or new rule, and `test_ci_ok.py` from 18 to 20. Measured on 2026-11-09: the release-codegen cells
  took 809–1101 s (x86_64) and 653–671 s (aarch64), the Windows cell 251–450 s and `gate (light)`
  1540–1600 s, and 69 full-lane runs had a median of 18.3 minutes. What this costs, stated in
  `AGENTS.md`, `CONTRIBUTING.md` and [CI and the gate](docs/ci-and-the-gate.md#what-runs-after-the-merge):
  no per-PR job runs the determinism subset under release codegen, compares a debug artifact with
  a release one, builds on Windows or executes `.agents/gate.sh`. The author's
  `bash .agents/gate.sh` (commands unchanged; comments and closing report updated) runs the subset
  under release codegen on one host and is the pre-merge release-codegen evidence; after merging a
  change that runs the determinism lane, the release-gate run on the merge commit is watched, and a
  red run is fixed forward or reverted before the next determinism-lane merge. Product contract
  revision 24 records the evidence-delivery change in PC-041's limitation; PC-037 is unchanged.
  No crate source or golden changed.

- **`oce-api`'s integration tests build as eight binaries instead of 103, and the determinism
  subset's release-codegen build takes a sixth of the time** (GitHub #546, issue #504; the pilot of
  the owner decision of 2026-11-09). Cargo built every file under `crates/oce-api/tests/` as its own
  binary, and under the release profile (`lto = "thin"`, `codegen-units = 1`) each linked the
  dependency graph again. `crates/oce-api/Cargo.toml` now sets `autotests = false` and lists the
  targets: `facade` holds 86 files as modules and `facade_allocation_census` the 11 that call
  `allocation_counter::measure`, because naming that crate links its counting global allocator
  into a binary and so no test changes the allocator it runs under. `blob_goldens`,
  `readme_quickstart`, `conformance`, `content_id_pin`, `export_content_oracle` and
  `frame_observations` stay their own binaries: a gate step, a workflow or a documented command
  selects each by name. Module files keep their names; a helper several modules of one binary
  share is declared once in its root, and the two lines that declared it in each file became a
  comment and a `use`, so no line moved and the product contract's line anchors still hold. A test
  in a merged file is now `oce-api::facade <file>::<test>`; select one file with
  `-E 'test(/^<file>::/)'` where `--test <file>` was used. `tests/facade.rs` checks that every
  file under `tests/` is compiled into exactly one binary, with controls for a file in no binary,
  one in two and discovery left on. Measured on one 10-core arm64 Mac from a clean `target/`,
  `cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --no-run`: release 406 s to
  61 s wall (2,452 s to 391 s of CPU) and `target/` 391 MiB to 137 MiB; debug 11.8 s to 10.5 s and
  2.0 GiB to 819 MiB. The 718 tests listed before are all listed after under the same module paths,
  plus the two layout tests. No engine source, fixture or golden changed.

- **`oce-author`'s integration tests build as one binary instead of 40, and its release test build
  takes a quarter of the time** (GitHub #548, issue #504; the second crate after `oce-api`).
  `crates/leverage/oce-author/Cargo.toml` sets `autotests = false` and lists one target,
  `authoring`, of which every file under `tests/` is a module. No file needs a binary of its own:
  the crate's bless command names the package, and nothing in `.agents/`, `.github/`, `scripts/`,
  `docs/` or the tests selects a test binary of this crate by name. Of the 37 files that
  declared `mod support;`, 36 now say `use crate::support;` on the same line, with the helper
  declared once in `tests/authoring.rs`, and `import_graph_v0_refusals.rs`, which never used it,
  drops the line; no test body changed. A test in a merged file is now
  `oce-author::authoring <file>::<test>`; select one file with `-E 'test(/^<file>::/)'`.
  `tests/authoring.rs` checks that every file under `tests/` is compiled into exactly one binary,
  reading the root's declarations as the compiler does: `pub mod`, a comment after the semicolon,
  `mod` inside a block comment or a string, a `#[path]` module counted when it names a file under
  `tests/`, and a file named by two roots or by a root and a target, with controls for each, for a
  file in no binary and for discovery left on. No test sets a global allocator, an environment
  variable, a static, the working directory or a temporary file, so none changes meaning in a
  shared binary. Measured on one 10-core arm64 Mac from a clean `target/`, `cargo nextest run
  -p oce-author --locked --no-run`: release 166 s to 41.5 s wall (933 s to 132 s of CPU) and
  `target/` 219 MiB to 122 MiB; debug 15.0 s to 12.2 s and 1.0 GiB to 469 MiB. The 358 tests
  listed before are all listed after under the same file and test names, plus the six layout
  tests. No source, fixture or golden changed.

- **`oce-cxf`'s integration tests build as 12 binaries instead of 79, and its release test build
  takes about a third of the time** (GitHub #549, issue #504; the third crate after `oce-api` and
  `oce-author`). `crates/oce-cxf/Cargo.toml` sets `autotests = false` and lists the targets:
  `exchange` holds 68 files as modules, and `fixture_port_order`, `fixture_structural_oracle`,
  `export_golden`, `export_declared_input_attrs`, `composite_contract_corpus`,
  `composite_contract_doc`, `resolve_golden`, `resolve_array`, `resolve_array_expression`,
  `vendored_corpus_delta` and `parameter_grounding_corpus` stay their own binaries, because
  `.agents/gate.sh` (`binary(..)`), a document, a recorded command or the file's own bless command
  selects each by name. No selector changed. Of the 38 files that declared `mod bless;`, the 29 now
  in `exchange` say `use crate::bless;` on the same line, and the four that declared `mod render;`
  say `use crate::render;`, with both helpers declared once in `tests/exchange.rs`; no line moved,
  so the line anchors into this crate's tests still hold, and no test body changed.
  `open_modelica_reliefs_windows_boundary.rs` keeps its `#![cfg(windows)]`, which now gates the
  module: it compiles into `exchange` on Windows only, and the Windows cell in `release-gate.yml`
  runs that binary. A test in a merged file is now `oce-cxf::exchange <file>::<test>`; select one
  file with `-E 'test(/^<file>::/)'`. The layout check (`tests/binary_layout/`) is adapted from
  GitHub #551's and closes the gaps its review found: it follows the helper directories a
  declaration reaches, resolves a plain `mod` in a `#[path]` file as the compiler does, refuses a
  `cfg_attr` path and a declaration inside braces instead of guessing, counts a `cfg`-gated
  declaration as compiled on purpose, and accepts only `autotests = false` exactly. No test sets a
  global allocator, an environment variable, a static or the working directory; each golden a
  bless run writes is written by one file and read by no other, the one temporary directory is
  named for the process and used by one test, and the `git ls-files` call only reads, so none
  changes meaning in a shared binary or under plain `cargo test`, and an `OCE_BLESS=1` run of the
  merged binary rewrites every golden byte-identically. Measured on one 10-core arm64 Mac from a
  removed `target/`, `cargo nextest run -p oce-cxf --locked --no-run`: release 191 s to 60 s wall
  (1,424 s to 298 s of CPU) and `target/` 276 MiB to 123 MiB; debug 15.8 s to 8.3 s and
  1.32 GiB to 642 MiB. The 1,001 tests listed before are all listed after under the same file and
  test names, plus the eight layout tests. No source, fixture or golden changed.

- **`oce-conformance`'s integration tests build as eight binaries instead of 90, and its release
  test build takes a sixth of the time** (GitHub #550, issue #504; the last of the four crates).
  `crates/oce-conformance/Cargo.toml` sets `autotests = false` and lists the targets: `conformance`
  holds 83 files as modules, and seven stay their own binaries because a workflow, a gate step or a
  recorded command selects each by name: the strict-bit evidence tests (`strict_bits`,
  `per_block_reals_transcendental`, `per_block_reals_sources_transcendental`,
  `per_block_psychrometrics` and `per_block_utilities`, run with `--test` by the strict-bit cells
  and left out of the Windows cell by `binary_id(..)`), `blob_round_trip` and `open_modelica_nand`.
  Neither those seven files nor anything under `tests/strict_bits/` changed, and no selector
  changed: each `--test`, `binary_id(..)` and `test(=..)` selector of this crate lists the same
  tests before and after. The helpers several modules share (`block_harness`, `support`,
  `funnel_band_policy`, `sequences`, `scoped_open_modelica_line`) are declared once in
  `tests/conformance.rs`, and each module says `use crate::<helper>;`, with a comment, on the lines
  that declared it; three modules that declared `block_harness` only for their scoped OpenModelica
  helper now carry a comment there, and `g36_determinism/sequence_inputs.rs` imports its parent's
  point identities with `use super::*;` instead of `use crate::*;`. No line moved and no test body
  changed. A test in a merged file is now `oce-conformance::conformance <file>::<test>`; select one
  file with `-E 'test(/^<file>::/)'`. The layout test reuses `oce-cxf`'s check
  (`crates/oce-cxf/tests/binary_layout/`) through `#[path]` instead of copying it, so its controls
  run here too, and a second test refuses a manifest that folds one of the seven name-selected
  binaries into `conformance`. No test sets a global allocator, an environment variable or the
  working directory; the three temporary paths are named for the process and used by one test
  each; the two statics are `OnceLock` caches of data parsed from an `include_str!`; and the G36
  bless writer (`OCE_BLESS_G36`) writes one golden per sequence, so three bless runs of the merged
  binary under plain `cargo test` rewrote every golden byte-identically. Measured on one 10-core
  arm64 Mac from a removed `target/`, `cargo nextest run -p oce-conformance --locked --no-run`:
  release 270 s to 45 s wall (2,185 s to 304 s of CPU) and `target/` 452 MiB to 156 MiB; debug
  12.7 s to 9.3 s and 2.1 GiB to 668 MiB. The 338 tests listed before are all listed after under
  the same file and test names, with the one ignored test still ignored, plus nine layout tests. No
  `src/` file, fixture or golden changed.

### Documentation

- **Design note for bounding a vector run's work** (GitHub #568, issue #561), ruled by the owner
  on 2026-11-11, with no code. `docs/design/vector-work-budget.md` bounds one `run_vectors` call
  with four budgets, each a simple product:
  - frames (`max_total_frames`, kept at 2^24);
  - compared instants (`max_total_instants`, 2^28);
  - bytes loaded at bind and while scoring mutants (`max_total_load_bytes`, 2^30);
  - frames weighted by document bytes (`max_total_frame_bytes`, 2^40).

  The budgets are checked in one step at bind, after the subject's first load and its name
  resolution and before every other load, so `too-many-frames` now wins over `param-refused`. Each
  refuses with its own code. They are lower-only, as decision 35 states for every limit, and the
  receipt records none of them. The subject's parameter trials are deduplicated by distinct edit
  list. Measured on 139 library and in-repo sets and on synthetic documents up to 8 MiB, today's
  worst case is over 5 hours in `release`. At these defaults it is about one minute, and the
  corpus sits 700x to 6,500x under them. The rejected alternatives are recorded with the ten
  rulings; engine reuse through checkpoints is the follow-up #569, after part 4. The note is
  registered in `docs/README.md` and the book. `docs/design/vectors.md` is amended by the
  implementing PR, which closes #561, because a test reads it.
- **The facade migration page is retired, with the other prose the format reset made false**
  (GitHub #542, issue #531). `docs/facade-migration.md` is deleted: nothing was published to migrate from, and
  its rows still stated diagnostics schema revisions 2 and 3, descriptor revisions advancing to 2
  and an execution-profile schema "revision 2" after every format and schema went back to 1
  (GitHub #518). Its links are repointed at the pages that own the current facts: the README's two
  bullets and its documentation table, `docs/README.md`, the book summary,
  `docs/facade-contracts.md` (serialized admission and load replacement now link to
  `docs/host-responsibilities.md`) and `docs/host-responsibilities.md`. The product contract moves
  to revision 27, traceability and editorial only: PC-003, PC-028 and PC-029 ground on the compiler
  controls, the package validator and the producer-evidence section in place of the retired page,
  and its fulfilled-contraction section now states the source inspection itself, corrected to the
  tree (the only `unreachable!` arms in engine source outside tests are three invariant arms in
  `oce-blocks`; the two the retired page listed in the CXF resolver no longer exist). The
  content-id pin is renamed for what it is: `tests/fixtures/content_id/restamp.tsv` becomes
  `pin.tsv` and its owner test `content_id_restamp.rs` becomes `content_id_pin.rs` (the file's
  `# Owner test:` line moves with the name; all 47 ids are unchanged), and decision 17 of
  `docs/design/content-id.md` is struck with the ruling. `docs/design/vectors.md` decision 33
  states the version rule in place of "`0.4.0` in the tree today", an `oce-author` test comment
  drops "format 2", and `.agents/project-facts.md` lists everything a workspace version bump
  moves (`Cargo.toml`'s workspace dependency versions, `Cargo.lock`, the README sentence and the
  blob note's header table were missing). No behaviour, signature, golden or content id changes.
- **Design note for the canonical content id** (GitHub #471, issue #470).
  `docs/design/content-id.md` fixes, before any code, the one content id that replaces the
  order-bearing `cxf:fnv1a128:` export tag: SHA-256 (via `sha2`) over an `OCECANON` version 1
  encoding of the loaded flat graph, written `oce:canon1:<64 hex>` and minted only by
  `Engine::content_id`. Names are inside the id (instance namespace stripped through the last `#`,
  `/` or `:`, the sequence name kept, whole-IRI fallback rather than refusal); blocks sort by name
  with class path, class signature hash and ground parameter bits, presence being the
  evaluated-or-not flag; connections sort by target then source port; boundary outputs include
  pass-through outputs; boundaries carry type, `quantity`, `unit`, `displayUnit` and an input's
  effective domain; `SemanticPoint` records, `extends`, comments and authored order never enter it.
  It states what equal ids promise (equal outputs and per-block state on every successful frame,
  resting on the two-pass tick and the feedthrough contract; not equal blobs, point inventories or
  snapshot interchange), the canonical element-name refusal that makes sorted parameters sound, the
  surface changes (`ExportReport` loses both id methods and keeps `complete()`, blob format version
  2, compatibility descriptor revision 2 with a `content` line, PC-018/PC-021, the call sites and
  docs), the TSV mapping table for the library re-stamp, and two worked examples with exact byte
  dumps and ids (the second shows that binding a default moves the id). The architect review's
  rulings are recorded in the note. It is registered in `docs/README.md` and the book.
- **Design note for the `oce-author` importer** (GitHub #484, issue #483).
  `docs/design/importer.md` fixes, before the implementation PRs, how flat library CXF and the frozen
  prototype's v0 graph JSON enter the authoring model, and the model changes gate item 1 waits on:
  sequence parameters of an enumeration type (`"type": "SimpleController"`, references checked by
  class identity, lowered as the corpus does; 17 in-subset fixtures), optional `min`/`max` on
  boundary declarations (`@min`/`@max`, `"min": []`; 9 in-subset fixtures; input bounds are the frame
  acceptance domain the engine refuses outside of, not a clamp), and library constants
  (`OperationModes.occupied`, G36 Integer constants rather than enumerations) as member values
  resolved through one facade data table (#468, widened to the G36 enumerations and constant
  packages). A full-construct scan of the 46 G36 fixtures puts 12 outside the M2 subset (4 nested, 7
  conditional, 1 expression), not 6, so the gate denominator is 34; 31 round-trip with the decisions
  and the widened #468, and three with block-port bounds stay refused (#486). It also fixes the
  normalisation of a constant-package-typed Integer boundary, a `#[non_exhaustive]` `ImportError`
  kind with pinned codes and a fixed refusal precedence, an `ImportReport` of dropped properties
  (root-parameter bounds and units), the v0 mapping with its comment rules and three vendored golden
  pairs, the naming and re-stamp rules, the test plan and the order #468, model PR, importer PR. The
  review's rulings on its ten questions are recorded in the note. It is registered in
  `docs/README.md` and the book.
- **Design note for vocabulary v1** (M2 item 6, tracking issue #455). `docs/design/vocabulary.md`
  fixes, before any code, where the block vocabulary comes from and how it is served. A scratch run
  of modelica-json at the recorded pins showed that `-o doc` (the plan's wording) is the G36
  sequence-document generator and yields nothing for elementary classes, while
  `-o cxf --elementary --cxfCore` gives a byte-stable `CXF-Core.jsonld` with a description on every
  connector and public parameter; the note proposes vendoring it at the existing pins and
  generating from it with a regenerate-and-diff test, walking the catalog's names. It describes 262 of 265 named ports,
  all 57 vector connectors and 183 of 186 parameters of the 133 classes, leaving six to curate. The
  data is two strict-JSON files (`generated.json`, `curated.json` with typed notes) in a new
  leverage crate `oce-vocab`; `oce-api`, its catalog JSON, the content id and class signature hashes
  are unchanged. `oce catalog` serves the block vocabulary and `oce vocab` gains a time-only
  scale-factor table, so `1 h` becomes legal with exact scaling and an author-only
  `unit-suffix-mismatch` check (amending authoring decision 13). Gate item 4 (agent-usable) is
  deferred to the milestone that lands the benchmark harness (owner-confirmed), and M2 closes on
  items 1 to 3. The review's rulings on its ten questions are recorded in the note. It is registered
  in `docs/README.md` and the book.
- **Design note for vectors, the verify replayer and receipts** (GitHub #512, issue #508).
  `docs/design/vectors.md` fixes, before any `oce-verify` code, the one vector format
  (`oce/vectors/v1`, changed in place until 1.0.0 by the owner's ruling of 2026-11-09: no importer
  from the old shape, no dual-read path), how a set is bound to a loaded sequence and replayed on
  `oce-replay`, the vector receipt body under the header shared with the commissioning verifier,
  mutant mode and its integer score, the complete error-code table, the `oce verify` verb and the
  in-repo fixtures to rewrite: 37 numbered decisions with their rejected alternatives and a
  normative receipt field table. The owner ruled on 2026-11-09, accepting all eleven questions the
  draft put as recommended, and the rulings are recorded in the note (Real `equals` by bits,
  tolerance names, expected state deferred, warnings
  reported and not failed, a refused frame failing its scenario, the mutant score not gating
  `passed`, the AHU-0016 mutants, zero timings by default, the argument shape, where the in-repo
  replay gate runs, and #460's `FrameSlots::output_types()` moving into M3 so outputs can be typed
  before any frame runs). Checked against the
  tree, it records where the code overrules the issue's outline: a run-time warning carries a class
  path and a message but no code and no instance, so a diagnostic expectation can match only those;
  `Grid` replay runs to the last source row, so a step after the horizon is refused; the replay
  name rule is private and input-only; the catalog has no Real `>=` class. The "vector format v2"
  wording in `docs/design/replay.md`, `docs/design/commissioning.md` and the README's M3 roadmap
  row becomes "the vector format". Nothing is implemented. It is registered in `docs/README.md`
  and the book.
- **The published standard is named, its clauses are cited beside the OBC sections, and the
  coverage claim says what the engine does not execute** (GitHub #539, issue #510; owner rulings
  of 2026-11-09). CDL is published as ANSI/ASHRAE Standard 231-2026, and the README, the
  architecture page and `docs/cdl-coverage.md` now say so. This project has read the standard's
  published table of contents, not its text, and claims no conformance to it: every clause number
  of the standard in the documentation comes from that table, and what the documentation says a
  section says is quoted from the public OpenBuildingControl (OBC) specification. Where a page
  cited "CDL §7.17" for the metadata seam it now cites OBC §7.17 beside clause 5.13 of the
  standard, and says the engine applies that section's rule (none of the metadata affects the
  computation of a control signal) to point lists, trends, tags and semantic records. The
  commissioning verifier follows OBC §12; the standard's table of contents lists no verification
  or conformance clause. The architecture page's seam heading and the diagram's label drop the
  bare section number (the heading's anchor changes; the three design notes that link to it
  follow). The README's "CDL §7.1 resolver" loses a number that was never CDL's, and "CDL §7.7.2"
  and "CDL §7.16" on the architecture and facade-contracts pages become OBC citations.
  `docs/cdl-coverage.md` gains two sections. The first records the clauses, that the standard's
  public page (`data.ashrae.org/standard231`, read 2026-11-09) bases its normative elementary
  blocks on Modelica Buildings Library v12.1.0, that the file list under
  `Buildings/Controls/OBC/CDL` is identical at tag `v12.1.0` and at the vendored pin `a131864`
  (checked 2026-11-09; contents not compared), and that the standard's CXF namespace is
  `http://data.ashrae.org/S231#` on the evidence of OBC §11.3 and modelica-json's
  `CXF-Core.jsonld`, while the `oce-cxf` exporter and the `oce-author` emitter still bind
  `http://data.ashrae.org/S231P#` until issue #527. The second states that the engine executes
  elementary and composite blocks only: an extension block is implemented by an FMU (OBC §7.13
  and §8.7; clauses 5.9 and 6.7 of the standard), FMI is out of scope before 1.0, and an instance
  of an unregistered class with no children is refused with `class-not-found` (read from the
  resolver; no fixture exercises an extension block, and a dedicated refusal code with fixtures
  is issue #537). Documentation only: no behaviour, signature, fixture or golden changes. Left
  for issue #527: `docs/cxf-composite-subset.md` (an engine test reads it), the bare §7.10,
  §7.4.1 and §8.2 citations on other pages, and the citations in crate rustdoc, `Cargo.toml` and
  `AGENTS.md`.

## 0.4.0

Prepared 2026-11-08 on `development`; the release date is the date the `v0.4.0` tag is cut from
`main`. Every workspace member is 0.4.0. Nothing is published to crates.io.

0.4.0 closes milestone M1b: the portable `no_std` core. The core four (`oce-model`, `oce-expr`,
`oce-blocks`, `oce-graph`) build `no_std + alloc` behind a default `std` feature, and their
workspace entries carry `default-features = false` (GitHub #428, issue #397); every `f64` operation
`core` lacks goes through one `libm`-backed `math` module per crate, and clippy refuses the `std`
float methods there (GitHub #431, issue #399); the core is hash-free, and no hash-collection order
reaches a golden, a receipt or snapshot bytes (GitHub #433, issue #398). A loaded executable encodes
on the host into the canonical `OCEB` compiled-schedule blob, and `oce_graph::blob::decode` decodes
it on a `no_std + alloc` target under the same engine version, refusing hostile bytes with typed
errors (GitHub #438, issue #400). The HostTick v1 frame driver moved into the core as
`oce_graph::frame`, and `oce_graph::blob::Executable` is the `no_std` blob execution entry that runs
a decoded blob through it with zero allocations per frame (GitHub #441, issue #401). The
`no-std-build` job became the bare-metal gate cell: the core builds, lints and passes a `cargo tree`
std scan on `thumbv7em-none-eabihf`, `riscv32imac-unknown-none-elf` and `wasm32-unknown-unknown`,
and the `oce-firmware-link` image links for the two bare-metal targets within a size budget per PR
and runs under QEMU nightly (GitHub #449, issue #402). The blob round trip records one host run of
the 46 G36 fixtures' blobs and replays it bit for bit on x86_64, arm64 and `wasm32-wasip1` per PR
(GitHub #451, issue #403). The workspace has 13 members, 10 publishable and three private. The
fixture and Tier-A provenance goldens (`tools/golden-gen/goldens`) are byte-identical to the
`v0.3.0` tag; the blob goldens move only by the engine-version bytes and their CRC (see Release).
The design notes for the M2 authoring layer and the three passes M3 waits on (historical replay,
the commissioning verifier, `oce-diff`) are written (GitHub #442, #443, #446, #447).

### Release

- **Workspace version 0.4.0** (GitHub #452, issue #404). All 13 members and the internal
  path+version table move from 0.3.0. `CompatibilityDescriptor` reports `oce-api-version:0.4.0`,
  so persisted descriptor receipts and replay records from 0.3.0-labelled builds refuse with a
  `Build` mismatch, as the fail-closed policy requires; the `compatibility.txt` and
  `compatibility_export.txt` fixtures, the `CompatibilityDescriptor` doctest and the descriptor
  refusal tests that name the current or a neighbouring version (`compatibility_tests.rs`,
  `replay_codec.rs`) move from 0.3.x to 0.4.x as GitHub #427 did for 0.3.0. New with this release:
  the `OCEB` header carries the encoder's engine version and a decoder accepts only its own, so the
  46 blob goldens under `crates/oce-api/tests/fixtures/blob` are re-blessed with
  `OCE_BLESS_BLOB=1`. In each, exactly two things change: the engine-minor byte at offset `0x0a`
  (3 → 4) and the 4-byte CRC-32C trailer; every other byte, every blob length, the class-signature
  table and the per-fixture run digests (`blob_frame_digests.tsv`) are unchanged.
  `the_worked_example_has_the_design_note_layout` pins version bytes `0, 0, 4, 0, 0, 0`, and the
  design note's worked example shows engine 0.4.0 with its CRC-32C recomputed. The
  `check-no-std.sh` scan fixtures name `v0.4.0`. `tools/golden-gen/goldens` and every numerical
  golden are byte-identical. No catalog, contract-schema, state-format, execution-ABI,
  blob-format or replay-format revision changes. The v0.3.0 sync-back merge (GitHub #430) carried
  no change of its own.

### Execution core

- **One HostTick v1 frame driver, in the core, and the `no_std` blob execution entry**
  (GitHub #441, issue #401; design note decision 11). The frame around `eval_tick` — time checks
  (finite, `|t| ≤ MAX_FRAME_TIME_MAGNITUDE`, nondecreasing, a finite step, representable by every
  stateful block), per-slot input checks (exact type, then the non-finite policy, then the declared
  domain), the accepted-frame sequence, staging into every fan-out target, the single evaluation
  and the previous-time update — moves from `oce-api` into `oce_graph::frame` as a slot-indexed,
  name-free `FrameDriver` with a typed `FrameRefusal`. `MAX_FRAME_TIME_MAGNITUDE` and
  `NonFiniteInputPolicy` move with it; `oce-api` re-exports both, so host code is unchanged. The
  facade keeps name resolution, the unknown/duplicate/missing refusals, the prepared plan and its
  incarnation token, the load and pending-edit preconditions, the durable-restore window, replay
  records and the owned `CompletedFrame`; its refusal order, goldens, replay records, snapshot
  bytes and per-frame allocation census are unchanged, and every existing facade test passes
  unmodified. New: `oce_graph::blob::Executable`, the `no_std + alloc` entry a bare-metal or WASM
  target links — `from_blob(&[u8])`, the I/O table with `input_slot`/`output_slot`,
  `execute_frame(time, &[Value], diagnostics)` by slot through the same driver (default policy
  `Admit`, as on the facade), borrowed outputs by slot, `target_bound`, and the state as raw words
  (`state_words`, `restore_state_words`, refused with a typed `StateWordsRefusal` on a wrong length,
  non-initial words before the first frame, or a region its block's state contract rejects). Its
  rustdoc example is a `no_implicit_prelude` firmware-shaped module over a blob golden,
  compile-tested. Evidence: the dual-path test now runs every G36 fixture's 160 frames through the
  facade, the core driver on a second load and the blob entry, plus the same bad frames (NaN time,
  time going backwards, a wrong type, a non-finite Real under `Refuse`, a value above a bound), and
  compares outputs, refusals, clock, sequence and the whole `RunState` bit for bit after every frame
  and refusal; the facade's run must reproduce a per-fixture digest table
  (`crates/oce-graph/tests/fixtures/blob_frame_digests.tsv`) that `oce-graph`'s new
  `blob_execution` test checks against the entry built `no_std + alloc`, in the same census binary
  that requires zero allocations over all 7,360 frames (decode counted, 6,013 allocations across
  the 46 goldens). `frame_driver_ownership.rs` refuses a facade source that re-implements a frame
  rule; `scripts/ci/check-no-std-entry-closure.sh` requires the `no_std` `oce-graph` closure to hold
  no `oce-cxf`, `oce-api`, `serde_json` or `std`, with controls; both new steps run in
  `no-std-build` and `bash .agents/gate.sh`. The [product contract](docs/product-contract.md)
  advances to revision 20 (grounding and evidence anchors only).
- **The `OCEB` compiled-schedule blob: `Engine::compile_blob` encodes the loaded executable and
  `oce_graph::blob::decode` decodes it on a `no_std + alloc` target** (GitHub #438, issue #400;
  format and decisions in `docs/design/compiled-schedule-blob.md`, contract in
  `docs/compiled-schedule-blob.md`). The blob carries every class with its signature hash, every
  instance with its ground parameters in table order, the frozen block order, the connections and
  the boundary I/O table (typed acceptance domains, fan-out targets, opaque `SemanticPoint` payloads
  that the decoder bounds and never reads), the facade content id (`None`, 32 zero bytes, for the 13
  partial exports) and the engine version, in one canonical little-endian byte order with a CRC-32C
  trailer; no run state and no architecture field. The decoder refuses with a typed `BlobError` —
  never a panic — in a documented order, bounds every count against the remaining bytes before
  allocating, re-instantiates each block from its own registry, checks arity, kind, state layout,
  connection types and directions, the I/O table (finite, uncrossed bounds), emit-before under its
  own `feeds_through`, `MAX_STATE_WORDS` and each block's initial-state CRC, and allocates exactly
  what its module documentation lists (pinned per fixture). A port list longer than any class
  resolves (the new `oce_blocks::MAX_RESOLVED_ARITY`, twice `MAX_RESOLVED_PORT_WIDTH`) is refused
  before anything is sized by it, and connector ownership costs 8 bytes per id until every arity is
  checked, so peak memory is linear in the blob: a hostile million-id port list is refused at 2.0×
  the blob length, and the densest accepted blob holds 26.6× (pinned with bounds). `oce-blocks` gains `class_signature_hash` (FNV-1a-64
  over a canonical class record; a derived default's formula text is excluded) and
  `class_is_target_bound`, both on `CatalogEntry`; the 15-class target-bound list moved there from
  `oce-api`, which now reads the flag (snapshot bytes unchanged). `OcError::Blob` and the blob error
  types are re-exported from `oce-api`. Evidence: 46 blob goldens and the per-class hash table under
  `crates/oce-api/tests/fixtures/blob/`, re-encode and decode→re-encode byte equality, an
  independent test-side writer, the determinism matrix comparing blob bytes across x86_64/aarch64
  and debug/release, a facade-versus-decoded dual-path test comparing every boundary output and the
  whole `RunState` after every tick for all 46 fixtures over about 3.3 hours of model time (with a
  negative control and per-fixture output-variation floors), one typed-refusal test per defect, a
  10,000-case byte-mutation property test, a §7.17 payload-mutation seam test, and a
  hostile-parameter fuzz over every registry constructor. The determinism fan-in also checks each
  blob artifact's size, and `bash .agents/gate.sh full` writes the artifact as the cells do. The
  [product contract](docs/product-contract.md) advances to revision 19 with requirement PC-041 for
  the blob.
- **`oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` build `no_std + alloc` behind a default
  `std` feature** (GitHub #428, issue #397). Each has `default = ["std"]`, `std = ["alloc", …]` and
  `alloc = []`; `--no-default-features --features alloc` builds it `#![no_std]` on `core` and
  `alloc` alone, and a build with neither feature is refused by a `compile_error!`. The default
  build is unchanged: goldens, the cross-architecture portable snapshot and the allocation census
  are byte-identical — the portable snapshot digest stays `2d21ed57…` (SHA-256) and the frame
  allocation census is identical for all five fixtures (arithmetic, delay, feedback, controller,
  assertion). Errors derive `core::error::Error` through `thiserror` with its default
  features off. The workspace `serde` and `thiserror` entries drop their default features (each std
  crate names `std`, and `rc` for `serde`, itself), and the four crates' workspace entries carry
  `default-features = false`, so every std crate that depends on one names `features = ["std"]`.
  `f64::{floor, ceil, trunc, round, sqrt}` are not in `core` on Rust 1.97: every call in the four
  goes through `libm` in all builds — exact IEEE 754 operations, checked against the `std` methods
  bit for bit over boundary values, a 2^20 bit-pattern sweep and a 2^18-case half-integer sweep
  (`oce-blocks/tests/libm_std_parity.rs`) — and `f64::rem_euclid` through a helper that evaluates the standard library's own expression.
  `oce-expr` now depends on `libm`. Two differences without `std`: `oce_blocks::catalog()` is absent
  (its cache is a `OnceLock`), and `oce-expr`'s evaluation-fuel meter is one program-wide slot built
  from `core` atomics instead of a thread-local, so a `no_std` host evaluates from one execution
  context at a time. A `no-std-build` job in the determinism lane of `pr-gate.yml` builds the four
  for `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`, runs clippy `-D warnings` on the first
  and the `oce-expr` unit suite without `std`; `.agents/gate.sh` runs the same commands and fails
  with the `rustup target add` command when a target is missing.
- **Every `f64` operation `core` lacks goes through one `math` module per crate, and clippy refuses
  the `std` float methods in the core four** (GitHub #431, issue #399). `oce-blocks` and `oce-expr`
  each gain a private `math` module re-exporting the pinned `libm` functions (and the
  `rem_euclid` helper, moved there from `source_timetable.rs`); every former `libm::` call in the
  four goes through it, calling the same function, so goldens and the strict-bit captures are
  unchanged. The root `clippy.toml` lists all 37 `std`-only `f64` methods (and their `f32` twins)
  under `disallowed-methods`, each found by a `no_std` probe of the 1.97.1 toolchain — `core` has
  only `abs`, `signum`, `copysign`, `min`, `max`, `clamp`, `recip`, `to_degrees`, `to_radians`,
  `total_cmp` and `midpoint`; `mul_add`, `powi`, `fract` and `div_euclid` are `std`. The
  workspace lint table allows the lint and the four crate roots deny it, so it binds each core
  crate's `src/` tree, unit tests included, while `std` crates and `tools/golden-gen` (whose
  manifest allows it) keep their `std` methods; files under `tests/` are separate crates outside
  the lint, deliberately, because that is where the `std` methods serve as the oracle
  (`libm_std_parity.rs`). Three test-only `std`
  calls move to `math` or an exact constant. `scripts/ci/test-math-lint.sh`, run by the
  `no-std-build` job and `.agents/gate.sh`, re-probes the table, checks the list is total and
  the lint scoped (a core `tests/` file naming `libm::` must be a declared `std` oracle), and
  requires an `x.sin()` mutation in oce-blocks and an `f.sqrt()` mutation in oce-expr to fail clippy.
  `sqrt` stays on `libm` in every build (currently its software routine): 2.3 ns per call slower than `f64::sqrt` on Apple M5, but
  no frame measurement separated it from noise ([benchmarks](docs/benchmarks.md)).

### Verification

- **The blob round trip: blobs encoded on x86_64 run bit-identically on arm64 and on
  `wasm32-wasip1`, per PR** (GitHub #451, issue #403). A new `oce-conformance` suite,
  `tests/blob_round_trip.rs`, loads each of the 46 G36 fixtures through the facade, encodes its
  blob, decodes it with `oce_graph::blob::Executable` and runs the shared 160-frame schedule through
  both, comparing every boundary output by raw bits, every warning in order (source, message, time
  bits) and the raw state words after load and after every frame; it also pins each blob to its
  golden and the entry's run to `blob_frame_digests.tsv`. `OCE_ROUND_TRIP_OUT` records the blobs
  and expected results as a documented, line-oriented artifact (format version 1);
  `OCE_ROUND_TRIP_EXPECT` decodes and runs a recording's blobs on another target and compares the
  run with the recorded one. The reader bounds every file and count and refuses malformed input
  with a typed error naming the file and line, never a panic. In `pr-gate.yml` every determinism
  cell records, `determinism-compare` requires the four recordings identical and replays the
  x86_64 debug recording natively on aarch64, and a new `blob round trip (wasm32-wasip1 under
  wasmtime)` job in the determinism lane replays it on `wasm32-wasip1` under wasmtime 50.0.1,
  installed by `scripts/ci/install-wasmtime.sh` against pinned archive and binary SHA-256 digests
  and run through a cargo target runner (`.cargo/config.toml`, `scripts/ci/wasmtime-runner.sh`).
  `CI OK` requires all three legs. The controls flip one bit of a recorded output, state word
  (after load or after a frame), warning time or blob, change or remove a recorded warning, and
  record `-0.0` or another NaN payload against the produced Real; each must be reported with the
  fixture, frame, slot or index and both bit patterns.
  `bash .agents/gate.sh roundtrip` (also part of `full`) records and replays on the local host and
  on `wasm32-wasip1`, failing with the install command when the target or wasmtime is missing;
  `release-gate.yml`'s `gate (full)` installs both. New facade accessor: `Engine::state_words`
  borrows the raw state words in the layout `Executable::state_words` returns — a verification
  view, not a continuation format. Product contract revision 22 (PC-041 cites the round trip).
  Goldens byte-identical; no core-four or `oce-cxf` source changed.

- **The bare-metal gate cell: the core builds per PR for two bare-metal targets and links into a
  firmware image held to a size budget, which runs under QEMU nightly** (GitHub #449, issue #402).
  `pr-gate.yml`'s `no-std-build` job becomes the bare-metal cell. `scripts/ci/check-no-std.sh`
  builds and lints `oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` `no_std + alloc` on
  `thumbv7em-none-eabihf`, `riscv32imac-unknown-none-elf` (new) and `wasm32-unknown-unknown`, then
  scans each crate's `cargo tree` per target and fails on any `std` feature or host-only crate;
  `scripts/ci/test-check-no-std.sh` proves it fails on `extern crate std;` in `oce-graph` and on a
  `serde/std` that still compiles on wasm32. The new unpublished member `oce-firmware-link`
  (`firmware/link`) embeds the `multizone_vav_supply_fan` blob golden, decodes it with
  `oce_graph::blob::Executable` and runs the shared 160-frame schedule, printing a run digest per
  frame and the verdict against `blob_frame_digests.tsv`; it has no `unsafe` code.
  `scripts/ci/check-firmware-link.sh` builds its `blob-frames` image for both bare-metal targets
  with the new `firmware` profile and holds `.text`/`.rodata`/`.data`/`.bss`, flash and RAM to
  `firmware/link/size-budget.toml` through `scripts/ci/firmware_size.py` (copied with its tests from
  rusty-bacnet); its host tests require the image's schedule to equal `oce-graph`'s test schedule
  bit for bit and every golden to reach its table digest through the image's code. The new
  `qemu.yml` (nightly and dispatch, outside `CI OK`) runs the images on QEMU's `netduinoplus2` and
  riscv32 `virt` machines and requires every line to equal the host reference runner's; both
  targets reproduce the host digests. `bash .agents/gate.sh nostd` and `bash .agents/gate.sh qemu`
  run the cell and the emulation check alone. The [product contract](docs/product-contract.md)
  advances to revision 21: PC-041's limitation and evidence now cite the bare-metal image.
  Goldens are byte-identical.
- **No hash-collection iteration order reaches a golden, a receipt or snapshot bytes** (GitHub
  #433, issue #398). `oce-api/tests/hash_order_independence.rs` loads the 46 G36 fixtures and the
  resolver-contract document on 20 freshly spawned threads, each with independently seeded
  `RandomState` hashers, exports, runs four frames and snapshots. Export bytes, content id, load and
  export receipts, `input_definitions()`, point inventory, resolved model, schedule, topology,
  frame outputs and snapshot bytes must agree across all 20. Four refused documents pin the order of
  load errors and failure receipts the same way. A synthetic `HashMap`-order emitter run through
  the same harness must diverge, so the test cannot pass by reusing one seed; swapping the
  schedule's min-heap, the `input_definitions()` sort or the resolver's diagnostic sort for hash
  order each fails it. `clippy.toml` gains a `disallowed-types` list (`std`'s `HashMap`,
  `HashSet`, `RandomState`, `DefaultHasher`, `BuildHasherDefault`), allowed workspace-wide and
  denied in the roots of `oce-model`, `oce-expr`, `oce-blocks` and `oce-graph`, so it binds only
  those four; `oce-api/tests/core_crates_hash_free.rs` checks that wiring and also refuses
  `hashbrown`, `ahash`, `indexmap` and similar crates there. `clippy::iter_over_hash_type` is
  denied workspace-wide. The audit of `oce-api`, `oce-cxf`,
  `oce-validate` and `oce-store` found every output already canonical. Two resolver
  single-assignment refusals now also push in connector / IRI order (they were already sorted
  before leaving the resolver). `MemStore::list_models` now returns keys sorted rather than in
  `HashMap` order. Goldens are byte-identical. Method-call iteration as a clippy error follows in
  issue #432.

### Documentation

- **Design note for the structural diff (`oce-diff`)** (GitHub #447, issue #437).
  `docs/design/diff.md` fixes, before any code, how two revisions of an authoring model are
  compared: the authored block name is the identity, a class change is a removal plus an addition,
  and a bounded, deterministic rename pass (class and bit-exact parameters as hard gates,
  neighbourhood agreement, mutual-unique acceptance, `exact` or `high` confidence, caps of 2,048
  blocks per side and 65,536 pairs) reports renames without ever affecting `graph_equal`. It
  defines the closed change categories, a parameters section that keeps site retunes apart from
  graph edits, the schema-versioned serde `Diff` with its text and Markdown renderings, the thin
  CXF adapter over `Topology`, same-or-different blob comparison, and `oce diff` exit codes under
  the one `oce` CLI rule (0 yes, 1 no, 2 error). Three worked examples (a site retune; added,
  removed and rewired blocks with one rename; a wholesale rename the diff cannot fully match) show
  the JSON and text output. `docs/design/authoring-layer.md`'s CLI exit codes are aligned with the
  same rule (a parse refusal and an I/O failure exit 2, no exit 3), with a dated review ruling. The
  note is registered in `docs/README.md` and the book.
- **Design note for the commissioning verifier (CDL §12)** (GitHub #446, issue #436).
  `docs/design/commissioning.md` fixes, before any code, the tolerance mode of `oce-verify`: a
  strict-JSON superset of CDL §12.7 Listing 12.1 (LBL specs load unchanged, comments stripped),
  Listing 12.2 point mapping with a closed `units/v1` conversion table outside the engine, a
  faithful Rust port of LBNL funnel's tube construction proven by a differential test against
  `pyfunnel` 2.1.1, §12.7.2 indicator zeroing, zero-order-hold comparison sampling, per-output
  `pass` / `fail` / `no_data` reports, a receipt sharing its header with the M3 vector receipt, and
  `oce verify --commission`. Its worked example uses the Building 33 cooling-coil trends that the
  Modelica Buildings library ships. It states the owner's four decisions, six accepted
  recommendations and sixteen more settled in review, among them exact output names (a key with `*`
  or `?` is an anchored glob) and the `oce` exit-code convention (0 pass, 1 not passed, 2 error).
  The note is registered in `docs/README.md` and the book.
- **Design note for the historical replay API** (GitHub #443, issue #435). `docs/design/replay.md`
  fixes, before any code, the `oce-replay` leverage crate that every consumer (`oce-verify`,
  `oce-py`, `oce-wasm`, `oce-host`, the commissioning verifier) uses to drive the engine from
  recorded or scripted inputs: slot-indexed frame sources after a one-time name-to-slot mapping
  (columnar arrays, a bounded CSV with a column map, step lists), the `Recorded` and `Grid` time
  bases, a missing-sample policy that refuses and stops by default and records every substitution,
  a columnar bit-exact trace (89 B per frame for the G36 controller fixture's outputs) and a
  resumable `advance` cursor for the Python GIL and the browser worker. It states the owner's four
  decisions, six accepted recommendations and thirteen more settled in review, among them the
  `csv` crate inside the crate's own caps; #444 and #445 implement it. The note is registered in
  `docs/README.md` and the book.
- **Design note for the authoring layer (M2)** (GitHub #442). `docs/design/authoring-layer.md`
  fixes, before any code, what `oce-author` and the `oce` CLI are: leverage crates above the facade
  that never evaluate or schedule and reach the engine only as CXF through `Engine::load_cxf`. It
  covers the flat single-root authoring model, the terse text form (grammar, canonical printer,
  parse bounds), the strict-JSON wire form with a `schemars`-derived schema, a two-stage validator
  (author-side checks, then emit and load in a throwaway `Engine`) with a finding-code parity table,
  the fixed-layout deterministic emitter and the CLI's commands, exit codes and JSON output. It
  states the owner's nine decisions and twelve more settled in review, among them a wire `value`
  that is a number, boolean, string, `{"ref"}` or `{"member"}`, and `clap` for the CLI.
  `docs/architecture.md` now limits the no-binaries rule to the engine crates, so leverage crates
  may ship binaries above the facade; the note is registered in `docs/README.md` and the book.
- The contributor-facing docs no longer describe the retired previous host by name (GitHub #439).
  `AGENTS.md`, `CONTRIBUTING.md`, `.agents/project-facts.md` and `docs/ci-and-the-gate.md` keep
  the one fact that still matters, that pull-request numbers from before the October 2026 move
  share numbers with GitHub PRs so merged PRs are cited `(GitHub #N)`, and `CONTRIBUTING.md` now
  names the macOS and Windows observation cells beside the Linux runners.
- The README states where the project is and where it is going (GitHub #434): a status and
  roadmap section (v0.3.0 as M1a, M1b in progress toward v0.4.0, M2–M9 as planned), the
  `no_std + alloc` core and its math and collection discipline under "What it does today", the
  per-PR native strict-bit evidence in place of the retired retained receipt, and documentation
  table rows for the product contract, state compatibility, strict-bit evidence, facade migration
  and the design notes.

## 0.3.0

Prepared 2026-11-08 on `development`; the release date is the date the `v0.3.0` tag is cut from
`main`. Every workspace member is 0.3.0. Nothing is published to crates.io.

0.3.0 closes milestone M1a: cleanup and a pure `Engine`. What left: the compatibility ledgers and
the public-api surface gate (GitHub #412, issue #388); the reserved `oce-flatten`, `oce-extension`
and `oce-docs` crates (GitHub #414, issue #389); the `Store` type parameter on `Engine`, the three
load-time store calls and the verification-only `oce-reference-wal-adapter` (GitHub #415, issue
#390); the separate `oce-store-mem` crate, now `oce_store::MemStore` behind `oce-store`'s default
`mem` feature (GitHub #417, issue #391); and QEMU, with the digest-bound strict-bit receipt
(GitHub #413, issue #392). What became pure: `Engine::new()` builds a plain struct, `load_cxf`
returns `(LoadReport, LoadedModel)` as plain data and calls no storage, `oce-api` has no storage
dependency, and a host that persists does so itself through `oce_store::host::persist_loaded_model`.
The workspace has 12 members, 10 publishable and two private. What CI runs where: the per-PR gate
runs once per change, on the PR (GitHub #411, issue #408); the determinism and strict-bit matrices
run in four native cells each — x86_64 on `ubuntu-latest` and aarch64 on `ubuntu-24.04-arm`, debug
and release — so the slowest per-PR job dropped from 31m33s (run 42439645194) to 12m40s (the x86_64
release determinism cell in run 42482063762); a docs-only PR compiles nothing and finishes in 2m40s
(run 42476084422) and a leverage-only PR in 56s (run 42476081207) (GitHub #418, issue #393); a
macOS arm64 cell checks the portable state snapshot is byte-identical to both Linux architectures'
and a Windows cell builds the workspace and runs its 2,532-test suite (GitHub #425, issue #394); and
the docs jobs cache the verified mdBook binary (GitHub #426, issue #416). The release also carries
the load-time and frame-boundary hardening of GitHub #341 through #386. Fixture, Tier-A provenance,
projection and state-format goldens are byte-identical to the pre-M1a `development` head.

### Release

- **Workspace version 0.3.0** (GitHub #427, issue #395). All 12 members and the internal
  path+version table move from 0.2.0. `CompatibilityDescriptor` reports `oce-api-version:0.3.0`,
  so persisted descriptor receipts and replay records from 0.2.0-labelled builds refuse with a
  `Build` mismatch, as the fail-closed policy requires. The three places that pin that descriptor
  line move with it, one line each, as GitHub #333 did for 0.2.0: the `compatibility.txt` and
  `compatibility_export.txt` fixtures under `crates/oce-api/tests/fixtures` and the
  `CompatibilityDescriptor` doctest. The descriptor refusal tests that name the current or a
  neighbouring version (`compatibility_tests.rs`, `replay_codec.rs`) move from 0.2.x to 0.3.x
  likewise, keeping each case's relation to the current version. These are descriptor text, not
  numerical goldens: `tools/golden-gen/goldens` and every data golden are byte-identical. The
  compiled-schedule blob note's worked example shows engine 0.3.0, with its CRC-32C recomputed.
  No catalog, contract-schema, state-format, execution-ABI or replay-format revision changes.
- **The retained strict-bit receipt binding was retired, not re-captured** (GitHub #413, issue
  #392). The per-PR eight-cell native matrix is the standing strict-bit evidence; `receipts.json`
  is a log of the last green native run, not a gate. M1b (issue #406, closeout issue #404)
  follows: the `no_std` core and the compiled-schedule blob.

### Host facade

- **The engine is pure: `Engine` loses its `Store` type parameter and the three load-time store
  calls, and `load_cxf` returns the resolved model as plain data** (GitHub #415, issue #390). `Engine` is a
  plain struct built by `Engine::new()` (or `Default`); `Engine<S: Store = MemStore>`,
  `Engine::in_memory`, `Engine::with_store`, `Engine::store`, the `oce_api::oce_store` re-export,
  `OcError::Store` and `DiagnosticStage::{StoreRecovery, StoreSave, StoreInputs}` are removed.
  `oce-api` no longer depends on `oce-store` or `oce-store-mem`, and load no longer calls
  `recover`, `save_model` or `resolve_points`, so a refused load has no external effect to
  compensate. `load_cxf` returns `(LoadReport, LoadedModel)` and `load_cxf_with_receipt` returns
  `(LoadReceipt, LoadedModel)`: a `#[non_exhaustive]` plain-data value carrying `model_id`, the
  serde-serializable `ResolvedModel` projection, the `IoInventory`, the semantic warnings and every
  load diagnostic, plus `input_points()` as `(canonical path, value type)`. The resolved-model DTOs
  (`DomainKey`, `ResolvedModel`, `PointDto` and the rest) move verbatim from `oce-store` to
  `oce_model::resolved_model`; `oce-store` re-exports them unchanged and `oce-api` re-exports them
  as `oce_api::DomainKey`, `oce_api::ResolvedModel` and `oce_api::resolved_model`, so neither crate
  depends on the other and their serde bytes are identical. A resolved-model projection
  inconsistency, formerly `OcError::Store`, is now `OcError::Load` with detail prefixed
  `model projection failed:`. The diagnostics schema is revision 3: `Export` moves from rank 10 to
  rank 7 and every other stage keeps its rank. The snapshot restore window (`restore_state` only
  before the first mutation) is snapshot-codec state, not storage, and is unchanged. The
  verification-only `oce-reference-wal-adapter` crate, whose subject was the removed engine
  storage lifecycle, is deleted: the workspace has 13 members, 11 publishable and two private, and
  the release workflow's comment and approval digest are re-pinned. `check-default-no-db.sh` and
  the package validator now refuse any `oce-store*` crate in `oce-api`'s normal graph, the facade
  compiler contract proves each removed name no longer compiles, and `frame_purity.rs` checks the
  manifest, lockfile closure and sources. Load allocates less (the in-memory store's model copy is
  gone); frame and snapshot allocation is unchanged. Product contract revision 18 narrows the
  Store claims. Fixture, provenance, projection and state-format goldens are byte-identical.
- **The reserved `oce-flatten`, `oce-extension` and `oce-docs` crates are deleted, and `oce-api`
  no longer declares the no-op `mem` feature** (GitHub #414, issue #389). The workspace has 14 members, 11
  publishable and three private. `oce-flatten::flatten` returned its input unchanged — the `oce-cxf`
  resolver already lowers composites, grounds parameters and normalizes array parameters — so the
  load pipeline now goes straight from import to attribute unification rather than keeping a no-op
  stage. That removes `OcError::Flatten` and `DiagnosticStage::Flatten`; the diagnostics schema is
  revision 2, whose stage ranks are 0..10 with every remaining stage in the same relative order.
  `oce-extension` (an unconsumed FMI DTO) and `oce-docs` (an `unimplemented!` exporter) had no
  dependents. `oce-api` has no features section: `cargo build -p oce-api` and
  `--no-default-features` build the same thing, and a manifest naming `features = ["mem"]` must drop
  it. The facade compiler contract now checks the single default selection, the package validator
  expects no `oce-api` features and one selection, and the release workflow's comment and approval
  digest are re-pinned for the new 11/3 split. The architecture page, both diagrams, the
  crate lists and the CI notes are updated. Fixture and provenance goldens are byte-identical.
- **Frame times are bounded so elapsed time cannot overflow, and non-finite bounds are refused
  at load** (GitHub #382, issues #366, #372). Finite times whose difference overflowed (t = -1e308
  then +1e308) were accepted by a model without sampled blocks and gave an integrator an infinite
  `dt`, while a model with a sampled block refused them, so acceptance depended on model contents.
  Finite steps alone would not close it: -1e308 → 0 → 1e308 left a Timer reporting an infinite
  elapsed time. Preparation and execution now refuse any time whose magnitude exceeds the new
  `MAX_FRAME_TIME_MAGNITUDE` (`f64::MAX / 4`) with the new `OcError::TimeOutOfRange { now, limit }`,
  after `NonFiniteTime` and before `TimeRegression`, for every model. Any single difference of two
  accepted times is then at most `f64::MAX / 2`; the factor-of-two headroom absorbs upward rounding
  drift in accumulated elapsed time, which a `f64::MAX / 2` limit did not (a five-step schedule
  drove an integrator to `inf`). Checkpoint and durable restore refuse a clock beyond the limit
  (`InvalidBlockState`). The new `OcError::NonFiniteTimeStep { now, prev }`, after
  `TimeRegression`, is kept as a defensive check. Within the limit there is no maximum step.
  Replay records with `|t|` above the limit, which models without sampled blocks used to accept,
  now fail with `TimeOutOfRange` when replayed. Times that a sampled block used to refuse as
  `ModelTimeUnrepresentable` above the limit (for example `f64::MAX`) now report `TimeOutOfRange`.
  Five existing tests were adjusted: three moved their unrepresentable probe from `f64::MAX` to
  1e305, the period-`f64::MAX` clock test now advances to the limit, and the execution preflight
  matrix gained out-of-range, overflowing-step and unrepresentable causes. At load, a NaN or infinite
  `min`, `max` or `nominal` on a Real connector or a boundary input/output declaration — which
  loaded and then refused every frame, because no value compares with NaN — is refused with the new
  `DiagCode::NonFiniteBound` (`non-finite-bound`) error at the attribute-unification stage, before
  unification can compare or propagate it. Non-finite Real input values are unchanged (issue #363 stays open). The
  release-compatibility checker re-selects its current implementation baseline at the change's
  source commit.
- **Hosts can opt in to refusing non-finite Real inputs** (GitHub #382, issue #363). The engine
  had no policy for a NaN or infinite input sample on an unbounded Real input: it reached the
  blocks, where one NaN latched `PID`, `Derivative` and `MovingAverage` state until reload, and
  `Line` and `LimitSlewRate` turned it into a believable finite command. The new
  `Engine::set_non_finite_input_policy(NonFiniteInputPolicy::Refuse)` makes `prepare_frame` refuse
  every non-finite Real boundary input value — NaN of either sign and payload, and both
  infinities, bounded or not — with the new `OcError::InputNonFinite(path)`, naming the canonical
  input; per input it follows `InputType` and precedes `InputDomain`. `execute_frame` rechecks a
  plan against the policy in force, so a plan prepared under `Admit` cannot execute a non-finite
  value after a switch. Block maths is unchanged. `Engine::non_finite_input_policy` reads the
  setting. The default `NonFiniteInputPolicy::Admit` (non-exhaustive) keeps today's behaviour
  byte-for-byte, because the Tier-A goldens and the bound strict-bit conformance evidence feed NaN
  and ±∞ through the facade; the default becomes `Refuse` at the next native-evidence refresh.
  The policy is host configuration like `cxf_byte_limit`: it persists across loads, resumes and
  restores, and no snapshot, checkpoint or replay record carries it. Production hosts are
  recommended to select `Refuse` (see `docs/host-responsibilities.md`). The release-compatibility
  checker re-selects its current implementation baseline at the change's source commit.

### Storage port

- **`oce-store-mem` folds into `oce-store` behind a default `mem` feature, and `oce-store` gains
  the host-side post-load helper** (GitHub #417, issue #391). `oce_store_mem::MemStore` is now
  `oce_store::MemStore`, unchanged in behaviour and tests; `default-features = false` builds the
  ports, DTOs and helper with no `MemStore`. The new `oce_store::host` module puts back, on the
  host's side, the sequence load ran before the engine became pure (GitHub #415):
  `persist_loaded_model(&store, &loaded.model, loaded.input_points())` calls `Durable::recover`,
  then `ModelStore::save_model`, then `PointStore::resolve_points` for every input, refusing a
  handle-count mismatch with `StoreError::Validation`, and returns the input handles; the three
  steps are public on their own as `recover`, `save_model` and `validate_inputs`. It takes only
  the `oce_model::resolved_model` DTO and `(canonical path, value type)` pairs, so `oce-store`
  still never depends on `oce-api` (and `oce-api` still never depends on `oce-store`). It never
  commits and rolls nothing back. Tests pin the call order against a recording `Store` double,
  failure propagation at each step, the mismatch message, the `MemStore` happy path, and — against
  a minimal write-ahead-log fixture in `crates/oce-store/tests/recovery_ordering.rs` — that the
  order is load-bearing: a save made before `recover` is discarded with the uncommitted tail, and
  a handle resolved before `recover` indexes replaced state. The workspace has 12 members (10
  publishable); the `oce-store` suite now runs per PR in the `crate-tests` job;
  `check-default-no-db.sh`, the package validator and the release-workflow byte pin follow the
  new member set.

### CXF import and export

- **Modeled `S231:` properties are read in every JSON-LD key spelling** (GitHub #386, issue
  #383). The importer matched `S231:min`, `S231:max`, `S231:nominal`, `S231:unit` and the other
  21 modeled properties only under the compact key. The same property keyed by its full IRI
  (`"http://data.ashrae.org/S231P#min"`), by another prefix bound to the S231 namespace, or by a
  declared `@context` term fell into the opaque pass-through map, so a bound loaded with no
  diagnostic, was never validated or enforced on frames, and vanished from export. On the
  resolver's working clone, every pass-through key is now expanded against the document
  `@context` (the same expansion identity tokens use) and, when it names a modeled term in the
  document's own `S231` namespace (the S231P namespace when the document binds none), moves into
  the typed field before any other stage reads the node. All 25 modeled terms are covered, structural edges
  included. Compact and expanded spellings give the same model, diagnostics and export bytes;
  export always writes the compact key. A modeled term in any other known S231 namespace — S231P,
  the `…/S231#` namespace upstream modelica-json emits, or either `https:` spelling — is refused as
  `non-subset-construct` in both directions, naming the namespace the document reads modeled keys
  in. Writing one property twice on a node — compact (including an explicit `null` or `[]`) and
  expanded, or two expanded spellings — is refused as `malformed-document` naming both spellings
  rather than picking one, as is a key that names a modeled property only after trimming
  surrounding whitespace, any key written more than once on one `@graph` node (JSON parsing kept
  one value silently; no tracked fixture or vendored translation repeats a key), and an expanded
  value that does not fit the field (the resolver-side
  twin of the parse error the compact key raises). Unmodeled terms and other vocabularies still
  pass through, and the Layer-A DTO
  (`parse_document`/`write_document`) is unchanged and still lossless. New tests re-key every
  modeled property of every tracked fixture to its full IRI and require an identical import
  outcome and export, and check enforcement at the facade (`crossed-bounds`,
  `non-finite-bound`, `InputDomain`).

- **Out-of-range `integer()` bindings, crossed bounds and `S231:nominal` no longer load
  silently** (GitHub #384, issues #374, #378, #379). Three load-time values that were wrong but
  plausible are now refused or carried through:
  - `integer(x)` for a Real whose floor lies outside `[-2^63, 2^63)` — `NaN` and `±inf` included —
    refuses with `ExprError::DomainError` in every evaluation mode instead of saturating
    (`integer(1e300)` loaded as `i64::MAX`, `integer(NaN)` as `0`). At load that is
    `grounding-failed` on the binding's node, for parameters and connector bounds alike.
    `integer(-9223372036854775808.0)` is still exactly `i64::MIN`; `2^63` itself refuses. The
    runtime `CDL.Conversions.RealToInteger` block keeps its pinned saturating conversion on
    purpose: a tick must emit an output. No other float-to-Integer conversion in `oce-expr`
    saturates (`fill`, `size`, `cat` and subscripts take Integer operands only; range counts are
    capped before conversion). The `Modelica.Constants.inf` alias stays `f64::MAX`, now documented
    and pinned as ordinary finite arithmetic (`inf/inf` = 1, `inf-inf` = 0, `inf+1` = `f64::MAX`);
    overflowing it is still refused at load.
  - A bound pair with `min > max` on a connector, a boundary input or a boundary output
    declaration — which loaded and then refused every frame with `InputDomain` — is refused with
    the new `DiagCode::CrossedBounds` (`crossed-bounds`) error at the attribute-unification stage,
    beside `non-finite-bound`. So is a boundary input whose declaration and driven child inputs
    combine to an empty range, and a range that §7.10 propagation would cross (the propagation is
    rolled back) and is reported once per cluster, on its source output. This tightens acceptance:
    internal connectors whose one-sided bounds propagate into an empty range (one member's
    `min = 5`, another's `max = 3`) used to load and run, because the frame enforces bounds only on
    boundary inputs; they are now refused. `min == max` stays legal and admits exactly that value;
    IEEE-equal signed zeros are equal, not crossed; a non-finite pair reports only
    `non-finite-bound`. An undeclared
    Integer bound counts as the CDL Integer limit, so a declared Integer `min` above
    `2_147_483_647` alone is crossed.
  - `S231:nominal` is parsed onto `RealAttrs::nominal` (a modeled `oce_cxf::Node::nominal` field)
    instead of sitting unread in the pass-through map, so the deep gate refuses a non-finite one
    (`non-finite-bound`) and export emits a finite one as a bare number, round-tripping it; before,
    import ignored it with no diagnostic and export dropped it. It is still not unified (R13.4),
    never bounds input values, and is a `malformed-document` error on a non-Real connector.

  No tracked fixture carries crossed bounds, an `integer()` call or `S231:nominal`, and no
  fixture's Integer bound exceeds the i32 range, so no fixture changes acceptance; the import
  corpus is unchanged. The release-compatibility checker re-selects its current implementation
  baseline at the change's source commit.
- **Five hostile CXF load amplifications are now bounded with typed refusals, and an over-large
  state window refuses instead of panicking** (GitHub #381, issue #365). Bounded per load:
  `containsBlock` lowering work (each instance once), inherited-scope memory (linear), array
  parameter elements (2^20 per document), `[S]` state words (2^22 per model) and binding
  evaluation work (2^24 units per evaluation, 2^26 per document). Not bounded: the total memory
  and time of an arbitrary below-cap document — isolation stays a host obligation (PC-004).
  - **State budget.** `oce_graph::allocate_state` now returns `Result` and sizes the `[S]` state
    buffer with checked arithmetic and `try_reserve_exact` against the new
    `oce_graph::MAX_STATE_WORDS` (`1 << 22` words, 32 MiB). `CDL.Discrete.TriggeredMovingMean`
    with `n = 2^62` panicked the load with "capacity overflow"; it is now
    `BuildError::StateTooLarge` (`resource-limit-exceeded`, subject = the block, stage
    `Schedule`), refused before any allocation. The window conversion saturates rather than
    truncating on a narrow `usize`. `Engine::resume` returns the same error when edited windows
    cross the budget, leaving the engine halted with its edits pending.
  - **`CDL.Integers.Stage`** answers its stage search with an exact `O(log n)` binary search
    instead of the source's `O(n)` loop per stage event, so `n = i64::MAX` cannot stall a tick.
    Results are identical to the loop on every input; tests cross-check against the literal loop
    on every threshold, ±1 ulp, signed zeros, infinities and NaN.
  - **Shared children.** A `containsBlock` child reached twice — listed twice by one composite,
    or by two composites — is refused with the new `DiagCode::SharedInstance`
    (`shared-instance`, untagged, subject = the child): a CDL instance has exactly one parent.
    Composite `C_i` listing `[C_{i+1}, C_{i+1}]` previously quadrupled the load every two levels
    (depth 23: about 8.5 s and 3.1 GB). The `rejected/diamond_cycle.jsonld` corpus fixture now
    reports its cycle once, plus one `shared-instance` for the second path, instead of two
    `contains-cycle` diagnostics; `docs/cxf-composite-subset.md` is updated. The published
    composite rule catalog is unchanged.
  - **Array budget.** All preserved array parameters of one document share
    `MAX_DOCUMENT_ARRAY_ELEMENTS` (`1 << 20`, equal to the existing per-parameter cap, so every
    document whose arrays fit one maximal parameter still loads). Four broadcast `p[1048576]`
    parameters in a 1.5 KB document loaded at about 1 GB; the second now refuses with the new
    `DiagCode::ResourceLimitExceeded` (`resource-limit-exceeded`) before allocating.
  - **Scopes and expressions.** Each composite's grounded declarations are one `Arc`-shared,
    hash-indexed frame instead of a copy per leaf, so inherited-scope memory is linear in
    parameters plus leaves (a 1.5 MB document once peaked at 6.8 GB); own-declaration chains sort
    with a heap-driven Kahn order. `oce-expr` charges deterministic work units (one per AST node,
    one per array element built, read or reduced) against `oce_expr::MAX_EVAL_FUEL` (`1 << 24`)
    per evaluation, and the new `oce_expr::with_eval_budget` shares one budget across many
    evaluations; `oce_cxf::import_cxf` runs every binding of a document on
    `oce_cxf::MAX_DOCUMENT_EVAL_FUEL` (`1 << 26`). Exhaustion is the new
    `ExprError::ExpressionTooExpensive`, naming the budget that ran out and surfaced as
    `grounding-failed`. `sum({sum(1:1048576) for i in 1:1048576})`, once extrapolated to about
    23 minutes, refuses in tens of milliseconds; a leaf with 200 bindings of
    `sum({sum(1:2800) for i in 1:2800})` took about 10 s in a release build with only the
    per-evaluation cap (scaling with the binding count) and now refuses in about 50 ms at any count. The most expensive
    single evaluation in the tracked test suites costs 77 units, and the largest non-hostile
    per-document total 101,101.

  No class parameter rule, catalog entry or compatibility fact changes; every tracked fixture and
  golden loads byte-identically. The release-compatibility checker re-selects its current
  implementation baseline at the change's source commit.

- **A declared but unread root boundary input now warns at load** (GitHub #346, issue #339).
  A root `S231:hasInput` declaration whose node exists but that no connection reads was left out of
  `input_definitions()` with an empty load receipt, so a host driving frames from its own point
  list learned of it only from a later unknown-input refusal. Resolution now emits a
  `DiagCode::UnusedBoundaryInput` warning (`unused-boundary-input`, subject = the input's expanded
  `@id`); it reaches the successful load receipt and survives into the failure receipt when a later
  stage refuses. The frame contract is unchanged — the unread input is still not a frame input, and
  it does not re-export. Inputs read through a pass-through, a nested-composite boundary hop or a
  reverse-authored edge stay silent; a root input whose only route ends at a nested input nothing
  reads warns; nested composites' own interfaces, missing nodes, dual-listed input/output IRIs,
  role aliases (already refused as `malformed-document`) and inactive conditional inputs do not.
  Under `deny_warnings` the warning refuses the load. No checked-in G36, conformance,
  composite-contract or vendored fixture emits it; four synthetic test documents that had dropped
  an input's only connection now pin the warning. The release-compatibility checker re-selects
  its current implementation baseline at the change's source commit; policy and rows are
  unchanged.

### Binding expressions

- **Long operator chains are no longer refused as too deep, and `-9223372036854775808` is a
  literal** (GitHub #386, issue #371). The AST depth check counted the length of a
  left-associative chain as nesting, so `a+b+…` with more than 64 terms (and the same for `*`,
  `/`, `-`, `and`, `or`) failed with `NestingTooDeep`. `nesting_depth` now leaves the left operand
  of a `Binary` that is itself a `Binary` at its parent's level, so only real nesting (right
  operands, parentheses, signs, calls, arrays, ranges, subscripts, comprehensions) counts, and a
  chain is bounded by `MAX_EXPR_NODES` (2048 terms) at parse and by the fuel budget at evaluation.
  The evaluator walks a chain's left spine with an explicit stack in exactly the recursive order —
  same fuel charges, same strict-finite checks at every inner node, same first error — so stack
  use stays bounded by the depth limit: a hand-built 100,000-term chain evaluates on a 2 MiB
  thread. The CXF resolver's two expression walkers (declaration ordering and G36 guard
  validation) walk chains the same way. A 2000-operand guard chain that used to be refused at
  parse is now validated operand by operand. Separately, `9223372036854775808` (2^63) was refused
  by the lexer, so `i64::MIN` could not be written; a unary minus applied directly to that literal
  now folds to `i64::MIN`, and only that form — the literal anywhere else, including
  `-9223372036854775808[1]`, is still out of range.

### Block library

- **`CDL.Reals.MovingAverage` now averages over the whole of `delta` however many ticks fall inside
  it** (GitHub #373, issue #364). The block kept at most 64 checkpoints and dropped the oldest when
  more arrived. With more than 64 ticks per window it averaged over only about the last 64 samples
  and warned once. At 1 s ticks the G36 `ReliefFanGroup` and `ReturnFanDirectPressure` sequences,
  which use `delta = 300`, were therefore averaging over about 64 s. A 600 s window with a 0 → 100
  step at t = 640 reported 95.3125 at t = 700. It now reports 6100/600 = 10.1666…, because each
  tick's input covers the gap that ends at that tick. The checkpoint ring is now sized from `delta`
  at build time: `clamp(2·ceil(delta), 2048, 131072)` slots plus 4, which sets a history resolution
  `r = delta/slots`. That gives `r ≤ 0.5 s` for every window up to 65536 s. The full-window divisor
  is now exactly `delta`; it used to be `t - fl(t - delta)`, so other traces can move by a sub-ulp
  amount. The oldest checkpoint is never later than the window start. The output is
  bit-identical to an unbounded history as long as every pair of consecutive tick gaps sums to at
  least `2r`. For a nominal 1 s host with `delta ≤ 65536 s` that holds while each tick stays
  within ±0.5 s of its nominal time. Denser ticks are merged into the newest checkpoint: the
  window still spans `delta`, interpolation inside a merged gap moves `y` by at most
  `r·range/(2·delta)`, and the first merge warns once per instance. A window longer than 65536 s
  keeps its full span at resolution `delta/131072`. **State layout changes:** the region grows
  from 134 words to `6 + 2·(slots + 4)` words: 4110 (about 33 KB) for any `delta ≤ 1024 s`,
  14414 for `delta = 3600`, and at most 262158 (about 2.1 MB). Snapshots stay capped at 64 MiB,
  so a model with a few dozen maximum-size windows runs but cannot be captured; nothing checks
  this at load. The validator now checks the region against the instance's own `delta`. It
  refuses a ring that no longer covers the window start. A snapshot from an earlier build refuses
  at restore on the manifest's state-length comparison, consistent with
  current/current-only continuation. Three new standalone Tier-A goldens (`MovingAverage/*`, 200 to
  600 ticks per window, including the 600 s step) integrate the input piece by piece without
  interpolating `mu`. The engine matches them bit-exactly in `per_block_reals_dynamics.rs`. Per-block
  Tier-A coverage rises to 129 of 133 CDL classes. The release-compatibility checker re-selects its
  current implementation baseline at the change's source commit.

### Verification

- **The compatibility records and the API surface gate are retired until 1.0** (GitHub #412, issue #388). The
  engine is pre-1.0 with no backwards-compatibility promise, so the release-compatibility matrix
  and its checker, the compatibility manifest, the package-publication and public-surface ledgers,
  the dated stability baseline and its tool, and the `cargo public-api` snapshots for `oce-api` and
  `oce-store` are deleted with their tests, docs and dev-dependencies. `release-gate.yml` no longer
  installs a nightly toolchain, and a change to `Cargo.lock` or `crates/*/src` no longer needs a
  "select … as the current candidate" commit. The package validator now reads publication state
  from each crate's `Cargo.toml` and keeps its structural checks (private or registry-ready, one
  shared `oce-api` feature closure with no database or async runtime, approved `release.yml`
  bytes; its `test_validate.py` has 19 tests). The prose guard that scanned docs and rustdoc for
  seven historical false claims lived in the deleted public-surface contract test and is retired
  with the contract. Retired facade names stay refused by the facade compiler controls, which now also cover
  the two removed realtime `OcError` variants. The product contract advances to revision 17 and
  the authority index drops its two ledger rows; the identity glossary moves to the facade
  contracts and the host fallback checklist to the host responsibilities, and a new sweep in
  `export_content_oracle.rs` keeps the manifest test's one behavioural check (complete exports mint
  the FNV-1a-128 of their bytes, 13 partial ones refuse). A surface baseline returns at 1.0.

- **The retained native strict-bit receipt is marked `recapture-pending` for the M1 refactor
  window** (GitHub #409, issue #387). `receipts.json` gains an optional evidence `status` (absent or
  `admitted` keeps the previous behaviour) and a `pending` record naming the reason, the date, the
  closing issue and the superseded receipt's matrix digest. The window closes when GitHub #413
  retires the digest binding, with issue #404 as the milestone closeout. While pending,
  `retained_native_linux_evidence_is_complete_exact_and_source_bound` skips only the comparison of
  captured source digests with the checkout and prints `RECAPTURE_PENDING:` with every drifted
  bound path; the retained captures' numbers, topology, repeats, inventory, provenance and matrix
  digest are still checked, and the per-PR eight-capture comparison and hostile controls are
  unchanged. New controls prove a pending status cannot admit evidence even with exact digests,
  that it never masks a one-bit or repeat-capture drift, and that malformed status records are
  refused. The per-PR strict-bit cells rerun the retained test with its output shown. The
  release-compatibility evidence digests for `matrix.rs` and `receipts.json` are refreshed.
- **`CDL.Logical.Nor`, `CDL.Logical.Pre` and `CDL.Utilities.Assert` now have standalone Tier-A
  goldens** (GitHub #385, issue #371). Before this, the three relied on unit tests, facade contract
  tests and indirect G36-sequence evidence. `tools/golden-gen` now derives four new signal goldens
  from the vendored Buildings sources, citing the `.mo` file and pinned commit in each provenance
  record. `Nor` gets the exhaustive two-input truth table, with `(F,F)` revisited, from `Nor.mo`
  line 15. `Pre` gets two HostTick v1 traces: in the default one `pre_u_start` is `false` and `u` is
  already true on the first tick; in `pre_u_start_true` `u` is false on the first tick. Each first
  tick must emit `pre_u_start`, and later ticks emit the previous tick's input through holds and
  single-tick toggles. Both records carry `execution_profile: "HostTick v1"` and a
  `diverges_from_modelica_event_iteration` note, because a converged Modelica event iteration would
  report `y == u` instead. `Assert` has no output connector, so its golden is a `warning`
  observation column read from `Assert.mo` line 11. A `1.0` means exactly one Warning record per
  evaluation with `u` false, including the first tick; repeats are not suppressed. A new suite,
  `per_block_utilities_assert.rs`, compares that column with each completed frame's diagnostics:
  count, source, message and time bits. It is a separate file because `per_block_utilities.rs` is
  a bound strict-bit source. `per_block_logical.rs` compares `Nor` and both `Pre` traces exactly,
  and the engine matches all four goldens bit-exactly. Per-block Tier-A coverage rises to 132 of
  133 CDL classes; only `CDL.Logical.Sources.Constant` remains on indirect evidence. The CDL
  catalog moves the three classes from `legacy_evidence_exemption` to `golden_manifest`, which
  changes the catalog fingerprint. Tier-A totals rise to 422 provenance records and 420 signal
  goldens; 22 of those signals now check HostTick v1. The summary in
  `docs/verification-evidence.md` had trailed its body by two comparisons since GitHub #356 added two G36
  signals; recounted from the tree, both now say 398 source-semantics comparisons (377 existing
  exact plus 21 qualified-Linux exact).

- **The scheduler, validator, model, CXF and WAL adapter suites, and the docs checks, now gate
  every PR** (GitHub #380, issues #370, #377). Per-PR CI ran nextest only for `oce-api`,
  `oce-blocks` and `oce-expr`, so a regression in `oce-graph`, `oce-validate`, `oce-model`,
  `oce-cxf` or `oce-reference-wal-adapter` surfaced only in the daily release gate. A new
  `crate-tests` job in `pr-gate.yml` runs those five suites in x86_64 debug, beside the determinism
  matrix rather than inside its QEMU-emulated legs; their release-codegen runs stay in the release
  gate. The docs checks in `docs-pages.yml` are path-filtered and not required, and GitHub #373
  merged with them failing. A new `docs` job runs the same sequence on every PR: checker contracts,
  authority literals through mdBook, staging, the mdBook build and the link check.
  `docs-pages.yml` keeps the Pages publish from `main`. `CI OK` needs both jobs, and
  `check-workflow-gates.sh` and its fixtures now require their commands. `.agents/gate.sh` does not
  run them yet because it is a bound source of the retained native strict-bit evidence;
  `.agents/project-facts.md` gives the local commands.
- **golden-gen's own hygiene and its goldens' freshness now gate every PR** (GitHub #360, issue
  #350). `tools/golden-gen` is a standalone cargo workspace, so no workspace command linted or
  tested it, and nothing checked that the checked-in Tier-A goldens are what it emits. The new
  `golden-gen-hygiene` job in `pr-gate.yml`, which `CI OK` needs, runs its `cargo fmt --check`,
  `cargo clippy --all-targets -D warnings` and `cargo test`, then regenerates every golden in
  release and fails on any modified, deleted or untracked file under `tools/golden-gen/goldens`.
  The generator source is reformatted and its clippy findings fixed (a derived `Default`, a
  merged identical branch, iterator loops in place of index loops); two eight-argument golden
  builders carry a scoped `too_many_arguments` allow. Regeneration leaves every golden
  byte-identical. `.agents/gate.sh` does not run the job yet because it is a bound source of the
  retained native strict-bit evidence; `.agents/project-facts.md` gives the local commands.
- **The G36 FreezeProtection heating-coil command now has a continuous-time reference** (GitHub #357, issue
  #344). The 60 s column stays as it is. Its `Ti = 0.5 s` PI controllers run there at
  `k·dt/Ti = 120`, so it only checks the forward-Euler discretization the engine shares, and it
  keeps its `frame_rate_dependence` note, which now points at the new scenario.
  `multizone_vav_freeze_protection_fine_step` runs the same fixture at a 1/16 s frame
  (`k·dt/Ti = 0.125`) for 330 s. Over that window it latches stage 3, resets to stage 2, and
  steps the mixed-air and supply-air temperatures so both PI controllers ramp, saturate, leave
  saturation and hold. The independent golden-gen reference emits `freeze_protection_stage` and
  `heating_coil_command`, which the engine matches bit-exactly.
  A new conformance suite solves the `CDL/Reals/PID.mo` PI with anti-windup in closed form,
  event by event. That solution is exact for these grid-aligned piecewise-constant inputs. The
  suite holds every row within a per-row forward-Euler bound: one-frame truncation errors
  carried through the Euler map's Lipschitz constant. The largest bound is ≈ 0.0099 and the
  largest observed error ≈ 0.0049. The bound rejects the closed form with `Ti` off by 1%, with
  `Ni` off by 5%, and with `yMax = 0.95`. The fine-step stage column must match a schedule
  derived from the `.mo`. The suite also shows the same oracle departing from the 60 s column
  by 1.0, and forward Euler converging to the closed form at first order at `k = 1` and
  `k = 0.37`. At `k = 1` the anti-windup gain forms `1/(k·Ni)`, `k/Ni` and `1/Ni` coincide;
  the CDL PID Tier-A goldens (`k = 0.37`) pin the `k ≠ 1` form. The
  frame-rate guard now surveys every golden scenario, including scenarios that reuse another
  scenario's fixture, and documents the new ratio. Tier-A records go from 413 to 415, and G36
  signal goldens from 133 to 135.

- **The G36 SupplyFan golden now exercises its duct static pressure trim-and-respond**
  (GitHub #351, issue #343). The `multizone_vav_supply_fan` scenario ended at 900 s, but after
  the 360 s fan restart
  the source-default `delTim = 600 s` and `samplePeriod = 120 s` hold the reset until 1080 s, so
  the request steps never moved the setpoint and only the fan PI was covered. The scenario keeps
  the source defaults and its first 900 s unchanged, runs to 1920 s, and adds request and
  duct-pressure steps: the setpoint responds to 140 Pa (capped at `maxRes`) and 143 Pa, trims at
  exactly `numIgnReq` requests to 131 Pa, trims to 119 and 107 Pa, then responds to 125, 143 and
  161 Pa. The independent golden-gen reference adds a `duct_static_pressure_setpoint` column
  (`staPreSetRes.y`), which the engine matches bit-exactly with fan status and speed. A new
  check fails if the reference stops showing a respond, a trim after it, a respond after that,
  and two respond sizes. The oce-api sequence test pins every setpoint hold interval. The
  generator's reset chain now follows `TrimAndRespond.mo`: it publishes `iniSet` until the
  TrueDelay elapses and resets to `iniSet` rather than `maxSet` while the fan is off. Neither
  difference could show in the old scenario, and its first 900 s regenerate byte-identically.
  The engine determinism snapshot is re-blessed for the longer run.

- **The G36 trim-and-respond references now restart through both of the block's switches**
  (GitHub #355, issue #349). golden-gen had three private copies of `Generic/TrimAndRespond.mo`: the
  standalone, SupplyFan and SupplyTemperature oracles. They are now one shared chain modeling
  the TrueDelay, Sampler, UnitDelay and net reset, plus both switches: `swi` publishes `iniSet`
  until the TrueDelay elapses, and `swi2` feeds `iniSet` into the UnitDelay while the device is
  off. The standalone copy had no `swi`, and SupplyTemperature published the reset chain
  directly. Neither difference could show in the old scenarios, and every reference
  regenerated byte-identically before the scenarios changed. The `trim_and_respond_have_hol_false`
  scenario now runs to 2400 s. It responds and trims to 9.1, then turns the device off for
  1260..1379 s, a window holding one sample instant, and restarts between instants. The
  setpoint stays at `iniSet = 10` through the re-armed delay, although the UnitDelay still
  holds 9.1 at the restart. It then resumes from `iniSet`: 10.1, 9.5, 9.6, 9.7.
  `multizone_vav_supply_fan` runs to 3000 s and adds a fan stop at 1980..2099 s, spanning the
  2040 s instant while 161 Pa is held. The setpoint holds 120 Pa until the re-armed delay
  releases at 2820 s, then reads 138 Pa and trims to 126 Pa. The engine matches both references
  bit-exactly. The oce-api tests pin every hold interval. A new funnel check fails if either
  reference loses the restart shape. Removing `swi` or `swi2` from the chain changes both
  references and fails the engine comparison. Both engine determinism snapshots are re-blessed
  for the longer runs.

- **The G36 trim-and-respond references now pin the short-trip reseed boundary and label it**
  (GitHub #356, issue #354). `Generic/TrimAndRespond.mo` documents `iniSet` while the device is
  off, but its graph feeds `iniSet` to the reset only through `swi2` into a `UnitDelay` that
  captures at `samplePeriod` instants. A device-off window holding no instant therefore leaves
  the pre-trip value in the chain. The engine and the reference both follow the graph.
  `trim_and_respond_have_hol_false` now runs to 4920 s with two more trips. A short trip at
  2820..2879 s, strictly between the 2760 s and 2880 s instants, is released at 3600 s from the
  stale 8.8: it reads 8.9, where reseeding would give 10.1. A boundary trip, whose single off
  tick is the 3960 s instant, does reseed: it is released at 4740 s at 10.1, where the stale 7.9
  would give 8.0. The engine matches bit-exactly, the oce-api test pins every hold, and a new
  `g36_trim_and_respond_reseed` guard fails if the reference is changed to reseed on any off
  edge or to skip the reseed at an instant. The standalone, SupplyFan and SupplyTemperature
  references now carry a `diverges_from_g36_intent` note, checked against the vendored wiring,
  and `docs/cdl-coverage.md` states that OCE follows the `.mo` graph, not the prose. The engine
  determinism snapshot is re-blessed for the longer run.

- **G36 Tier-A references now say which ones are not G36 answer keys** (GitHub #353,
  issues #342, #345, #344). Every `tools/golden-gen/goldens/G36/**/*.prov.json` carries three new fields from the
  generator: `sequence_derivation`, `diverges_from_g36_intent` and `frame_rate_dependence`. The
  last two are a note or `null`.
  - `ahu_supply_air_temp_reset` claimed to be an independent re-derivation of
    `SupplyTemperature.mo`, but its inputs (`zone_temp`, `cooling_setpoint`) and its clamped-ramp
    rule are not in that file. It, and the other two fixture-only sequences (`ahu_economizer`,
    `vav_single_zone`), which had the same false claim, are now labelled
    `fixture-local-composition`. They cite only the CDL block `.mo` files whose semantics they
    apply.
  - `multizone_vav_economizer_enable` and the economizer controller reference faithfully
    reproduce pinned `Enable.mo`, which joins the high-limit hysteresis outputs with `Or` and so
    enables the economizer only *above* the cutoff. `multizone_vav_supply_temperature` reproduces
    `SupplyTemperature.mo`'s `cooDowMod k=3` and `0<m<3` mode mapping, which swaps cool-down and
    setup (`coolDown=2`, `setUp=3`). All three now carry `diverges_from_g36_intent` notes. The
    supply-temperature note also says that its golden drives mode 1 only, so the trace never
    exercises the swap. Separately from the `.mo`, its CXF fixture writes the nested
    trim-and-respond parameters as literals where the `.mo` binds them; the literals equal the
    outer values today.
  - `multizone_vav_freeze_protection/heating_coil_command` alternates between 1 and 0 on every
    frame. That comes from forward-Euler integration of a `Ti = 0.5 s` PI at the 60 s frame step,
    which is the same recurrence the engine uses. The column is now marked frame-rate dependent,
    and its `source` no longer claims an independent re-derivation. Seven more signals driven by a
    `Ti = 0.5 s` PI get the same label:
    - three signals of the economizer controller reference, which also flip between limits every
      60 s frame (`k·dt/Ti = 120`);
    - four return-fan signals, at a 1 s frame (`k·dt/Ti = 2`).

    The criterion is `k·dt/Ti ≥ 2`, where forward Euler stops tracking the integrator. PI
    references below it (largest ratio 0.2) are listed as frame-step-insensitive.

  No trace values, CSVs or engine code change. Two new `oce-conformance` suites guard the labels:
  - `g36_reference_provenance_labels` checks:
    - the fields are present and well formed;
    - the fixture-local set equals the catalog's fixture-only sequences;
    - the divergence set matches the reviewed list;
    - each note still holds against the vendored `.mo` or fixture it describes.
  - `g36_reference_frame_rate_labels` checks the frame-rate labels:
    - it recomputes `k·dt/Ti` from each fixture's resolved PI parameters and its golden's time
      grid;
    - it follows the fixture's connections to the outputs each PI reaches;
    - it requires the labels to be exactly the signals an at-threshold PI reaches;
    - it fails on any integrating-PI scenario that has not been reviewed.

### CI and infrastructure

- **The `docs` job and `docs-pages` cache the verified mdBook binary, so a release-CDN outage no
  longer fails them** (GitHub #426, issue #416). `actions/cache` restores `target/mdbook-bin` under
  an exact key (no `restore-keys`); `install-mdbook.sh` is now idempotent and self-verifying, trusting
  an existing binary only if its SHA-256 equals the digest of the extracted executable newly recorded
  in `site/mdbook-checksums.txt`, and otherwise downloading and verifying the archive and the binary.
  `check-workflow-gates.sh` pins the cache step and refuses `restore-keys`;
  `scripts/docs/test-install-mdbook.sh` covers the refuse-a-wrong-binary branch. No second download
  source was added.

- **macOS arm64 and Windows platform cells in the per-PR gate** (GitHub #425, issue #394).
  Three determinism-lane jobs join `pr-gate.yml`, each required by `CI OK` and pinned to the
  determinism lane: `portable snapshot (macOS arm64, debug)` runs the `oce-api`/`oce-blocks`/
  `oce-expr` subset on `macos-15` and uploads its portable and target-bound state vectors;
  `portable snapshot comparison` requires the macOS portable snapshot to equal the Linux x86_64
  and aarch64 debug portable snapshots byte for byte and its target-bound snapshot to differ
  from both; `std build and nextest (Windows x86_64, debug)` builds the workspace on
  `windows-latest` and runs its nextest suite without the five strict-bit binaries. Both are
  observations, not qualification: strict-bit transcendentals stay Linux-qualified and macOS and
  Windows keep the 1e-12 aligned band. They sit in `pr-gate.yml` rather than a separate workflow
  because `CI OK` is the only required status and cannot need a job in another workflow.
  `check-workflow-gates.sh` now also refuses `continue-on-error` anywhere in `pr-gate.yml` and
  any step-level `if:` other than a static matrix selector or `${{ !cancelled() }}`, and pins the
  new cells' runners, commands, comparison and strict-bit exclusion, each with a failing fixture;
  `ci_ok.py` refuses a non-object `NEEDS_JSON` with its own error instead of a traceback, and its
  controls cover `leverage lane` and `gate (light)` skipped while their lanes ran. Two `oce-cxf`
  OpenModelica reference test groups that cannot build on Windows are now `#[cfg(unix)]` like their
  siblings: the symlink-rejection test (it creates symlinks with `std::os::unix::fs::symlink`) and
  the line-reference `manifest_contract_tests` (its temporary repositories are claimed by unix
  device and inode); Linux and macOS run both unchanged. Two more fail-on-Windows causes are fixed in
  test code: the hook truthiness test sets `PATH` on its `bash` command, so Windows resolves Git Bash
  rather than the WSL launcher in `System32`, and passes the hook path without the verbatim `\\?\`
  prefix and with forward slashes; the Line evidence validator drops the verbatim prefix from its
  canonical root, which the digest-pinned canonicalizer's per-component check could not stat.
- **Two per-PR lanes under one `CI OK`, and `gate (light)` only when the gate changes** (GitHub
  #418, issue #393). A new first job in `pr-gate.yml`, `classify changed paths`, lists the PR's
  changes with plain git (the tested merge against its base parent, renames as a deletion plus an
  addition) and classifies them with `scripts/ci/classify-changes.sh`. The rules fail closed: any
  path that is not documentation or under `crates/leverage/` runs the **determinism lane** — every
  job that compiles the engine — and documentation a test reads (`README.md`,
  `docs/cxf-composite-subset.md`) is not documentation for this purpose, with a fixture that scans
  the crates for such reads. `crates/leverage/**` runs the new `leverage lane` job (layout and
  `oce-api`-only dependency rule via `cargo tree`, then fmt, clippy, rustdoc, nextest and
  cargo-machete per leverage crate; none exist yet, so it prints `no leverage crates yet`), and
  every engine change runs it too. A docs-only PR compiles nothing. `gate (light)`, which re-ran
  what the other jobs had run (17m28s), now runs only when `.agents/gate.sh`, a CI or repository
  script, a workflow or `.config/` changes, and such a change runs every job; its facade-contract
  and Quickstart steps move to a `facade contract and Quickstart` job in the determinism lane and
  the gate-script coverage checks to `gate behavior fixtures`. `CI OK` now runs
  `scripts/ci/ci_ok.py`, which accepts a `skipped` job only when the classifier's output for its
  lane was `false` and fails on any other skip, failure, cancellation or inconsistent
  classification, with hostile controls. `check-workflow-gates.sh` requires every `pr-gate.yml`
  job-level `if:` to be a lane filter that `CI OK`'s lane map lists, pins each named job to its
  lane, and fails when the result check or a classifier output is removed, with a fixture for each.
  `crates/leverage/` is created with its rule, the workspace members become `crates/oce-*` and
  `crates/leverage/*`, the unsafe-forbid layer check covers leverage crates, and
  `bash .agents/gate.sh leverage` mirrors a leverage-only PR.

- **The determinism and strict-bit matrices run on native arm64 runners, and the digest-bound
  strict-bit receipt is retired** (GitHub #413, issue #392). `determinism-matrix` and `strict-bit-matrix` in
  `pr-gate.yml` are now four native cells each — x86_64 on `ubuntu-latest`, aarch64 on
  `ubuntu-24.04-arm`, debug and release — with no cross-linker, QEMU or emulated nextest
  configuration (`scripts/ci/nextest-emulated.toml` is deleted), and the one test the emulated leg
  had to exclude runs again. Two fan-in jobs take over the cross-cell checks from uploaded
  artifacts: `determinism-compare` compares the state vectors across codegen and architecture and
  has each architecture refuse the other's target-bound snapshot through the public restore path,
  and `strict-bit-compare` runs the eight-capture comparison from a nextest archive of the x86_64
  debug cell's `strict_bits` binary and uploads `matrix.json` with a candidate receipt-log record.
  The slowest per-PR job drops from 31m33s (run 42439645194) to 13m24s, the x86_64 release determinism cell, almost all of it compiling the `oce-api` test binaries under the release profile. The per-PR native matrix is now the
  standing strict-bit evidence (owner decision in issue #405): `source_digests()`, its 35-file
  binding, the admission test, the `recapture-pending` status from GitHub #409 and
  `retained_native_linux_evidence_is_complete_exact_and_source_bound` are removed, new captures
  carry no source map, and `receipts.json` becomes a closed log of the last green native run (run
  id, head and merge SHAs, rustc, libm, per-cell capture digests, matrix digest) plus the two
  retained capture archives, which are still reconstructed byte for byte. PC-037 now describes
  per-PR native evidence. `.config/nextest.toml` loses the dead `public-api*` profiles and
  `.agents/gate.sh`'s closing report describes the native cells. `check-workflow-gates.sh` checks
  the cell matrices, both fan-ins and their `needs:`, and refuses any QEMU or cross-target string in
  the gating workflows, with a fixture for each. Jobs with the same build shape in `pr-gate.yml` and
  `release-gate.yml` share a rust-cache `shared-key`, so the daily release-gate run seeds the PR
  cache. Every `pr-gate.yml` job now has `timeout-minutes: 45`, and new controls refuse a capture
  from another revision (`mixed Git revisions`) and a matrix captured at a revision other than the
  comparing checkout's.

- **The per-PR gate runs once per change** (GitHub #411, issue #408). `pr-gate.yml` ran on the PR and again
  on the post-merge push to `development`, on the same tree: runs 42439645194 and 42441875338
  took about 31 minutes each for one change. The `push` trigger is gone; the PR run is the only
  one, and the daily `release-gate.yml` run on the `development` tip stays the safety net.
  `check-workflow-gates.sh` now refuses any `push` trigger in `pr-gate.yml` (block, flow-list or
  unfiltered form) and requires its `pull_request` trigger, with a fixture for each.
  `docs/ci-and-the-gate.md` gains a "What runs when" section, and the branch protection it
  describes — a required pull request, the required `CI OK` status, and a branch up to date with
  `development` — is applied as a repository setting.

### Documentation

- **Design note for the compiled-schedule blob (OCEB) format** (GitHub #410, issue #396).
  `docs/design/compiled-schedule-blob.md` fixes the byte format M1b encodes on a `std` host and
  decodes on a `no_std` target, before any code: a 52-byte header, five fixed-order sections
  (classes with per-class signature hashes, instances, schedule, connections, I/O) and a CRC-32C
  trailer. It reuses the state snapshot's value encodings without changing snapshot bytes, puts
  the decoder and the single HostTick v1 frame driver in `oce-graph`, states the trust model, and
  gives a size budget measured over the 46 G36 catalog fixtures (largest blob 17,001 bytes).
  `docs/README.md` gains a design-notes section and the book a design-notes chapter.
- **Agent guide rewritten around the issue-driven milestone workflow** (GitHub #407).
  `AGENTS.md` now opens with the reading order, then one section that says how a work item moves
  from a GitHub issue to a squash-merged PR: pick up only `agent-ready` issues in the current
  milestone, branch `<area>/<what-it-does>` from `development`, one PR per issue, paste the tail of
  `bash .agents/gate.sh full`, confirm goldens byte-identical, add the changelog entry in the same
  PR, confirm `CI OK` reported. Eight standing rules collect what every PR must respect, including
  the full list of strict-bit bound sources (`Cargo.toml`, `Cargo.lock` and
  `crates/oce-model/src/lib.rs` are bound too, not only the three files earlier text named) and
  the `crates/leverage/` dependency rule. The Forgejo, codex-handoff and divergent-gate-copy
  history is cut to one sentence each; the checkout-local `.agents/` notes are no longer described
  by name. A pull-request template and a work-item issue template carry the same contract.

- **Docs drift fixed, and two host-visible limits stated** (GitHub #375, issues #368, #369).
  - The runtime-introspection section of the CDL coverage page now describes `oce_api::catalog()`
    and its companions `catalog_to_json`, `catalog_content_id` and `CATALOG_JSON`. It no longer
    says `oce-api` lacks a catalog re-export, and the README points at `oce_api::catalog()`.
  - The CDL coverage page and the README now carry a note that `TrueHoldWithReset`, counted among
    the 133 CDL classes, has a local two-input interface, so a conforming single-input document is
    refused by the arity guard. The page links the authority claims index and PC-022.
  - The CDL coverage page states that the 44 vendored upstream modelica-json CXF translations do
    not load today, why (no cross-document class library; array connectors and instances refused
    by Rule 7 of the composite subset), and that a host must pre-flatten. The cross-document
    class library stays tracked in GitHub #369.
  - Stale line-number citations to `oce-store`, `oce-diag`, `oce-docs` and `oce-cxf` items in the
    architecture and CXF round-trip pages are now symbol citations.

- **Open Control Sim is no longer treated as a current or future consumer** (GitHub #361, issue
  #359). The project is discontinued. The stability-baseline tool no longer accepts or looks for a
  Sim checkout: `--source open-control-sim=PATH` is refused as an unsupported name, and its
  documented command drops that line. The dated `stability-baseline-2026-08-26.json` is unchanged
  and `--check` still reproduces it byte for byte, because the tool keeps that capture's Sim row as
  frozen historical evidence; the doc says a later capture should not carry it. The strict-bit
  evidence page, the strict-bits fixture README, the product contract, the complete-frame contract
  and the host-responsibilities page no longer promise a Sim consumer, Sim adoption under M05-PR07
  or a Sim downstream role; the product contract records this as editorial revision 16, and its
  traceability checker accepts that revision. Historical statements of what the Sim inventory
  showed are kept. None of the edited files is a bound strict-bit evidence source, so no native
  re-capture is needed.
- **Twelve classes, not nine, can trigger the atomic-firing refusal of CDL-legal feedback**
  (GitHub #358, issue #347). The coverage table from GitHub #352 listed only the nine
  `CDL.Routing` `*ExtractSignal`, `*VectorFilter` and `*VectorReplicator` classes. Three more have
  mixed feedthrough and are refused as a `block-algebraic-loop` in the same way:
  - `CDL.Reals.MatrixMax` and `CDL.Reals.MatrixMin` with two or more outputs (`nRow ≥ 2` when
    reducing rows, `nCol ≥ 2` when reducing columns), because each output depends only on its own
    row or column. For example, `MatrixMin` with `nRow = 2, nCol = 1, rowMin = true` and feedback
    `y[1] → u[2, 1]` is refused.
  - `CDL.Reals.PIDWithReset` with `controllerType = P`. With no integrator it holds no state, so it
    reads `trigger` when it fires, although `y` depends only on `u_s` and `u_m`. Feedback such as
    `y → GreaterThreshold → trigger` is refused. In the `PI`, `PD` and `PID` modes it loads.

  `docs/cdl-coverage.md` now lists all twelve classes, each with its feedthrough and trigger
  condition, and adds the routing condition `nout ≥ 1` and a remedy for the reset PID. The
  `DiagCode::BlockAlgebraicLoop` and `BuildError::BlockAlgebraicLoop` rustdoc, and the
  `oce-graph` sort's module doc, no longer give a stateful block's state-only output as the cause.
  Every registered class that triggers the refusal is stateless with mixed feedthrough under its
  trigger condition.

  A census test in `oce-blocks` builds every registered class with its defaults and at every point
  of a grid derived from the class's parameter rules and defaults. The grid covers structural
  widths `0..=3`, every value of each Boolean and enumeration parameter, and every valid selector
  or mask. Every other scalar parameter with a default is probed with a few sample values at every
  grid point. No probe may change the mixed pairs, and the parameters whose probes change whether
  a block is stateful are pinned (today only comparator hysteresis `h`). The test pins the set of
  mixed-feedthrough classes to the documented twelve and checks each class's trigger condition at
  every grid point. It is exhaustive over that grid and sampled beyond it. Mutation controls show
  that it catches an added mixed class and a kind-changing probe.

  New `oce-graph` tests refuse feedback through a real instance of each of the twelve classes, on
  both matrix axes. Feedback into an input the output does depend on is refused as a
  connector-level loop instead, each class without the feedback compiles, and the reset PID
  compiles in its stateful modes. New `oce-api` tests drive the `MatrixMin` row, `MatrixMax`
  column and P-mode reset PID refusals end to end from CXF, against new receipt goldens. No
  behaviour, error text or receipt message changes. The release-compatibility checker re-selects
  its implementation baseline at the change's source commit.

- **Atomic block firing's refusal of some CDL-legal feedback is documented as a coverage limit,
  and the refusal now says so** (GitHub #352, issue #347). The engine fires each block's outputs together, so
  a feedback path from one output back into an input that output does not depend on is refused
  as a `block-algebraic-loop`, even though CDL §7.16 is satisfied. The refusal fails closed and
  is never mis-ordered. `docs/cdl-coverage.md` now describes the trigger and lists the classes
  that can hit it. From a feedthrough check of every registered class, those are the nine
  `CDL.Routing` `*ExtractSignal`, `*VectorFilter` and `*VectorReplicator` classes with more than
  one element; no registered stateful class has mixed feedthrough. The page also gives the
  remedies: split the block, or insert a delay where that is acceptable. `execution-profile.md`
  and `facade-contracts.md` link to it. The `BuildError::BlockAlgebraicLoop` display and the
  bounded `block-algebraic-loop` receipt message no longer cite CDL §7.16. They now say that the
  model may be CDL-legal, that the cycle exists only because a block fires all its outputs at
  once, and that the block can be split. Codes, subjects, stages and record order are unchanged,
  and so is the connector-level `algebraic-loop` text. The receipt's per-record bound is restated
  as 290 fixed bytes plus two counts. A new golden pins the reproduction's receipt, and a
  500-block extract ring pins the bound for block-granularity records. No tracked G36, Library
  rule, composite-contract, conformance or vendored document uses an affected class. The
  reproduction fixture is the only tracked document refused this way. Scheduling such models
  remains open in issue #347.
  The release-compatibility checker re-selects its current implementation baseline at the
  change's source commit; policy and rows are unchanged.

- **The complete export content id is documented as order-bearing, not a model identity**
  (GitHub #341, issue #338). `content_id_complete()` hashes the emitted document, which follows the
  authored `@graph` node order and `containsBlock` order that the composite-subset ordering
  contract already declares order-bearing, so two sources differing only in that order can load
  cleanly and receive different ids. The round-trip contract, the identity glossary and PC-018
  now say so, and no order-independent model identity is provided. A new test pins the
  issue's two documents (`9308541e…` versus `8bb1a1f9…`) and that reloading identical bytes
  reproduces each id. No behaviour, export bytes, state or replay identity changes.

### Diagnostics

- **Algebraic-loop load refusals carry coded receipt records** (GitHub #348, issue #340).
  `load_cxf_with_receipt` now records one `Error` per member of the reported cycle at stage
  `Schedule`: `algebraic-loop` for a connector-level feedthrough cycle and `block-algebraic-loop`
  for a block-granularity cycle, with each member's identity as the subject. Hosts and repair tools can branch on the code instead of
  the message. Each message is bounded independently of the loop length (member position, member
  count and next member, then the cause and remedy), so receipt size grows linearly with the loop;
  records stay in subject order and the stated positions reconstruct loop order. Cycle members in
  the build error and its display are now the authored connector/instance IRIs for CXF loads (the
  positional `<class>#b<n>…` form remains only for hand-built graphs without one), and the
  block-granularity error text no longer claims the cause is always a stateful block's state-only
  output. New `DiagCode` variants `AlgebraicLoop` and `BlockAlgebraicLoop`. The `OcError` chain is
  unchanged, and the legacy `load_cxf` error still carries no records for this refusal.
  The release-compatibility checker re-selects its current implementation baseline at the
  change's source commit; policy and rows are unchanged. A CDL-legal model that atomic block
  firing still refuses as a block-granularity loop is tracked in GitHub issue #347.
- **Non-finite Real parameters are refused at load and at rest** (GitHub #376, issue #362).
  `NaN`, `+inf` and `-inf` were accepted silently for every class whose rules did not declare
  finiteness — `MultiplyByParameter.k`, `AddParameter.p`, `Timer.t`, `IntegratorWithReset.k` and
  `y_start` among them. CXF literals such as `"NaN"`/`"INF"` and expressions that evaluate to
  infinity (`1/0`, `1e400`) loaded with no warning. `set_param` accepted them because
  `v < lo || v > hi` is false for NaN, so the value reached actuator outputs as NaN; a `Timer`
  with `t = NaN` never completed. Composite-level parameters were worse, because a function could
  absorb the value: a NaN root `topK` read as `integer(topK)` grounded the gain to 0, and
  `max(topK, 2.0)` to 2.0.
  - **At load**, parameter grounding is now strict at every level: root, nested-composite and leaf
    declarations, and each array element. A non-finite literal or any non-finite expression
    intermediate is refused with a new `DiagCode::NonFiniteParameter` (`non-finite-parameter`)
    error. Its subject is the declaring instance and its message names the parameter.
  - **The strict evaluation lives in `oce-expr`**, as new `eval_finite`/`eval_str_finite` entry
    points, a new `ExprError::NonFinite` and a defaulted `Scope::requires_finite`. Every node is
    checked as it is produced, so `integer`, `sign`, `min`/`max`, `abs` and comparisons cannot
    hide one. `Modelica.Constants.inf` stays the finite `f64::MAX`; `inf * 2` refuses. Plain
    `eval`/`eval_str` keep their IEEE propagation, and `oce-expr` runs only at load, never per
    tick. Conditional-component guards use the strict evaluator too.
  - **On the flattened graph**, an `oce-validate` backstop applies the same code to every block
    parameter. For that block it suppresses all class-rule diagnostics, including
    `missing-required-parameter` and array-shape ones, which surface once the value is fixed.
  - **At rest**, `set_param` returns the existing `OcError::ParamRange`, using the new shared
    predicate `oce_validate::parameter_value_is_finite`.
  - **Existing tests:** NaN cases on classes that carried finiteness rules now report
    `non-finite-parameter` instead of `parameter-out-of-range`.
  - **No opt-out.** No registered class takes a non-finite parameter.
  - **State snapshots** were already safe: they compare parameters bit-exactly and never restore
    them.
  - **Tests:** a census over all 103 Real parameters of the 136 registered classes pins both the
    load and the edit refusal. A corpus golden, generated from the pre-change resolver, proves all
    213 tracked `.jsonld` fixtures import byte-identically. Non-finite input *signals* are
    unchanged.
  - The release-compatibility checker re-selects its current implementation baseline at the
    change's source commit; policy and rows are unchanged.

## 0.2.0

Prepared 2026-10-05 on `development`; the release date is the date the `v0.2.0` tag is cut from
`main`. Every workspace member is 0.2.0. Nothing is published to crates.io.

### Release

- **Workspace version 0.2.0** (GitHub #333). All 17 members and the internal path+version table move from 0.1.0;
  `CompatibilityDescriptor` reports `oce-api-version:0.2.0`, so persisted descriptor receipts and
  replay records from 0.1.0-labelled builds now refuse with a `Build` mismatch — which
  the fail-closed policy already required, since equal package strings were never build authority.
  No catalog, contract-schema, state-format, execution-ABI or replay-format revision changes.
- **A compatibility manifest is published with the release**
  ([`docs/compatibility-manifest.json`](docs/compatibility-manifest.json)): the canonical descriptor,
  each contract domain's schema revision and schema-bytes tag, the CXF import contract and vendored
  CDL source commits, the composite rule ids, the catalog content id, the content id of every
  complete export in the 47-document G36 CXF corpus (34; the other 13 carry their byte tag and
  deferral-warning count), and the state and replay format revisions. It is generated and held
  current by `crates/oce-api/tests/compatibility_manifest.rs`.
- **The release-compatibility candidate is re-selected at the version bump.** The retained matrix
  now names that commit as the current implementation baseline and 0.2.0 as the current package
  against the historical 0.1.0 pin; its 36 directed rows and the no-N-1 policy are unchanged.
- **The retained native strict-bit receipt is refreshed for 0.2.0.** The receipt binds
  `Cargo.toml` and `Cargo.lock` bytes, so the version bump took a new native Linux
  x86_64/aarch64 capture (run 37382761120, zero mismatches, byte-identical repeats) admitted
  data-only per [strict-bit evidence](docs/strict-bit-evidence.md); no bound source changed.

### Facade contraction

- **Frame-only execution, product-contract revision 9** (#325): preparation followed by consuming
  `execute_frame` is the sole public state-advancing execution path. Removed sparse staging,
  Store-backed ticks, built-in simulation/realtime profiles, epoch configuration, raw `Outputs`
  access and associated source/spec/trace/report types without aliases. `CompletedFrame` retains
  immutable boundary outputs and warnings; `get_output` and `watch` remain latest-state non-receipt
  inspections. Frame execution performs no Store reads or writes. First-party consumers and both
  conformance cadences now submit complete frames; omitted determinants refuse before comparison.
  HostTick v1, input domains, independent numerical references, catalog and state bytes are unchanged.
  Assertion/execution descriptors advance to revision 2; public-surface and product-contract hostile
  controls guard the contraction. Sibling migration and external delivery orchestration remain host work.
- **Earlier intentional pre-release source break** (#316): removed `Engine::load_modelica`,
  `Engine::load_from_semantic`, `TemplateRef`, the flat `oce_api::SemanticQuery` alias and
  `InputSource::Csv`, all tied to never-working facade paths. Conditional storage query types and
  working CXF/Constant/Closure behavior were retained at that revision; the frame-only contraction
  above supersedes those execution conveniences. Reference CSV/table replay stays host-side.
- **Warning-only assertions** (#316): removed the never-emitted `AssertLevel::Error` variant and changed
  `Default` from Error to Warning deliberately. Existing Warning collection, execution and state
  bytes are unchanged; this adds no escalation or safety policy. `point_list(None)` keeps its
  signature/behavior; device filtering remains outside support with the same `OcError::Load`
  refusal. See [migration notes](docs/facade-migration.md) and product-contract revision 2.

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
- **Activity does not suppress active leaf parameter bindings** (#303). A false-guarded sibling can
  mark another leaf's parameter node inactive, but the active leaf still grounds it through either
  ordinary `S231:hasParameter` or a classified `S231:hasInstance` member. Twin accepted fixtures
  render byte-identically and pin `CDL.Reals.Sources.Constant.k` to the exact bits for `7.0`;
  applying the forbidden member filter removes that golden row and makes the public API reject the
  missing required parameter.
- **The unclassifiable-member refusal has a count-preserving control** (#304). The accepted and
  rejected documents are now six-member `CDL.Reals.Sources.Sin` twins whose bytes differ only in
  the final node-less member name: declared output `y` versus unsupported `zzz`. A relational test
  pins both member counts and the single member-IRI substitution, so a cardinality-based refusal
  can no longer masquerade as name classification.
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

- **Complete input frames** (#321, #322, #323). `prepare_frame` builds an owned, generation-bound
  input frame and refuses unloaded, stale, unknown, wrong-direction, duplicate, missing, type,
  domain and time failures before any mutation; consuming `execute_frame` then runs one in-place
  HostTick transition and returns an immutable `CompletedFrame` with lexical boundary outputs,
  warnings and a lifetime-local sequence. Frame execution makes no Store or host callback. Native
  frames and the then-remaining legacy ticks shared one private HostTick evaluation core (#323)
  until #325 made frames the only execution surface.
- **Versioned catalog and diagnostic receipt contracts** (#317). The facade exposes a typed
  catalog projection with canonical JSON, a packaged JSON Schema and a deterministic
  `catalog:1:fnv1a128:` content id, plus immutable load/export diagnostic receipts with
  stage-aware machine keys; failures keep the original error and the diagnostics of completed
  stages. IO, value, parameter, assertion and execution-profile contracts are documented and
  packaged as versioned descriptors.
- **Bounded CXF admission** (#318). Serialized CXF above 8 MiB (`MAX_CXF_BYTES`) is refused before
  parsing or any Store call, with a per-engine tighten-only limit and typed count-only refusals. A
  failed reload preserves the complete in-memory run image; Store effects of a failed load are not
  rolled back and are documented as host compensation.
- **Closed host compatibility descriptor** (#326). `CompatibilityDescriptor` records public
  catalog, schema-revision, HostTick-profile, package-version and optional complete-export facts in
  ten canonical lines, with distinct `CatalogContentId` and `CompleteExportContentId` wrappers and
  an exact first-cause `CompatibilityMismatch`. It is not a build, executable or state identity.
- **State continuation and portability contracts** (#328). Same-loaded-executable continuation is
  the supported contract, under a mandatory host build/deployment envelope; snapshot portability
  (`Portable` or `TargetBound`) is readable. State format and execution ABI advance to revision 2 so
  effective input bounds and computation units participate in compatibility. **Revision-1
  snapshots refuse; there is no migration.**
- **Canonical exact replay records** (#329). A bounded revision-1 `ReplayRecord` captures one
  accepted complete-frame transition — descriptor, placement, inputs, outputs and diagnostics, with
  exact Real bits — without a second execution path. Ordering, authentication, storage and snapshot
  sidecars stay host-owned; hosted cross-architecture replay comparison remains pending.
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

- **Strict-bit evidence for the 21 aligned-tolerance Real signals** (#327). Canonical raw-bit
  captures from native Linux x86_64/aarch64 × debug/release runs are retained under a 35-source
  receipt, and those cells now compare exactly; macOS and other targets keep the 1e-12 aligned
  band. Closes #250. See [strict-bit evidence](docs/strict-bit-evidence.md).
- **Fail-closed release-compatibility evidence** (#330). A machine-checked 36-row directed matrix
  covers the current candidate and the historical `v0.1.0` pin under the owner-selected policy:
  current/current only, no N-1 support or migration. Host-boundary cold-start and external-rollback
  fixtures and a migration/fallback guide accompany it. See
  [release compatibility](docs/release-compatibility.md).
- **CXF composition ordering identity is pinned** (#331). The existing executable ordering profile
  is frozen with hand-derived permutation, serialization, diagnostic and export-survivor evidence:
  `containsBlock` traversal stays depth-first array order, and dense ids remain internal storage,
  never durable host identity. No resolver behaviour changed.
- **CDL Integer conformance goldens stay in the portable 32-bit domain** (#324). Generator and
  corpus guards reject out-of-domain integer fixtures and unsafe integer-to-real serialization;
  wide-i64 stress coverage stays in `oce-blocks` unit tests.
- **Durable state continuation is proven over two published Library fault rules** (GitHub #332). A new
  `oce-api` test loads AHU-0016 (TrueDelay) and AHU-0004 (MovingAverage plus TrueDelay) from
  byte-copied Library fixtures, snapshots after every tick (single and chained restarts), restores
  into a freshly loaded engine through `EngineStateSnapshot::from_bytes` and `restore_state`, and
  shows boundary outputs, internal outputs and canonical state bytes identical to an uninterrupted
  run. Restore guards refuse an advanced target and a foreign execution. Restore continues model
  time, so the meaning of an outage gap is host policy, not engine state; this is documented in
  `docs/state-compatibility.md`. Test and docs only; no public surface changes.
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
- **One finite Real OpenModelica differential now exists for `CDL.Reals.Line`** (#306).
  Four explicit limit modes run over below-range, endpoint, interior, and above-range inputs whose
  operations are exact in binary64. Two repeat-identical runs are retained from native arm64 and
  amd64 hosts, and their strict keep-last canonical outputs match exactly. The public facade runs
  four closed views at the emitted event instants with zero tolerances and no warnings; both
  implementations are checked against an independent expected-bit table. Native keep-first output
  and inspection metadata are retained for each architecture. The structural sentinel executes the
  explicit keep-first path from raw input, reproduces both artifacts exactly, and derives schedule
  mismatches at rows 2, 4, 6, and 8. This is not a facade-comparator control; consistent pre-event
  rows can remain green for this stateless block. External flag, output mapping, and arithmetic
  reference mutations fail through the facade comparator at pinned rows.
  Native records bind pinned Rust/Python artifact tools and distinguish committed source bytes from
  materialized `git archive` bytes, including the explicit Modelica package attribute override.
  Raw repeat identity is per architecture; only canonical output is compared across architectures.
  The result covers only this finite matrix: global Tier 3 stays skipped, with no arbitrary Real,
  sequence, solver, general tolerance, or cross-architecture raw-byte claim.
- **One composed G36 leaf now has scoped OpenModelica evidence** (#307, #308). The pinned Buildings
  `Reliefs` class runs one seven-state dyadic input schedule with source-default parameters on native
  arm64 and amd64 hosts. Repeat raw runs are identical within each architecture, and strict
  keep-last canonical bytes match across architectures. The public facade compares all 14
  `yOutDam` and `yRetDam` cells exactly at the emitted timestamp bits; topology checks preserve the
  declared root identities while binding them to the internal Min and Max drivers. Controls cover a
  parameter-only mutation, swapped and nonexistent roots, explicit keep-first projection, and both
  final overwrite paths. The retained manifest binds the source cone, OCI graph, host artifact
  toolchain, sandbox limits, cgroup peak measurement, and native logs. This remains one composed
  leaf at one parameterization and schedule: global Tier 3 stays skipped, with no broader class,
  solver, tolerance, or cross-architecture raw-byte claim.
  Follow-up hardening binds both architectures to a fixed execution-checkout observation, a complete
  generator-input digest map, and the exact retained input bytes without requiring Git ancestry.
  The manual workflow verifies both native artifacts, admits them through the documented assembler,
  and uploads the emitted candidate manifest and contract. Run logs are closed records with one
  exact `SimulationResult`, bounded timings, fixed metadata, and a terminal completion marker. Rust
  and Python reject the same provenance fields, and the final-clamp control fixes every raw and
  selected input tuple before accepting its outputs.
  Deadline polling refreshes before each Docker subprocess, Python entrypoints suppress bytecode,
  and Windows retained-evidence reads reject reparse points, hardlinks, and handle-identity changes.
- **Conformance driver failures preserve the load/runtime boundary** (#296, fixes #291).
  `DriverError::Load` identifies failure at `Engine::load_cxf`, and only that variant becomes a
  failed Tier 0 static-load report. Facade failures after a successful load remain driver errors
  instead of being recast as conformance verdicts. Tests pin both malformed CXF and an executed
  time-regression failure.
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

- **Composite boundary lowering is iterative and resource-bounded** (#290, #298, #299, #300). A
  path may enter at most 64 non-top `isConnectedTo` boundary nodes, and one document may cause at
  most 65,536 target
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
  The composite-nesting refusal runs before orientation indexes are built and stops pre-lowering,
  so an over-depth graph cannot trigger the prior quadratic ancestor work.
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

- **Executable-CXF/HostTick product contract** (#315), with a traceability checker over its 40
  requirements; the complete generation-atomic frame contract was ratified ahead of its
  implementation (#320). Later API PRs advanced the contract revision as they promoted
  requirements.
- **Public surface, package and authority policies are machine-checked.** A public-surface contract
  and exhaustive ledger classify `oce-api` and `oce-store` (#310, closes #254); a package,
  feature and publication policy fixes 12 publishable and 5 private members and pins the approved
  release workflow by raw-byte SHA-256 (#313); and a closed authority-claim index with a supersession
  map keeps cross-document claims consistent (#314).
- **A dated stability baseline and downstream pin ledger** (#309) records the 2026-08-26 branch
  topology and downstream pins as historical evidence, with a standard-library verifier.
- Agent workflow guidance now defers to the active client and treats historical handoffs as evidence,
  not current authority (#319).
- **CI moved back to GitHub Actions; Forgejo retired** (GitHub #334). GitHub is the primary host
  again for code, PRs, issues and CI. The per-PR gate is `.github/workflows/pr-gate.yml` (PRs into
  and pushes to `development`), alongside `release-gate.yml` (release PRs into and pushes to
  `main`, plus the daily `development`-tip run), `advisories.yml` and `docs-pages.yml`, which again
  validates docs PRs and publishes `main` to Pages. Each gating workflow keeps its single `CI OK`
  status and the emulated aarch64 legs; actions in those workflows are pinned to commit SHAs with
  read-only token permissions, and `check-workflow-gates.sh` now refuses a tag-referenced action.
  `.github/workflows/ci.yml` stays byte-frozen as a bound strict-bit source and is disabled in the
  Actions settings; `.forgejo/workflows/` is removed.
- **CI moved to Forgejo Actions** (Forgejo #332). Forgejo became the primary host and the GitHub
  repository a public push mirror with Actions disabled. The per-PR gate, release gate, advisories
  scan and docs-site PR validation run from `.forgejo/workflows/` on one Linux x86_64 runner, each gating
  workflow summarized by a single `CI OK` status. The aarch64 legs of the determinism and
  strict-bit matrices now run under QEMU user-mode emulation, which keeps the cross-architecture
  comparisons per PR but is not native aarch64 evidence; the retained native receipt is still
  verified. `.github/workflows/` keeps the crates.io release, a manual-only Pages publish, the
  native-arm64 OpenModelica evidence workflows and a byte-frozen `ci.yml` (a bound strict-bit
  source), all dormant on the mirror.
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
