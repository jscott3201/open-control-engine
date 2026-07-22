# Composite-subset conformance corpus

The fixture files behind the published composite-subset contract
(`docs/cxf-composite-subset.md`). An external CXF emitter can validate its output against exactly
these documents: everything under `accepted/` imports warning-free, everything under `rejected/`
fails with the pinned `(DiagCode, subject, message)` triple. The drivers live in
`crates/oce-cxf/tests/composite_contract_corpus.rs` (resolver layer) and
`crates/oce-api/tests/conformance.rs` (full `Engine::load_cxf` pipeline).

**Index scope:** this index covers the `*.jsonld` fixture files only. The accepted fixtures'
byte-exact goldens live in the shared golden tree
(`tests/fixtures/golden/composite_contract_*.modelgraph.txt`), not under this directory; re-bless
them with `OCE_BLESS=1 cargo test -p oce-cxf --test composite_contract_corpus`. The
`readme_index_lists_exactly_the_sorted_jsonld_corpus` test holds this index equal to the on-disk
listing — adding, removing, or renaming a fixture without updating this file fails the suite.

## Accepted

| Fixture | Rules | Demonstrates | Expected outcome |
| --- | --- | --- | --- |
| `accepted/minimal_nested.jsonld` | 1, 3, 5, 6 | One unregistered composite under the root; sibling parameter reference (`kTop = kBase + 0.25`), `hasConstant` grounding, parent-scope inheritance into the leaf gain; boundary input rewired to the leaf and non-top boundary output followed through. | Imports warning-free; golden `composite_contract_minimal_nested.modelgraph.txt`. |
| `accepted/two_level_nesting.jsonld` | 1, 3, 5, 6 | Two composite levels (`root → outer → inner → leaf`); the parameter scope chains across both levels (`kRoot → kOuter → kInner = kOuter + 1.0 → gain.k = 4.0`); boundary connectors elided at every level. | Imports warning-free; golden `composite_contract_two_level_nesting.modelgraph.txt`. |
| `accepted/registered_leaf_carveout.jsonld` | 1 | The rule 1 carve-out: a registered `Constant` leaf carrying `containsBlock` (a protected implementation child) under a valid unregistered top composite. The leaf imports as a normal block; the protected child is elided. | Imports warning-free; golden `composite_contract_registered_leaf_carveout.modelgraph.txt`. |

## Rejected

| Fixture | Rule | Demonstrates | Expected outcome |
| --- | --- | --- | --- |
| `rejected/multi_root.jsonld` | 2 | Two unreferenced composites (`#M`, `#M2`). | `composite/root-count` (`malformed-document`); candidates enumerated in `@graph` order, first candidate is the subject. |
| `rejected/pure_cycle.jsonld` | 2 | A pure `containsBlock` cycle `A→B→C→A` with no root at all. | `composite/root-count` (`malformed-document`) with **no subject** — zero candidate roots; never reported as a cycle. |
| `rejected/reachable_cycle.jsonld` | 4 | A valid root reaching the cycle `A→B→C→A`. | `composite/contains-cycle` (`malformed-document`) naming all participants in path order, closing with the re-entered id `#A`, which is the subject. |
| `rejected/self_loop.jsonld` | 4 | A composite containing itself (`A→A`). | `composite/contains-cycle` (`malformed-document`); the degenerate two-entry participant list `#A -> #A`. |
| `rejected/banned_key_bare.jsonld` | 7 | The bare banned key spelling (`redeclare`). | `composite/banned-modelica-key` (`non-subset-construct`); subject is the owning node; the message names the key as authored. |
| `rejected/banned_key_prefixed.jsonld` | 7 | The prefixed banned key spelling (`S231:extendsFrom`). | `composite/banned-modelica-key` (`non-subset-construct`); subject is the owning node; the message names the key as authored. |
| `rejected/banned_key_absolute_iri.jsonld` | 7 | The absolute-IRI banned key spelling (`http://data.ashrae.org/S231P#moSource`). | `composite/banned-modelica-key` (`non-subset-construct`); subject is the owning node; the message names the key as authored. |
| `rejected/array_parameter.jsonld` | 5 | An `isArray` parameter on the top composite. | `composite/array-parameter` (`non-subset-construct`); subject is the parameter node `#M.p`. |
| `rejected/replaceable.jsonld` | 7 | `S231:isReplaceable: true` on a child component. | `composite/replaceable` (`unresolved-polymorphism`); subject is the replaceable node `#M.c2`. |
