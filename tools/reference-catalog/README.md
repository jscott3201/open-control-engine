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
- `oce-blocks.registry-manifest.json` is the machine-readable manifest of the `oce-blocks` native
  block registry, generated from the registry itself rather than fetched upstream. It is a
  top-level ordered JSON array in registry catalog order; each element carries `class_path`, typed
  positional `inputs`/`outputs` (`"Real"`/`"Integer"`/`"Boolean"`, resolved at default parameters
  — variadic blocks whose width parameters default to 0 record empty port lists), `width_driven`
  (true when a `Structural`/`StructuralArrayElements` rule makes port arity parameter-driven), and
  the complete `param_rules` list (each rule object names its variant under `"rule"` plus every
  embedded guard field in declaration order). The regenerate-and-diff test
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
