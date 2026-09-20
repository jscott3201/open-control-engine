# Canonical accepted-frame replay record — revision 1

## Scope and authority

This is the execution-maintainer detail of PC-039 in [product contract revision 14](product-contract.md).
It defines **one accepted-frame record**, not a sequence, historian, state snapshot, deployment
receipt, network protocol or second evaluator. OCE remains synchronous, in-process, database-free
and file-I/O-free. Nothing is published or qualified for cross-release migration by this format.

`CompletedFrame::inputs()` retains the canonical accepted boundary inputs independently of caller
buffers and later engine mutation. `CompletedFrame::replay_record()` encodes those inputs, that
transition's outputs/warnings/time and its captured placement, without executing again. It records
the public `CompatibilityDescriptor::current(None)` facts: **export is explicitly absent**, not
silently claimed complete. The public descriptor is constant for that compiled process; it is not
the loaded executable. No export, state capture, private fingerprint or Store operation is involved.

The record contains no sequence number. `CompletedFrame::sequence()` remains Engine-lifetime
correlation only, never durable replay position. Hosts own the authenticated order, gap detection,
duplicate policy and optional start/end `EngineStateSnapshot` sidecars. Even identical record bytes
can represent two distinct accepted transitions. Equal timestamps are not idempotence keys.

## Host workflow and trust boundary

1. Bound transport/envelope buffering. Authenticate an envelope binding the **exact** record bytes,
   ordered position, compatible executable/parameters, approved same-build/deployment qualification,
   and any snapshot sidecars. Check freshness/generation and missing/duplicate policy outside OCE.
   Build qualification can include source/dependency lock, compiler, features, target and codegen.
   Package version, catalog/export tags and replay content identity are not build authority.
2. Decode an approved record with `ReplayRecord::from_bytes`. This checks canonical representation,
   corruption and bounds, not host trust. Do not patch bytes or relabel placement to make them pass.
3. Call `record.check_compatible(&CompatibilityDescriptor::current(None)?)` **before preparation**.
   It checks exact public facts in canonical order, then foreign target placement. V1 only decodes
   exact-bits policy. It never accesses an engine, performs execution or implements a tolerance.
4. Use an isolated engine loaded with the host-approved compatible executable/parameters. If supplied,
   decode and restore the separately authenticated compatible start snapshot under the existing
   [startup window and manifest checks](state-compatibility.md). A per-frame record intentionally
   does not prove prior state or complete executable identity. Without a snapshot the host qualifies
   the exact cold-start or preceding continuation state.
5. Borrow `record.inputs()` into `Vec<(&str, Value)>`, call existing `prepare_frame(record.time(), …)`,
   then consuming `execute_frame` **once**. Preparation still checks completeness, types, domains,
   time and current executable; ordinary refusal preserves all state and restore readiness.
6. Call `record.verify(&completed)` to compare exact time, accepted inputs, outputs and diagnostics.
   Comparison also rechecks current public facts (`export:none`) and receipt placement. A mismatch
   happens **after** that transition and does not roll it back. Stop the isolated host replay loop;
   do not blindly retry or use comparison failure as equipment fail-safe policy.
7. Drop that record before reading the next unless host retention is intended. Compare optional
   end-state snapshot bytes exactly. OCE keeps no last record or whole-sequence buffer. Persisting
   bytes, sidecars and authenticated order, recovery and any equipment command remain host work.

The public-only [host prototype](../crates/oce-api/tests/replay.rs) executes this workflow through
`oce_api` alone. It is a deterministic integration fixture, not authentication implementation,
external downstream adoption or host/equipment qualification. The rustdoc workflow is compiled too.

## Wire grammar

All integers are unsigned little-endian unless specified. `str` is a u32 byte length followed by
exact UTF-8 bytes, with no terminator, normalization or padding. Counts are u32. There are no optional
extensions, reserved flags, alignment gaps, compression, serde encodings or trailing bytes.

| Position/order | Encoding |
| --- | --- |
| 0..8 | ASCII `OCERPLY` then one NUL byte |
| 8..12 | u32 format revision, exactly 1 |
| 12..20 | u64 body byte length, excluding this header and the trailer |
| Body first | u8 exactness: 0 = `ExactBits`; every other tag refuses |
| Placement | u8: 0 = `Portable`; 1 = `TargetBound`, followed by `str arch`, `str os`; other tags refuse |
| Public facts | `str` canonical descriptor, grammar below |
| Model time | u64 raw binary64 bits, finite seconds; signed zero is retained |
| Inputs | u32 count; each row is `str canonical_path`, native value |
| Outputs | u32 count; each row is `str canonical_path`, native value |
| Diagnostics | u32 count; each row is `str block`, `str message`, u64 raw time bits, u8 severity (0 = Warning only) |
| Trailer | u128 FNV-1a-128 over all preceding header/body bytes |

Input/output paths are nonempty and **strictly increasing lexical UTF-8 bytes**, separately in
each section. Duplicate keys refuse even if values agree. Diagnostic order is emission order;
identical repeated diagnostics are allowed. Source/message strings can be empty and include NUL or
LF; diagnostic times preserve all bits, without adding a new producer-time restriction. Native frame
model time remains finite. Severity is closed Warning-only: never Error, escalation or interlock.

Target labels contain 1..64 ASCII lowercase letters, digits or underscores; capture uses Rust
`std::env::consts::ARCH` and `OS`. Structurally valid foreign labels can decode for inspection but
refuse eligibility. Placement uses the exact same conservative 15-class predicate as state capture;
any participating target-bound class binds the record. `Portable` removes only that target check,
not other compatibility or host qualification requirements. The finite Linux strict-bit corpus
does not imply universal mathematics or relax the [state placement policy](state-compatibility.md#portability-domains-and-evidence-limits).

### Native value encoding

| u8 tag | Payload |
| --- | --- |
| 0 — Real | u64 raw IEEE-754 binary64 bits, including subnormals, infinities, both zeros and every NaN payload/sign |
| 1 — Integer | i64 two's-complement bits; no f64 conversion or codec-level i32 narrowing |
| 2 — Boolean | u8 exactly 0 or 1 |
| 3 — String | `str` exact UTF-8, including empty, NUL and LF |
| 4 — Enum | `str` canonical registered enum class path, then u32 1-based ordinal in that class's domain |

Other tags refuse. Enum aliases, unknown classes and illegal ordinals refuse; no process-local
`EnumClassId` is serialized. The currently registered CDL/G36 enum paths/members are the existing
value/catalog contract, not a new extension registry. The codec covers every closed `Value` variant;
detached full-domain probes do not add String/enum signal ports to the live CXF profile. Playback
preparation retains its executable-specific input-domain checks, including CDL Integer bounds.

### Public descriptor grammar

The descriptor is at most 1024 UTF-8 bytes, with the ten existing labeled LF-terminated lines in
exact order: `oce-compatibility`, `catalog-schema`, `catalog`, `io-schema`, `value-schema`,
`parameter-schema`, `execution-profile`, `execution-profile-schema`, `oce-api-version`, `export`.
No missing/extra lines, CR/BOM, whitespace or escaping is accepted. Revision fields are positive
u32 decimal with no leading zeros. Catalog text is `catalog:1:fnv1a128:` plus 32 lowercase hex
digits; export is `none` or `cxf:fnv1a128:` plus 32 lowercase hex digits. The profile is `HostTick-v`
plus a positive u32 decimal. Package version uses SemVer syntax: three canonical u64 decimal
components, optional dot-separated ASCII alphanumeric/hyphen prerelease (numeric identifiers
have no leading zeros), and optional build identifiers. Empty identifiers refuse.

Structurally valid differing facts can decode, then fail `check_compatible` with the existing
`CompatibilityMismatch` cause. Descriptor revision other than 1 always refuses eligibility.
Native capture/verification use `export:none`; a decoded present export never matches absence.
Inspection/comparison of externally carried export facts does not make OCE attest their provenance.
No standalone public descriptor parser or arbitrary serde-stability promise is added.

## Integrity, identity and exactness

The trailer uses offset `0x6c62272e07bb014262b821756295c58d`, XOR each byte then multiply by
`0x0000000001000000000000000000013b` modulo 2^128. `ReplayContentId` applies the same FNV-1a-128
algorithm to **all canonical bytes including the trailer**; Display is
`replay:1:fnv1a128:<32 lowercase hex digits>`. Its private typed representation cannot be confused
with catalog/export identity. Both hashes are noncryptographic, collision-prone accidental-content
checks. An attacker can recompute them. Only exact host-authenticated bytes are the trust boundary.

Exact means `Value::bit_eq` and exact diagnostic source/message, severity and time bits, in order.
Time uses `to_bits`, not numeric equality: +0 and -0 differ; matching NaN payloads agree as signal
values. No epsilon, funnel, tolerance class or conformance comparison is reused. A record result is
computation evidence, never sensor quality, freshness, actuator delivery or security authority.

## Bounds and deterministic first cause

`MAX_REPLAY_BYTES` is **67,108,864 inclusive**, counting header, body and trailer. Above-limit
input refuses before header inspection and allocation. In addition, canonical decoded-work charge
is at most **134,217,728 inclusive**: 64 bytes per input/output/diagnostic row, every encoded string's
UTF-8 length (including enum paths), plus 24 bytes per String value for Arc metadata/alignment.
These charges are fixed across architectures; compile-time layout guards ensure the Rust row/Arc
representation does not outgrow them. Counts also have to fit the remaining raw minimum row bytes.
All sums/products and slice boundaries are checked.

The decoder scans/checks every field and charges the entire workspace **without allocation**, then
materializes exact-capacity vectors, owned strings and a copy of canonical bytes. It never trusts
a count to allocate before admission. Work is linear in byte/row counts with no recursive structure.
Canonical bytes plus charged decoded storage bound the owned record, not total process RSS: caller
buffers, allocator overhead, clones, expected-descriptor capture and engine state are separate.
Encoding sizes before allocating its bounded byte vector and validates through this decoder; capture
temporarily holds that encoding as well as the decoded record. Allocation failure/panic/process
death remain outside ordinary returned-error guarantees. This is not a CPU deadline promise.

Decode precedence is size; short/wrong header; format; total length; integrity; then body fields in
wire order (exactness, placement, descriptor, finite model time, inputs, outputs, diagnostics, exact
end). `ReplayError` distinguishes those causes, UTF-8, native/placement/severity tags, enum domain,
noncanonical keys/Boolean/labels/suffix and workspace limit. Offsets are in the complete record.
Eligibility compares descriptor fields in their existing canonical order, then target architecture/OS.
Verification checks current eligibility and receipt placement, time, input rows, output rows, then
diagnostic rows. The first differing row is reported; on pure count mismatch the index is the common
length. Helpers never mutate engine or Store. They do not replace preparation preflight or restore
manifest validation. A post-execution mismatch does not undo a prior accepted transition.

## Evidence and remaining qualification

- `tests/fixtures/replay_{add,values,warning}.hex` are **hand-authored expected payload bytes**, not
  blessed engine output. The existing hand-authored descriptor golden and independent
  `tests/replay_oracle/mod.rs` compose complete header/body/trailer bytes. Its two-word multiplication
  implements the specified checksum independently of the product's u128 hashing. Add's 3.75 output
  and warning fields are analytical expectations. No external Modelica oracle exists for this
  OCE-specific record grammar.
- `tests/replay_codec.rs` covers complete-domain decoding, every truncation boundary, rechecksummed
  hostility, descriptor fields, key ordering/duplicates, target labels, all unknown value tags,
  other closed tags, native bits, enum bounds, UTF-8, counts/lengths/trailing bytes, integrity,
  inclusive cap/one-past, workspace amplification and repeated allocation-free refusal.
- `src/replay_capture_tests.rs` checks exact independent encoding of every value domain on inputs
  **and** outputs, numeric bit classes, diagnostic fields/order/duplicates/counts, enum identity,
  empty records, oversized capture and unchanged sequence-independent bytes.
- `tests/replay.rs` exercises actual accepted receipt ownership, Store noninterference, retained
  capture, wrong-descriptor/target pre-execution refusal and restore-window preservation, warning
  goldens, equal-time stateful replay from separately decoded start snapshots, exact end-state bytes,
  and a 16,384-record public-only host stream. Each iteration has identical bounded allocation
  peak/total and zero retained allocations; an explicit larger allocation is the positive control.
  This fixture is not a whole-program performance or all-model memory claim.
- `tests/replay_matrix.rs` supplies repeat-exact portable and target-bound local vectors via optional
  `OCE_PORTABLE_REPLAY_OUT` / `OCE_TARGET_REPLAY_OUT` paths and foreign-placement inspection via
  `OCE_FOREIGN_TARGET_REPLAY_IN`. The existing debug/release `oce-api` matrix runs these tests, but
  **cross-architecture replay artifact comparison is pending**. Its artifact plumbing was not
  extended because `ci.yml` is itself pinned in the accepted strict-bit 35-file source boundary;
  changing that workflow would require separate requalification rather than silently reblessing
  evidence. Local artifact equality never fabricates hosted results or macOS qualification.

No state format/ABI, HostTick, catalog facts, conformance tolerance, dependencies, supported feature
selections, downstream repository pins or release claims change. The new facade items are additive
stable candidates; the exact public baseline and classification ledger record them.
