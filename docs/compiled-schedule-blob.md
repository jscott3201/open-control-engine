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

// Target (no_std + alloc, same engine version): decode into a ready-to-tick executable.
let decoded = oce_graph::blob::decode(&blob.bytes)?;
// decoded.model, decoded.schedule, decoded.blocks and decoded.state feed oce_graph::eval_tick;
// decoded.inputs and decoded.outputs are the boundary slots, sorted by path.
```

`Engine::compile_blob` refuses with `OcError::State` when no model is loaded or parameter edits are
pending, and with `OcError::Blob` when the executable cannot be encoded. It decodes its own output
before returning, so every success is a blob the same build accepts. Executed frames do not change
the bytes; a resumed parameter edit does. The HostTick v1 frame driver that a target wraps around
`eval_tick` (time checks, input checks, staging, outputs) is GitHub #401.

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
| Content id | The facade's `cxf:fnv1a128` content id at encode time, or "none" for a partial export |
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
  non-computational (CDL §7.17). The `SemanticPoint` payload slots exist for them and the decoder
  never reads them.
- **The CXF document and its parameter expressions.** Bindings are already evaluated.
- **Host policy.** The non-finite input policy, diagnostics handling, cadence and every
  [host responsibility](host-responsibilities.md) stay with the host.
- **Placement.** A blob has no architecture or operating-system field: its bytes do not depend on
  the encoder's platform. A target-bound class (one of the 15 listed in
  [state continuation](state-compatibility.md)) still makes the *outputs* depend on the target's
  math library; the decoded executable reports this as `target_bound`. None of the 46 G36 catalog
  fixtures uses one.
- **A signature.** See the trust model.

## Versioning

A decoder accepts exactly one format version (1), one execution profile (HostTick v1) and its own
engine version; anything else, older or newer, is a typed refusal. Between release tags every
`development` build shares one version string, so a blob and the target that runs it must come
from the same build — the same current/current rule as state snapshots. The reserved header flags
must be zero. Any change to the layout, a tag table, the class signature record, the CRC or the
content-id meaning bumps the format version; v1 has no optional or skippable sections.

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
the validator's parameter rules are not linked on a target. Authenticate and authorize blobs in your
host's envelope before decoding them, as you would snapshot bytes.

For **memory safety** a blob is hostile: `decode` never panics, refuses malformed bytes with a
typed `BlobError` that names the offset, index or hashes concerned, and checks every count against
the bytes that remain before allocating for it. Every registered block constructor is fuzzed with
hostile parameter tables bounded by `MAX_RESOLVED_PORT_WIDTH`.

**Integrity** is a CRC-32C trailer over every preceding byte. It detects corruption in flash,
transfer or storage; it is not tamper evidence, because anyone can recompute it.

## Refusals

The decoder checks, in order, and returns the first failure: the 64 MiB size limit; header shape,
magic and total length; format version; the CRC; profile, engine version and flags; structure
(section order and lengths, bounded counts, UTF-8, tags, canonical list order, no trailing bytes);
classes (registered, same hash, each used) and instances (class index, unique parameter names,
finite Reals, known enum classes and in-range ordinals, the constructed block's kind, state length
and arity, every connector exactly once); boundary inputs (not Enum-typed, typed targets used once,
typed bounds), connections (output to input, equal kinds, distinct blocks, never into a boundary
input, one driver per input), undriven inputs and boundary outputs (output sources, unique paths);
the schedule (a permutation that respects emit-before under the target's own feedthrough); the state
budget (`MAX_STATE_WORDS`) and each initial-state CRC.

## Cost

A blob of the 46 G36 catalog fixtures is 306 to 17,001 bytes (123,329 in all). `decode` allocates
what it returns — the model arena, the blocks and whatever their constructors allocate, the schedule,
the run state and the I/O table — plus one default-parameter block per class for its signature
hash and four scratch vectors, all freed before it returns; I/O paths and payloads borrow the input
bytes. `crates/oce-api/tests/blob_allocations.rs` pins that count exactly for every fixture. Ticking
the decoded executable allocates nothing in the scheduler; the
[architecture's](architecture.md#determinism-and-the-two-allocation-carve-outs) two carve-outs
still apply.

## Evidence

| Claim | Evidence |
| --- | --- |
| One canonical byte sequence per executable | 46 goldens in `crates/oce-api/tests/fixtures/blob/`, re-encoded byte for byte, and decoded then re-encoded byte for byte (`blob_goldens.rs`); an independent test-side writer reproduces every golden (`blob_hostile_bytes.rs`) |
| Identical on x86_64 and aarch64, debug and release | Every cell of the per-PR determinism matrix compares its encoding with the goldens, and the `determinism-compare` fan-in requires the four cells' bytes identical ([CI and the gate](ci-and-the-gate.md#dev-light-release-heavy)); the macOS and Windows platform cells run the same golden test |
| Same outputs and state as the host | `src/tests/blob_dual_path_tests.rs`: for every fixture, the facade and the decoded executable take 160 frames, and every boundary output and the whole `RunState` match bit for bit after load and after every tick |
| Typed refusal, never a panic | `blob_hostile_bytes.rs` and `blob_hostile_model.rs` (one test per refusal), `blob_fuzz.rs` (10,000 seeded random mutations, re-checksummed) and `oce-blocks` `tests/hostile_parameters.rs` |
| `SemanticPoint` payloads are non-computational | `blob_semantic_seam.rs` fills and mutates every payload byte and requires an identical executable and identical outputs |

The blob is not yet executed on a bare-metal target by CI: the `no_std` build of the decoder is
gated (`no-std-build`), and running it on a target is GitHub #401 and #402.
