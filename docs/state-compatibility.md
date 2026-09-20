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

“Same-build” is a **mandatory host-envelope precondition**, not an OCE comparison. Before calling
`EngineStateSnapshot::from_bytes`, the host authenticates a sealed envelope binding the exact
snapshot bytes to its approved compiled-build/deployment qualifier and checks freshness and
generation. That qualifier can cover source/dependency lock, compiler, features, target and
codegen qualification. OCE emits, accepts, stores and enforces **no build token**. Pass only the
OCE snapshot bytes after approval. A compatible manifest from a differently compiled implementation
can pass OCE checks; that is not build qualification. See the [restart checklist](host-responsibilities.md#persist-engine-state-outside-the-store-port).

`CompatibilityDescriptor`, package version, catalog metadata tag, diagnostic model ID and complete
export tag are not substitutes for either the host envelope or the state manifest. No universal
identity is introduced. Differences in unused catalog entries do not affect this executable;
changes to implementation code with unchanged descriptors belong to host build qualification.

## Capture, parse and restore

| Operation | Contract |
| --- | --- |
| `checkpoint` | Owned process-local image; compatible restore can rewind. No persistence format or external authority. |
| `state_snapshot` | Validate loaded state, stable authored identities and registered state contracts; encode and run the bounded canonical decoder before returning. Every success can be parsed by this build. No evaluation or Store calls. |
| `from_bytes` | Validate wire structure and manifest self-consistency, not compatibility with a loaded target or every class-specific state invariant. Unknown execution ABI can parse but cannot restore. |
| `restore_state` | Check readiness, target domain, execution ABI, full manifest/fingerprint and payload before one in-memory commit. No evaluation or Store calls. |
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

All **returned** restore errors precede mutation and preserve both engine state and Store state.
Panic, allocation failure, process termination and external host effects are outside this guarantee.
Capture/restore do not create a Store transaction. A later delivery failure cannot undo a frame.
At restored time, another accepted frame is another HostTick transition, not event iteration.

## Compatibility and deterministic refusal

OCE compares complete structures, not just a collision-prone fingerprint. Incompatible structural
catalog/profile/executable/IO/state/target facts refuse at parse or restore as follows:

| Check / first-cause order | Typed evidence |
| --- | --- |
| Supplied bytes above 64 MiB, before header inspection | `SnapshotTooLarge` with actual and maximum counts |
| Short/invalid header; unsupported format; inconsistent total length | `MalformedSnapshot` / `UnsupportedFormat` |
| Corrupted body/trailer after valid header and length | `IntegrityMismatch` with computed and carried checksums |
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
PointStore bounds. Unit/quantity changes affect host interpretation and refuse. Display units and
other non-executing presentation metadata are not compatibility determinants. Model ID is diagnostic
only; changing it alone does not refuse. Parameter edits need a compatible newly loaded target.

Typed variants and structured fields are the host action seam; human-readable subject/detail text
is deterministic diagnostic evidence, not a second versioned machine protocol. Hostile text in
restore incompatibility/state diagnostics is bounded. Do not match error prose to authorize a build.

## Wire revision and integrity

Current **format revision 2 / execution-state ABI revision 2** retains fixed **HostTick v1**.
Revision 2 adds connector computation unit/quantity and effective input definitions. Revision-1
bytes lacked those compatibility determinants and now return `UnsupportedFormat { revision: 1 }`.
There is no fallback, silent upgrade or migration. Rollback uses the prior qualified build and its
own authenticated state, or a host-approved cold start—not a header edit.

The byte cap is 67,108,864 inclusive. Integers and Real/state words are little-endian; Real payloads
preserve all bits, including signed zero and NaN payloads. Strings are length-prefixed UTF-8.
Compound keys sort by complete encoded key bytes (including little-endian lengths/indices), not
Rust numeric or natural string ordering. Named parameter and input-definition paths use UTF-8
lexical order. Schedules and state-slot layout retain execution order/offsets; they are not freely
permutable. Connection vector order is canonicalized; duplicate connections are noncanonical.

The fixed header is `OCESTAT\0`, u32 format, u32 execution ABI, u64 body length. The body contains
u128 execution fingerprint, model ID, model time bits, optional prior-time bits, length-prefixed
manifest, keyed connector values, state words and zero reserved flags. A 16-byte trailer follows.
Revision-2 connector records append optional unit and quantity (0 = absent; 1 plus string = present).
After boundary outputs, the manifest appends a u32 input-definition count; each entry is path,
native type, optional typed min and optional typed max (0 = absent; 1 plus value = present).

Both fingerprint and trailer use FNV-1a-128: offset `0x6c62272e07bb014262b821756295c58d`, XOR each
byte and multiply by `0x0000000001000000000000000000013b` modulo 2^128. The fingerprint covers
little-endian execution ABI followed by canonical manifest bytes. The trailer covers all preceding
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
| `Portable` | No architecture/OS restriction in the current class policy; all other checks still apply. | Native Linux x86_64/aarch64 debug/release CI compares a populated portable state vector. This is bounded execution evidence, not every input/program/target. |
| `TargetBound { arch, os }` | Exact capture architecture and OS labels required. | CI compares same-target debug/release bytes, requires cross-architecture bytes to differ, and exercises foreign-target public restore refusal. Same target does not mean same build. |

Inspection is not admission: an unknown execution ABI may expose a parsed placement tag but still
refuses restore. Hosts keep encoded bytes intact and never rewrite the tag to move state.

The conservative target-bound set remains exactly 15 classes: `CDL.Reals.Acos`, `Asin`, `Atan`,
`Atan2`, `Cos`, `Exp`, `Log`, `Log10`, `Sin`, `Tan` (each under `CDL.Reals`);
`CDL.Reals.Sources.Sin`; `CDL.Psychrometrics.DewPoint_TDryBulPhi`,
`SpecificEnthalpy_TDryBulPhi`, `WetBulb_TDryBulPhi` (each under `CDL.Psychrometrics`);
and `CDL.Utilities.SunRiseSet`. Any participating target-bound class binds the executable.

The [accepted strict-bit receipt](strict-bit-evidence.md) covers only 21 Linux signal cases on
pinned rustc/libm, four native architecture/codegen cells and a selected 35-file source boundary.
It is not a full compiled dependency closure, arbitrary-input guarantee, mathematical oracle or
macOS qualification. It therefore does **not** relax this state-placement policy. Local macOS
passing tests do not supply the hosted Linux evidence or broaden either variant's claim.

## Evidence map

- `tests/state_contract.rs`: public inspection, changed bounds/units/quantities, historical wire
  refusal and signed-zero/equal-time continuation against a hand-derived Boolean recurrence.
- `src/tests/state_manifest_refusal_tests.rs`: rechecksummed field mutation corpus, repeat typed
  diagnostics, full snapshot/checkpoint before/after images, retained lifecycle fences and Store
  noninterference; capture refuses a deliberately injected noncanonical producer image.
- `src/tests/state_format_golden_tests.rs`, `state_tests.rs`, `state_io.rs`: independent full
  codec construction, checked byte/fingerprint goldens and hand-assembled domain encoding.
- `src/tests/state_codec_tests.rs`, `state_resource_tests.rs`, `state_restore_validation_tests.rs`:
  every truncation boundary, checksum/length/tag hostility, exact cap and one-past, block-state
  refusals and decode/capture resource checks.
- `src/tests/state_portability_tests.rs`: all 15 actual target-bound class captures, independent
  arch/OS refusals, and artifacts consumed by the unchanged hosted state matrix.
- Existing continuation, family, lifecycle and Pre suites retain complete-frame continuation and
  current warning/state semantics. These tests qualify OCE boundaries, never host compliance.
