# Compiled-schedule blob

A compiled-schedule blob (`OCEB`, format version 1) is the loaded executable as bytes: what a `std`
host computes from CXF — every block with its ground parameters, the frozen evaluation order, the
connections and the boundary I/O table — in a form a `no_std + alloc` target decodes and runs
without the CXF importer, the expression evaluator or the validator. It is the sibling of
[state continuation and portability](state-compatibility.md): a snapshot carries *state* for an
executable both sides already loaded; a blob carries the *executable* and no state.

The byte layout, the reasoning behind each choice and the rejected alternatives are in the
[design note](design/compiled-schedule-blob.md) (GitHub #396). This page is the contract a host
relies on (GitHub #400).

## Producing and decoding a blob

```rust,ignore
// Host (std): load CXF, then encode the executable.
let mut engine = oce_api::Engine::new();
engine.load_cxf(&cxf_bytes)?;
let blob = engine.compile_blob()?;          // CompiledBlob { bytes, content_id }

// Target (no_std + alloc, same engine version): decode into a runnable executable.
let mut executable = oce_graph::blob::Executable::from_blob(&blob.bytes)?;
executable.set_non_finite_input_policy(oce_graph::frame::NonFiniteInputPolicy::Refuse);
let t_ret = executable.input_slot(T_RET_PATH).unwrap();     // resolve paths once, at boot
let t_cut = executable.output_slot(T_CUT_PATH).unwrap();
let mut inputs = vec![Value::Real(0.0); executable.inputs().len()]; // one value per input slot
inputs[t_ret] = Value::Real(297.15);
executable.execute_frame(0.0, &inputs, &oce_blocks::NoopDiagnostics)?;
let cutoff = executable.output(t_cut);                      // borrowed from the run state
```

`Engine::compile_blob` refuses with `OcError::State` when no model is loaded or parameter edits are
pending, and with `OcError::Blob` when the executable cannot be encoded. It decodes its own output
before returning, so every success is a blob the same build accepts. Executed frames do not change
the bytes; a resumed parameter edit does. `oce_graph::blob::decode` alone returns the decoded parts
(model arena, schedule, blocks, run state, I/O table); `Executable` wraps them with the HostTick v1
frame driver ([executing a blob](#executing-a-blob)). `oce_api::inspect_blob` reads what a blob
records about itself and reports whether this build accepts it, without running it
([inspecting a blob](#inspecting-a-blob)).

## What travels

| Content | Form |
| --- | --- |
| Every block class used | Canonical class path and its class signature hash (below) |
| Every block instance, in `BlockId` order | Class, kind, state word count, a CRC-32C of its initial state words, its input and output connector ids, and its resolved parameter table exactly as the constructor received it, in table order |
| The evaluation order | The block order the host's schedule froze |
| Connections | Producer output and consumer input connector ids, ordered by consumer |
| Boundary inputs | Path, exact type, the effective inclusive acceptance domain, and every fan-out target connector |
| Boundary outputs | The executable boundary identity and its source output connector |
| `SemanticPoint` payloads | One opaque, length-bounded payload per input and output, empty in v1 |
| Content id | The 32 SHA-256 digest bytes of the loaded model's [canonical content id](design/content-id.md) (`Engine::content_id`, `oce:canon1:`) at encode time, for every loaded document, partial export or not; 32 zero bytes ("none") only for a hand-built model with no id. Carried verbatim, never interpreted by the decoder |
| Engine version | The encoder's `major.minor.patch` |

Block and connector ids are the host's own, so the decoded `RunState` compares with the host's word
for word. Every parameter is a ground value; Enum values travel by canonical class path and ordinal,
never by the run-local class id. Reals travel as their IEEE-754 bits.

## What does not travel

- **Run state.** The decoder allocates the same initial state the host allocates after load and
  checks each stateful block's region against the carried CRC. Continuing a running host's state on
  a target is not supported in v1.
- **Authored identities beyond the I/O paths.** Instance and connector IRIs, units, quantities,
  display units, point-list, trend and semantic metadata stay on the host; they are
  non-computational (§7.17 of the OpenBuildingControl (OBC) specification; ANSI/ASHRAE 231-2026
  §5.13). The `SemanticPoint` payload slots exist for them and the decoder never reads them.
- **The CXF document and its parameter expressions.** Bindings are already evaluated.
- **Host policy.** The non-finite input policy, diagnostics handling, cadence and every
  [host responsibility](host-responsibilities.md) stay with the host.
- **Placement.** A blob has no architecture or operating-system field: its bytes do not depend on
  the encoder's platform. A target-bound class (one of the 15 listed in
  [state continuation](state-compatibility.md)) still makes the *outputs* depend on the target's
  math library; the decoded executable reports this as `target_bound`. None of the 46 G36 catalog
  fixtures uses one.
- **A signature.** See the trust model.

## Executing a blob

`oce_graph::blob::Executable` is the `no_std + alloc` execution entry (GitHub #401): what a
bare-metal or WASM target links instead of the facade. It decodes the bytes once
(`Executable::from_blob`) and runs frames through the same HostTick v1 frame driver
(`oce_graph::frame`) the facade's `Engine::execute_frame` calls, so the time checks, the input
checks, the non-finite policy, the accepted-frame sequence, staging and the single evaluation per
frame are one implementation on both sides, and an equal timestamp advances state again
([execution profile](execution-profile.md#one-frame-driver-two-callers)).

- **Slots, not names.** Inputs and outputs are addressed by their position in the blob's I/O table
  (`inputs()`, `outputs()`), sorted by UTF-8 path. The host resolves each path once
  (`input_slot`, `output_slot`, a binary search) and passes one value per input slot each frame;
  a slice of another length is `FrameRefusal::InputCount`. There is no hold-last value and no
  substitution, as on the facade.
- **Refusals** are `oce_graph::frame::FrameRefusal`, slot-indexed, in the facade's order: time,
  input count, then per slot type, non-finite policy and domain, then sequence. Each precedes any
  mutation.
- **Non-finite inputs.** The policy starts at `NonFiniteInputPolicy::Admit`, the facade's default;
  production hosts are recommended to select `Refuse` before the first frame. The blob does not
  carry the policy.
- **Outputs** are borrowed from the run state (`output(slot)`, `output_values()`), never copied.
- **Placement.** `target_bound()` reports a target-bound class, as `CompletedFrame` does.

### State words

`state_words()` returns the stateful blocks' words in the decoded layout, raw: no portability tag,
manifest or checksum. They are not the facade's portable state snapshot, which stays in `oce-api`,
and a target-side snapshot format is a later decision (M9's warm restart). `restore_state_words`
replaces them all or nothing and refuses with a typed `StateWordsRefusal`: a different length;
before the first accepted frame, anything but the initial words; after it, any region its block's
own state contract rejects at the executable's current model time (the check the facade's restore
applies), so words that would index outside a block's buffers are refused, not executed. Model
time, sequence, policy and connector values are untouched; the next frame recomputes the outputs
from the words and its inputs. Keep words with the executable and model time they came from.

On the host, `Engine::state_words` borrows the facade's words in the same layout, so a verifier can
compare the two paths word for word after the same frames (the [round trip](#the-blob-round-trip)
does). It is a read-only verification view, not a continuation format: continue state with
`state_snapshot` or `checkpoint`.

## Inspecting a blob

`oce_api::inspect_blob(&bytes)` reads what a blob records about itself and returns a
`BlobSummary`, without running it (issue #448). The summary also says whether this build accepts
the blob (`decode_refusal`, issue #680). It is what a comparison of two blobs needs: the
[structural diff](design/diff.md) compares blobs as same or different by content id, by class
path and class signature hash, and by each instance's initial-state CRC, and a leverage crate may
not call `oce_graph::blob`. That comparison is `oce_diff::compare_blobs`
([the blob comparison](../crates/leverage/oce-diff/README.md#the-blob-comparison), issue #667),
which reads both sides through `inspect_blob` and carries each side's `decode_refusal`, so it
costs two summaries ([what a summary costs](#what-a-summary-costs)). No `Engine` is involved. `oce_graph::blob::inspect` is the same reader
in the `no_std + alloc` core, returning a `Summary` whose paths borrow from the bytes.

```rust,ignore
let summary = oce_api::inspect_blob(&blob.bytes)?;           // BlobSummary, owned
assert_eq!(summary.engine_version, oce_api::ENGINE_VERSION); // any other version is refused
assert_eq!(summary.content_id, blob.content_id);             // Option<ContentId>
assert_eq!(summary.decode_refusal, None);                    // this build accepts the blob
for instance in &summary.instances {
    let class = &summary.classes[instance.class as usize];   // the index is always in range
    let (hash, crc) = (class.signature_hash, instance.initial_state_crc);
    println!("{} {} {hash:016x} {crc:08x}", instance.block, class.class_path);
}
```

| Field | What it holds |
| --- | --- |
| `format_version` | The header's format version: 1 |
| `engine_version` | The encoder's `major.minor.patch`: this build's, because any other is refused |
| `content_id` | The header's 32 digest bytes read back as the canonical content id (`oce:canon1:`), `None` when they are all zero (a hand-built model with no id). Read, never recomputed or checked against the body |
| `classes` | The class table in blob order (strictly increasing by path bytes): `class_path` and the `signature_hash` the encoder recorded |
| `instances` | One record per block, in block id order: `block`, `class` (an index into `classes`), `stateful`, `state_words` and `initial_state_crc` (CRC-32C of the initial state words, each as eight little-endian bytes), all three as recorded |
| `inputs`, `outputs` | The boundary I/O paths in slot order |
| `target_bound` | Whether any class in the table is target-bound in this build's registry |
| `decode_refusal` | This build's verdict on the blob, an `Option<BlobError>`: `None` when its full decoder accepts the bytes, `Some(error)` with the decoder's own first refusal when it does not |

### Whether this build accepts the blob

**Having a summary is not acceptance. `decode_refusal` is the verdict.** Read the result of
`inspect_blob` in three cases:

| Result | Meaning |
| --- | --- |
| `Ok(summary)`, `summary.decode_refusal == None` | **This build accepts the blob**: `oce_graph::blob::decode` returns `Ok` for these bytes, so `Executable::from_blob` of the same build constructs them |
| `Ok(summary)`, `summary.decode_refusal == Some(error)` | **This build refuses the blob**, and `error` is exactly what `decode` returns for it. The other fields describe the refused blob as written |
| `Err(OcError::Blob(error))` | The bytes have no summary, and this build refuses them too |

So `inspect_blob(bytes).is_ok()` is not an acceptance check. A host that admits a blob writes:

```rust,ignore
let accepted = matches!(oce_api::inspect_blob(bytes), Ok(s) if s.decode_refusal.is_none());
```

The verdict comes from the decoder, not from a second set of checks. After the checks described
below, the reader runs the decoder's own building pass once over the same bytes (the function
`decode` itself calls for its steps 7 to 10 in [Refusals](#refusals): classes and their signature
hashes, parameters, constructed blocks, connectors, boundary I/O, connections, the schedule, the
state budget and each initial-state CRC), keeps its refusal and drops what it built. Blocks are
constructed from the blob's parameters and their state is seeded, as on a target; the schedule never ticks. The verdict is this
build's: a blob is accepted only by the engine version that wrote it, and within one version a
build whose class registry differs can answer differently. Acceptance is about the bytes, not
their origin: the CRC-32C trailer is not tamper evidence ([trust model](#trust-model)), so
authenticate the bytes first, as for a decode.

### What has a summary

The reader runs the decoder's own checks up to the end of its structural pass, by the same code
and in the same order: in [Refusals](#refusals), everything from the size limit through
"structure". Those refusals are the decoder's, the same variant at the same offset, and a blob
another engine version wrote has no summary: `EngineVersionMismatch`
([which refusal wins](#which-refusal-wins)). Two checks follow, because the summary's own fields
need them: every class path is registered in this build (`UnknownClass`; `target_bound` is read
from this build's registry and cannot be answered for a class it does not hold), and every
instance's class index is inside the class table (`InstanceMismatch` with `ClassIndex`). Those are
the only `Err`s, and `decode_refusal` never carries one of them.

Every other refusal of the decoder leaves the summary in place and is reported in
`decode_refusal`. That is the point: a comparison can show the class whose hash changed between
two builds of one engine version, or the instance whose initial state changed, in blobs one of
the two builds would not run, and can say that it would not. The lists are not corrected to what
this build would construct: a signature hash, a state length and an initial-state CRC are
reported as recorded. After the structural pass an `Err` need not be the error `decode` reports:
given a wrong hash in class 0 and an unregistered class 1, the decoder stops at class 0 and the
summary reader, which checks the whole class table first, at class 1.

Read the list fields as the blob's own statement. Two initial-state CRCs compare only when their
`state_words` are equal. A repeated output path can appear in a summary, because the decoder
refuses it after the structural pass; `decode_refusal` then says so.

### What a summary costs

Every check runs before anything is allocated, so a refusal (`Err`) allocates nothing, whatever
counts the refused bytes declare: a 3 MB table of 200,000 unregistered class paths is refused
holding no memory.

A summary costs two things in turn, never held together:

1. **The decoder's building pass**, exactly what `decode` allocates for the same bytes up to its
   result or its refusal, all of it freed before step 2. Its peak is the decoder's
   ([Cost](#cost)), and it is bounded: at most 64 bytes per blob byte, plus three fixed terms
   of 32 MiB each. The first is what block constructors allocate from the blob's parameters,
   held to 4,194,304 words across the model (`MAX_CONSTRUCTED_WORDS`); the second is the one
   block under construction when that budget is checked; the third is the state the blocks
   declare, held to 4,194,304 words (`MAX_STATE_WORDS`). So **a summary of `n` blob bytes peaks
   within `64 n` bytes and 96 MiB**, whatever the parameters size: the 4,003-byte blob of 64
   `CDL.Reals.Sources.TimeTable` blocks that peaked at 536,903,536 bytes is now refused at its
   fourth block holding 33,564,016, and its summary carries `ConstructedTooLarge` as the
   refusal. For the 46 goldens the peak is 1.55× to 21.59× the blob length.
2. **The result.** The core reader's four lists are each allocated once at a count the structural
   pass proved against the bytes: four allocations at most, and at most 2 bytes held per blob
   byte (measured: 1.33× for 500,000 outputs with empty paths, 0.8× for 100,000 minimal
   instances; 0.17× to 0.41× for the goldens). The facade's owned summary adds one `String` per
   non-empty path and requests at most 4 bytes per blob byte while it is built. That bound counts
   the bytes asked of the allocator, not its per-allocation overhead, which a blob of millions of
   short paths multiplies.

So the peak of a summary is the larger of the two, each bounded as above, and its allocation
count in the core is the
decoder's for the same bytes plus one per non-empty list. Before issue #680 a summary cost step 2
alone; the verdict is what step 1 buys. Measured peaks, as multiples of the blob length:

| Shape | Verdict | Building pass | Lists (core) | Summary peak, core and facade |
| --- | --- | --- | --- | --- |
| The 46 goldens | accepted | 1.55× to 21.59× | 0.17× to 0.41× | 1.55× to 21.59× |
| One 200,000-input block fed by one boundary input | accepted | 26.62× | ≈0× | 26.62× |
| One block owning the whole state budget, a 287-byte blob | accepted | 32 MiB of state words | 76 bytes | 32 MiB |
| Four `TimeTable` blocks holding the whole constructed budget, a 403-byte blob | accepted | 32 MiB of tables | 104 bytes | 32 MiB |
| 64 `TimeTable` blocks of 8 MiB each, a 4,003-byte blob | refused at the fourth block (`ConstructedTooLarge`) | 33,564,016 bytes | 1,304 bytes | 33,564,016 bytes |
| 500,000 outputs with empty paths | refused at the first output | 3.33× | 1.33× | 3.33× |
| 100,000 minimal instances | refused at block 0's arity | 5.12× | 0.80× | 5.12× |
| One block listing 10⁶ input ids it does not have | refused at its arity check | 2.00× | ≈0× | 2.00× |

## Versioning

A decoder accepts exactly one format version (1), one execution profile (HostTick v1) and its own
engine version; anything else, older or newer, is a typed refusal. Between release tags every
`development` build shares one version string, so a blob and the target that runs it must come
from the same build — the same current/current rule as state snapshots. The reserved header flags
must be zero. The format has no optional or skippable sections.

The format version stays 1 until the engine's 1.0.0 (issue #509). A change to the layout, a tag
table, the class signature record, the CRC or the content-id meaning is made in place and the
goldens are re-blessed; there is no importer and no decoder that reads two layouts. The engine
version in the header is therefore what tells a blob of one layout from another, so the first
change of that kind after a release tag also moves the workspace version. A blob encoded by
release 0.4.0 shows why: it carries this same format number, valid sections, class hashes and
CRC, and the retired `cxf:fnv1a128:` export tag where today's layout carries the canonical content
id's digest. It is refused with `EngineVersionMismatch` and never becomes an executable
(`tests/blob_old_layout.rs`). A blob encoded by release 0.5.0 shows the other half, a change of
meaning with no change of layout (issue #683): that release encoded a Real parameter written as
an Integer as an Integer, and this build holds and encodes it as a Real, so the same document's
blob differs in that one parameter record and in the content id that covers it. With only its
version bytes and CRC rewritten the 0.5.0 blob would decode, so the version field is what
refuses it (`tests/engine_0_5_0_artefacts.rs`).

Each class carries an FNV-1a-64 **class signature hash** over a canonical record of the class: the
default instance's kind, state length and state-contract revision, its port kinds and names, the
naming policy, the width-driven, reserved and target-bound flags, every parameter rule and every
parameter default (a derived default's formula text excepted — it is documentation). The decoder
recomputes it from its own registry and refuses a class whose record differs. `oce_blocks::catalog()`
publishes the hash per class (`CatalogEntry::signature_hash`), and
`crates/oce-api/tests/fixtures/blob/class_signatures.tsv` pins all 136, so a change shows in review.
The hash does not cover tick behaviour; behavioural identity rests on the exact engine-version match.

## Trust model

For **semantics** a blob is trusted: a well-formed but wrong parameter executes as written, because
the validator's parameter rules are not linked on a target. One check is linked: a parameter of a
kind its catalog type does not admit is refused (`ParameterKindMismatch`, issue #734), since a
block would otherwise run on its default rather than on anything the blob wrote. A value of the
right kind outside a class rule's range still executes. Authenticate and authorize blobs in your
host's envelope before decoding them, as you would snapshot bytes.

For **memory safety** a blob is hostile: `decode` never panics, refuses malformed bytes with a
typed `BlobError` that names the offset, index or hashes concerned, and checks every count against
the bytes that remain before allocating for it. A port list longer than any registered class can
resolve (`oce_blocks::MAX_RESOLVED_ARITY`, twice `MAX_RESOLVED_PORT_WIDTH`) is refused before
anything is sized by it, and connector ownership costs 8 bytes per id until every block's arity
has been checked, so the framing and structural passes are linear in the blob length: a hostile
blob that lists a million connector ids its block does not have is refused holding 2.0× its
length (the test bounds it at 8×).

Block constructors are the one place an allocation is sized by a parameter's name or value
rather than by a count of bytes: a `CDL.Reals.Sources.TimeTable` sizes its table from the largest
`table_<row>_<col>` name it is given, a `CDL.Reals.MatrixGain` holds `nin × nout` gains whether
or not one is bound. Each block is capped by `MAX_RESOLVED_PORT_WIDTH`, and a model has many, so
the decoder holds them to a model-wide budget (issue #700): every constructed block reports the
words it holds outside its state (`Block::constructed_words`), the decoder sums them as it
constructs, and the first block that takes the total past `oce_graph::MAX_CONSTRUCTED_WORDS`
(4,194,304 words, 32 MiB) is dropped and the blob refused with `ConstructedTooLarge`. The state
the blocks declare has its own budget of the same size (`MAX_STATE_WORDS`, `StateTooLarge`). So
the decoder's peak is bounded:

```text
  64 x the blob length                               the linear term
+ 8 x MAX_CONSTRUCTED_WORDS                 32 MiB   the constructed budget
+ oce_blocks::MAX_CONSTRUCTION_PEAK_BYTES   32 MiB   one block under construction
+ 8 x MAX_STATE_WORDS                       32 MiB   the state budget
```

The linear term is the decoded executable itself, sized from counts the structural pass checked
against the bytes. Its costliest record is a connector id in an output list, four encoded bytes
for about 184 decoded: a block of 200,000 unread outputs holds 44.2× its blob, the densest shape
measured, and the tests hold every shape and every seeded hostile model to 64×. The constructed
budget is a budget of its own, not a share of the state budget, so a model may spend both in
full; it admits one block at the widest the per-block bound allows (2,228,223 words) and three
dense 1024 × 1024 gain matrices together, while the largest of the 46 G36 fixtures holds 42
words. A model past it is refused, not truncated: reduce the table, matrix or vector sizes, or
split the model. The CXF load applies the same budget with the same counter
(`BuildError::ConstructedTooLarge`, recorded as `resource-limit-exceeded`), so a model the host
loaded is one the target constructs. Every registered block constructor is fuzzed with hostile
parameter tables bounded by `MAX_RESOLVED_PORT_WIDTH`, and measured against what it reports.

**Integrity** is a CRC-32C trailer over every preceding byte. It detects corruption in flash,
transfer or storage; it is not tamper evidence, because anyone can recompute it.

## Refusals

The decoder checks, in order, and returns the first failure: the 64 MiB size limit; a header and
trailer's worth of bytes and the magic; format version, then engine version; total length, then
the CRC; profile and flags; structure
(section order and lengths, bounded counts, port lists no longer than any class resolves, UTF-8,
tags, canonical list order, no trailing bytes);
each class (registered, same hash), then each instance in turn (class index, unique parameter
names, finite Reals, known enum classes and in-range ordinals, each parameter a value its class's
catalog type admits (`ParameterKindMismatch`: a Real on an Integer parameter, an Integer past
2^53 on a Real one, an enumeration value of another class, a stand-in naming no member; the load
gate's own predicate, `oce_blocks::admits_parameter_value`, so a block is never built on the
default its reader falls back to, issue #734), the constructed budget
(`MAX_CONSTRUCTED_WORDS`, charged the moment the block is constructed), the constructed block's
kind, state length and arity, connectors in range and not already claimed), then every class
used and every connector claimed; boundary inputs (not Enum-typed, typed targets used once,
typed, finite and uncrossed bounds), connections (output to input, equal kinds, distinct blocks, never into a boundary
input, one driver per input), undriven inputs and boundary outputs (output sources, unique paths);
the schedule (a permutation that respects emit-before under the target's own feedthrough); the state
budget (`MAX_STATE_WORDS`) and each initial-state CRC.

The constructed budget is checked inside the instance pass, so `ConstructedTooLarge` names the
first block (in id order) whose construction crosses it and wins over everything checked after
that point: that block's own kind, state length, arity and connectors, every later block, an
unused class, a missing connector, and all of the boundary, connection, schedule and state
checks, the state budget included even when the oversized state belongs to an earlier block.
It loses to everything checked before the crossing block is constructed: the header and
structure, the class table, any fault of an earlier block, and the crossing block's own class
index and parameters (`tests/blob_constructed_budget.rs`).

### Which refusal wins

One rule decides which refusal a blob or a [state snapshot](state-compatibility.md) reports when it
is wrong in more than one way (issue #535): **nothing that another engine version may lay out
differently is checked before the engine version.** Each decoder checks, in this order, the size
limit, that a header and a trailer fit, the magic, the format revision, the engine version, and
only then the total or body length, the integrity trailer and the rest. Every format stays at
revision 1 until 1.0.0 and changes in place, so a later field of another version's artefact (its
length, its trailer algorithm, its body) cannot be read until the version is known to match.

| Step | Blob (`BlobError`) | Snapshot (`EngineStateError`) |
| --- | --- | --- |
| Size limit, 64 MiB inclusive | `TooLarge` | `SnapshotTooLarge` |
| Shorter than a header and a trailer | `Malformed`, `Truncated` | `MalformedSnapshot` at offset 0 |
| Magic | `Malformed`, `BadMagic` | `MalformedSnapshot` at offset 0 |
| Format revision | `UnsupportedFormat` | `UnsupportedFormat` |
| Engine version | `EngineVersionMismatch` | `EngineVersionMismatch` |
| Total or body length | `Malformed`, `LengthMismatch` | `MalformedSnapshot` at offset 22 |
| Integrity trailer | `IntegrityMismatch` (CRC-32C) | `IntegrityMismatch` (FNV-1a-128) |
| Then | profile, flags, structure, classes, … | body, fingerprint, flags, … |

So an artefact another engine version wrote reports `EngineVersionMismatch` however else it is
damaged, unless it is over the size limit, too short to hold a header, not of its kind or of
another format revision. That holds while its magic, format and version fields sit where this
layout puts them. A layout that put them elsewhere is still refused with a typed error, though
not necessarily this one: the [state contract](state-compatibility.md#wire-revision-and-integrity)
shows this for snapshots written before the version field existed. Because the version comes
first, a corrupted version field reports the version it now spells,
not an integrity failure; the trailer still covers the field, so a version field rewritten to
this build's fails the trailer. The fields between the format and the version (the blob's
execution profile, the snapshot's execution-state ABI revision) are read on the way and checked
later: the profile after the CRC, the ABI revision at restore. Under another engine version
either is a version refusal. No byte of either format changed with this rule.
`tests/blob_refusal_precedence.rs` and `src/tests/state_codec_tests.rs` in `oce-api` pin it with
every pair of defects for each artefact; the earlier check wins in every pair.

## Cost

A blob of the 46 G36 catalog fixtures is 306 to 17,001 bytes (123,329 in all). `decode` allocates
what it returns — the model arena, the blocks and whatever their constructors allocate, the schedule,
the run state and the I/O table — plus one default-parameter block per class for its signature
hash and four scratch vectors, all freed before it returns; I/O paths and payloads borrow the input
bytes. `crates/oce-api/tests/blob_allocations.rs` pins that count exactly for every fixture, and
`blob_peak_allocation.rs` pins peak memory for hostile and dense shapes against the bound in the
[trust model](#trust-model): the linear term, the constructed budget and the state budget.
Executing frames allocates nothing: `crates/oce-graph/tests/blob_execution.rs` decodes every golden
once and runs its 160 frames inside a counting-allocator region that must record zero
allocations, against the `no_std + alloc` build of `oce-graph` in the `no-std-build` job (the 46
decodes total 6,013 allocations and 992,984 bytes; the 7,360 frames, none). The
[architecture's](architecture.md#determinism-and-the-two-allocation-carve-outs) two carve-outs
still apply.

## The blob round trip

The round trip is the standing test that a blob encoded on one machine runs the same everywhere it
is decoded (issue #403). `crates/oce-conformance/tests/blob_round_trip.rs`, for each of the 46
fixtures: load the CXF through the facade, `compile_blob`, decode with `Executable::from_blob`, run
the shared 160-frame schedule (`crates/oce-graph/tests/support/frame_schedule.rs`, the schedule the
run digests and the firmware image already use, defined for every fixture) through both, and
compare bit for bit after every frame:

- every boundary output, by tag and raw bits (Reals as IEEE-754 hex, never formatted);
- every warning the frame emitted, in order: source, message and time bits;
- the raw state words, `Engine::state_words` against `Executable::state_words`, after load and
  after every frame. The portable snapshot is not part of the blob path and is not compared here.

The suite also requires the encoded blob to equal the checked-in golden and the entry's run to
reproduce the pinned run digest. A difference is reported as the fixture, the frame, the output
slot (and its path) or the state word index, and both bit patterns.

**Three legs, one recording.** With `OCE_ROUND_TRIP_OUT=<dir>` the suite also records each
fixture's blob bytes and expected results; with `OCE_ROUND_TRIP_EXPECT=<dir>` it reads a recording,
requires its blob bytes to equal this target's encoding, decodes and runs the *recorded* bytes, and
compares this target's run with the recorded one (still comparing the facade with the entry
locally). A relative directory resolves against the workspace root. Per PR, every determinism cell
records, the x86_64 debug cell's recording is the artifact, the aarch64 fan-in leg replays it
natively, and a separate job replays it on `wasm32-wasip1` under a checksum-pinned wasmtime
([CI and the gate](ci-and-the-gate.md#the-blob-round-trip)). `bash .agents/gate.sh roundtrip`
records on the local host and replays here and on `wasm32-wasip1`.

**The recorded artifact, recording format 1.** One directory holding exactly two files per fixture;
the reader refuses any other name. `<fixture>.oceb` is the blob the recording target encoded.
`<fixture>.trace.tsv` is UTF-8 text, one record per line, fields separated by one tab, every line
ending in a newline:

```text
oce-blob-round-trip<TAB>1
fixture<TAB><name>
blob<TAB><byte length><TAB><FNV-1a-64 of the blob file, 16 hex digits>
outputs<TAB><n>
output<TAB><slot><TAB><path>                      n lines, slots 0 to n-1
words<TAB><w>                                     state words per snapshot
load<TAB><word>...                                w words after load
frame<TAB><k><TAB><time bits><TAB><sequence>      per frame, k from 0
out<TAB><value>...                                n boundary outputs, slot order
warn<TAB><source><TAB><message><TAB><time bits>   zero or more, emission order
state<TAB><word>...                               w words after the frame
end<TAB><frames>
```

Words and time bits are sixteen lowercase hex digits. A value is `r:<16 hex>` (a Real's bits),
`i:<decimal>`, `b:0` or `b:1`, `s:<text>` or `e:<class>:<ordinal>`. Text escapes `\`, tab, newline
and carriage return as `\\`, `\t`, `\n` and `\r`, and other control characters as `\xHH`. Counts
are canonical unsigned decimals. The 46 fixtures record 92 files, about 34 MB, most of it the
state words of the three fixtures with over 4,000 words each.

The reader treats a recording as hostile: it refuses a trace over 32 MiB or a blob over 1 MiB before
reading it, bounds every declared count (outputs, the engine's 4,194,304-word state budget,
frames), requires the exact grammar, and returns a typed error naming the file and line, never a
panic. Its controls feed the comparison a recording with one bit flipped in an output, a state word
(after load or after a frame), a warning's time or the blob, with a changed or removed warning, and
with `-0.0` or another NaN payload against the produced Real, and require the report to name the
fixture, frame, slot or index and both bit patterns; they also refuse 28 malformed shapes (a `\xHH`
escape outside the control characters the writer escapes among them), every truncation, 2,000 seeded byte mutations
without a panic, and a missing, foreign, swapped or oversized file.

## Evidence

| Claim | Evidence |
| --- | --- |
| One canonical byte sequence per executable | 46 goldens in `crates/oce-api/tests/fixtures/blob/`, re-encoded byte for byte, and decoded then re-encoded byte for byte (`blob_goldens.rs`); an independent test-side writer reproduces every golden (`blob_hostile_bytes.rs`) |
| Identical on x86_64 and aarch64, debug and release | Every determinism cell compares its encoding with the goldens, and the `determinism-compare` fan-in requires the cells' bytes identical: all four cells (x86_64 and aarch64, debug and release codegen) per PR ([CI and the gate](ci-and-the-gate.md#dev-light-release-heavy)); the macOS (per PR) and Windows (merge gate) platform cells run the same golden test |
| Same outputs, refusals and state as the host, through one frame driver | `src/tests/blob_dual_path_tests.rs`: for every fixture, the facade, the core frame driver on a second load and the blob entry take 160 frames spanning about 3.3 hours (steps from 0.75 s to 300 s) plus the same bad frames (NaN time, time going backwards, a wrong type, a non-finite Real under `Refuse`, a value above a declared maximum), and every boundary output, refusal, clock, sequence and the whole `RunState` match bit for bit after load and after every frame and refusal; every stateful fixture's state words change, each fixture moves at least a recorded number of outputs, and a perturbed input on the blob side fails the comparison. The facade's run must reproduce `crates/oce-graph/tests/fixtures/blob_frame_digests.tsv`, which `oce-graph`'s `blob_execution` test checks against the entry built `no_std + alloc` |
| No second frame driver in the facade | `crates/oce-api/tests/frame_driver_ownership.rs`: the facade re-exports the core's `MAX_FRAME_TIME_MAGNITUDE` and `NonFiniteInputPolicy`, and no facade source defines them, evaluates the schedule or re-implements a time or sequence rule |
| Executing frames allocates nothing; the target closure is clean | `crates/oce-graph/tests/blob_execution.rs` (zero allocations over every golden's frames, run against the `no_std + alloc` build in `no-std-build`) and `scripts/ci/check-no-std-entry-closure.sh` (no `oce-cxf`, `oce-api`, `serde_json` or `std` in the closure, with controls) |
| The same run on bare metal | `firmware/link/tests/run_tests.rs` (every golden reaches its digest through the image's run code) and `schedule_tests.rs` (the image's schedule equals the test schedule bit for bit), per PR; `qemu.yml` (nightly): the image on `netduinoplus2` and riscv32 `virt` prints the host's run digest after each of 160 frames |
| Encoded on one machine, the same run on x86_64, aarch64 and wasm32 | `crates/oce-conformance/tests/blob_round_trip.rs` ([above](#the-blob-round-trip)): every output, warning and raw state word after every frame of all 46 fixtures, recorded in all four per-PR determinism cells (required identical, release codegen included) and replayed from the x86_64 debug recording on aarch64 and on `wasm32-wasip1` under wasmtime, per PR; its controls flip one recorded bit and require the report to name it |
| Raw state words restore all or nothing | `crates/oce-graph/tests/blob_state_words.rs` |
| Decoder memory is bounded by the blob length and two fixed budgets | `blob_peak_allocation.rs`: the shapes of issue #700 (one, 16 and 64 `TimeTable` blocks sized by one parameter name, a dense 1024 × 1024 `MatrixGain`) decode or refuse with `ConstructedTooLarge` within the words they hold and 64× their length; blocks holding exactly the constructed budget decode and one more block of four words is refused; a blob spending the constructed and the state budget in full decodes within their sum; a block of 200,000 unread outputs stays within the linear term. `blob_hostile_parameter_models.rs`: 100 seeded models of up to thirteen blocks with hostile parameter tables decode or refuse exactly as predicted from the words each block reports, the summary carries the same verdict, and no peak passes the bound. `blob_constructed_budget.rs`: the refusal's place among every other refusal, and `inspect`, `inspect_blob` and `Executable::from_blob` reporting it alike. `oce-graph`'s `constructed_budget` unit tests: exactly the budget accepted, one word past refused, overflow saturated. `oce-blocks` `tests/constructed_words.rs`: every registered class, under the hostile tables, holds no more and no less than it reports, and no construction peaks above `MAX_CONSTRUCTION_PEAK_BYTES` |
| Typed refusal, never a panic | `blob_hostile_bytes.rs` and `blob_hostile_model.rs` (one test per refusal), `blob_fuzz.rs` (10,000 seeded random mutations, re-checksummed) and `oce-blocks` `tests/hostile_parameters.rs` |
| A blob of an earlier layout under the same format number is refused | `blob_old_layout.rs`: the release-0.4.0 encoding of one fixture, kept unblessed, is refused with `EngineVersionMismatch` by `decode` and by `Executable::from_blob` |
| A blob of the same layout and an earlier meaning is refused | `engine_0_5_0_artefacts.rs`: the release-0.5.0 encoding of `trim_and_respond_have_hol_false`, kept unblessed, differs from this build's in one parameter record (an Integer where a Real parameter now holds a Real) and the content id, and is refused with `EngineVersionMismatch` by `decode`, `Executable::from_blob` and `inspect_blob`; restamped with this version it would decode |
| Which refusal wins is one rule for the blob and the snapshot | `blob_refusal_precedence.rs` and `oce-api`'s `src/tests/state_codec_tests.rs`: every pair of defects in each artefact reports the earlier check, every truncation of another version's blob below a header is `Truncated` and above it `EngineVersionMismatch`, and every flipped version bit is a version refusal; `blob_old_layout.rs`: the release-0.4.0 blob stays a version refusal with its length, trailer, profile or body damaged |
| `SemanticPoint` payloads are non-computational | `blob_semantic_seam.rs` fills and mutates every payload byte and requires an identical executable and identical outputs |
| A summary states what the blob holds, identically on every cell | `blob_goldens.rs`: the summary of each of the 46 blobs equals its checked-in JSON under `crates/oce-api/tests/fixtures/blob_summary/` byte for byte, in every determinism cell and both platform cells, and an independent JSON reader finds the same facts in it; `blob_summary.rs`: each summary equals what the test-side writer derives from the decoded executable (class per block, this build's hashes, the CRC of the words the decoder allocated) and the content id the engine minted and the content-id pin records; `oce-graph`'s `tests/blob_summary.rs` holds the core reader to the decoder over the same goldens |
| A changed class hash or initial-state CRC shows in the summary, and nowhere else | `blob_summary.rs`: every bit of every class hash and of every instance's CRC of one golden, then state length, kind, content id and I/O paths. The decoder refuses a changed hash, CRC, state length or kind with its named error and accepts a changed content id or path, which it carries without checking; in every case the summary differs from the original in exactly that field |
| A summary has no refusal exactly when this build's decoder accepts the blob, and otherwise carries the decoder's own error | `support/blob_summary_facts.rs`: every summary any test reads is checked there, core and facade, against `decode` on the same bytes. `blob_goldens.rs` and `blob_summary.rs`: all 46 goldens, fresh and checked in, have no refusal. `blob_summary_acceptance.rs`: nine blobs the decoder refuses after its structural pass (schedule, undriven input, duplicate output path, non-finite and duplicated parameters, connector out of range, unused class, another initial state) and fifteen more (every other instance field, both enum faults, a claimed connector, boundary input, connection and output faults, the other schedule faults, one word past the state budget) summarise as written with exactly the error `decode` returns; a blob with four defects reports the decoder's first, then the next as each is repaired, then none; an unregistered class and a class index outside the table are refused by index and never reported as a decode refusal. `blob_summary.rs`: every flipped bit of every class hash and initial-state CRC carries its named refusal. `blob_summary_hostile.rs`: the 10,000 seeded mutations hold the field to the decoder's verdict in every case. `oce-graph`'s `tests/blob_summary.rs`: every one-bit change of two goldens, resealed, through the core reader alone |
| The summary reader refuses hostile bytes as the decoder does, never a panic | `blob_summary_hostile.rs`: every prefix and resealed truncation, every header defect alone and in every pair that can occur (the earlier check wins, as in the decoder), `u32::MAX` in every count and length at its own offset, misframed sections, trailing bytes, invalid UTF-8, tags outside their tables and every list out of canonical order give the decoder's identical error, and 10,000 seeded byte mutations never part from the decoder's verdict; `blob_summary_allocations.rs` pins zero allocations for a refusal of every kind (framing, structure, a 200,000-entry unregistered class table, a class index outside the table on the last of 100,000 blocks); for every golden, three blobs the decoder refuses, the 200,000-input accepted shape, a blob at the state budget, blobs at and past the constructed budget and the longest lists it requires the summary's allocation count to equal `decode`'s for the same bytes plus one per non-empty list, and its peak to equal the larger of `decode`'s peak and the lists' bytes |

On a bare-metal target the blob runs in the firmware image (`firmware/link`, GitHub #449, issue
#402): it embeds the `multizone_vav_supply_fan` golden, decodes it with `Executable` and runs the
same 160-frame schedule. Per PR the [bare-metal cell](ci-and-the-gate.md#the-bare-metal-cell)
builds the image for `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf` within its size
budget and runs the image's code on the host against every golden's digest; the [nightly emulation
run](ci-and-the-gate.md#the-nightly-emulation-run) executes the image under QEMU and requires its
run digest after every frame to equal the host's. CI never runs it on silicon, and emulation is not
part of `CI OK`.
