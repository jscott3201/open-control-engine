# Buildings CDL Reference Catalog

This directory holds the checked-in, non-network reference catalog used to keep Open Control Engine
native CDL coverage aligned with `lbl-srg/modelica-buildings`.

- `Buildings.Controls.OBC.CDL.catalog.json` is the source-verified classification snapshot.
- `Buildings.Controls.OBC.CDL.prov.json` pins the upstream repository, branch, commit, fetch date,
  package-order files, selected structural `.mo` files, and a catalog content hash.
- `Buildings.Controls.OBC.ASHRAE.G36.catalog.json` is the source-verified sequence/profile
  classification snapshot for the first G36 composite-sequence phase. It is intentionally
  conservative: current in-tree G36 examples include selected restricted explicit-CXF
  `supported-runtime-sequence` variants, `supported-fixture-only` representative fragments, and
  restricted `supported-import-fixture` structural evidence; they are not broad canonical upstream
  runtime-sequence support.
- `Buildings.Controls.OBC.ASHRAE.G36.prov.json` pins the upstream G36 package/type files and local
  evidence files used by that sequence/profile snapshot.
- `modelica-buildings-cdl.hash-manifest.json` pins every whitelisted byte in the vendored
  Modelica Buildings CDL tree, including per-file Git blob OIDs, SHA-256 digests, sizes,
  provenance buckets, and a `subtree_tree_sha` field recomputed from the on-disk bytes. The
  independent pin is the hand-edited `SUBTREE_TREE_SHA` Rust constant. The
  `third_party_manifest::checked_in_manifest_bytes_equal_fresh_render` test in the structural
  oracle input-hygiene binary keeps it byte-identical to the tree. Re-bless a deliberate vendor
  update with `OCE_BLESS=1 cargo test -p oce-cxf --test fixture_structural_oracle
  checked_in_manifest_bytes_equal_fresh_render`, hand-update the Rust tree-SHA constant from the
  documented Git command, and review both diffs.
- `oce-blocks.registry-manifest.json` is the machine-readable manifest of the `oce-blocks` native
  block registry, generated from the registry itself rather than fetched upstream. It is a
  top-level ordered JSON array in registry catalog order; each element carries `class_path`, typed
  positional `inputs`/`outputs` (`"Real"`/`"Integer"`/`"Boolean"`, resolved at default parameters
  — variadic blocks whose width parameters default to 0 record empty port lists), `width_driven`
  (true when a `Structural`/`StructuralArrayElements` rule makes port arity parameter-driven), and
  the complete `param_rules` list (each rule object names its variant under `"rule"` plus every
  embedded guard field in declaration order). Appended metadata records `port_naming`, named
  `input_names`/`output_names` where applicable, the conservative `stateful` class hint,
  `reserved` lowering identities, and `param_defaults`. Defaults distinguish literal, derived,
  and required sources; width-indexed names use the published `<i>`, `<row>`, and `<col>`
  templates. The regenerate-and-diff test
  `registry::manifest_tests::checked_in_manifest_matches_regenerated_bytes` in `oce-blocks` keeps
  this file byte-identical to the live registry; re-bless a deliberate registry change with
  `UPDATE_EXPECT=1 cargo nextest run -p oce-blocks checked_in_manifest_matches_regenerated_bytes`
  and review the diff. Two by-design limits: widened variadic port element types are not in the
  manifest — they are defined by block semantics, so a consumer drawing a widened
  `MultiAnd`/`VectorFilter` instance must look beyond the manifest; and compound param-rule
  semantics (e.g. `RealTimesIntegerInclusiveRange`, or `IntegerArrayElementsInRange`'s integer
  `min` vs parameter-name-string `max`) are defined in the `ParamRule` rustdoc in
  `crates/oce-blocks/src/lib.rs` — the manifest carries names and fields, the rustdoc carries
  meaning.
- `oce-cxf.composite-rules.json` is the machine-readable catalog of the `oce-cxf`
  composite-subset contract rules, generated from the in-crate rule table
  (`crates/oce-cxf/src/resolve/composite_rules.rs`) rather than fetched upstream. It is a
  top-level JSON object keyed by rule id, in rule-table order; each entry carries `diag_code`
  (the `DiagCode::as_str` string the rejection is emitted under), `message_prefix` (the exact
  `composite/<rule-id>: ` tag every rejection message under that rule begins with — the prefix
  is the machine convention, the remainder of the message stays human prose), and a one-line
  `summary`. An external CXF generator maps a rejection back to its rule by matching the message
  prefix, and back to its source graph node by the diagnostic `subject`. The regenerate-and-diff
  test `resolve::composite_rules_tests::checked_in_catalog_matches_regenerated_bytes` in
  `oce-cxf` keeps this file byte-identical to the live rule table; re-bless a deliberate rule
  change with `UPDATE_EXPECT=1 cargo nextest run -p oce-cxf
  checked_in_catalog_matches_regenerated_bytes` and review the diff. Scope limit by design: only
  the five composite-subset contract rules are cataloged — the resolver's generic lookup-miss
  (`unresolved-reference`) and grounding (`grounding-failed`) diagnostics are shared machinery,
  not contract rules, and carry no tag. The normative statement of the rules behind these
  identities — including the non-rejecting classification/ordering/transform rules that have no
  catalog entry — is `docs/cxf-composite-subset.md`, with its conformance corpus under
  `crates/oce-cxf/tests/fixtures/composite_contract/`; the drift-guard test
  `crates/oce-cxf/tests/composite_contract_doc.rs` holds that document's rule table to this
  artifact.

CI and local tests must use these checked-in files only. Updating the catalog requires re-fetching
the upstream files named in the provenance file and updating this snapshot deliberately.

Runtime class source files are derived from the package path and class name, for example
`CDL.Reals.Add` maps to `Buildings/Controls/OBC/CDL/Reals/Add.mo`. Structural packages and drift
entries carry explicit source notes because they are not native runtime block registry entries.

G36 sequence entries have a separate support vocabulary. `supported-runtime-sequence` requires a
canonical `Buildings.Controls.OBC.ASHRAE.G36.*` class path, source provenance, supported parameter
variants, fixture, deterministic golden trace, and independent oracle evidence. `supported-fixture-only`
means the current fixture is a hand-authored, pre-flattened CXF graph whose executable child blocks
are native CDL entries. Some fixture-only rows are `source-reviewed-fragment` evidence: they record
the upstream files reviewed, fixture-local parameter/input/output manifests, known deferred branches,
and unsupported variants while still avoiding any canonical runtime-sequence claim.
`supported-import-fixture` means a canonical G36 top class is proven through restricted explicit CXF
import, modelgraph/API tests, and source provenance, but still lacks the whole-sequence oracle
evidence required for `supported-runtime-sequence`.
