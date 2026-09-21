# Release-candidate compatibility and refusal

## Current policy

**Fail closed; no N-1 support.** This is PC-040's bounded current policy in the
[product contract](product-contract.md), not a published release or an M06 release-strategy
ratification. Nothing is published to crates.io. Only current/current is a supported candidate
pairing, subject to the existing artifact contracts and the host's actual build/deployment,
executable, prior-state, placement and safety qualification. No downstream pin changes follow.

The retained [machine-readable matrix](release-compatibility.json) has exactly 36 ordered rows:
nine artifact classes times four producer/consumer directions. The selected identities are:

- **Current implementation:** `e81480b02271456719d55cbe1e5090b0dea6d63c`.
  Delivery adds tests/evidence/docs only; it does not pretend its future commit can hash itself.
- **Historical source candidate:** annotated `v0.1.0`, tag object
  `7f3b614dc0e466d54cab4677ce4bb08a5bfaf033`, peeled source
  `909a8ba699e6a2fccf3de6ac0616a9e83a04060f`, 178 commits behind that baseline.
  This is a historical git pin, **never a published release or supported N-1**.
- Both package strings are **0.1.0**. Equal package strings do not identify the source, lockfile,
  compiler, features, target, codegen, binary or deployment. Neither descriptor nor FNV identity
  is build authority. OCE still has no build token.

The [historical receipt](release-compatibility-history.json) records an isolated exact-source
archive, its pinned Rust 1.95.0, default-feature debug build and 127/127 facade tests in 26 binaries
on `aarch64-apple-darwin`. The initial offline attempt lacked cached `stats_alloc`; the bounded
locked online attempt succeeded. Zero historical doctests were present. Historical public-API
extraction was unarmed, not an armed surface check. The task-owned historical package/target was
removed after capture. No historical workspace/release/other-target or equipment qualification is
claimed. Rebuilding successfully does not turn removed APIs into supported APIs.

## Directed artifact matrix

Legend: **A** = accept within the named current contract, **U** = unsupported/unqualified,
**N** = unavailable in producer, **H** = host-envelope refusal before decode. Directions always
mean producer to consumer, not upgrade versus downgrade. N is absence, **not a decoder refusal**.
H is host policy, **not an engine build check**; there is no historical parser to call for these
new artifacts. Supposed historical bytes and unknown build qualifiers are rejected by the host
before current OCE decoding too, even though no genuine historical artifact exists.

| Artifact | Current to current | Historical to current | Current to historical | Historical to historical | Scope / evidence |
| --- | --- | --- | --- | --- | --- |
| Package/public API | A | U | U | U | Exact current facade/storage baselines and compiler contraction controls. Historical tick/simulate/step_realtime were removed; matching versions do not restore source compatibility. |
| Facade catalog/schema | A | N | U | N | Current canonical facade catalog/schema revision 1; not a claim that the old `oce-blocks` catalog did not exist. |
| Public descriptor | A | N | H | N | Closed descriptor revision 1, public facts only; absent export is not a wildcard. |
| Diagnostics | A | N | U | N | Versioned producer receipts revision 1 and Warning-only assertion descriptor revision 2. Old diagnostic vectors existed, not this versioned contract; text is not a frozen schema. |
| Execution profile | A | U | U | U | Fixed HostTick v1, execution descriptor revision 2. No equivalence of historical execution modes is claimed. |
| Executable/frame | A | N | U | N | Complete typed frame preparation then one consuming transition; not durable executable serialization or cross-engine prepared-frame portability. |
| Snapshot | A | N | H | N | Format 2 / execution ABI 2; loaded-executable manifest, startup window and Portable/TargetBound placement remain mandatory. |
| Replay | A | N | H | N | Format 1, ExactBits only, descriptor and Portable/TargetBound placement; host authenticates ordered envelope and optional state sidecars. |
| Strict-bit qualification | A | N | U | N | Only the retained finite 21-signal Linux x86_64/aarch64 debug/release corpus; not whole-executable exactness. macOS/other targets remain unqualified. |

For current/current, read [facade contracts](facade-contracts.md), the
[complete-frame contract](complete-frame-contract.md), [state compatibility](state-compatibility.md),
[replay record](replay-record.md), [public surface](public-surface-contract.md) and
[strict-bit evidence](strict-bit-evidence.md). “A” never bypasses their limits. The strict-bit source
guard and workflows are unchanged; local macOS tests are not new native Linux evidence. Hosted
cross-architecture replay-byte comparison remains pending.

### Typed refusal controls are separate from historical absence

The matrix's `typed_controls` classify deliberate current-format/contract mutations as
**typed-refusal**. Existing tests exercise exact variants and preservation:

- `state_contract` refuses the retained **intermediate revision-1** snapshot with
  `EngineStateError::UnsupportedFormat { revision: 1 }`. That fixture is **not from v0.1.0**.
- State manifest/portability suites refuse ABI/manifest changes with `IncompatibleExecution` and
  foreign placement with `TargetDomainMismatch`, before mutation or Store calls.
- Descriptor mutation tests assert each exact `CompatibilityMismatch` first cause.
- Replay codec/eligibility suites assert `UnsupportedFormat`, `UnsupportedExactness`,
  `DescriptorMismatch` and `TargetMismatch`. A verification mismatch after execution is different:
  it does not undo the accepted transition.

## Migration and refusal guide

There is no cross-candidate API, snapshot, replay, descriptor, diagnostic or executable support.
Hosts migrate their **application source** deliberately using the
[facade migration guide](facade-migration.md), then qualify the exact new binary and supported CXF.
OCE does not translate old CXF semantics or state. Never rewrite a header, ABI, descriptor, checksum
or placement label to gain admission. Do not treat a compatible OCE manifest as build approval.

On an incompatible/unsupported state or replay envelope, select one explicitly approved host action:

1. **Cold load and safe requalification:** fence commands, load approved CXF/parameters into a
   fresh isolated current engine, start with fresh parameter-seeded state, validate complete inputs
   and expected outputs, reconcile external history/time/ownership, then separately authorize delivery.
   Load may call Store; even failed load has the documented compensation boundary. Cold start is not
   a state-preserving upgrade and does not silently happen after a restore refusal.
2. **External rollback:** select the prior **qualified** binary with its own authenticated compatible
   state and host configuration. It is not current OCE restoring prior-build bytes. The historical
   v0.1.0 pin is not qualified by this document and has no OCE snapshot/replay API; do not fabricate
   a historical snapshot or claim an actual rollback run. Any prior host's state is its own contract.
3. **Remain fenced / NO_EVAL** if neither action is approved. Halt is not an equipment stop.

The [public-only host fixture](../crates/oce-api/tests/release_fallback.rs) refuses missing,
unauthenticated, unknown and cross-candidate envelopes before its decoder closure, compares fresh
and advanced snapshots and Store counters, preserves restore readiness, and contrasts accepted
continuation with the independent Pre/Not cold-start recurrence. Valid current bytes carrying a
hostile historical label are synthetic controls, not historical producer bytes. Its rollback test
only routes opaque prior bytes to a matching external handler, never to current OCE. Authentication,
real prior-binary execution and field actuation are deliberately outside this fixture.

### Host fallback checklist

- [ ] Preserve exact original bytes and refusal cause; do not migrate or rewrite them.
- [ ] Fence external actuator ownership/commands and select NO_EVAL while deciding.
- [ ] Authenticate exact bytes, full build qualifier, freshness, generation and replay order before
  decoding. Same Cargo version, descriptor equality and integrity checks are insufficient.
- [ ] Choose explicitly approved cold requalification or separately qualified external rollback;
  never auto-fallback or cross-build restore.
- [ ] For cold start, qualify CXF, parameters, complete IO domains, cadence and seeded behavior;
  compensate/reconcile load-time Store effects and external state separately.
- [ ] For rollback, bind the prior binary to its own authenticated state and configuration; reject
  any mismatch before dispatch. No current decoder participates.
- [ ] Verify field fencing, quality policy, model-time mapping and exclusive command authority before
  delivery. Successful load, restore, replay or local tests alone authorize no field effect.

## Evidence checker and review boundary

The [standard-library checker](../scripts/release_compatibility/check.py) derives current version,
state ABI/wire, descriptor and domain revisions from source/contracts, checks the existing descriptor
golden, and binds exact evidence bytes. Its selected implementation boundary includes all tracked
or pending `crates/*/src/**`, `crates/*/contracts/**`, crate manifests/build scripts, root manifest,
lockfile, toolchain and Cargo config: **326 files at the selected baseline**. Canonical path plus
SHA-256 records, sorted lexically, produce the retained boundary digest. Missing, added, changed or
symlinked source refuses. This is a source drift boundary, not a compiled-binary/dependency-cache or
host build attestation; integration tests and their fixtures are separate retained evidence.

The retained JSON is compared **byte-for-byte**, including object/row order and the final LF, with
the independently closed candidate policy and live facts. Missing/extra/duplicate/reordered rows,
keys, evidence, malformed encoding and mutated identities, revisions, statuses or directions refuse.
`--candidate` prints review input only; no auto-bless mode exists. Source-boundary updates require a
new reviewed baseline/decision, not replacing a failed golden. The
[hostile tests](../scripts/release_compatibility/test_check.py) include independent direction
expectations, physical evidence failures, repeat output and a no-op enforcement control.

```bash
python3 scripts/release_compatibility/check.py
python3 scripts/release_compatibility/test_check.py
python3 scripts/release_compatibility/check.py --verify-history
```

The last command additionally verifies annotated/peeled Git objects and baseline source bytes;
it fails when those objects are unavailable (for example, a shallow clone), never silently fetches.
Ordinary checks use the pinned retained history receipt so existing shallow CI remains usable.
An integration sentinel in `oce-api` runs the checker and all 16 hostile tests in the existing gate;
no CI/release workflow or strict-bit guard changes are needed. Run the actual repository gate via
[the gate script](../.agents/gate.sh); source/digest equality never substitutes for behavioral runs.
Coordinated edits to checker and evidence are a review boundary, not a cryptographic security defense.

## Future RC release-note template

Copy this section as a checklist for a separately authorized RC, not as a release announcement:

- Candidate source, implementation boundary, full host build/deployment qualifier and package version:
  record all separately; attach matrix/checker and exact-candidate test results.
- Candidate pairs and artifact directions: enumerate accepted, typed-refused, host-refused,
  producer-unavailable and unsupported/unqualified cases without treating absence as decoder coverage.
- Public API/catalog/diagnostics/frame changes: cite exact baselines and source migration decisions.
- State/replay/profile: record format/ABI/descriptor/exactness/placement revisions and refusal causes;
  explicitly state **no N-1, no migration, no cross-build restore** under this policy.
- Fallback: cite host-approved cold requalification or prior-qualified-binary/own-state rollback,
  freshness/fencing requirements and actual qualification evidence; disclose unexecuted steps.
- Validation: distinguish local debug/release/doctests, armed surfaces, hosted native architectures,
  finite strict-bit receipt, pending replay artifact comparison and downstream equipment qualification.
- Authorization: record the independent release/publication decision. This matrix grants neither;
  do not infer publication from a tag, eligible manifest, package version or green local gate.
