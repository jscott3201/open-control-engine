# Buildings CDL Reference Catalog

This directory holds the checked-in, non-network reference catalog used to keep Open Control Engine
native CDL coverage aligned with `lbl-srg/modelica-buildings`.

- `Buildings.Controls.OBC.CDL.catalog.json` is the source-verified classification snapshot.
- `Buildings.Controls.OBC.CDL.prov.json` pins the upstream repository, branch, commit, fetch date,
  package-order files, selected structural `.mo` files, and a catalog content hash.

CI and local tests must use these checked-in files only. Updating the catalog requires re-fetching
the upstream files named in the provenance file and updating this snapshot deliberately.

Runtime class source files are derived from the package path and class name, for example
`CDL.Reals.Add` maps to `Buildings/Controls/OBC/CDL/Reals/Add.mo`. Structural packages and drift
entries carry explicit source notes because they are not native runtime block registry entries.
