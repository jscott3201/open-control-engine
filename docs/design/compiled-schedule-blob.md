# Compiled-schedule blob (OCEB), format version 1

**Design note, written before M1b code (GitHub #396).** It fixes the byte format that #400 encodes
and decodes, #401 executes on a `no_std` target, #402 sizes in a firmware image and #403
round-trips across architectures. Where this note departs from the owner's starting sketch, it says
so and why.

## What the blob is

A `std` host runs the full pipeline: CXF import, validation, block instantiation, Kahn
scheduling and state allocation. The blob carries that pipeline's output in a form a `no_std`
target can execute without `oce-cxf`, `oce-expr`, `oce-validate` or `oce-api`. Every parameter is
a ground value, the block evaluation order is frozen, and connector numbering is unchanged. The
target re-instantiates blocks from its own registry. It checks every class against the host's
class signature. Then it runs the unchanged `oce_graph::eval_tick`.

Block and connector ids in the blob are the `std` path's own `BlockId` and `ConnectorId`. A
decoded executable is therefore comparable with the host's `RunState` word for word, not only at
the boundary outputs.

## Encoding rules

- Every integer is fixed-width little-endian (`u8`, `u16`, `u32`, `u64`, `i64`). Nothing on the
  wire is a `usize`.
- `str` is a `u32` byte length followed by UTF-8 bytes, with no terminator or normalization. This
  is the snapshot's string encoding (`Writer::string` in
  [`state_wire.rs`](../../crates/oce-api/src/state_wire.rs)).
- Reals are IEEE-754 bit patterns (`f64::to_bits`), so signed zero and NaN payloads survive.
  **No floating-point text appears anywhere.**
- Typed values, value types and optional values use the snapshot's tag tables unchanged (table
  below).
- Sections appear in a fixed order, each exactly once. Each has a section frame (`u32` id, `u32`
  payload length), and its payload must be consumed exactly.
- Every list has one canonical order, stated per section. The decoder refuses a blob whose lists
  are out of order, duplicated, or carry unused entries.
- A whole blob is at most 64 MiB, the snapshot cap (`MAX_SNAPSHOT_BYTES`), so every section
  length fits a `u32`.

| Encoding | Bytes |
| --- | --- |
| `value` | tag `u8`, then: 0 Real `u64` bits · 1 Integer `i64` · 2 Boolean `u8` 0/1 · 3 String `str` · 4 Enum `str` class path + `u32` 1-based ordinal |
| `vtype` | tag `u8`: 0 Real · 1 Integer · 2 Boolean · 3 String · 4 Enum followed by `str` class path |
| `opt<value>` | `u8` 0 = absent, or `u8` 1 followed by `value` |

## Layout

**Header**, 52 bytes:

| Offset | Width | Type | Meaning |
| --- | --- | --- | --- |
| 0 | 4 | bytes | Magic `OCEB` (`4f 43 45 42`) |
| 4 | 2 | `u16` | Format version, `1` |
| 6 | 2 | `u16` | Execution profile, `1` = HostTick v1 ([execution profile](../execution-profile.md)) |
| 8 | 6 | `u16`×3 | Encoder engine version: major, minor, patch (`0, 2, 0` today) |
| 14 | 2 | `u16` | Reserved flags, must be 0 |
| 16 | 4 | `u32` | Total blob length in bytes, header and trailer included |
| 20 | 32 | bytes | Content id (decision 5) |

**Section frame.** Each section is `u32` section id, then `u32` payload length, then the payload.
Ids 1 to 5 appear in that order.

**1 Classes.** The payload is a `u32` count followed by one record per class, strictly increasing by
UTF-8 bytes of `class_path`. Every class is used by at least one instance.

| Width | Type | Meaning |
| --- | --- | --- |
| 4 + n | `str` | Canonical `class_path`, as `oce_blocks::lookup` accepts it |
| 8 | `u64` | Class signature hash (decision 2) |

**2 Instances.** The payload is `u32` block count B, then `u32` connector count C, then B records
in `BlockId` order:

| Width | Type | Meaning |
| --- | --- | --- |
| 4 | `u32` | Class index into section 1 |
| 1 | `u8` | Resolved kind: 0 algebraic `[A]`, 1 stateful `[S]` |
| 4 | `u32` | State word count (`Block::state_len`; 0 for `[A]`) |
| 4 | `u32` | CRC-32C of the block's initial state words as `u64` LE (0 for `[A]`); see decision 3 |
| 4 + 4·nᵢ | `u32`, `u32`×nᵢ | Input arity, then the input `ConnectorId`s in port order |
| 4 + 4·nₒ | `u32`, `u32`×nₒ | Output arity, then the output `ConnectorId`s in port order |
| 4 + … | `u32`, (`str`, `value`)×p | The resolved `ParamTable` as `make` received it: count, then name/value pairs in table order |

Every id in `0..C` appears in exactly one port list. Connector value types and directions are not
encoded; the decoder takes them from the constructed block's resolved signature. Ids are explicit
rather than implied by arity. In one of the 46 catalog fixtures,
`multizone_vav_outdoor_airflow_sumzone`, the loader does not number connectors contiguously per
block.

**3 Schedule.** The payload is a `u32` count (= B), then B `u32` block ids in evaluation order. This
is `Schedule::order`. `connector_order` is not carried; only trace tooling reads it.

**4 Connections.** The payload is a `u32` count K, then K pairs (`u32` producer output id, `u32`
consumer input id), strictly increasing by consumer id. The order makes "one driver per input" a
neighbor comparison. The decoder rebuilds `driver_of` with the unchanged `build_driver_of`.

**5 I/O.** Inputs first: a `u32` count, then records strictly increasing by path bytes.

| Width | Type | Meaning |
| --- | --- | --- |
| 4 + n | `str` | Canonical absolute `@id` (`InputDefinition::path`) |
| 1 + … | `vtype` | Exact native type, enum class included |
| 1 + … | `opt<value>` ×2 | Effective inclusive min, then max: the acceptance domain `Engine::input_definitions` reports |
| 4 + 4·t | `u32`, `u32`×t | Fan-out target input `ConnectorId`s, t ≥ 1, strictly increasing |
| 4 + s | `u32`, bytes | `SemanticPoint` payload, opaque (decision 4) |

Outputs follow: a `u32` count, then records strictly increasing by path. Each record holds the
`str` path (the executable boundary output identity), the `u32` source output `ConnectorId`, and
the `u32` length plus bytes of its `SemanticPoint` payload.

**Trailer**, 4 bytes: `u32` CRC-32C over every byte before it.

### Worked example

The fixture is `generic_air_economizer_high_limits_ashrae_differential`, 418 bytes. Twelve of the
46 catalog fixtures (`crates/oce-cxf/tests/fixtures/g36`) tie at one block. Among them, this one and
the four `title24_differential_offset_*` variants are the only ones that also have a boundary input
and output, so every section except Connections is non-empty. Of those five, this one is the
smallest. The model is one `CDL.Reals.AddParameter` (`p = 0.0`): boundary input `TRet` drives
connector 0 (`u`), and connector 1 (`y`) is boundary output `TCut`. The bytes come from a throwaway
prototype of this layout. Two fields are placeholders: `HH`, because #400 fixes the signature
record, and `CC`, which depends on it. Both I/O paths are 89 bytes:
`http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.` + `TRet` /
`TCut`.

```text
0000  4f 43 45 42                    magic "OCEB"
0004  01 00  01 00                   format 1, profile 1 (HostTick v1)
0008  00 00 02 00 00 00  00 00       engine 0.2.0, flags 0
0010  a2 01 00 00                    total length 418
0014  fd e5 78 37 69 de fb 04        content id: FNV-1a-128 as u128 LE, i.e.
      25 eb 62 b8 87 d9 ee 9a          cxf:fnv1a128:9aeed987b862eb2504fbde693778e5fd
      00 ×16                           zero padding
0034  01 00 00 00  26 00 00 00       section 1 Classes, 38 bytes
003c  01 00 00 00                    1 class
0040  16 00 00 00 "CDL.Reals.AddParameter"
0056  HH HH HH HH HH HH HH HH        class signature hash
005e  02 00 00 00  37 00 00 00       section 2 Instances, 55 bytes
0066  01 00 00 00  02 00 00 00       1 block, 2 connectors
006e  00 00 00 00  00                block 0: class 0, kind [A]
0073  00 00 00 00  00 00 00 00       0 state words, state CRC 0
007b  01 00 00 00  00 00 00 00       1 input: connector 0
0083  01 00 00 00  01 00 00 00       1 output: connector 1
008b  01 00 00 00                    1 parameter
008f  01 00 00 00 70  00  00 ×8      "p", Real, bits of +0.0
00a1  03 00 00 00  08 00 00 00       section 3 Schedule, 8 bytes
00a9  01 00 00 00  00 00 00 00       order = [block 0]
00b1  04 00 00 00  04 00 00 00       section 4 Connections, 4 bytes
00b9  00 00 00 00                    0 connections
00bd  05 00 00 00  d9 00 00 00       section 5 I/O, 217 bytes
00c5  01 00 00 00                    1 input
00c9  59 00 00 00 "…TRet"            path (89 bytes)
0126  00  00  00                     Real, no min, no max
0129  01 00 00 00  00 00 00 00       1 target: connector 0
0131  00 00 00 00                    SemanticPoint payload, empty
0135  01 00 00 00                    1 output
0139  59 00 00 00 "…TCut"            path (89 bytes)
0196  01 00 00 00  00 00 00 00       source connector 1, empty payload
019e  CC CC CC CC                    CRC-32C of bytes 0000..019d
```

## Decisions

**1. The decoder lives in `oce-graph`** as a `blob` module. Its entry point is
`oce_graph::blob::decode(&[u8])`, which returns the model arena, `Schedule`, block instances,
`RunState` and the I/O table; it replaces the sketched `Schedule::from_blob`, because it returns
more than a schedule. `cargo tree -e normal` shows `oce-graph → {oce-blocks, oce-model, thiserror}`
and `oce-blocks → {oce-model, libm}`. That is exactly what decoding needs: the registry
(`lookup`, `make`, class hashes), the model types, and `Schedule`, `RunState`, `allocate_state` and
`build_driver_of`.

The byte writer sits in the same module, so one file owns the tag tables in both directions.
`oce-api` adds the facade-only facts: content id, input definitions and output identities.

The target does not need:

- **`oce-cxf`**: JSON-LD parsing, IRI expansion and `serde_json`. The host has already resolved
  everything.
- **`oce-expr`**: binding expressions are already evaluated into parameter values.
- **`oce-validate`**: the decoder runs its own narrower structural checks, listed below.
- **`oce-api`**: the Store port, facade lifecycle and `std` collections.

Rejected alternatives:

- **`oce-blocks`.** It cannot see `Schedule` or `RunState`; the dependency points the other way.
- **A new `oce-exec`.** It would depend on exactly `oce-graph` and own nothing but a codec for
  `oce-graph`'s types. That would split the format from the types it encodes, and it would add a
  package-policy and publication-ledger row for no capability.

**2. Class signature hash.** The hash is FNV-1a-64 over one canonical record per class, computed by
one function in `oce-blocks` that both the encoder and the decoder call. The record is built from a
default-parameter instance and contains, in order:

- `class_path`
- resolved kind, `state_len` and `state_contract_revision`
- the input and output port kinds and port names
- the naming policy, `width_driven` and `reserved` flags
- every `ParamRule`, as a variant tag and its fields
- every `ParamDefault`, as name, source tag, and literal or formula text

Counts are `u32`, Reals are bits, and strings use `str`. The tag tables use exhaustive matches, so
a new `ParamRule` variant cannot compile without one. The hash does not cover:

- tick behavior (`step_algebraic`, `emit_from_state`, `update_state`, `feeds_through`)
- `validate_state`
- non-default arities
- catalog order

The decoder checks three of those per instance instead:

- **Arity and kinds:** the constructed block's resolved arity and port kinds must match the record.
- **State layout:** `kind`, `state_len` and the initial-state CRC must match (decision 3).
- **Schedule:** the order must be valid under the target's own `feeds_through`.

As with snapshots, implementation identity beyond that is host build qualification
([state compatibility](../state-compatibility.md)). An unknown class is `UnknownClass`, and a
different hash is `ClassSignatureMismatch`; both carry the class index and both hashes where they
apply. Neither panics.

Rejected: **one registry-wide digest.** It would refuse a blob because an unused class changed, the
case the state contract already excludes. It would also refuse every blob on a firmware image that
compiles a reduced registry to meet #402's budget. Per class, that image accepts every blob whose
classes it has and names the class it lacks.

**3. Parameters and initial state.** `oce-model` has exactly five `Value` kinds. The parameters of
the 46 fixtures use all five:

| Kind | Parameter count |
| --- | --- |
| Real | 611 |
| Integer | 140 |
| Boolean | 90 |
| Enum | 17 |
| String | 16 |
| Total | 874 |

Arrays and tables are already flattened by the resolver into scalar entries named `base_i` and
`base_row_col` (`TimeTable`'s `table_1_2`), so no array encoding exists or is needed. The blob
carries the `ParamTable` exactly as `make` received it, in table order. Equal constructor input
gives an equal block. Enum values carry the class path, never the run-local `EnumClassId`.

Initial state words are **not** carried. The decoder calls the unchanged `allocate_state`. That
call seeds each region from `init_state` and fires constant sources once. It then compares each
stateful block's region with that block's carried CRC-32C; a difference is `InitialStateMismatch`
naming the block. This departs from the sketch. Carrying the words would cost 8 bytes per word,
almost all of them a moving-average ring of zeros. `multizone_vav_relief_fan_group` would grow from
16,438 to about 49,800 bytes, and the decoder would still have to run `init_state` to check them.

**4. I/O.** The path strings are what the host already uses:

- **Inputs:** `InputDefinition::path`.
- **Outputs:** the executable boundary identity.

The slot is the record's position in its list. The host resolves names to slots once, at startup,
and indexes thereafter. The domain is typed, not "min/max as `f64` bits": Integer inputs have `i64`
bounds, and the snapshot's input definitions already encode typed optional bounds, so the blob
reuses that encoding. The decoder checks four things:

- each target is an input connector of the record's type;
- each target has no connection driver;
- every input connector has exactly one driver or input target;
- each output source is an output connector.

The non-finite input policy is host configuration, not blob content.

`SemanticPoint` payloads are non-computational under CDL §7.17 (the seam in
[architecture](../architecture.md)). The decoder bounds each one by its length, skips it, and never
reads, stores or forwards it. No `SemanticPoint` type exists yet, so the v1 encoder writes empty
payloads. A test in the style of `crates/oce-api/tests/frame_purity.rs` proves the seam (see the
test plan).

**5. Content id.** The 32-byte field holds whatever the facade's content id is for this engine
version, `cxf:fnv1a128:` over the exported CXF bytes (`export.rs`). The 16 hash bytes are written as
a `u128` LE, followed by 16 zero bytes. The field's position and width are stable. Its meaning is
"the facade's content id at encode time" until M3 replaces the algorithm. Changing the meaning bumps
the format version.

`content_id_complete` refuses partial exports. 13 of the 46 fixtures export partially today,
because the CXF exporter defers every PID whose `controllerType` is enum-valued, together with
its downstream consumers. For those, the field is 32 zero bytes, meaning "no content id". The decoder never
interprets the field.

**6. Versioning.** The format version is a `u16`. Any change to layout, a tag table, the
signature-record definition, the CRC, or the content-id meaning bumps it. A decoder accepts
exactly its own version and its own execution profile. Any other version, newer or older, is
`UnsupportedFormat { found, supported }`. This is current/current only, as for snapshots
([release compatibility](../release-compatibility.md)). Engine version must also match exactly;
a mismatch is `EngineVersionMismatch` naming both versions.

Additive or skippable sections are not allowed in v1. A decoder that skips what it does not
understand would execute a model differently from what its encoder meant, for example by
ignoring a future override section. In a format that commands equipment, every byte that can
affect computation must be understood or refused. The only skipped bytes are `SemanticPoint`
payloads, and §7.17 makes those non-computational.

**7. Integrity.** The trailer is CRC-32C, the Castagnoli polynomial (reflected `0x82F63B78`,
initial value and final XOR `0xFFFFFFFF`; the check value of `"123456789"` is `0xE3069283`). It
covers every byte before the trailer. The implementation is a bitwise, table-less loop of about 15
lines in `oce-graph`. It adds no dependency, `deny.toml` is unchanged, and it puts no 1 KiB table in
flash. Decoding happens once per boot.

CRC-32C detects every burst of 32 bits or fewer, and it keeps Hamming distance 4 across the whole
64 MiB range. Corruption in flash, transfer or storage is therefore detected. Tampering is not:
anyone can recompute the trailer. Authentication and signing belong to the host's envelope, as for
snapshot bytes.

Rejected: **reusing the snapshot's FNV-1a-128.** FNV gives no burst or Hamming-distance guarantee,
costs 16 bytes, and lives in `oce-api`.

**8. Size budget.** The formula is below. #402 measures its firmware image against it.

**9. Target-bound flag: none in v1, and no architecture stamp.** A portable blob's bytes do not
depend on the encoder's architecture. An encoder stamp would make x86_64 and arm64 encoders
disagree, which defeats #403's byte-equality check. The 15 target-bound classes affect outputs,
not blob bytes, and the class table already names every class. A host can apply the snapshot's
target-bound policy to the blob without a flag. None of the 46 fixtures uses one of those classes.
A variant that embeds a platform stamp, as the snapshot's `TargetBound { arch, os }` does, would be
format version 2. The reserved `u16` flags must be zero, matching the snapshot's zero reserved
flags.

**10. Out of scope for v1.** Do not add any of these in M1b:

- run-time parameter edits
- partial or incremental update
- compression
- signing or encryption
- more than one model per blob
- string interning tables
- endianness negotiation
- carried state words or snapshot restore on the target
- skippable or optional sections
- an architecture stamp

## Decoding, determinism and allocation

The decoder refuses in this order:

1. Length above 64 MiB: `TooLarge`.
2. Short input, wrong magic, or a header length that differs from the input length: `Malformed`.
3. Wrong format version: `UnsupportedFormat`.
4. Trailer mismatch: `IntegrityMismatch`.
5. Wrong profile or engine version, or nonzero flags.
6. Structural problems, each with a byte offset: section order, bounded counts, UTF-8, tags,
   ordering, trailing bytes.
7. Classes and per-instance checks.
8. The schedule.

A count is accepted only if the remaining section bytes could hold that many minimal records, so
no allocation precedes that proof. The schedule must be a permutation of `0..B`. Each connection
whose consumer reads it at emit time, as `topo.rs` defines that, must have its producer earlier in
the order. Any valid order gives the same outputs, so the decoder does not need to reproduce the
host's tie-break. Then `allocate_state` enforces `MAX_STATE_WORDS`, and the state CRCs are compared.

Errors carry static labels, indices and offsets, never formatted text. The decode path is panic-free
and links no `core::fmt`, the pattern `rusty-bacnet`'s firmware images use. The `oce-api` encoder
runs the decoder on its own output before returning, as `state_snapshot` does. It refuses while
parameter edits are pending.

**Determinism.** The bytes are identical on x86_64, arm64 and wasm32 because of how they are
built, not because a test happens to pass:

- Nothing on the wire is a `usize`. Counts are `u32`, and the decoder widens them with checked
  `try_from`.
- No hash-map iteration order reaches the output.
- Every list has one canonical order: classes and I/O by UTF-8 bytes, connections by consumer id,
  instances by `BlockId` (the loader's deterministic declaration order), parameters in table order.
- Section order is fixed.
- Reals are bits.
- There is no padding, and nothing is uninitialized.
- Fields are read with `from_le_bytes` from byte slices, so alignment never matters.
- The blob carries no host or architecture field.

**Allocation.** The decoder allocates what it returns:

- the `Model` block and connector arenas, connections, and `Arc<str>` parameter names and values;
- one `Box<dyn Block>` per instance, plus whatever its constructor allocates (as on the host);
- `Schedule::order` and `driver_of`;
- `RunState`: values, words, slots, `slot_of` and scratch.

I/O paths and `SemanticPoint` payloads are borrowed from the input slice, which on firmware is the
`'static` blob in flash. The tick allocates nothing in the scheduler. The carve-outs in
[architecture](../architecture.md) still apply: `Reals.Sort` above width 64, and the diagnostics
sink.

## How this relates to the state snapshot

The blob reproduces the conventions of [`state_codec.rs`](../../crates/oce-api/src/state_codec.rs)
and its manifest codec:

- little-endian fixed-width integers;
- `u32`-prefixed UTF-8 strings;
- the five-tag `value` and `vtype` tables;
- `0`/`1` optional values;
- Real bits that preserve signed zero and NaN;
- zero reserved flags;
- typed errors with offsets;
- counts bounded before allocation;
- one `u64` per state word.

It does **not** share:

| Field | Snapshot | Blob |
| --- | --- | --- |
| Magic | `OCESTAT\0` (8 bytes) | `OCEB` (4 bytes) |
| Integrity | FNV-1a-128 trailer | CRC-32C (decision 7) |
| Identity | Execution manifest, fingerprint, authored block keys | Dense indices |
| Placement | Portability tag | None (decision 9) |
| Parameter order | Lexical | Constructor (table) order |

The primitives are duplicated in `oce-graph`, about 100 lines, rather than moved out of `oce-api`.
Moving them is a separate refactor that would have to prove snapshot goldens byte-identical.
**Snapshot bytes and code are unchanged by M1b.**

## Size budget

| Item | Bytes |
| --- | --- |
| Fixed: header 52, five section frames 40, section counts 28, trailer 4 | 124 |
| Per class | 12 + class-path length |
| Per block instance, schedule entry included | 29 + 4 per port + parameters |
| Per parameter | 5 + name length + value (Real/Integer 9, Boolean 2, String 5 + length, Enum 9 + class-path length) |
| Per connection | 8 |
| Per input | 15 + path length + 4 per fan-out target + 9 per present bound + payload |
| Per output | 12 + path length + payload |

The formula was measured by the prototype over all 46 fixtures. Sizes are exact; the hash values
are placeholders.

| Fixture | Blocks | Connectors | Connections | Parameters | Inputs (targets) / outputs | Classes | Blob bytes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Smallest: `…ashrae_fixed_18` (and two siblings) | 1 | 1 | 0 | 1 | 0 / 1 | 1 | 306 |
| Worked example | 1 | 2 | 0 | 1 | 1 (1) / 1 | 1 | 418 |
| Most blocks: `multizone_vav_relief_fan_group` | 226 | 621 | 328 | 104 | 11 (37) / 9 | 32 | 16,438 |
| Largest blob: `cooling_only_controller` | 213 | 521 | 268 | 167 | 14 (43) / 10 | 39 | 17,001 |

Across the corpus, the blobs total 123,329 bytes and the median is 1,657. A block instance averages
about 53 bytes with its ports and parameters. An I/O point averages 97 bytes, about three quarters
of it IRI text.

**For #402:**

- **Flash:** the largest blob is 17,001 bytes, in `.rodata` if it is embedded with
  `include_bytes!`, on top of the executor code.
- **RAM:** RAM is a separate term the blob does not carry. `relief_fan_group` decodes to 4,166
  state words (33,328 bytes), 4,110 of them in one `Reals.MovingAverage` ring. Its 621 connector
  values come on top of that.

## Test plan

What #400, #401 and #403 must prove:

- **Goldens.** 46 blob goldens are checked in under `crates/oce-api/tests/fixtures/blob/`, together
  with a per-class signature-hash table, so any change shows up in review.
- **Round trip.** Decoding and re-encoding reproduces every golden byte for byte.
- **Cross-architecture bytes.** The determinism matrix encodes on x86_64 and arm64, and #403 adds
  wasm32. Every leg must equal the goldens.
- **Hostile input.** Each case is a typed refusal, never a panic, and mutations are re-checksummed
  so that they reach the check under test:
  - every truncation boundary;
  - bad magic, version ±1, a length mismatch, and a flipped CRC;
  - unknown classes and class-hash mismatches;
  - `u32::MAX` counts and lengths;
  - noncanonical ordering in each list;
  - a duplicated or missing connector id;
  - a non-permutation schedule, and one that violates emit-before;
  - nonzero flags and trailing bytes;
  - invalid UTF-8 and invalid tags;
  - initial-state CRC mismatches;
  - state above `MAX_STATE_WORDS`.
- **Same outputs on both paths.** For each fixture, the `std` path and the decoded blob take the
  same frames, the Tier-2 trace inputs. After every tick, every boundary output and the whole
  `RunState` (values and words) are compared bit for bit. This runs on the host in #400, on the
  `no_std` target in #401, and on x86_64, arm64 and wasm32 in #403.
- **§7.17 seam.** A `frame_purity`-style test fills and mutates every `SemanticPoint` payload byte
  and requires an identical decoded executable and identical outputs.

## Questions for the reviewer

1. **Initial state as per-block CRCs, not words** (decision 3). Carrying the words would make
   `relief_fan_group` about three times larger and buys only a richer mismatch diagnostic.
2. **Zero content id for partial exports** (decision 5). It currently applies to 13 of the 46
   fixtures. The alternative is to hash the loaded CXF bytes, which has different semantics from
   the facade's id.
3. **No implementation-revision counter in the class hash** (decision 2). A hand-bumped counter
   is a promise the compiler cannot check. This note leaves behavioral identity to host build
   qualification, the initial-state check, and the dual-path CI.
