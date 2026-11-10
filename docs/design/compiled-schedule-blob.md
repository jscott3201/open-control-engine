# Compiled-schedule blob (OCEB), format version 1

**Design note, written before M1b code (GitHub #396).** It fixes the byte format that #400 encodes
and decodes, #401 executes on a `no_std` target, #402 sizes in a firmware image and #403
round-trips across architectures. Where this note departs from the owner's starting sketch, it says
so and why. The user-facing contract #400 shipped is [Compiled-schedule blob](../compiled-schedule-blob.md).
The content-id field's meaning changed once after the first release that shipped the format
(GitHub #470, decision 5). The format number is 1 and stays 1 until 1.0.0 (issue #509, decision 6):
the change was made in place, and the engine version in the header separates the two layouts.

## What the blob is

A `std` host runs the full pipeline: CXF import, validation, block instantiation, Kahn
scheduling and state allocation. The blob carries that pipeline's output in a form a `no_std`
target can execute without `oce-cxf`, `oce-expr`, `oce-validate` or `oce-api`. Every parameter is
a ground value, the block evaluation order is frozen, and connector numbering is unchanged. The
target re-instantiates blocks from its own registry. It checks every class against the host's
class signature. Then it runs the unchanged `oce_graph::eval_tick` through the one HostTick v1 frame driver
(decision 11).

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
- A whole blob is at most 64 MiB, so every section length fits a `u32`. The value equals the
  snapshot cap, but `MAX_SNAPSHOT_BYTES` is `pub(crate)` in `oce-api`, so `oce-graph` declares its
  own `MAX_BLOB_BYTES`.

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
| 8 | 6 | `u16`×3 | Encoder engine version: major, minor, patch (`0, 5, 0` today) |
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
neighbor comparison. For every pair the decoder also requires:

- the producer is an output connector and the consumer an input connector;
- the producer and consumer have the same `PortKind`;
- the producer and consumer belong to different blocks;
- the consumer is not an I/O input target.

Without the kind check, a release tick would read a mismatched value as the type zero, which is
wrong data on a control path. The decoder then rebuilds `driver_of` with the unchanged
`build_driver_of`.

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
46 G36 catalog fixtures in `crates/oce-cxf/tests/fixtures/g36` tie at one block; that directory's
47th document is the resolver-contract fixture `member_list_interface.jsonld`. Among the twelve,
this one and the four `title24_differential_offset_*` variants are the only ones that also have a
boundary input and output, so every section except Connections is non-empty. Of those five, this
one is the smallest.

The model is one `CDL.Reals.AddParameter` (`p = 0.0`): boundary input `TRet` drives connector 0
(`u`), and connector 1 (`y`) is boundary output `TCut`. The content id field holds the SHA-256
digest of its canonical content id,
`oce:canon1:22d010e1298d0a1392a8bcc0475d9e201c2b41406bd90593c5207b60aa015a51`
([content-id note](content-id.md)). Both I/O paths are 89 bytes:
`http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.` + `TRet` /
`TCut`.

The dump below walks the encoded bytes field by field. For strings longer than four bytes it shows
only the `u32` length prefix; the UTF-8 bytes follow. It was first printed by the throwaway
prototype at engine 0.2.0 with a placeholder class hash; it is regenerated (2026-11-09, issue #509)
from the checked-in golden
(`crates/oce-api/tests/fixtures/blob/generic_air_economizer_high_limits_ashrae_differential.oceb`)
at engine 0.5.0, so the class hash, the content id and the CRC are the real bytes.

```text
0000  4f 43 45 42                      magic "OCEB"
0004  01 00                            format 1
0006  01 00                            profile 1 (HostTick v1)
0008  00 00 05 00 00 00                engine 0.5.0
000e  00 00                            flags 0
0010  a2 01 00 00                      total length 418
0014  22 d0 10 e1 29 8d 0a 13 92 a8 bc c0 47 5d 9e 20
                                       content id: SHA-256 digest bytes 0-15
0024  1c 2b 41 40 6b d9 05 93 c5 20 7b 60 aa 01 5a 51
                                       content id: digest bytes 16-31
0034  01 00 00 00 26 00 00 00          section 1 Classes
003c  01 00 00 00                      1 class
0040  16 00 00 00                      str "CDL.Reals.AddParameter"
005a  0d c3 5f e4 a5 cf 1f e7          class signature hash 0xe71fcfa5e45fc30d
0062  02 00 00 00 37 00 00 00          section 2 Instances
006a  01 00 00 00 02 00 00 00          1 block, 2 connectors
0072  00 00 00 00 00                   block 0: class 0, kind [A]
0077  00 00 00 00 00 00 00 00          0 state words, state CRC 0
007f  01 00 00 00 00 00 00 00          1 input: connector 0
0087  01 00 00 00 01 00 00 00          1 output: connector 1
008f  01 00 00 00                      1 parameter
0093  01 00 00 00 70                   name str "p"
0098  00 00 00 00 00 00 00 00 00       value: tag 0 (Real), bits of +0.0
00a1  03 00 00 00 08 00 00 00          section 3 Schedule
00a9  01 00 00 00 00 00 00 00          order = [block 0]
00b1  04 00 00 00 04 00 00 00          section 4 Connections
00b9  00 00 00 00                      0 connections
00bd  05 00 00 00 d9 00 00 00          section 5 I/O
00c5  01 00 00 00                      1 input
00c9  59 00 00 00                      path str "…TRet" (89 bytes)
0126  00 00 00                         vtype Real, no min, no max
0129  01 00 00 00 00 00 00 00          1 target: connector 0
0131  00 00 00 00                      SemanticPoint payload, empty
0135  01 00 00 00                      1 output
0139  59 00 00 00                      path str "…TCut" (89 bytes)
0196  01 00 00 00 00 00 00 00          source connector 1, empty payload
019e  26 83 7c 9b                      CRC-32C
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
`oce-api` adds the facade-only facts: content id, input definitions and output identities. The
HostTick v1 frame driver moves beside the decoder (decision 11).

The target does not need:

- **`oce-cxf`**: JSON-LD parsing, IRI expansion and `serde_json`. The host has already resolved
  everything.
- **`oce-expr`**: binding expressions are already evaluated into parameter values.
- **`oce-validate`**: the decoder runs its own narrower structural checks, listed below.
- **`oce-api`**: the facade lifecycle and `std` collections (it has no storage dependency since #390).

Rejected alternatives:

- **`oce-blocks`.** It cannot see `Schedule` or `RunState`; the dependency points the other way.
- **A new `oce-exec`.** It would depend on exactly `oce-graph` and own nothing but a codec for
  `oce-graph`'s types. That would split the format from the types it encodes, and it would add a
  package to classify and publish for no capability.

**2. Class signature hash.** The hash is FNV-1a-64 over one canonical record per class, computed by
one function in `oce-blocks` that both the encoder and the decoder call. The record is built from a
default-parameter instance and contains, in order:

- `class_path`
- resolved kind, `state_len` and `state_contract_revision`
- the input and output port kinds and port names
- the naming policy, `width_driven`, `reserved` and `target_bound` flags
- every `ParamRule`, as a variant tag and its fields
- every `ParamDefault`, as name, source tag and literal value

A `Derived` default contributes only its source tag. Its `formula` text is documentation, and a
wording edit must not refuse every blob that uses the class.

Counts are `u32`, Reals are bits, and strings use `str`. The tag tables use exhaustive matches, so
a new `ParamRule` variant cannot compile without one. The hash does not cover:

- tick behavior (`step_algebraic`, `emit_from_state`, `update_state`, `feeds_through`)
- `validate_state`
- non-default arities
- catalog order
- `Derived` formula text

The decoder checks three of those per instance instead:

- **Arity and kinds:** the constructed block's resolved arity and port kinds must match the record.
- **State layout:** `kind`, `state_len` and the initial-state CRC must match (decision 3).
- **Schedule:** the order must be valid under the target's own `feeds_through`.

Behavioral identity beyond that rests on the exact engine-version match (decision 6) and the
per-PR dual-path test. Between release tags, every `development` build shares one version string.
A blob and the target that runs it must therefore come from the same build: the current/current
rule that applies to snapshots ([state compatibility](../state-compatibility.md)). An unknown class is `UnknownClass`, and a
different hash is `ClassSignatureMismatch`; both carry the class index and both hashes where they
apply. Neither panics.

Rejected: **one registry-wide digest.** It would refuse a blob because an unused class changed, the
case the state contract already excludes. (A firmware image with a reduced registry would also
benefit from per-class hashes, but no such capability exists: `registry::CATALOG` is one static
slice. That is a possible #402 feature, not part of this rationale.)

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

Non-finite Real parameters cannot reach the encoder; load and `set_param` have refused them since
GitHub #376. NaN-payload portability is therefore moot for parameters, and the encoder checks it
again and refuses rather than encoding a non-finite parameter. Boundary input values are frame
data, not blob content.

Initial state words are **not** carried. The decoder calls the unchanged `allocate_state`, which
does five things:

- seeds every connector with its type zero (Enum ordinal 1, String empty);
- seeds each stateful region from `init_state`;
- fires **every input-less algebraic block** once at `t = 0` with a no-op diagnostics sink, not
  only constants;
- builds `slots`, `slot_of` and the gather scratch;
- sets `t = 0.0`.

The decoder then compares each stateful block's region with that block's carried CRC-32C; a
difference is `InitialStateMismatch` naming the block. All of this matters because the decoded
`RunState` is compared word for word with the host's. This departs from the sketch. Carrying the words would cost 8 bytes per word,
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
- each output source is an output connector;
- no input `vtype` is Enum. The tag stays in the table for parity with the snapshot, but
  `oce-validate` makes an Enum input unreachable, so the decoder refuses it;
- no two outputs share a path. The encoder refuses duplicates too, because
  `executable_boundary_outputs` sorts unstably on path alone; this note defines refusal, not a
  tie-break.

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

*Amended 2026-11-09 (GitHub #470, [canonical content id](content-id.md)).* M3 replaced the
algorithm: the field holds the 32 SHA-256 digest bytes of the canonical content id
(`oce:canon1:`, over the loaded flat graph) in digest output order, with no padding, and is present
for every loaded document, the 13 partial exports included. 32 zero bytes now means only "no id"
(a hand-built model). The decoder still never interprets the field, so no hash is linked on a
target. The rest of this decision records the field as release 0.4.0 wrote it; that change did not
keep its own format number (decision 6, as amended).

`content_id_complete` refused partial exports. 13 of the 46 G36 fixtures export partially today;
in all 13, a PID's `controllerType` is enum-valued. For those, the field is 32 zero bytes, meaning
"no content id". The `oce-api` encoder API reports the id as an `Option`. The decoder never
interprets the field. It does keep it verbatim, and the decoded executable also exposes it as an
`Option`, so no host has to test for 32 zero bytes. M3's canonical id
must be defined for every loadable model, whether or not its export is partial.

The enum deferral is not specific to PID: export defers any block with an Enum connector or
parameter, and its downstream consumers with it.

**6. Versioning.** The format version is a `u16`. Any change to layout, a tag table, the
signature-record definition, the CRC, or the content-id meaning bumps it. A decoder accepts
exactly its own version and its own execution profile. Any other version, newer or older, is
`UnsupportedFormat { found, supported }`. This is current/current only, as for snapshots
([state continuation and portability](../state-compatibility.md)). Engine version must also match exactly;
a mismatch is `EngineVersionMismatch` naming both versions.

*Amended 2026-11-09 (issue #509).* The owner ruled that every format the project defines stays at
revision 1 until 1.0.0, with no backwards compatibility: a change of the kinds listed above is made
in place and the goldens are re-blessed, and nothing reads two layouts. The format number therefore
no longer separates layouts; the engine version does, because the header carries it and the decoder
accepts only its own. The first PR after a release tag that changes the format's bytes or meaning
bumps the workspace version on `development`. This was found the hard way: with the number reset
and the workspace still at 0.4.0, all 46 blobs of release 0.4.0 decoded, 33 of them presenting the
retired export tag as a canonical content id.

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
not blob bytes.

That list, `TARGET_BOUND_CLASSES`, lived in `oce-api/src/state_manifest.rs`, which the target
does not link. #400 moved it into `oce-blocks` as a per-class `target_bound` flag
(`oce_blocks::class_is_target_bound`, `CatalogEntry::target_bound`), part of the hashed signature
record (decision 2). `oce-api` then derives its list from that flag, with the
same 15 classes, so snapshot bytes are unchanged. The decoded executable exposes
`target_bound: bool` computed from its instances, as `CompletedFrame.target_bound` does today.
None of the 46 fixtures uses one of those classes.

A variant that embeds a platform stamp, as the snapshot's `TargetBound { arch, os }` does, would be
a further format version. The reserved `u16` flags must be zero, matching the snapshot's zero reserved
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

**11. One HostTick v1 frame driver, in the core.** The note so far decides the bytes; this
decision decides what runs them. `eval_tick` is not a frame. Today `execute_frame` and
`prepare_frame` in `crates/oce-api/src/frame.rs` add a frame around it. That frame moves to
`oce-graph`, beside the decoder, as a slot-indexed driver. The facade and the `no_std` entry (#401)
both call it, so HostTick v1 exists once. #401 performs the extraction, and the dual-path test then
compares the facade with the core driver.

What moves into the core driver, in the facade's documented refusal order, every refusal before
any mutation:

- **Time checks:** finite; `|t| ≤ MAX_FRAME_TIME_MAGNITUDE`, which moves to the core and is
  re-exported; `t ≥ prev_t`; finite step; `time_is_representable` across every stateful block.
- **Input checks, per slot in slot order:** exact type, then the non-finite Real policy
  (`NonFiniteInputPolicy` moves to the core), then the declared domain. Inputs arrive as a slice
  with one value per slot, so completeness is structural.
- **Sequence:** the accepted-frame counter and `FrameSequenceExhausted`.
- **Commit:** stage each value into every fan-out target, run `eval_tick`, set `prev_t`.
- **Diagnostics:** the sink is a parameter (`oce_blocks::Diagnostics`).
- **Outputs:** projected in output-slot (path) order, borrowed, not copied.
- **Placement:** the `target_bound` flag.

What stays in `oce-api`, because it needs names or `std`:

- name resolution, unknown, duplicate and missing keys;
- the `PreparedInputFrame` plan, its generation token and the stale-plan check;
- load and pending-edit preconditions, and the durable-restore window;
- replay records;
- the `AssertCollector`'s `String` diagnostics and the owned `CompletedFrame`;
- nothing from the storage port: #390 removed it from the facade.

Rejected: **a second, `no_std` HostTick loop in #401.** Two implementations of the time and input
rules would drift, and the drift would only show on equipment.

**12. Trust model.** For semantics, a blob is a *trusted* artifact: a well-formed but wrong
parameter executes as written, because `oce-validate` and its `ParamRule` checks are not linked on
the target. For memory safety, a blob is a *hostile* artifact: the decoder never panics, bounds
every allocation before performing it, and refuses malformed bytes with typed errors.

The consequence for #400 is a hostile-parameter fuzz over `make` for every class, bounded by
`MAX_RESOLVED_PORT_WIDTH`. The two `unreachable!` sites in `source_timetable.rs`
(`smoothness_from_member` and `extrapolation_from_member`) are already unreachable from any typed
table: their only non-test callers pass the module constants `TIMETABLE_SMOOTHNESS_DEFAULT_MEMBER`
and `TIMETABLE_EXTRAPOLATION_DEFAULT_MEMBER`. A parameter value goes through `from_value`
instead. That returns `None` for an unknown token, and the caller then falls back to the module
constant. #400 keeps that proof as a test that feeds every member token,
plus arbitrary strings, through `from_value` and `make`.

## Decoding, determinism and allocation

The decoder refuses in this order:

1. Length above 64 MiB: `TooLarge`.
2. Short input, wrong magic, or a header length that differs from the input length: `Malformed`.
3. Wrong format version: `UnsupportedFormat`.
4. Trailer mismatch: `IntegrityMismatch`.
5. Wrong profile or engine version, or nonzero flags.
6. Structural problems, each with a byte offset: section order, bounded counts, UTF-8, tags,
   ordering, trailing bytes.
7. Classes and per-instance checks, each with a named variant:
   - unknown class (`UnknownClass`) or hash mismatch (`ClassSignatureMismatch`);
   - duplicate parameter names within one instance (`DuplicateParameter`);
   - an Enum value whose class path is unknown (`UnknownEnumClass`), or whose ordinal is outside
     `1..=members` (`EnumOrdinalOutOfRange`);
   - resolved arity, port kinds, kind and state length (`InstanceMismatch`).
8. Connections and I/O, as listed in their sections, including a same-block connection,
   duplicate output paths and an Enum input.
9. The schedule (`InvalidSchedule`).
10. `MAX_STATE_WORDS` via `allocate_state` (`StateTooLarge`), then the state CRCs
    (`InitialStateMismatch`).

A count is accepted only if the remaining section bytes could hold that many minimal records, so
no allocation precedes that proof.

The schedule must be a permutation of `0..B`, and it must respect emit-before as `topo.rs` defines
it. An algebraic `[A]` consumer reads every input at emit time. A stateful `[S]` consumer reads
input *i* at emit time only if `feeds_through(i, j)` holds for some output *j*. For every
connection into an input its consumer reads at emit time, the producer's position must be
strictly less than the consumer's. The inequality is strict, so a same-block connection could never
pass; section 4 refuses it before this check. Any valid order gives the same outputs, so the
decoder does not need to reproduce the host's tie-break.

Errors are plain enums carrying static labels, indices and offsets, never formatted text;
`Display` exists only under `std`. The decode path is panic-free. That it links no `core::fmt` is
a goal, the pattern `rusty-bacnet`'s firmware images use, which #401 and #402 verify. It depends on
#397, which converts the core four from `std` to `alloc` and is a prerequisite of #400. The `oce-api` encoder
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

The decoded `Model` fills the fields the execution path never reads with fixed values:
`decl_order` equals the id, connector and instance `iri` are `None`, and each connector gets the
default `Attrs` for its `value_type`. The decoder keeps the content id, the engine version and the
`SemanticPoint` payloads verbatim, so decoding and re-encoding reproduces the goldens.

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

Since GitHub #530 the two do share the engine version: the snapshot header carries the same
`u16`×3 field from the same constant (`oce_graph::blob::ENGINE_VERSION`), and its decoder refuses
another version with its own `EngineVersionMismatch`
([state contract](../state-compatibility.md#wire-revision-and-integrity)).

## Size budget

| Item | Bytes |
| --- | --- |
| Fixed: header 52, five section frames 40, section counts 28, trailer 4 | 124 |
| Per class | 12 + class-path length |
| Per block instance, schedule entry included | 29 + 4 per port + parameters |
| Per parameter | 4 + name length + `value`, where `value` includes its tag byte (Real/Integer 9, Boolean 2, String 5 + length, Enum 9 + class-path length) |
| Per connection | 8 |
| Per input | 15 + path length + 4 per fan-out target + 9 per present bound + payload |
| Per output | 12 + path length + payload |

In the worked example, the parameter is 4 + 1 + 9 = 14 bytes, the instance 29 + 8 + 14 = 51, and
the whole blob 124 + 34 + 51 + 108 + 101 = 418. The formula was measured by the prototype over all 46 fixtures. Sizes are exact; the hash values
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
- **Hostile parameters.** A fuzz over `make` for every class, with widths bounded by
  `MAX_RESOLVED_PORT_WIDTH`, and the `source_timetable.rs` member-token test (decision 12).
- **Connection checks.** Refusals for a kind mismatch, reversed direction, a same-block connection
  and a consumer that is also an I/O target.
- **Duplicate names.** Refusal of duplicate output paths, by the decoder and by the encoder, and of
  duplicate parameter names within an instance.
- **Enum values.** Refusal of an Enum input `vtype`, an unknown enum class path and an
  out-of-range ordinal.
- **One frame driver.** For every fixture's frame sequence, the facade's `execute_frame` and the
  core driver produce identical outputs, refusals and state (decision 11).
- **§7.17 seam.** A `frame_purity`-style test fills and mutates every `SemanticPoint` payload byte
  and requires an identical decoded executable and identical outputs.

## Review rulings

The architect review on the draft PR accepted:

- initial state carried as per-block CRCs;
- the zero content id for partial exports, with `Option` presence and the M3 requirement added to
  decision 5;
- the class hash with no implementation-revision counter, with the same-build caveat added to
  decision 2;
- `BlockId` order with a separate schedule list, I/O sorted by path, and an exact engine-version
  match;
- the decoder in `oce-graph`, with decision 11 added.
