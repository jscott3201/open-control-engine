# State continuation and portability contract

## Authority and scope

This is the execution-maintainer contract for **same-loaded-executable continuation** (PC-038 in
the [product contract](product-contract.md)). It is a pre-release supported boundary, not a
cross-release migration promise. OCE is synchronous, in-process and database-free. It captures
bytes, not durable storage, a replay log or permission to command equipment.

The separate [canonical replay record](replay-record.md) describes one accepted frame. Optional
start/end snapshot bytes remain sidecars in the host's authenticated ordered envelope; they are not
embedded in a record. Replay reuses this placement policy without exposing or relabeling the private
execution fingerprint as build authority. Neither snapshot format nor execution ABI changes for replay.

State is current/current only, with no N-1 support or cross-build state migration (see the
[host fallback checklist](host-responsibilities.md)). The historical v0.1.0 source pin has no
snapshot API.

**A snapshot restores only on the engine version that wrote it** (issue #530). Its header names
that version, and `EngineStateSnapshot::from_bytes` refuses any other with
`EngineVersionMismatch` before it reads anything further, so `restore_state` never sees such
bytes. This is the one comparison OCE makes about where bytes came from, and it is a release
version, not a build identity: two builds of one version, from different compilers, features,
targets or dependency locks, write and accept the same version bytes.

“Same-build” therefore remains a **mandatory host-envelope precondition**, not an OCE comparison.
Before calling `EngineStateSnapshot::from_bytes`, the host authenticates a sealed envelope binding
the exact snapshot bytes to its model, its approved compiled-build/deployment qualifier, freshness
and generation. That qualifier can cover source/dependency lock, compiler, features, target and
codegen qualification. OCE emits, accepts, stores and enforces **no build token**; the engine
version in the header is not one. Pass only the OCE snapshot bytes after approval. A compatible
manifest from a differently compiled implementation of the same version can pass OCE checks; that
is not build qualification. See the [restart checklist](host-responsibilities.md#persist-engine-state-outside-the-engine).

`CompatibilityDescriptor`, package version, catalog metadata tag, diagnostic model ID and canonical
content id are not substitutes for either the host envelope or the state manifest. No universal
identity is introduced. Differences in unused catalog entries do not affect this executable;
changes to implementation code with unchanged descriptors belong to host build qualification.

## Capture, parse and restore

| Operation | Contract |
| --- | --- |
| `checkpoint` | Owned process-local image; compatible restore can rewind. No persistence format or external authority. |
| `state_snapshot` | Validate loaded state, stable authored identities and registered state contracts; encode and run the bounded canonical decoder before returning. Every success can be parsed by this build. No evaluation or storage calls. |
| `from_bytes` | Refuse bytes another engine version wrote, then validate wire structure and manifest self-consistency, not compatibility with a loaded target or every class-specific state invariant. Unknown execution ABI can parse but cannot restore. |
| `restore_state` | Check readiness, target domain, execution ABI, full manifest/fingerprint and payload before one in-memory commit. No evaluation or storage calls. |
| `restore_checkpoint` | Same prepare/commit discipline, without the startup-window restriction. Not a durable-byte bypass. |

A successful load opens the durable restore window. Accepted frame execution, dirty-parameter
resume, or either successful restore closes it. Snapshot/checkpoint capture, read-only inspection,
frame preparation, refused operations, halt and clean resume do not close it. A successful reload
opens a new window; an ordinary failed reload preserves the prior one. Pending parameter edits
refuse capture and restore before any window check. Restore preserves the current parameter-edit
mode, model identity, IO metadata, admission policy, private frame incarnation and Engine-lifetime
accepted-frame sequence. It replaces connector values, state words, absolute model time and the
prior-time guard only. A preprepared frame still rechecks time at execution. Retained completed
frames remain immutable and are not restored or reissued.

All **returned** restore errors precede mutation and preserve engine state; the engine has no storage
state to preserve.
Panic, allocation failure, process termination and external host effects are outside this guarantee.
Capture/restore do not create a storage transaction. A later delivery failure cannot undo a frame.
At restored time, another accepted frame is another HostTick transition, not event iteration.

## Compatibility and deterministic refusal

OCE compares complete structures, not just a collision-prone fingerprint. Incompatible structural
catalog/profile/executable/IO/state/target facts refuse at parse or restore as follows:

| Check / first-cause order | Typed evidence |
| --- | --- |
| Supplied bytes above 64 MiB, before header inspection | `SnapshotTooLarge` with actual and maximum counts |
| Input shorter than a header and trailer; invalid magic; unsupported format | `MalformedSnapshot` / `UnsupportedFormat` |
| Header naming another engine version, before the length, trailer or body is read | `EngineVersionMismatch` with the found and expected versions |
| Inconsistent total length | `MalformedSnapshot` at the body-length field (offset 22) |
| Corrupted header, body or trailer after a valid version and length | `IntegrityMismatch` with computed and carried checksums |
| Invalid tags/UTF-8/counts, duplicate or unordered sections, inconsistent references, fingerprint, flags or payload shape | `MalformedSnapshot` with deterministic offset/detail |
| Restore without a successful load; pending edits; advanced durable target | `NoLoadedModel`, `PendingParameterEdits`, `DurableTargetAdvanced`, in that order |
| Foreign target-bound architecture or OS | `TargetDomainMismatch` before execution compatibility |
| Different execution-state ABI, including a different transition profile | `IncompatibleExecution`, subject `execution-state ABI revision` |
| Different manifest | `IncompatibleExecution`, first named subject in the order below |
| Wrong fingerprint or connector mapping after matching manifest | `IncompatibleExecution` |
| Invalid initialized/pre-first-tick words, clock relationship or class-specific state | `InvalidBlockState` |
| Missing stable authored identities or registered state contract during capture/target construction | `IneligibleModel` |

Manifest comparison order is portability; referenced enum paths and ordered members; blocks by
canonical key (key, canonical class, algebraic/stateful kind, state revision/length, raw parameter
names and bit-exact typed values, ordered input/output bindings); connectors (key, path, declaration
order, native type, computation unit and quantity); connections; block schedule; connector schedule;
driver map; state-slot offsets/lengths; external input keys; boundary output identities/bindings;
then canonical complete-frame input definitions (path, native type and effective min/max).

The input definitions include the intersection of declaration and all fan-out target bounds,
with exact Integer/enum values and Real bits. They are the actual acceptance domain, not lossy
projected-point bounds. Unit/quantity changes affect host interpretation and refuse. Display units and
other non-executing presentation metadata are not compatibility determinants. Model ID is diagnostic
only; changing it alone does not refuse. Parameter edits need a compatible newly loaded target.

Typed variants and structured fields are the host action seam; human-readable subject/detail text
is deterministic diagnostic evidence, not a second versioned machine protocol. Hostile text in
restore incompatibility/state diagnostics is bounded. Do not match error prose to authorize a build.

## Wire revision and integrity

The wire is **format revision 1 / execution-state ABI revision 1**, on fixed **HostTick v1**. Both
numbers stay 1 until 1.0.0 (issue #509): a change to the layout is made in place and the vectors
are re-blessed, with no fallback, silent upgrade, migration or decoder that reads two layouts. Any
other format revision returns `UnsupportedFormat`, and any other execution-state ABI revision
refuses at restore. Rollback uses the prior qualified build and its own authenticated state, or a
host-approved cold start—not a header edit.

**The revision numbers do not tell one layout from another; the engine version does.** The header
carries the major, minor and patch of the engine that wrote the snapshot, each a little-endian
`u16`, from the same constant a [compiled-schedule blob](compiled-schedule-blob.md) records
(`oce_graph::blob::ENGINE_VERSION`). A pre-release or build suffix is not recorded, for blobs and
snapshots alike: `0.5.0-rc.1` and `0.5.0` write and accept the same bytes. The decoder accepts
exactly its own version and returns `EngineVersionMismatch { found, expected }` for any other.
It makes that check straight after the two revision fields and before the body length, the
trailer or any body byte, because another version may lay all of those out differently; a
corrupted version field therefore reports the version it now spells, not an integrity failure.
The compiled-schedule blob decoder checks its version at the same point, straight after its
format (issue #535): [which refusal wins](compiled-schedule-blob.md#which-refusal-wins) is one
rule for both artefacts. Nothing relaxes the rule before 1.0.0: there is no range of accepted versions, no optional form of
the field and no reader for a header without it. A version is not a build token, and the host
envelope above still binds bytes to a model, a build and a deployment.

Both integrity values cover the field. The trailer covers every header byte, and the execution
fingerprint is taken over the version bytes as well as the ABI revision and the manifest. A
snapshot another version wrote, with only its version bytes rewritten to this build's, fails its
trailer (`IntegrityMismatch`); with the trailer recomputed too it fails its fingerprint
(`MalformedSnapshot` at offset 30). Both values are FNV, not authentication: rewriting all three
yields well-formed bytes, which is what the host envelope is for.

Snapshots of earlier layouts, kept unblessed (`tests/state_old_layout.rs`). None of the three has
a version field: its body length sits at bytes 16..24, so this build reads the low six bytes of
that length as a version and refuses it.

- `state_unstamped.hex`: revision 1 without the version field, as `development` wrote it between
  the format reset and issue #530; no release did. Refused with `EngineVersionMismatch` (found
  1462.0.0, its body length). Given the six version bytes by hand it fails its trailer, and with
  a recomputed trailer its fingerprint.
- `state_revision_two.hex`: the same state as releases 0.2.0 to 0.4.0 wrote it, numbered format 2
  and ABI 2. Refused with `UnsupportedFormat`; with its format field, or both numbers, edited to 1
  and the trailer recomputed, it is an unstamped header and refuses the same way.
- `state_legacy.hex`: an empty-model vector of the layout before those releases, carrying
  today's two numbers. Refused with `EngineVersionMismatch` (found 114.0.0).

One unstamped header passes the version check: a body whose length, in its low six bytes, spells
this build's version (327,680 bytes for 0.5.0, and none under the byte cap once the patch is above
zero). The body-length field then reads bytes that cannot be the true length, and the decoder
refuses it there (`MalformedSnapshot` at offset 22).

The byte cap is 67,108,864 inclusive. It bounds the whole snapshot, so it also bounds the state
words a capturable model may hold. Most classes hold a few words, but `CDL.Reals.MovingAverage`
sizes its checkpoint ring from `delta`: 4110 words (about 33 KB) for `delta ≤ 1024 s`, about
`4·delta` words up to 65536 s, and at most 262158 words (about 2.1 MB). A model with a few dozen
maximum-size windows loads and runs, but `state_snapshot` refuses it with `SnapshotTooLarge`.
Nothing checks this total at load. Integers and Real/state words are little-endian; Real payloads
preserve all bits, including signed zero and NaN payloads. Strings are length-prefixed UTF-8.
Compound keys sort by complete encoded key bytes (including little-endian lengths/indices), not
Rust numeric or natural string ordering. Named parameter and input-definition paths use UTF-8
lexical order. Schedules and state-slot layout retain execution order/offsets; they are not freely
permutable. Connection vector order is canonicalized; duplicate connections are noncanonical.

The fixed header is 30 bytes:

| Offset | Bytes | Field |
| --- | --- | --- |
| 0 | 8 | Magic `OCESTAT\0` |
| 8 | 4 | `u32` format revision, 1 |
| 12 | 4 | `u32` execution-state ABI revision, 1 |
| 16 | 6 | Engine version: major, minor, patch, each a `u16` |
| 22 | 8 | `u64` body length |

The body contains
u128 execution fingerprint, model ID, model time bits, optional prior-time bits, length-prefixed
manifest, keyed connector values, state words and zero reserved flags. A 16-byte trailer follows.
Connector records end with optional unit and quantity (0 = absent; 1 plus string = present).
After boundary outputs, the manifest appends a u32 input-definition count; each entry is path,
native type, optional typed min and optional typed max (0 = absent; 1 plus value = present).

Both fingerprint and trailer use FNV-1a-128: offset `0x6c62272e07bb014262b821756295c58d`, XOR each
byte and multiply by `0x0000000001000000000000000000013b` modulo 2^128. The fingerprint covers
the little-endian execution ABI, the six engine-version bytes as the header writes them, then the
canonical manifest bytes. The trailer covers all preceding
snapshot bytes. These are **accidental-corruption checks, not cryptographic authenticity, freshness
or adversarial collision protection**. Recomputing both can produce well-formed forged bytes.
Host authentication binds exact bytes, not these FNV values.

The decoder bounds counts, lengths and charged allocation workspace; the byte cap is not a
process peak-memory or CPU deadline guarantee. Capture also bears encoding/decoding allocation
costs; it is not an allocation-free hot-path operation. OCE owns no filesystem. Truncation tests
model partial durable bytes, not device power-loss resilience or atomic file publication.

## Portability domains and evidence limits

`EngineStateSnapshot::portability()` returns a borrowed, cloneable `StatePortability`:

| Policy | Placement check | Evidence boundary |
| --- | --- | --- |
| `Portable` | No architecture/OS restriction in the current class policy; all other checks still apply. | Native Linux x86_64/aarch64 CI compares a populated portable state vector: across architectures and across debug/release codegen, per PR. This is bounded execution evidence, not every input/program/target. |
| `TargetBound { arch, os }` | Exact capture architecture and OS labels required. | Per PR, CI requires cross-architecture bytes to differ, exercises foreign-target public restore refusal, and compares same-target debug/release bytes. Same target does not mean same build. |

Inspection is not admission: an unknown execution ABI may expose a parsed placement tag but still
refuses restore. Hosts keep encoded bytes intact and never rewrite the tag to move state.

The conservative target-bound set remains exactly 15 classes: `CDL.Reals.Acos`, `Asin`, `Atan`,
`Atan2`, `Cos`, `Exp`, `Log`, `Log10`, `Sin`, `Tan` (each under `CDL.Reals`);
`CDL.Reals.Sources.Sin`; `CDL.Psychrometrics.DewPoint_TDryBulPhi`,
`SpecificEnthalpy_TDryBulPhi`, `WetBulb_TDryBulPhi` (each under `CDL.Psychrometrics`);
and `CDL.Utilities.SunRiseSet`. Any participating target-bound class binds the executable. The
list is the per-class `target_bound` flag in `oce-blocks` (`oce_blocks::class_is_target_bound`,
`CatalogEntry::target_bound`), part of each class's signature in a
[compiled-schedule blob](compiled-schedule-blob.md); the facade reads that flag, so there is one
list.

The [per-PR native strict-bit matrix](strict-bit-evidence.md) covers only 21 Linux signal cases on
pinned rustc/libm in four native architecture/codegen cells. It is not an arbitrary-input
guarantee, mathematical oracle or macOS qualification. It therefore does **not** relax this state-placement policy. Local macOS
passing tests do not supply the hosted Linux evidence or broaden either variant's claim.

No hash-collection iteration order reaches the snapshot bytes, or any other output a host or a
golden observes. `std`'s `HashMap` and `HashSet` iterate in an order seeded per thread from the OS,
so an order that leaked would differ between two runs on the same build and target. The state image
is built from the schedule, block and connector arrays of the `no_std` core crates, which contain no
hash collections at all (clippy's `disallowed-types` and `tests/core_crates_hash_free.rs` refuse
one). The `std` crates keep hash
maps for lookup, and every place one is iterated either sorts first or cannot affect what is emitted;
`clippy::iter_over_hash_type` is denied workspace-wide, so a new `for` loop over one needs a stated
reason. `tests/hash_order_independence.rs` loads all 46 G36 fixtures and the resolver-contract
document on 20 fresh threads, each with independently seeded hashers, and requires identical
export bytes, content id, load and export diagnostics, input definitions, schedule, frame outputs
and snapshot bytes after four frames. A harness control proves a seeded `HashMap` order differs
between those threads, so the comparison cannot pass by reusing one seed.

## Continuation of published Library rules

`tests/library_rule_state_continuation.rs` runs two fault rules from open-control-library through
the durable path exactly as a host would: `state_snapshot`, a host-envelope approval stand-in,
`EngineStateSnapshot::from_bytes`, a freshly loaded engine and `restore_state` inside the startup
window. The CXF graphs are unedited copies and the vectors are the Library's scenarios in the
`oce/vectors/v1` format (`tests/fixtures/library_rules/`):

- **AHU-0016** (simultaneous heating and cooling): a `Logical.TrueDelay` persistence timer of 900 s.
- **AHU-0004** (excessive operating-state changes): `Integers.Change`, a 3600 s
  `Reals.MovingAverage` window and a 3600 s `Logical.TrueDelay`.

For all 16 scenarios, a restart after load and after every tick, and a chained restart after every
tick, reproduce the uninterrupted run bit for bit: every boundary and internal output, and the full
canonical state bytes. Named restarts land mid-dwell (AHU-0016 at 480 s, half way through the
delay) and inside partly filled or draining windows (AHU-0004 at 900, 1800, 7800 and 11400 s); a
cold start at the same point diverges in every case, so the restored state is load-bearing. The
uninterrupted run also meets every Library expectation window. No block in either rule loses
state across a restart.

Restore continues **model time**; it does not decide what a gap in that time means. If the first
frame after a restart arrives later than one tick after the snapshot, the restored blocks treat the
gap as elapsed time: a satisfied `TrueDelay` counts the outage toward its delay, and
`MovingAverage` integrates the first post-restart input over the whole gap (with AHU-0004, one
state change seen across a one-hour gap reads as twelve). The same frames would do the same in an
engine that never restarted. Choosing the resume time, and whether an outage is a gap, a cold start
or a host `NO_EVAL` window, is host frame policy.

## Evidence map

- `tests/state_contract.rs`: public inspection, changed bounds/units/quantities and
  signed-zero/equal-time continuation against a hand-derived Boolean recurrence.
- `tests/state_engine_version.rs`: the header's version field against the package version and
  the blob header; byte-identical capture and restore on the same version; refusal of a
  consistent snapshot that differs by patch, minor or major only; a patched version field failing
  the trailer and then the fingerprint; every bit of the field, every truncation and the
  precedence against magic, format, length, trailer and body; no input-sized allocation on
  refusal; the host's decode-then-restore path stopping at decode, with a restoring control.
- `tests/state_old_layout.rs`: three snapshots earlier layouts wrote, none with a version field,
  refused at that field or by format revision; the repeatable capture recipe of the two
  populated ones, and the connector values and state words it leaves, read out of both.
- `src/tests/state_manifest_refusal_tests.rs`: rechecksummed field mutation corpus, repeat typed
  diagnostics, full snapshot/checkpoint before/after images and retained lifecycle fences; capture refuses a deliberately injected noncanonical producer image.
- `src/tests/state_format_golden_tests.rs`, `state_tests.rs`, `state_io.rs`: independent full
  codec construction, checked byte/fingerprint goldens and hand-assembled domain encoding.
- `src/tests/state_codec_tests.rs`, `state_resource_tests.rs`, `state_restore_validation_tests.rs`:
  every truncation boundary, checksum/length/tag hostility, exact cap and one-past, block-state
  refusals and decode/capture resource checks.
- `src/tests/state_portability_tests.rs`: all 15 actual target-bound class captures, independent
  arch/OS refusals, and artifacts consumed by the unchanged hosted state matrix.
- `tests/hash_order_independence.rs`, `tests/core_crates_hash_free.rs`: snapshot bytes and every
  load, export and frame output identical across independently seeded hashers, with a harness
  control; no hash collection in the four `no_std` crates.
- `tests/library_rule_state_continuation.rs`: durable restart of two unedited Library fault rules
  (persistence timer, moving window) at every tick, chained and mid-dwell, against an
  uninterrupted run, with cold-start divergence controls and typed refusals on advanced or foreign
  targets.
- Existing continuation, family, lifecycle and Pre suites retain complete-frame continuation and
  current warning/state semantics. These tests qualify OCE boundaries, never host compliance.
