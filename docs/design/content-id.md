# Canonical content id

**Design note, written before any content-id code (GitHub #470).** It fixes what one canonical
content id covers, how a loaded sequence becomes canonical bytes, which hash names those bytes, and
what moves on `ExportReport`, the `OCEB` blob header, the compatibility descriptor and the mapping
table the library re-stamps its cards from. Decisions 1 to 3 are the owner's and are stated here,
not reopened. Decisions 4 to 17 were made while drafting and ruled on in review (see
[Review rulings](#review-rulings)); the note is amended to match. The implementation is the core
PR the issue names; nothing here is built yet.

## What the id is, and is not

It is **one identity for a loaded sequence**: `oce:canon1:` and 64 lowercase hex digits, the
SHA-256 of a canonical byte encoding of the loaded flat graph. Re-serializing the document,
reordering its arrays or moving it to another instance namespace leaves it unchanged; changing
anything that reaches execution or the boundary I/O definitions (the inputs a frame takes, their
types and acceptance domains, and the outputs it returns) moves it. It is not an identity for
every host-visible fact: the point inventory and the snapshot manifest also name internal
connectors, which the id leaves out (see [What equal ids promise](#what-equal-ids-promise)). It replaces today's
`cxf:fnv1a128:` export tag outright (decision 1).

It is not:

- **`LoadReport::model_id`.** That is the authored top-composite `@id` (or a synthetic projection
  key), kept verbatim for diagnostics. It does not move when a parameter is edited and does move
  when only the namespace changes. Both stay, in different roles (PC-021).
- **The old export tag.** `cxf:fnv1a128:` hashed the emitted bytes, so it followed authored
  `@graph` and `containsBlock` order, and it was undefined for the 13 of 46 G36 fixtures whose
  export is partial. The canonical id is defined for every loadable document, partial export or
  not, which the blob note required of it (its decision 5).
- **The blob's per-block CRCs or the class signature hash.** A CRC-32C checks one block's initial
  state words after decode; a class signature hash (FNV-1a-64, GitHub #438) names one class's
  interface, parameter rules and state layout. The canonical id hashes a whole document and
  includes each used class's signature hash, so it composes them; neither replaces it.
- **A build, state or authority identity.** Equal ids on two engine versions do not promise equal
  behaviour; the blob's exact engine-version match still does that. The id is not a signature: the
  host's envelope still owns authenticity (PC-027).

## The canonical graph

The id is computed over the **loaded** model, after the resolver has flattened composites, bound
ports, grounded every parameter and lowered the boundary, and after `oce-validate`'s §7.10
unification. Every document the engine loads has one, whatever dialect it was written in
(`containsBlock` nesting, `hasInstance` members, nested composites), because the loader has already
reduced all of them to one flat `ModelGraph` (decision 4).

### Names

Every instance and boundary IRI is reduced to a **name** before it enters the bytes:

1. **Strip the namespace.** The namespace is the IRI up to and including its last `#`, `/` or `:`.
   The remainder is the name, form 0 (relative). `http://example.org#g36.source.vav.TZon` and
   `urn:oce:author:g36.source.vav.TZon` both become `g36.source.vav.TZon`.
2. **The sequence stays in the name.** The authoring scheme is `{namespace}{sequence}.{local}`, but
   the sequence part cannot be separated reliably: the engine's own exporter replaces the root `@id`
   with `urn:open-control:cxf-export:root` while keeping every instance IRI, and sequence names
   contain dots. So the name is `{sequence}.{local}`, and renaming a sequence moves the id
   (decision 5).
3. **Fall back to the whole IRI, never refuse.** An IRI whose remainder is empty (it ends in a
   delimiter) is kept whole, form 1 (absolute). If two IRIs of one kind (blocks, inputs or outputs)
   reduce to the same relative name, which takes two namespaces in one document, every IRI in that
   collision is kept whole, form 1. Full IRIs are unique in a loadable document, so the names stay
   unique and the encoding stays injective. Such a document loses only namespace invariance. No
   authored or library document reaches this fallback: authored names contain no delimiter, the
   authoring emitter writes one namespace, and every node `@id` in the G36 fixtures reduces
   cleanly.
4. **Class paths are not IRIs here.** The encoder reads `BlockInstance::class_iri`, which despite
   its name holds the bridged class path (`CDL.Reals.Add`) the resolver took from the class IRI's
   fragment (authoring note decision 10), so no class base or instance namespace reaches the bytes.

No other normalization is applied: no Unicode normalization, no case folding, no percent-decoding.
Names compare and sort by `(form, UTF-8 bytes)`.

Every G36 fixture uses the one namespace `http://example.org#`, and no node `@id` in them has a
`#`, `/` or `:` after it, so no fixture needs form 1. A namespace that does not end in a delimiter
(`http://example.org/lib.`) would leak its tail into every name; `oce-author` therefore refuses an
`--namespace` override that does not end in `#`, `/` or `:` (review ruling 2). The exporter's
synthetic root is unchanged.

### Interface

**Boundary inputs**, one record per `ModelGraph::boundary_inputs` entry (an input with at least one
surviving target connector; an unused input is not part of the executable and not part of the id),
sorted by name:

- the name;
- the value type, Enum with its class path;
- `quantity`, `unit` and `displayUnit` as loaded, each optional;
- the **effective** acceptance domain, min then max, exactly as `Engine::input_definitions` reports
  it and the blob carries it: the declaration's bounds intersected with every target connector's;
- the fan-out targets as `(block name, input port index)`, sorted, pass-through blocks excluded.
  An input whose only target is a pass-through still has a record, with an empty target list: the
  resolver pushes the pass-through's input connector onto `external_inputs` before
  `boundary_inputs` is materialized, so the declaration survives.

**Boundary outputs** are the same disjoint union `executable_boundary_outputs` (in
`crates/oce-api/src/io.rs`) and `Topology::boundary_outputs` use: every
`ModelGraph::boundary_outputs` entry, plus the minted output connector of every block whose class
path starts with `urn:oce:lowering#`. `ModelGraph::boundary_outputs` alone is not enough: the
pass-through lowering (`pass_through.rs`) mints the output connector and the block but no
`BoundaryOutput`. One record per output, sorted by name:

- the name: the declaration's IRI, or for a pass-through the minted output connector's IRI (the
  declared output's `@id`);
- the value type;
- `quantity`, `unit` and `displayUnit` as loaded: the declaration's attributes after §7.10
  unification, or for a pass-through the minted output connector's, which carry the declared
  output node's authored attributes;
- the driver: `(block name, output port index)`, or for a pass-through the name of the boundary
  input whose connector feeds the pass-through's input (that connector's IRI is the input's).

**Pass-through blocks are not blocks.** When a boundary input feeds a boundary output directly, the
resolver mints a reserved `urn:oce:lowering#PassThrough.{Real,Integer,Boolean}` block with no
instance IRI. The id records it only as the output's driver ("input `X`") and leaves it out of the
block list and out of the input's target list, as `Topology` already does. Reserved means the
catalog's `reserved` flag, which is set for exactly those three classes (decision 6). No G36
fixture has one; the test plan adds one.

### Blocks

One record per non-reserved block, sorted by name: the name, the class path, the class signature
hash (`oce_blocks::class_signature_hash`), and the parameter table.

**Parameters.** The loaded `ParamTable` holds exactly the parameters the document bound, each
evaluated by `oce-expr` to a ground value at load (arrays already flattened to `k_1`, `table_1_2`).
The id writes them **sorted by name**, not in table order: table order is the authored
`hasParameter` order, which the blob keeps because `make` receives it. Values are the blob's tagged
values: Real as `f64::to_bits`, Integer `i64`, Boolean, String, Enum as class path plus ordinal.

Sorting is sound only if no class reads two entries as the same parameter, and today one can.
Duplicate detection is by exact name, but `TimeTable` walks `params.values` in table order and
parses `table_<r>_<c>` and `offset_<i>` with `usize::parse` (`source_timetable.rs`
`parse_matrix_param_name`, `parse_array_param_name`), which accepts `table_01_2` and `table_+1_2`;
the validator's matrix check (`params_flattened.rs` `check_time_table_matrix`) inserts into a map,
so an aliased spelling silently overwrites the canonical one, and the one that wins is the later
in table order. Two documents differing only in that order would share an id and execute
differently. **The implementation PR therefore makes the load gate refuse every non-canonical
array or matrix element name**: an index with a sign, a leading zero, or any form other than the
one `oce_api::param_element_name` writes, refused with a typed load error before `make`. With one
spelling per element, names are unique per meaning and sorting loses nothing. This is the same
family as GitHub #467 (the facade and load gate accept parameter names outside the declared
extent); the implementation PR names the gap there and closes the spelling half of it.

**The evaluated-or-not flag is presence.** `oce-model` has no unevaluated value: every table entry
is ground. A parameter the document did not bind is absent, and `make` supplies the class default.
A literal default's value and a derived default's source tag are inside the class signature hash,
so the id covers what an absent parameter means without evaluating it. A parameter bound to a value
equal to its default is present, so it is a different id from the same document with the binding
removed, although both execute alike (decision 8; the [second worked
example](#worked-example-a-bound-default) shows the bytes). Parameter edits applied by `resume`
change the table and therefore the id. While edits are pending, `Engine::content_id` refuses, with
the same precondition `compile_blob` has, so an id always names what would execute;
`Engine::export_cxf` is unchanged and still exports the applied table.

### Connections

One record per `ModelGraph::connections` edge: `(target block name, input port index)` then
`(source block name, output port index)`, sorted by that tuple. Each input has one driver, so the
target pair alone is already unique.

A **port** is its index in the block's resolved input or output list, which is class-signature
order for named ports and authored position for positional ones (the
[ordering contract](../cxf-composite-subset.md#ordering-and-identity-contract)). It is not the
connector node's IRI and not the synthesized port name (decision 7).

### What never enters the id

- `SemanticPoint` records, `extends`, comments, annotations, `@context` spelling and every authored
  order: `@graph`, `containsBlock`, `isConnectedTo`, `hasParameter` (decision 3).
- The root composite's `@type`, `S231:label`, and any composite parameter no leaf references: none
  of them survives flattening.
- Internal connector node IRIs and internal connector attributes, except through an input's
  effective domain. Output `min`/`max`, `nominal` and `unbounded`: the engine enforces none of them
  and the exporter emits only `unit`, `quantity`, `displayUnit`, `min` and `max` (decision 10).
- Internal connector paths, units and quantities as the point inventory (`point_rows_at_load`)
  and the snapshot manifest publish them.
- `BlockId`, `ConnectorId`, `decl_order` and the schedule.
- The engine version and anything the host configures (non-finite input policy, limits).

## Canonical bytes

The rules are the blob's and the replay trace's:

- Every integer is fixed-width little-endian; nothing is a `usize`.
- `str` is a `u32` byte length then UTF-8, no terminator.
- Reals are `f64::to_bits`. **No floating-point text appears anywhere.**
- `vtype`, `value` and `opt<value>` are the blob's tag tables, unchanged.
- `opt<str>` is `u8` 0 (absent) or `u8` 1 then `str`.
- `name` is `u8` form (0 relative, 1 absolute) then `str`.
- Every list is a `u32` count then its records in the stated order; there are no section frames
  and no padding.

| Part | Encoding |
| --- | --- |
| Header | `OCECANON` (8 bytes), `u16` version 1, `u16` flags 0 |
| Inputs | count; per input: `name`, `vtype`, `opt<str>` quantity, unit, displayUnit, `opt<value>` min, max, then a `u32` target count and per target `name` and `u32` input port |
| Outputs | count; per output: `name`, `vtype`, `opt<str>` quantity, unit, displayUnit, then `u8` driver tag: 0 followed by block `name` and `u32` output port, or 1 followed by input `name` |
| Blocks | count; per block: `name`, `str` class path, `u64` class signature hash, then a `u32` parameter count and per parameter `str` name and `value` |
| Connections | count; per edge: target `name`, `u32` input port, source `name`, `u32` output port |

Records sort by name as `(form, bytes)`; targets and connections sort by their tuples in field
order. The encoder is a pure function of the loaded model and the class registry, allocates one
`Vec<u8>` bounded by the already-capped model size (the prototype measured the largest G36
encoding at 64,491 bytes, for `multizone_vav_relief_fan_group`; the implementation PR pins the real
figure in a test), and never panics: a missing class signature is a typed error,
not an `unwrap`.

## Hash and id format

The id is **SHA-256** over the canonical bytes, written `oce:canon1:` followed by the 64 lowercase
hex digits of the digest in output order. `canon1` is the encoding version; changing the encoding
or the hash is `canon2`, never a silent change under `canon1`.

FNV-1a-128 is what the repo uses for every other tag, and it is documented as "non-cryptographic
and collision-prone in adversarial use". That was acceptable for an integrity tag over bytes the
host already holds. It is not acceptable here: the registry fetches by this id
(`oce fetch <id>@<content-id>`), and verification receipts will name it as the thing that passed.
A constructed collision would let one graph claim another's receipt. SHA-256 is collision
resistant, it is 32 bytes and so fills the blob header's 32-byte field exactly, and it is the
digest most hosts can recompute without anything from this repo (decision 12).

The implementation uses `sha2` 0.10 with `default-features = false`, in `oce-api` only. It is
already in `Cargo.lock` as a dev-dependency of `oce-cxf` and `oce-conformance`, so `cargo deny`
already admits it and its tree (`digest`, `block-buffer`, `crypto-common`, `generic-array`,
`typenum`, `cpufeatures` with `libc`, `cfg-if`, and `version_check` as `generic-array`'s build
dependency), all already locked. The cost is one new normal edge from `oce-api`; the core four
gain nothing, because the `no_std` target never computes an id (the blob decoder never interprets
the field). A digest of a fixed input is a function, so `cpufeatures` choosing a SHA extension or
the portable code cannot change the output.

## Worked example

The smallest corpus-like sequence: one Real boundary input `TOut` with unit, quantity and display
unit; `off`, a `CDL.Reals.AddParameter` with `p = -273.15`, reads it; `gai`, a
`CDL.Reals.MultiplyByParameter` with `k = 1.8`, reads `off`; boundary output `y` reads `gai`. Two
blocks, one connection, one input, one output. The core PR checks this document in as
`crates/oce-api/tests/fixtures/content_id/offset_gain.jsonld`, byte for byte as below (it loads
with no warnings and exports completely):

```json
{
  "@context": { "S231": "http://data.ashrae.org/S231P#" },
  "@graph": [
    { "@id": "http://example.org#ex.offset_gain", "@type": "S231:Block",
      "S231:hasInput": [{ "@id": "http://example.org#ex.offset_gain.TOut" }],
      "S231:containsBlock": [{ "@id": "http://example.org#ex.offset_gain.off" },
                             { "@id": "http://example.org#ex.offset_gain.gai" }],
      "S231:hasOutput": [{ "@id": "http://example.org#ex.offset_gain.y" }] },
    { "@id": "http://example.org#ex.offset_gain.off",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.AddParameter",
      "S231:hasParameter": [{ "@id": "http://example.org#ex.offset_gain.off.p" }],
      "S231:hasInput": [{ "@id": "http://example.org#ex.offset_gain.off.u" }],
      "S231:hasOutput": [{ "@id": "http://example.org#ex.offset_gain.off.y" }] },
    { "@id": "http://example.org#ex.offset_gain.off.p", "@type": "S231:Parameter",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:value": { "@value": "-273.15", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
    { "@id": "http://example.org#ex.offset_gain.off.u", "@type": "S231:RealInput",
      "S231:isOfDataType": { "@id": "S231:Real" } },
    { "@id": "http://example.org#ex.offset_gain.off.y", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:isConnectedTo": [{ "@id": "http://example.org#ex.offset_gain.gai.u" }] },
    { "@id": "http://example.org#ex.offset_gain.gai",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
      "S231:hasParameter": [{ "@id": "http://example.org#ex.offset_gain.gai.k" }],
      "S231:hasInput": [{ "@id": "http://example.org#ex.offset_gain.gai.u" }],
      "S231:hasOutput": [{ "@id": "http://example.org#ex.offset_gain.gai.y" }] },
    { "@id": "http://example.org#ex.offset_gain.gai.k", "@type": "S231:Parameter",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:value": { "@value": "1.8", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
    { "@id": "http://example.org#ex.offset_gain.gai.u", "@type": "S231:RealInput",
      "S231:isOfDataType": { "@id": "S231:Real" } },
    { "@id": "http://example.org#ex.offset_gain.gai.y", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:isConnectedTo": [{ "@id": "http://example.org#ex.offset_gain.y" }] },
    { "@id": "http://example.org#ex.offset_gain.TOut", "@type": "S231:RealInput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:unit": "K", "S231:quantity": "ThermodynamicTemperature", "S231:displayUnit": "degC",
      "S231:isConnectedTo": [{ "@id": "http://example.org#ex.offset_gain.off.u" }] },
    { "@id": "http://example.org#ex.offset_gain.y", "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" } }
  ]
}
```

The canonical bytes are 394 bytes. Every byte is shown; the class signature hashes are today's
values from `crates/oce-api/tests/fixtures/blob/class_signatures.tsv`, written as `u64` LE. Note
that `gai` sorts before `off`, the reverse of authored order.

```text
0000  4f 43 45 43 41 4e 4f 4e                           magic "OCECANON"
0008  01 00                                             version 1
000a  00 00                                             flags 0
000c  01 00 00 00                                       inputs: count 1
0010  00                                                  name form 0 (relative)
0011  13 00 00 00                                         name length 19
0015  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 54     "ex.offset_gain.TOut"
0025  4f 75 74
0028  00                                                  vtype 0 (Real)
0029  01                                                  quantity present
002a  18 00 00 00                                         length 24
002e  54 68 65 72 6d 6f 64 79 6e 61 6d 69 63 54 65 6d     "ThermodynamicTemperature"
003e  70 65 72 61 74 75 72 65
0046  01                                                  unit present
0047  01 00 00 00                                         length 1
004b  4b                                                  "K"
004c  01                                                  displayUnit present
004d  04 00 00 00                                         length 4
0051  64 65 67 43                                         "degC"
0055  00                                                  min absent
0056  00                                                  max absent
0057  01 00 00 00                                         targets: count 1
005b  00                                                    block form 0
005c  12 00 00 00                                           length 18
0060  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 6f       "ex.offset_gain.off"
0070  66 66
0072  00 00 00 00                                           input port 0
0076  01 00 00 00                                       outputs: count 1
007a  00                                                  name form 0
007b  10 00 00 00                                         length 16
007f  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 79     "ex.offset_gain.y"
008f  00                                                  vtype 0 (Real)
0090  00                                                  quantity absent
0091  00                                                  unit absent
0092  00                                                  displayUnit absent
0093  00                                                  driver 0 (block output)
0094  00                                                    block form 0
0095  12 00 00 00                                           length 18
0099  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 67       "ex.offset_gain.gai"
00a9  61 69
00ab  00 00 00 00                                           output port 0
00af  02 00 00 00                                       blocks: count 2
00b3  00                                                  name form 0
00b4  12 00 00 00                                         length 18
00b8  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 67     "ex.offset_gain.gai"
00c8  61 69
00ca  1d 00 00 00                                         class length 29
00ce  43 44 4c 2e 52 65 61 6c 73 2e 4d 75 6c 74 69 70     "CDL.Reals.MultiplyByParameter"
00de  6c 79 42 79 50 61 72 61 6d 65 74 65 72
00eb  64 a2 3a 67 75 83 a6 52                             class signature hash 0x52a68375673aa264
00f3  01 00 00 00                                         parameters: count 1
00f7  01 00 00 00                                           name length 1
00fb  6b                                                    "k"
00fc  00                                                    value tag 0 (Real)
00fd  cd cc cc cc cc cc fc 3f                               bits of 1.8 (0x3ffccccccccccccd)
0105  00                                                  name form 0
0106  12 00 00 00                                         length 18
010a  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 6f     "ex.offset_gain.off"
011a  66 66
011c  16 00 00 00                                         class length 22
0120  43 44 4c 2e 52 65 61 6c 73 2e 41 64 64 50 61 72     "CDL.Reals.AddParameter"
0130  61 6d 65 74 65 72
0136  0d c3 5f e4 a5 cf 1f e7                             class signature hash 0xe71fcfa5e45fc30d
013e  01 00 00 00                                         parameters: count 1
0142  01 00 00 00                                           name length 1
0146  70                                                    "p"
0147  00                                                    value tag 0 (Real)
0148  66 66 66 66 66 12 71 c0                               bits of -273.15 (0xc071126666666666)
0150  01 00 00 00                                       connections: count 1
0154  00                                                  target form 0
0155  12 00 00 00                                         length 18
0159  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 67     "ex.offset_gain.gai"
0169  61 69
016b  00 00 00 00                                         input port 0
016f  00                                                  source form 0
0170  12 00 00 00                                         length 18
0174  65 78 2e 6f 66 66 73 65 74 5f 67 61 69 6e 2e 6f     "ex.offset_gain.off"
0184  66 66
0186  00 00 00 00                                         output port 0
```

The id is:

```text
oce:canon1:fbbea1ef32cb24797011163d09ead7ee5c17757a4aad551b6989c745740a07a5
```

The old tag of the same document is `cxf:fnv1a128:f2c3e3606b4bcc41529f4f50c0f6a222`. A throwaway
prototype (not committed) produced the bytes from the loaded `ModelGraph`, and `shasum -a 256` and
Python's `hashlib` agree on the digest. The same prototype gave the same id for the document with
`@graph` and `containsBlock` reversed and for the document with every instance IRI moved to
`urn:oce:author:`, and a different id (`acde31d2…`) with `k` one ulp higher. Over the corpus, every
document whose export is complete (33 G36 fixtures and `member_list_interface.jsonld`) gave
byte-identical canonical bytes after load → export → load. That corpus result and the 64,491-byte
maximum are prototype measurements, not review-verified facts; the implementation PR's tests
re-establish both. The two class hashes move with their classes, as the blob goldens do (see the
test plan).

### Worked example: a bound default

Presence is the evaluated-or-not flag, so binding a parameter to its class default moves the id.
`CDL.Logical.TrueDelay` has a required `delayTime` and `delayOnInit` with the literal default
`false`. Document A binds only `delayTime = 60.0`; document B is A plus one parameter node binding
`delayOnInit` to `false`. Both load with no warnings and execute identically. The core PR checks
them in as `true_delay.jsonld` and `true_delay_default.jsonld` beside the first example. A:

```json
{
  "@context": { "S231": "http://data.ashrae.org/S231P#" },
  "@graph": [
    { "@id": "http://example.org#ex.true_delay", "@type": "S231:Block",
      "S231:hasInput": [{ "@id": "http://example.org#ex.true_delay.u" }],
      "S231:containsBlock": [{ "@id": "http://example.org#ex.true_delay.del" }],
      "S231:hasOutput": [{ "@id": "http://example.org#ex.true_delay.y" }] },
    { "@id": "http://example.org#ex.true_delay.del",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.TrueDelay",
      "S231:hasParameter": [{ "@id": "http://example.org#ex.true_delay.del.delayTime" }],
      "S231:hasInput": [{ "@id": "http://example.org#ex.true_delay.del.u" }],
      "S231:hasOutput": [{ "@id": "http://example.org#ex.true_delay.del.y" }] },
    { "@id": "http://example.org#ex.true_delay.del.delayTime", "@type": "S231:Parameter",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:value": { "@value": "60.0", "@type": "http://www.w3.org/2001/XMLSchema#double" } },
    { "@id": "http://example.org#ex.true_delay.del.u", "@type": "S231:BooleanInput",
      "S231:isOfDataType": { "@id": "S231:Boolean" } },
    { "@id": "http://example.org#ex.true_delay.del.y", "@type": "S231:BooleanOutput",
      "S231:isOfDataType": { "@id": "S231:Boolean" },
      "S231:isConnectedTo": [{ "@id": "http://example.org#ex.true_delay.y" }] },
    { "@id": "http://example.org#ex.true_delay.u", "@type": "S231:BooleanInput",
      "S231:isOfDataType": { "@id": "S231:Boolean" },
      "S231:isConnectedTo": [{ "@id": "http://example.org#ex.true_delay.del.u" }] },
    { "@id": "http://example.org#ex.true_delay.y", "@type": "S231:BooleanOutput",
      "S231:isOfDataType": { "@id": "S231:Boolean" } }
  ]
}
```

B adds `{ "@id": "http://example.org#ex.true_delay.del.delayOnInit" }` after the `delayTime`
reference in `del`'s `S231:hasParameter`, and this node before `del.u`:

```json
    { "@id": "http://example.org#ex.true_delay.del.delayOnInit", "@type": "S231:Parameter",
      "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:value": false },
```

Bytes `0000` to `00b9` are identical in both: the header, the Boolean input `ex.true_delay.u`
(no attributes, one target `del` port 0), the Boolean output `ex.true_delay.y` driven by `del` port
0, and the block's name, class and signature hash. A's block section, from its count:

```text
007f  01 00 00 00                                       blocks: count 1
0083  00                                                  name form 0
0084  11 00 00 00                                         length 17
0088  65 78 2e 74 72 75 65 5f 64 65 6c 61 79 2e 64 65     "ex.true_delay.del"
0098  6c
0099  15 00 00 00                                         class length 21
009d  43 44 4c 2e 4c 6f 67 69 63 61 6c 2e 54 72 75 65     "CDL.Logical.TrueDelay"
00ad  44 65 6c 61 79
00b2  7d 02 d6 fe 4f 11 21 d3                             class signature hash 0xd321114ffed6027d
00ba  01 00 00 00                                         parameters: count 1
00be  09 00 00 00                                           name length 9
00c2  64 65 6c 61 79 54 69 6d 65                            "delayTime"
00cb  00                                                    value tag 0 (Real)
00cc  00 00 00 00 00 00 4e 40                               bits of 60.0 (0x404e000000000000)
00d4  00 00 00 00                                       connections: count 0
```

B, from the parameter count where it departs:

```text
00ba  02 00 00 00                                         parameters: count 2
00be  0b 00 00 00                                           name length 11
00c2  64 65 6c 61 79 4f 6e 49 6e 69 74                      "delayOnInit"
00cd  02                                                    value tag 2 (Boolean)
00ce  00                                                    false
00cf  09 00 00 00                                           name length 9
00d3  64 65 6c 61 79 54 69 6d 65                            "delayTime"
00dc  00                                                    value tag 0 (Real)
00dd  00 00 00 00 00 00 4e 40                               bits of 60.0 (0x404e000000000000)
00e5  00 00 00 00                                       connections: count 0
```

A is 216 bytes and B 233. Their ids:

```text
A  oce:canon1:aff7775cd0e7efb52af23c0347455f5806f789ee719df142c7c33f458ed9e91c
B  oce:canon1:05b0f40f98a38ed7e6dffca3157cae70d5f077c1893cb1587880a33ebc11752b
```

B's `delayOnInit` sorts before `delayTime` because `O` (0x4f) precedes `T` (0x54).

## Permutation classes

**Same id:**

- `@graph`, `containsBlock`, `isConnectedTo`, `hasInput`/`hasOutput` and `hasParameter` arrays
  permuted, as long as the permutation loads to the same ground values (a leaf's later binding may
  reference an earlier sibling, which the ordering contract makes order-bearing);
- re-serialization: JSON whitespace and key order, compact against expanded IRIs, singleton
  relation spellings, and load → export → load of a complete export;
- the instance namespace changed, for IRIs in form 0;
- `SemanticPoint` records, `extends`, comments, `S231:label`, the root's `@type`, unreferenced
  composite parameters, output `min`/`max`, `nominal`, `unbounded`.

**Different id:**

- any parameter bit (a Real one ulp away, `-0.0` against `0.0`, an Integer ±1, an enum member);
- binding a parameter to its default value, or removing that binding;
- renaming a block, a boundary or the sequence;
- rewiring a connection, including a same-kind port swap on a positional block;
- a class change under the same name, or a change to a used class's signature in a new engine
  build;
- a boundary type change; a `quantity`, `unit` or `displayUnit` change on a boundary; a change to an
  input's effective acceptance domain.

### What equal ids promise

Two documents with the same id, loaded by the same engine build, have the same blocks (class,
signature hash and ground parameters), the same wiring per port and the same boundary types and
domains. For every **successful** frame they therefore produce **the same boundary outputs and the
same per-block state, bit for bit**, given the same frames keyed by name; a frame one refuses, the
other refuses too, because time, type and domain checks read only those facts. The argument rests
on two facts of the core, both stated where they live:

- **The two-pass tick** (`crates/oce-graph/src/tick.rs`). Pass 1 emits every block's outputs in
  schedule order: an algebraic block through `step_algebraic`, a stateful one through
  `emit_from_state` from its prior state. Pass 2 then runs every `update_state`, after all emits.
  So no update observes another block's mid-tick state, whatever the order.
- **The feedthrough contract** (`Block::feeds_through`). A stateful block's emit ignores the
  inputs it does not feed through, which is what lets the scheduler cut a loop at it. The schedule
  is a topological order of the feedthrough DAG, and every topological order of one DAG gives each
  emit the same input values; each update then reads current-tick inputs that are equal too.

The only order-dependent output of a successful frame is the **order of its warnings**, which
follows the schedule: two equal-id documents emit the same set of warnings per frame, possibly in a
different order. Equal ids do **not** promise byte-identical executables or interchangeable state,
and the exceptions are exactly what the id leaves out:

- boundary paths carry the namespace, so the blob's I/O strings and frame paths differ across
  namespaces;
- `BlockId`, `ConnectorId` and schedule tie-breaks follow authored order, so blob bytes, the
  state-word layout and run digests can differ across permutations;
- the **point inventory** (`point_rows_at_load`) publishes every connector, internal ones
  included, with its path, `quantity`, `unit`, `displayUnit`, `min` and `max`, and the **snapshot
  manifest** embeds every connector's path, unit and quantity; internal connector IRIs and
  attributes are outside the id, so both can differ;
- therefore **snapshot restorability**: a state snapshot taken on one document need not restore on
  another with the same id. Equal ids do not imply snapshot interchange; the manifest check
  decides that, as it does today.

Two documents with equal ids, the same namespace, the same authored order and the same internal
connector IRIs and attributes compile to byte-identical blobs. The core PR proves the frame-level
claim by test rather than by this argument (see the test plan).

## Surface changes

**Facade API.**

- **`Engine::content_id() -> Result<ContentId, OcError>`**, new, and the **only** place an id is
  minted. It has `compile_blob`'s preconditions: a model loaded, and no pending parameter edits
  (it refuses while edits are pending; `export_cxf` is unchanged). It is defined for every loaded
  CXF document. The only refusal beyond the preconditions is a hand-built model with an unnamed,
  non-reserved block, which no public load path produces.
- **`ExportReport::content_id_complete()` and the deprecated `ExportReport::content_id()` are
  removed** (review ruling 10). An id captured on a report would sit beside a public, mutable
  `bytes` field and could drift from it. `ExportReport` keeps only a completeness check,
  `complete(&self) -> Result<(), ExportIncomplete>`, where `ExportIncomplete { warning_count }`
  replaces `ContentIdError::Incomplete`; a host that wants "the id of a completely exportable
  model" calls both. `ExportReport::bytes` are unchanged, so the export goldens stay
  byte-identical.
- **`CompleteExportContentId` becomes `ContentId`**, with the same no-public-constructor and
  no-conversion compile-fail guarantees against `CatalogContentId` and `DomainKey`.
- **`guards.rs`** (the accessor-signature pin, ~lines 205 to 219): the pin of
  `ExportReport::content_id_complete` and the comment explaining why `content_id` is not pinned
  are replaced by pins of `Engine::content_id` and `ExportReport::complete`.

**Blob.**

- The 32-byte content-id field holds the SHA-256 digest bytes in output order, with no padding,
  and 32 zero bytes only when there is no id (a hand-built model). It is present for all 46
  fixtures, including the 13 with partial exports.
- `CompiledBlob::content_id` becomes **`Option<ContentId>`**, typed rather than a string; `None`
  only for a hand-built model.
- `oce_graph::blob::content_id_field(Option<u128>)` takes the 32 digest bytes instead.
- The field's meaning changes, so `FORMAT_VERSION` goes from 1 to 2, as the blob note's decision 6
  requires; layout, sections and CRC rules are unchanged. The version loop in
  `crates/oce-api/tests/blob_hostile_bytes.rs` (~lines 132 to 150) and `format: 1` in
  `crates/oce-api/tests/support/blob_image.rs` (line 130) follow.
- Both blob dumps regenerate: the worked example in the user-facing
  [`docs/compiled-schedule-blob.md`](../compiled-schedule-blob.md) and the one in this directory's
  [`compiled-schedule-blob.md`](compiled-schedule-blob.md) design note, whose decision 5 also gets
  a dated amendment pointing here.

**Compatibility descriptor.**

- The descriptor revision goes from 1 to 2, and `check_compatible`'s rule that both revisions are
  1 becomes 2. The tenth line's label goes from `export` to `content`; its value is `none` or the
  `oce:canon1:` id.
- `CompatibilityDescriptor::current` takes an `Option<&ContentId>`;
  `CompatibilityDescriptor::export_content_id()` becomes `content_id()`.
- `CompatibilityMismatch::ExportPresence` and `ExportContent` are renamed `ContentIdPresence` and
  `ContentIdMismatch`, with messages to match.
- The replay record's descriptor parser (`replay_descriptor.rs`) accepts the new label and tag
  grammar, and `docs/replay-record.md` follows. `compatibility.txt` feeds the replay oracle at run
  time and moves; the `replay_*.hex` payloads do not.

**Product contract.**

- PC-018 is reworded: the facade **MUST** mint one canonical content id over the loaded graph
  (SHA-256, `oce:canon1:`) only through `Engine::content_id`, invariant under re-serialization,
  reordering and namespace, and moved by any parameter bit, name, wiring, class or boundary
  change; `ExportReport::complete` **MUST** still distinguish a partial export. Its three evidence
  tests change: `warning_bearing_content_id_identifies_the_partial_document` becomes a test of
  `complete()`; `content_id_tracks_exported_synthetic_document_while_model_id_stays_authored` is
  rewritten against `Engine::content_id`; and
  `content_id_identifies_authored_graph_order_not_an_order_independent_model` **inverts**: authored
  order no longer moves the id. The permutation and one-change tests below join the evidence.
- PC-021 names **canonical content identity** as a role distinct from the authored model id,
  executable compatibility, export bytes and build identity.

**Call sites and tests.** `crates/oce-api/tests/export_content_oracle.rs` (~lines 105 to 110 and
148 to 180) is reworked to check `complete()` and to hash the bytes itself; its file
`g36_export_content.txt` stays byte-identical. Further call sites: `tests/export_cxf.rs`,
`tests/compatibility.rs`, `tests/composition_identity.rs`, `tests/context_expansion_identity.rs`,
`tests/boundary_output_attrs.rs`, `tests/hash_order_independence.rs`, `tests/replay.rs`,
`src/compatibility_tests.rs` and `tests/fixtures/catalog_consumer.rs` (line 177) in `oce-api`, and
`crates/oce-graph/tests/blob_state_words.rs`.

**Docs in this repo.** The identity glossary in `facade-contracts.md` replaces "Complete export
content identity … No order-independent model identity is provided" with the canonical id;
`facade-migration.md` (line 143, the `content_id_complete()` row); `cxf-round-trip.md`'s
"`content_id_complete`: a checked integrity tag, not a digest" section (~lines 174 to 198); the
"Dense IDs versus authored identity" row of `cxf-composite-subset.md`; `compiled-schedule-blob.md`;
`replay-record.md`; and the M3 status row of `README.md` (the README has no other content-id
claim). The other design notes are listed under [Relation to other work](#relation-to-other-work).

**Leverage crates.** `oce-replay`'s `TraceMeta` records no content id, so nothing changes there. The
`oce-verify` receipt header carries the canonical id when it is built.

## Mapping table for the library re-stamp

One UTF-8 line per document, three fields separated by one tab, every line ending in `\n`, sorted
by source path:

```text
<old id><TAB><new id><TAB><source path>
cxf:fnv1a128:b29d7a3dba44418987cd19f0ccd194d8	oce:canon1:<64 hex>	crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld
-	oce:canon1:<64 hex>	crates/oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld
```

The old id is `-` where none existed (the export was partial). The source path is relative to the
repository root, with `/` separators. Lines starting with `#` are comments and come first: the
format line, the engine version the new ids were computed with and the owner test.

**In this repo** the table is `crates/oce-api/tests/fixtures/content_id/restamp.tsv`, one line per
G36 catalog fixture (46). Its owner test recomputes the new id of every fixture and compares the
whole file byte for byte; `OCE_BLESS=1` regenerates it. The old column is copied once from
`crates/oce-cxf/tests/fixtures/expectations/g36_export_content.txt`, which pins today's export
hashes and keeps doing so. It is a migration record, so after the first bless it only changes when
a new id moves.

**In the library** (track L3) the same format is produced by the library's own tooling: for every
card, its stored old id and its source path, and the new id from `oce id <file>` (a one-line `oce`
CLI verb printing `Engine::content_id`, cut with the core PR or after it). The re-stamp replaces a
card's id only where the card's stored id equals the table's old id for that source; a card with
no row, or whose stored id disagrees, is reported and left alone, never guessed. M3's gate row "ids
agree" then checks every re-stamped id against the id of the emitter's output for the same
authoring form.

## Decisions

**1. One canonical content id replaces the order-bearing export id** (owner, 2026-11-07). No
compatibility is owed: the old tag is removed, not kept beside the new one. Why: two ids invite
hosts to key on the wrong one, and nothing is published.

**2. Authored names are inside the id** (owner, 2026-11-09). Nodes sort by name after IRI
normalization; a rename is a new revision, and `oce-diff`'s rename pass explains it. Rejected: a
rename-invariant structural identity, which is graph canonicalization (isomorphism up to labels) in
a safety engine, with its worst cases and its subtle bugs. A later `structural_id` can be added
without moving this one.

**3. `SemanticPoint` records, `extends`, comments, annotations, `@context` and authored order
never enter the id** (owner). Why: the §7.17 seam keeps semantic records non-computational, and
authoring decisions 2, 5 and 6 keep `extends` as opaque provenance and order as presentation.
Rejected: hashing the `SemanticPoint` slot too (it would make M8's record shape, not yet designed,
part of every id); keeping authored order (that is the order-bearing id being replaced).

**4. The id is defined over the loaded flat graph.** Rejected: over the authoring model (CXF from
`modelica-json` would have no id); over export bytes (partial for 13 fixtures and order-bearing,
which is the problem being solved). Why: the loaded graph is what executes, every loadable
document has one, and every dialect has already been reduced to it.

**5. Namespace = through the last `#`, `/` or `:`; the sequence name stays inside each name;
collisions and empty remainders fall back to the whole IRI.** Rejected: stripping the root `@id`
(the engine's exporter replaces it with a fixed URN, so an id would move under its own round trip);
refusing an IRI outside `{namespace}{sequence}.{local}` (the id must exist for every loadable
document); stripping the longest common prefix (the result would depend on which other names are
present). Why: it is local, it survives export, and both namespaces in use end in a delimiter.

**6. Reserved pass-through blocks are recorded as an output's driver, not as blocks.** Rejected:
encoding them as blocks (they have no instance IRI to name them). Why: the input-to-output edge is
the whole of their meaning, and `Topology` already hides them.

**7. Ports are indices in the resolved signature.** Rejected: connector node IRIs (a CXF spelling
that the authoring layer never writes, and that positional binding ignores); synthesized port names
(they move with the class-ports golden for reasons unrelated to the document). Why: the index is
what the executable binds, and the class signature hash already fixes what each index means.

**8. Parameters are the loaded table sorted by name; presence is the evaluated-or-not flag.**
Rejected: a per-parameter flag over the class's declared parameter list (it needs the catalog's
template expansion inside the id, more surface for the same information); folding defaults into
the table (evaluating derived defaults puts class semantics into the id); table order (it is
authored order). Why: every table entry is already ground, and an absent entry's meaning is in the
class signature hash. Binding a parameter to its default is an authored change and reads as one,
as `oce-diff`'s `parameter-defaulted` already treats it.

**9. Each block carries its class signature hash.** Rejected: the class path alone. Why: when a
class's ports, defaults or state layout change in a new engine build, the same document executes
differently, and its id must say so. The re-stamp that follows is deliberate, like a blob re-bless.

**10. Boundaries carry type, `quantity`, `unit` and `displayUnit`, and inputs their effective
domain.** Rejected: every §7.4.1 attribute (the exporter drops `nominal` and `unbounded`, so the id
would move under a round trip); declared bounds instead of effective ones (an internal target's
bound would change the refusal domain without changing the id); every connector's attributes,
internal ones included (renaming or annotating an internal port node would then move the id with
no change to execution or the frame interface). Why: these are the facts of the boundary I/O
definitions (`InputDefinition` and the output projection) and what the frame driver enforces.
They are not the whole point inventory: `PointDto` rows also cover internal connectors, which stay
out (see [What equal ids promise](#what-equal-ids-promise)).

**11. `OCECANON` version 1 encoding**, under the blob's and the trace's rules, with no section
frames. Rejected: section frames as in the blob (`u32` id and length per section), and reusing
the blob encoding itself (it is in `BlockId` order and carries ids, which is what this one must not
depend on). Why: the bytes are hashed, never parsed, so framing would buy nothing; the counts make
the encoding injective.

**12. SHA-256 through `sha2`, `default-features = false`, in `oce-api` only.** Rejected:
FNV-1a-128 (collision-prone in adversarial use, and this id becomes a registry key and a receipt
subject); BLAKE3 (a dependency not yet in the lockfile, and less universal for hosts); a hand-rolled
SHA-256 (the blob's table-less CRC-32C is a precedent, but a cryptographic primitive is better
taken from a vetted crate than re-derived). Why: collision resistance, 32 bytes that fill the blob
field, a crate the lockfile and `cargo deny` already carry.

**13. `oce:canon1:<64 hex>`.** Rejected: `canon:1:sha256:<hex>`, mirroring
`catalog:1:fnv1a128:` (the hash is fixed by the version, so naming it again only adds a way to
disagree); a bare digest (no version to bump). Why: distinguishable from `cxf:fnv1a128:`,
`catalog:1:fnv1a128:` and `replay:1:fnv1a128:` at a glance, with the encoding version in the
prefix.

**14. `Engine::content_id` is the only entry; `ExportReport` loses both id methods and keeps a
completeness check** (review ruling 10). Rejected: the draft's `content_id_complete()` returning
the canonical id captured at export time (a captured id on a report with public, mutable `bytes`
is a claim that can drift from them). Why: the id names a model and is always defined; a host that
wants the id of a completely exportable model asks `Engine::content_id` and
`ExportReport::complete`.

**15. The blob field holds the digest, and `FORMAT_VERSION` becomes 2.** Rejected: keeping format
1 because the decoder ignores the field (the blob note's decision 6 says a change of the field's
meaning bumps the version, and a host comparing two blobs' ids must not compare across meanings).

**16. The compatibility descriptor goes to revision 2 with a `content` line.** Rejected: keeping
`export` at revision 1 with a new tag grammar (a persisted receipt would change meaning without a
revision). Why: the descriptor's contract is exact-byte comparison and refusal of unknown
revisions.

**17. The mapping table is plain TSV, sorted, with `-` for a missing old id**, owned by a test in
this repo and produced by the library's own tooling for its cards. Rejected: JSON (no gain for
three string fields, and a noisier diff); keying the table by old id alone (13 fixtures have
none). Why: one line per document diffs and greps cleanly, and the re-stamp refuses rather than
guesses.

## Relation to other work

- **#448 (facade blob accessor).** The accessor reports the blob's content id as an
  `Option<ContentId>`. Built after this PR, it reads the digest directly; built before, it changes
  with this PR.
- **`oce-diff` (decision 8 of its note).** Equal canonical ids prove the two **loaded graphs** are
  equal, so the CXF adapter and the blob comparison can return graph-empty without matching, and
  the authoring diff can skip its rename pass. They do not prove an empty authoring diff: a
  sequence-parameter rename, a literal against a reference with the same value, `extends`,
  comments, `SemanticPoint` fields and `reordered` entries are all outside the id (review ruling
  11 narrows decision 8 to that).
- **Other design notes: the implementation PR's doc scope.** Each of these becomes false or stale
  when the id ships, and the core PR amends it:
  - `diff.md`: "order moves the content id" (~line 98, under Categories, and decision 5, ~line
    530) is no longer true, `reordered` stays its own kind only because it is not semantic; the
    whole "Relation to content ids" section (~lines 153 to 163) is rewritten around this note, and
    decision 8 is narrowed as above.
  - `authoring-layer.md`: decision 6's rationale ("the content id is order-bearing") is superseded;
    the decision stands because authored order still assigns `BlockId` and schedule tie-breaks.
    The export content id is also named at ~line 313 (the throwaway load), ~line 330 (`oce check`
    reports "the export content id") and ~lines 514 to 515 (gate item 1 moving with M3); each
    names `Engine::content_id` instead.
  - `commissioning.md`: the receipt example's `content_id` (~line 305) becomes an `oce:canon1:`
    value, and decision 20 ("Rejected: a new hash dependency") is reconciled with the `sha2` edge:
    receipts may keep FNV-1a-128 for `spec_id` and `receipt_id`, but the content id they carry is
    SHA-256, and the receipt names both.
  - `compiled-schedule-blob.md`: decision 5 is amended (dated) to point here; its dump regenerates.
- **#464, M2 gate item 1.** "Load → export id equals the committed CXF's" becomes "the canonical id
  of the emitter's output equals the committed CXF's". The namespace is free, so the emitter's
  default `urn:oce:author:` matches the library's `http://example.org#`; the sequence name must
  match, because it is inside every name.
- **The registry.** `oce fetch <id>@<content-id>` keys on this id, which is the reason for decision
  12. Resolving `extends` stays the registry's.
- **Library L3.** The re-stamp consumes the mapping table as described above.
- **`oce-verify` receipts.** The shared receipt header's content id is this id.
- **M7, composite lowering.** The roadmap promises "a content id equal to the flattened form". This
  id is already defined over the flattened graph and ignores order, so a composite document and its
  hand-flattened form share an id exactly when lowering gives the leaves the same names, classes,
  parameters and wiring. M7's canonical lowering order cannot move it; how M7 names lowered leaves
  can, and M7 must choose names that equal the flattened form's.

## Test plan

What the core PR must prove:

- **Worked examples.** The three documents above are checked in; their canonical bytes equal the
  dumps byte for byte and their ids equal the ones printed, and the two `TrueDelay` documents have
  different ids. A hand-assembled byte vector in the test, not the encoder, is the oracle.
- **Mapping table.** `restamp.tsv` pins the new id of all 46 fixtures; any change shows in review.
- **Permutation classes, over all 46 fixtures**, enumerated, no RNG: `@graph` reversed and rotated;
  `containsBlock`, `isConnectedTo`, `hasInput`/`hasOutput` and `hasParameter` arrays reversed where
  the document still loads to the same ground values; compact and expanded IRIs; the namespace
  rewritten to `urn:oce:author:`; load → export → load for the 33 complete exports (a prototype
  measurement this test re-establishes). Each keeps the id.
- **Equal ids, equal behaviour.** For every fixture and each of its permuted variants, run the
  shared 160-frame schedule (`crates/oce-graph/tests/support/frame_schedule.rs`) and compare every
  boundary output by raw bits after every successful frame, keyed by name, and every block's state
  words keyed by block name; a refused frame must be refused by both. Warnings compare as sets per
  frame, the one documented order difference.
- **One-change matrix.** For every fixture: every Real parameter one ulp up, every Integer ±1, every
  Boolean flipped, every Enum to another member, `+0.0` to `-0.0` where present; one block and one
  boundary renamed; one connection rewired; one class swapped for a same-arity class; one boundary
  `quantity`, `unit` and `displayUnit` changed; one input bound tightened. Each moves the id, and
  no two variants of one fixture share an id.
- **Names.** A document with two namespaces that collide on one name uses form 1 for both; an IRI
  ending in a delimiter uses form 1.
- **Pass-through.** A document with one block plus a boundary input `a` wired straight to a
  boundary output `b`: the output list holds `b` (from the minted connector) with driver "input
  `a`", the input `a` has a record with an empty target list, the pass-through block is in neither
  the block list nor any target list, and changing `b`'s `unit` or rewiring `b` to a block output
  moves the id. (The prototype, run on such a document, produced exactly that.)
- **Element-name spellings.** A `TimeTable` document binding `table_01_2` or `table_+1_2`, alone or
  beside `table_1_2`, and the same for an `offset_<i>` and a width-driven array parameter (`k_01`),
  is refused at load with a typed error; the canonical spellings still load.
- **Hostile and boundary cases.** A hand-built model with an unnamed block is a typed refusal, not a
  panic; `Engine::content_id` refuses while parameter edits are pending and before a model is
  loaded; the largest fixture's encoding is pinned (the prototype's 64,491 bytes is re-measured
  here); the encoder never allocates past its one `Vec`.
- **SHA-256.** The FIPS 180-4 vectors (`""`, `"abc"` and the 448-bit message) pin the dependency's
  output, so a dependency bump that changed it would fail.
- **Goldens that re-bless, and why.** The 46 blob goldens move by their 32 id bytes, the format
  version and the CRC: the field's meaning changed (decision 15). `compatibility.txt`,
  `compatibility_export.txt`, the descriptor doctest and the replay-record goldens that embed a
  descriptor move by the revision and the `content` line (decision 16); the `replay_*.hex`
  payloads do not move. The two blob dumps in the docs regenerate. The firmware image's embedded
  `multizone_vav_supply_fan` blob moves with its golden; its run digests do not. Everything else stays byte-identical: export bytes,
  `g36_export_content.txt`, `blob_frame_digests.tsv`, the class-ports golden, the 46 fixture
  goldens and 422 provenance goldens.
- **Determinism across architectures.** The id bytes live in the blob goldens, and the determinism
  matrix already requires the 46 encoded blobs to be byte-identical in the four native cells
  (x86_64 and aarch64, debug and release), and the blob round trip requires the `wasm32-wasip1` leg
  to encode the same bytes. No new cell is needed.

## Out of scope

- A rename-invariant `structural_id` (decision 2).
- The registry, `oce fetch`, and resolving `extends`.
- A cryptographic signature or any authenticity claim; the host's envelope keeps that role.
- Making the id stable across engine builds that change a used class's signature.
- Sequence-level parameters as such: the id sees their grounded effect on leaves, not their names.
- The `oce id` CLI verb itself, beyond naming it as the library's source of new ids.

## Review rulings

The orchestrator's review of the draft PR ruled on the eleven questions it raised
([PR #471 comment](https://github.com/jscott3201/open-control-engine/pull/471#issuecomment-6733407567),
2026-11-09); the note above is amended to match:

1. **Hash:** SHA-256 via `sha2`, `default-features = false`, on `oce-api` only, accepted (decision
   12). Keeping FNV-1a-128 and hand-writing SHA-256 rejected.
2. **Namespace rule:** accepted as drafted; the exporter's synthetic root is unchanged, and
   `oce-author` refuses a `--namespace` override that does not end in `#`, `/` or `:`. A sequence
   rename moves the id, consistent with names inside it.
3. **Collisions:** the whole-IRI fallback stands; no authored or library document reaches it.
4. **`unit`:** inside the id; it is a connector attribute the engine reads.
5. **Bounds:** inputs carry the effective domain; output `min`/`max`, `nominal` and `unbounded`
   stay out.
6. **Ports:** indices; the class signature hash pins names and kinds.
7. **Defaults:** presence is the evaluated-or-not flag, and binding a default moves the id; the
   second worked example was added to show it.
8. **Blob:** `FORMAT_VERSION` 2, the digest fills the 32-byte field, present for the 13
   partial-export fixtures; both blob dumps regenerate.
9. **Descriptor:** revision 2 with a `content` line; `check_compatible` requires revision 2, and
   the `Export*` mismatch variants are renamed.
10. **`ExportReport`:** both id methods are dropped; the id comes only from `Engine::content_id`,
    and `ExportReport` keeps `complete() -> Result<(), ExportIncomplete>` (decision 14 rewritten).
11. **`oce-diff` and the other notes:** decision 8 narrowed, the "Relation to content ids" section
    rewritten, authoring decision 6's rationale amended, commissioning decision 20 reconciled, the
    M7 line added, all as the implementation PR's doc scope.

The same review's fix round required, and the note now has: boundary outputs as the union with
pass-through outputs (blocking); the headline narrowed to execution and the boundary I/O
definitions, with the point inventory, snapshot manifest and snapshot restorability as stated
exceptions; the canonical element-name refusal behind sorted parameters (the GitHub #467 family);
the two-pass tick and feedthrough contract behind the equal-ids claim; the complete surface-change
list; and the prototype measurements marked as such.
