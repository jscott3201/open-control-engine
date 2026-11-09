# oce-author

The authoring layer's library (M2; design note [`docs/design/authoring-layer.md`](../../../docs/design/authoring-layer.md),
issues #463 and #464). A leverage crate: it depends on `oce-api` only, never evaluates an expression,
and loads only throwaway engines of its own (validation stage 2), never one a host runs.
`publish = false`.

| Surface | What it is |
| --- | --- |
| `Sequence` and its statements | The typed model: one flat sequence of parameters, boundary inputs and outputs (with `quantity`, `display_unit` and a closed `SemanticPoint`), leaf blocks with ordered assignments and typed connections, in authored order. Each statement carries text-form `Annotations` (comments, a blank line before it) and assignments and parameters a text-only unit suffix; the wire form and the emitter ignore them. The model types are `#[non_exhaustive]`: build through the constructors or `SequenceBuilder`. |
| `SequenceBuilder` | A checked builder; refuses with a typed `BuildError` what it can see without the catalog. |
| `from_wire_str`, `from_wire_bytes`, `to_wire_string` | The strict-JSON wire form: closed objects, every key required, absent values `""`/`[]`/`false`, connections as `"a.y -> b.u"`. Reads are bounded (4 MiB, depth 8, 65,536 entries per list, 4,096-byte strings) and refuse hostile input with a typed `WireError`; writes are canonical and byte-stable. |
| `WIRE_SCHEMA`, `wire_schema()` | The JSON Schema (draft 2020-12) derived from the wire DTOs with `schemars`, published at [`schema/sequence.schema.json`](schema/sequence.schema.json) and pinned byte for byte; the `class` enum is the catalog's authorable class paths. Strict profile: closed objects, all required, `anyOf` only over the value shapes. |
| `validate`, `validate_author_side`, `Finding` | The two-stage validator. Stage 1 (`validate_author_side`): twenty-one finding codes (`STAGE_ONE_CODES`) from the model and the catalog, a repeated connection target (`single-assignment`) and an Integer above 2^53 on a Real parameter included. Stage 2, only when stage 1 is clean: emit, load in a throwaway `Engine::new()` and map every engine diagnostic back through the IRI scheme — every warning is an error finding with the engine's code (decision 14), `origin = engine`; a subject the scheme cannot name is attached to `sequence` with the raw subject in `detail`. Findings are sorted by authored position, code and detail. |
| `emit`, `EmitOptions`, `EmitError` | The emitter to flat CXF: authored order, never sorted; IRIs `{namespace}{sequence}.{local}` (default `urn:oce:author:`, an override must end in `#`, `/` or `:`); block `@type` under the fixed class base `http://example.org#Buildings.Controls.OBC.`; only the assignments the author wrote; arrays in the CXF array form (element by element under the flattened names when the author left a gap); boundary `unit`, `quantity` and `displayUnit` and no semantic record; a canonical writer (fixed key order, two-space indentation, ASCII, shortest round-trip reals); refuses stage-1 errors (`EmitError::Invalid`), a malformed namespace (`EmitError::InvalidNamespace`) and anything above `oce_api::MAX_CXF_BYTES` (`EmitError::TooLarge`). |
| `import_cxf`, `ImportError` | A minimal importer for the flat corpus dialect (gate item 1); see below. |

## The minimal importer

`import_cxf` reads one flat, single-root CXF document into the model and refuses everything outside
the authoring subset with a typed `ImportError`, never a panic: oversize input (`TooLarge`),
non-strict JSON — a key written twice, an integer outside `i64` (`Malformed`) — a construct outside
the subset (`Unsupported`) or a dangling reference (`Unresolved`). It deliberately ignores
`S231:label` everywhere, a parameter's descriptive `unit`, `quantity` and `displayUnit`, and the
root's `@type`. Gate item 1 is not yet met: of the 46 G36 fixtures, 6 are out of the M2 subset and 4
of the other 40 import and round-trip; the 36 others wait on model decisions (GitHub #475):

| First refusal | Fixtures |
| --- | --- |
| Sequence parameter of an enumeration type (CDL `SimpleController` or a G36 type) | 21 |
| `S231:min` / `S231:max` on a boundary or block-port connector | 10 |
| A G36 (library) enumeration as a block parameter value | 5 |
| Nested composite (out of subset) | 4 |
| Conditional component (out of subset) | 2 |

`ImportError::Unsupported { subject, construct }` carries `construct` as a `String` for now; its
shape is provisional until the full importer's design note (M2 item 5).

The stage-1 / engine parity table, the library round-trip table and the permutation classes are
in the tests (`src/validate/engine_tests.rs`, `tests/library_round_trip.rs`,
`tests/emit_permutations.rs`); emitted byte goldens are under `tests/fixtures/emit/`.

A block parameter `value` takes one of five shapes on the wire:

| Shape | Example | Model |
| --- | --- | --- |
| number | `1`, `0.5`, `1e-7` | `Literal::Integer` with no fraction or exponent, else `Literal::Real` |
| boolean | `true` | `Literal::Boolean` |
| string | `"text"` | `Literal::Text`; always text, never a reference |
| reference | `{"ref": "dTHys"}` | `ParamValue::Ref` to a sequence parameter |
| enumeration member | `{"member": "SimpleController.PI"}` | `Literal::EnumMember` |

A sequence parameter's value takes the same shapes except the reference.

Goldens regenerate only with `OCE_BLESS_AUTHOR=1 cargo nextest run -p oce-author` (see
`.agents/project-facts.md`).
