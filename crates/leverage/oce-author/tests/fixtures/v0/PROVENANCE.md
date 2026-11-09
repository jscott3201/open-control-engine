# Provenance of the v0 graph fixtures

The six files beside this one are verbatim copies (byte for byte, no edits) of the frozen
prototype's three golden pairs: each a compact block graph in the prototype's v0 graph JSON and the
CXF document the prototype's own deterministic emitter wrote for it. They are the conformance
fixtures of `oce_author::import_graph_v0` (importer note `docs/design/importer.md`, decision 18;
GitHub issue #496).

| Field | Value |
| --- | --- |
| Source repository | <https://github.com/jscott3201/mechanical-muse> |
| Pinned commit | `f951346` (`f9513467437b72171fddface05ca0cfe1b7f96e6`) |
| Licence | `MIT OR Apache-2.0` (the source's `LICENSE-MIT` and `LICENSE-APACHE`) |
| Copyright | Copyright (c) 2026 Justin Scott |

The CXF files keep their bytes but not their extension: the source's `.cxf.jsonld` is renamed
`.cxf.json` here, as the emitter's goldens are named, because `oce-cxf`'s
`parameter_grounding_corpus` test counts and pins every tracked `.jsonld` file in the workspace and
a leverage fixture must not move an engine census.

| File here | Source path at the pinned commit |
| --- | --- |
| `ahu_0005.graph.json` | `crates/mm-graph/tests/golden/ahu_0005.graph.json` |
| `ahu_0005.cxf.json` | `crates/mm-graph/tests/golden/ahu_0005.cxf.jsonld` |
| `ahu_0027.graph.json` | `crates/mm-graph/tests/golden/ahu_0027.graph.json` |
| `ahu_0027.cxf.json` | `crates/mm-graph/tests/golden/ahu_0027.cxf.jsonld` |
| `width_max_min.graph.json` | `crates/mm-graph/tests/golden/width_max_min.graph.json` |
| `width_max_min.cxf.json` | `crates/mm-graph/tests/golden/width_max_min.cxf.jsonld` |

The prototype is licensed `MIT OR Apache-2.0` with the same copyright holder as this repository,
so the verbatim copies need only this record. They are small conformance fixtures, not control
sequences (standing rule 7 is kept); the prototype's benchmark graphs belong to GitHub #457.

The prototype's CXF names every node under `urn:mechanical-muse:<name>#`, which ends in `#`, so
its names reduce under the content-id naming rule exactly as the authoring emitter's do and the
pairs need no re-stamp. `width_max_min` uses the width-driven `Reals.MultiMax` and
`Reals.MultiMin` (`nin = 3`), exercising port synthesis.

The `*.import.txt` files are this repository's own goldens, not copies: what the importer makes
of each graph (the report and the canonical text form, comments included).
