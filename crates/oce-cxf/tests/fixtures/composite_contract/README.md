# Composite-subset conformance corpus

The fixture files behind the published composite-subset contract
(`docs/cxf-composite-subset.md`). An external CXF emitter can validate its output against exactly
these documents: everything under `accepted/` imports warning-free, everything under `warned/`
imports successfully with exactly its pinned warning vector, and everything under `rejected/`
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
| `accepted/forward_sibling_reference.jsonld` | 1, 3, 5, 6 | The mutual own scope: the root's first parameter references a later-declared sibling parameter AND a sibling constant (`kDerived = kBase + cShift`) — declaration array order carries no meaning for a composite's own scope, and a parameter may read a sibling constant. | Imports warning-free; golden `composite_contract_forward_sibling_reference.modelgraph.txt`. |
| `accepted/leaf_array_parameter_conditional_member.jsonld` | 1, 5 | A leaf carrying a legal array parameter (`holdSet[2]`, expanded per-element) plus a false-guarded conditional input. The specialization pass grounds the leaf's chain for the guard, but its generic grounding machinery is non-emitting — the array binding's scalar-only grounding failure there no longer refuses the document. | Imports warning-free (conditional input pruned); golden `composite_contract_leaf_array_parameter_conditional_member.modelgraph.txt`. |
| `accepted/leaf_identity_parameter_modification.jsonld` | 1, 5 | The leaf identity-modification idiom: a leaf parameter `samplePeriod = "samplePeriod"` reading the same-named enclosing composite parameter, beside a false-guarded conditional input. The self-reference is a member modification, not an own-chain cycle — the tagged rules apply only to composite chains at the specialization pass — and the member value grounds enclosing-first. | Imports warning-free (`con.samplePeriod = 120.0`, conditional input pruned); golden `composite_contract_leaf_identity_parameter_modification.modelgraph.txt`. |
| `accepted/minimal_nested.jsonld` | 1, 3, 5, 6 | One unregistered composite under the root; sibling parameter reference (`kTop = kBase + 0.25`), `hasConstant` grounding, parent-scope inheritance into the leaf gain; boundary input rewired to the leaf and non-top boundary output followed through. | Imports warning-free; golden `composite_contract_minimal_nested.modelgraph.txt`. |
| `accepted/two_level_nesting.jsonld` | 1, 3, 5, 6 | Two composite levels (`root → outer → inner → leaf`); the parameter scope chains across both levels (`kRoot → kOuter → kInner = kOuter + 1.0 → gain.k = 4.0`); boundary connectors elided at every level. | Imports warning-free; golden `composite_contract_two_level_nesting.modelgraph.txt`. |
| `accepted/registered_leaf_carveout.jsonld` | 1 | The rule 1 carve-out: a registered `Constant` leaf carrying `containsBlock` (a protected implementation child) under a valid unregistered top composite. The leaf imports as a normal block; the protected child is elided. | Imports warning-free; golden `composite_contract_registered_leaf_carveout.modelgraph.txt`. |

## Warned

| Fixture | Rule | Demonstrates | Expected outcome |
| --- | --- | --- | --- |
| `warned/undriven_boundary_output.jsonld` | — | A root `hasOutput` declaring a node (`#M.y`) that exists but that no internal connector or boundary input drives. Untagged boundary-interface machinery, not a composite-shape rule. | Loads successfully with exactly one `undriven-boundary-output` **warning** (subject `#M.y`); the declared output enters no point surface. |

## Rejected

| Fixture | Rule | Demonstrates | Expected outcome |
| --- | --- | --- | --- |
| `rejected/multi_root.jsonld` | 2 | Two unreferenced composites (`#M`, `#M2`). | `composite/root-count` (`malformed-document`); candidates enumerated in `@graph` order, first candidate is the subject. |
| `rejected/pure_cycle.jsonld` | 2 | A pure `containsBlock` cycle `A→B→C→A` with no root at all. | `composite/root-count` (`malformed-document`) with **no subject** — zero candidate roots; never reported as a cycle. |
| `rejected/reachable_cycle.jsonld` | 4 | A valid root reaching the cycle `A→B→C→A`. | `composite/contains-cycle` (`malformed-document`) naming all participants in path order, closing with the re-entered id `#A`, which is the subject. |
| `rejected/self_loop.jsonld` | 4 | A composite containing itself (`A→A`). | `composite/contains-cycle` (`malformed-document`); the degenerate two-entry participant list `#A -> #A`. |
| `rejected/diamond_cycle.jsonld` | 4 | One structural cycle reachable via two paths (root contains `A` and `B`; `A→C`, `B→C`, `C→A`). | **Two** `composite/contains-cycle` (`malformed-document`) diagnostics — one per re-entry (`#A -> #C -> #A` subject `#A`, then `#C -> #A -> #C` subject `#C`); consumers must not assume one diagnostic per structural cycle. |
| `rejected/declaration_cycle.jsonld` | 5 | A reference cycle between two of the root's own parameters (`a = "b + 1.0"`, `b = "a + 1.0"`). | `composite/declaration-cycle` (`malformed-document`); ONE diagnostic per distinct cycle **per chain evaluation** — a composite evaluated once per `containsBlock` path can surface the same cycle once per visit, like `contains-cycle`. Subject is the participant earliest in chained declaration order (`#M.a`); the message's arrow list is the participant ring in chained order closing on the first, not the discovered edge path. |
| `rejected/self_reference.jsonld` | 5 | A self-referencing inner-composite parameter (`x = "x * 2.0"`) under a same-named enclosing binding — a length-1 cycle, never an enclosing read. | `composite/declaration-cycle` (`malformed-document`); the degenerate two-entry participant list `#M.sub.x -> #M.sub.x`. |
| `rejected/duplicate_declaration.jsonld` | 5 | One local name (`k`) bound twice in one composite's own chain — once under `hasParameter`, once under `hasConstant`. | `composite/duplicate-declaration` (`malformed-document`); one diagnostic per occurrence beyond the first in chained order, subject is that later occurrence (`#M.settings.k`), message names it and the first occurrence's `@id`. |
| `rejected/banned_key_bare.jsonld` | 7 | The bare banned key spelling (`redeclare`). | `composite/banned-modelica-key` (`non-subset-construct`); subject is the owning node; the message names the key as authored. |
| `rejected/banned_key_prefixed.jsonld` | 7 | The prefixed banned key spelling (`S231:extendsFrom`). | `composite/banned-modelica-key` (`non-subset-construct`); subject is the owning node; the message names the key as authored. |
| `rejected/banned_key_absolute_iri.jsonld` | 7 | The absolute-IRI banned key spelling (`http://data.ashrae.org/S231P#moSource`). | `composite/banned-modelica-key` (`non-subset-construct`); subject is the owning node; the message names the key as authored. |
| `rejected/array_parameter.jsonld` | 5 | An `isArray` parameter on the top composite. | `composite/array-parameter` (`non-subset-construct`); subject is the parameter node `#M.p`. |
| `rejected/array_connector.jsonld` | 7 | An active connector referenced by a block instance, carrying both array markers (`isArray: true` and `sizeOfDimensions`); either marker alone also rejects. | `composite/array-connector` (`non-subset-construct`); subject is the connector node `#M.c2.u`. |
| `rejected/array_instance.jsonld` | 7 | An active `isArray` block instance in `containsBlock`. | `composite/array-instance` (`non-subset-construct`); subject is the instance node `#M.c2`. |
| `rejected/replaceable.jsonld` | 7 | `S231:isReplaceable: true` on a child component. | `composite/replaceable` (`unresolved-polymorphism`); subject is the replaceable node `#M.c2`. |
| `rejected/shadowed_output_child_connector.jsonld` | — | The root's `hasOutput` referencing a child instance's own port node (`#M.c1.y`), so the declared identity is an existing connector path. Untagged boundary-interface machinery. | `boundary-output-shadows-connector`; subject is the shadowing declared IRI `#M.c1.y`. |
| `rejected/shadowed_output_input_output.jsonld` | — | One IRI (`#M.io`) listed in both the root's `hasInput` and `hasOutput`, so the "declared output" is an input path. Untagged boundary-interface machinery. | `boundary-output-shadows-connector`; subject is the dual-listed IRI `#M.io`. |
| `rejected/multi_driven_boundary_output.jsonld` | — | Two child outputs both wired to one declared boundary output (`#M.y`). Untagged boundary-interface machinery. | `single-assignment`; subject is the multiply driven declared IRI `#M.y`. |
