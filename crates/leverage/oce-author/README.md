# oce-author

The authoring layer's library (M2; design note [`docs/design/authoring-layer.md`](../../../docs/design/authoring-layer.md),
issue #463). A leverage crate: it depends on `oce-api` only, never evaluates an expression and never
loads an engine. `publish = false`.

| Surface | What it is |
| --- | --- |
| `Sequence` and its statements | The typed model: one flat sequence of parameters, boundary inputs and outputs (with `quantity`, `display_unit` and a closed `SemanticPoint`), leaf blocks with ordered assignments and typed connections, in authored order. Each statement carries text-form `Annotations` (comments, a blank line before it) and assignments and parameters a text-only unit suffix; the wire form and the emitter ignore them. The model types are `#[non_exhaustive]`: build through the constructors or `SequenceBuilder`. |
| `SequenceBuilder` | A checked builder; refuses with a typed `BuildError` what it can see without the catalog. |
| `from_wire_str`, `from_wire_bytes`, `to_wire_string` | The strict-JSON wire form: closed objects, every key required, absent values `""`/`[]`/`false`, connections as `"a.y -> b.u"`. Reads are bounded (4 MiB, depth 8, 65,536 entries per list, 4,096-byte strings) and refuse hostile input with a typed `WireError`; writes are canonical and byte-stable. |
| `WIRE_SCHEMA`, `wire_schema()` | The JSON Schema (draft 2020-12) derived from the wire DTOs with `schemars`, published at [`schema/sequence.schema.json`](schema/sequence.schema.json) and pinned byte for byte; the `class` enum is the catalog's authorable class paths. Strict profile: closed objects, all required, `anyOf` only over the value shapes. |
| `validate`, `validate_author_side`, `Finding` | The stage-1 validator: twenty finding codes (`STAGE_ONE_CODES`), sorted by authored position, code and detail. The engine-side stage arrives with the emitter. |

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
