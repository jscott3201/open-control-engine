# Provenance of the v0 graph fixtures

The six files beside this one are copies of the frozen prototype's three golden pairs, byte for
byte except the `S231` context IRI of the three CXF files (recorded below): each a compact block graph in the prototype's v0 graph JSON and the
CXF document the prototype's own deterministic emitter wrote for it. They are the conformance
fixtures of `oce_author::import_graph_v0` (importer note `docs/design/importer.md`, decision 18;
GitHub issue #496).

| Field | Value |
| --- | --- |
| Source repository | <https://github.com/jscott3201/mechanical-muse> |
| Pinned commit | `f951346` (`f9513467437b72171fddface05ca0cfe1b7f96e6`) |
| Licence | `MIT OR Apache-2.0` (the source's `LICENSE-MIT` and `LICENSE-APACHE`) |
| Copyright | Copyright (c) 2026 Justin Scott |

The three CXF files equal the source's bytes except the `S231` context IRI, which GitHub #527
rebound to `http://data.ashrae.org/S231#` (one occurrence per file, nothing else): the engine
reads the vocabulary in no other namespace, so the prototype's own bytes no longer load. The
graph files are untouched.

| File here | SHA-256 of the source's bytes | SHA-256 here |
| --- | --- | --- |
| `ahu_0005.cxf.json` | `598b250167a77cbaf89204a2a11dcdd3333e1117553813786c8b8bfb67775459` | `c0d7101e401fb887942d80a5b1ba8660627a6c316da134f2f3f7aa85332e60b8` |
| `ahu_0027.cxf.json` | `7198d6e3f4dd76440e01c43eb30540ee1d2be6209b9a571c86e12b842ebafff1` | `6152eccda308ef255eddbeed1413029b7610e767dff2646bee34d61d6f13363c` |
| `width_max_min.cxf.json` | `9bf44e0623d471b29b57968c27a4d923eab8409849d6b1be60039de6efa3d976` | `4ad336d3b714094671bab6354561786e136049c6513cf5ffee95cd25809d8f4d` |

The CXF files do not keep the source's extension either: the source's `.cxf.jsonld` is renamed
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
so the copies need only this record. They are small conformance fixtures, not control
sequences (standing rule 7 is kept); the prototype's benchmark graphs belong to GitHub #457.

The prototype's CXF names every node under `urn:mechanical-muse:<name>#`, which ends in `#`, so
its names reduce under the content-id naming rule exactly as the authoring emitter's do and the
pairs need no re-stamp. `width_max_min` uses the width-driven `Reals.MultiMax` and
`Reals.MultiMin` (`nin = 3`), exercising port synthesis.

The `*.import.txt` files are this repository's own goldens, not copies: what the importer makes
of each graph (the report and the canonical text form, comments included).
