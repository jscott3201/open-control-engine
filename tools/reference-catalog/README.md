# Buildings CDL Reference Catalog

This directory holds the checked-in, non-network reference catalog used to keep Open Control Engine
native CDL coverage aligned with `lbl-srg/modelica-buildings`.

- `Buildings.Controls.OBC.CDL.catalog.json` is the source-verified classification snapshot.
- `Buildings.Controls.OBC.CDL.prov.json` pins the upstream repository, branch, commit, fetch date,
  package-order files, selected structural `.mo` files, and a catalog content hash.
- `Buildings.Controls.OBC.ASHRAE.G36.catalog.json` is the source-verified sequence/profile
  classification snapshot for the first G36 composite-sequence phase. It is intentionally
  conservative: current in-tree G36 examples are `supported-fixture-only`, not canonical upstream
  composite runtime support.
- `Buildings.Controls.OBC.ASHRAE.G36.prov.json` pins the upstream G36 package/type files and local
  evidence files used by that sequence/profile snapshot.

CI and local tests must use these checked-in files only. Updating the catalog requires re-fetching
the upstream files named in the provenance file and updating this snapshot deliberately.

Runtime class source files are derived from the package path and class name, for example
`CDL.Reals.Add` maps to `Buildings/Controls/OBC/CDL/Reals/Add.mo`. Structural packages and drift
entries carry explicit source notes because they are not native runtime block registry entries.

G36 sequence entries have a separate support vocabulary. `supported-runtime-sequence` requires a
canonical `Buildings.Controls.OBC.ASHRAE.G36.*` class path, source provenance, supported parameter
variants, fixture, deterministic golden trace, and independent oracle evidence. `supported-fixture-only`
means the current fixture is a hand-authored, pre-flattened CXF graph whose executable child blocks
are native CDL entries.
