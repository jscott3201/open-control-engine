# Pinned strict-bit signal evidence

## Status and claim boundary

**Accepted native Linux corpus evidence:** [receipt data](../crates/oce-conformance/tests/fixtures/strict_bits/receipts.json)
identifies the native matrix from run **35494403523**, with an exact, reviewed **35-file selected
source boundary** covering checker/admission/comparison/workflow/direct formula/harness and
supporting sources. The ordinary retained-qualification test requires every enumerated path and
digest alongside the eight raw captures and reference/input inventory. This is not a complete
compiled dependency closure or proof of current whole execution semantics.

The [machine-readable corpus](../crates/oce-conformance/tests/fixtures/strict_bits/corpus.json)
is the authoritative inventory of the **21 existing aligned-tolerance Real signal paths**.
It contains 161 samples: 101 finite (including 18 signed-zero samples), 52 NaNs and 8 signed
infinities. `corpus.json` retains the original **macOS aarch64 debug** observation. Separately,
the [eight qualified native Linux captures](../crates/oce-conformance/tests/fixtures/strict_bits/qualified-linux/)
are accepted permanent evidence for **Linux x86_64/aarch64 × debug/release**, two independent
process runs per cell. Every run contains all 21 signals and 161 samples, with **zero mismatches**
against the unchanged Tier-A references and across cells under the exact comparator. Every
first/repeat pair is byte-identical, including NaN payloads. PC-037 is CURRENT only for this
bounded observed-output result. The four suites use exact comparison for these 21 Linux Real
signal cases; no Tier-A golden, comparison tolerance or bound source was changed for admission.

The local macOS debug/release tests still compare against the original observation. This is a
regression observation, **not macOS qualification**. macOS-arm64 remains explicitly
conservative/unqualified until M06-PR02; macOS and all other unqualified targets keep
`atoly=rtoly=ltoly=1e-12` in the four suites, with exact time/discrete comparison.

## Accepted native receipt

- [GitHub Actions run 35494403523](https://github.com/jscott3201/open-control-engine/actions/runs/35494403523)
  produced all eight native captures with the selected 35-file map and a successful cross-cell comparison.
- The run head was `d57e946126af76ed58ff10cef1e35ffbb627832f`. All captures preserve GitHub's
  actual synthetic PR merge checkout `d16d69a49857ea9abd35a12643e83139ca5c6a6f`, not the run head
  or a later delivery commit. Each capture binds **35 source digests, 21 signals and 161 samples**.
- rustc: `1.97.1 (8bab26f4f 2026-07-14)`; libm: `0.2.16`; baseline repository codegen,
  without custom `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`.
- Uploaded assembled artifact archive digest:
  `sha256:fc1a658c71f1b372767aa576bc5100b74c1dd697b4f63f3394fa8bc70d18e426`.
- Downloaded `matrix.json` SHA-256:
  `3f6f9efe8e4f9980a790c1a0d8eba1143111a6ae9138f0d486cca841aa19b7fc`;
  its comparison is `{"Ok":[]}`. The retained test reconstructs this exact aggregate from the
  eight canonical capture files and verifies the digest, without storing a redundant ninth copy.

| Native cell | SHA-256 of both first and repeat capture |
| --- | --- |
| Linux aarch64 debug | `e3769fb07630f2f1ea43101090b26416738c9b7c47e2519baa94d1f4ad2f7f5e` |
| Linux aarch64 release | `efcd3e1b7d88f2fff87abd9aa9adad24cc27e09c84f1112c552e4c1d8f42b747` |
| Linux x86_64 debug | `303654001e84d3dc4153cb38632a19fda0b0724e92ddcaf1c966387bbcf3dc9a` |
| Linux x86_64 release | `2f2703e20d5a119f2ff9df8f7fa013c6f6cf520571d64a1a74a928968eca6485` |

The collection run was intentionally **not a final green CI run**: at collection time the new
receipt had not been admitted. Each normal cell suite failed only its required receipt-admission
check, the cross-cell job failed only its subsequent cell-success check, and `gate (light)` failed
the strict-qualification prerequisite. Capture and cross-cell numerical comparison succeeded;
their artifacts are evidence of that bounded result, not evidence that every hosted gate passed.
Data-only admission now satisfies the ordinary retained check without changing any bound source.
Final hosted gate status remains a separate delivery check.

### Historical receipts

The [original Linux captures](../crates/oce-conformance/tests/fixtures/strict_bits/linux/) remain
unchanged historical evidence, separately pinned by `receipts.json.historical`:

- [GitHub Actions run 35492290613](https://github.com/jscott3201/open-control-engine/actions/runs/35492290613)
  passed all four strict-bit cells and the cross-architecture/codegen job.
- The provisional PR head was `dbce73fb20ada4a3a91653bb7ad9b48fae7ee87d`. All eight captures
  honestly record GitHub's synthetic PR merge checkout
  `8a63d4a042e1ca91d5dfe7bd3fc33d194f5102bb`, not that PR head or a later delivery commit.
- rustc: `1.97.1 (8bab26f4f 2026-07-14)`; libm: `0.2.16`; baseline repository codegen,
  without custom `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`.
- Uploaded assembled artifact digest:
  `sha256:d5b21ca70517c793f46a99d5402d236e2c78494466367fa583ccef27431c31c4`.
- Downloaded `matrix.json` SHA-256:
  `12a2fbdd28c9718c0145c5055240f0b7eab3d7898bc5d1f05ed0ee09db2978ad`;
  its comparison is `{"Ok":[]}`. The historical test reconstructs this exact aggregate from
  the eight canonical capture files and verifies that digest, avoiding a ninth duplicate of
  the raw data. The upload archive digest is a provenance locator, not a locally rebuilt archive.

The historical receipt test reconstructs that aggregate verbatim. Its 17-source map omitted
semantic capture/comparison/admission code now required by the selected 35-file admission boundary.
The otherwise successful exact-head run 35493024355 used the same smaller source map;
neither replaces the current selected-boundary receipt from run 35494403523. No old
raw file, Git revision, source map or candidate label is rewritten to imply those sources were captured then.

The active current-qualification test validates the admitted selected-boundary receipt and all four
cells, two runs per cell, 21 signals and 161 samples per run, zero mismatches, byte-exact repeats,
capture identity and aggregate integrity. It requires exactly the 35 enumerated source paths/digests
and current reference, provenance, input/CXF and time inventory. The synthetic checkout SHA is **not required to equal a
later HEAD** or exist in local history. A historical receipt cannot bypass a missing or changed
entry in that selected boundary, even when raw outputs agree. This guard does not establish
source identity for the rest of the compiled execution path.

**libm 0.2.16 does NOT promise cross-architecture correctly rounded transcendentals; strict
identity is an empirical pinned observation only. std 1.97.1 likewise does not promise
deterministic transcendentals.** The versioned [libm documentation](https://docs.rs/libm/0.2.16/libm/)
and [Rust f64 precision contract](https://doc.rust-lang.org/1.97.1/std/primitive.f64.html)
do not justify upgrading a finite-corpus observation into an arbitrary-input guarantee.

The Tier-A generator has an independent dependency direction, enforced by the existing
golden-gen firewall, but shares formulas and libm with these engine paths. Strict-bit evidence
proves implementation/plumbing/pinned portability over the corpus, **not mathematical correctness
or arbitrary inputs**. The original provenance's wording about deterministic math is retained
verbatim as historical provenance, not adopted as a library guarantee. CDL.Reals.Sqrt and G36
funnel bands are outside this inventory. Public runtime behavior, state/snapshot portability,
target-bound restore policy, algorithms, dependencies and non-21 comparison regimes are unchanged.

## Artifact contract

Schema 1 is UTF-8 JSON with a single header and one canonically ordered object per signal.
It is capped at 128 KiB per capture and 128 samples per signal. The header pins rustc's full
version, libm, Git revision, actual native OS/architecture/codegen cell, explicit qualification
limits and SHA-256 of the selected locks, toolchain, fixture definitions, harness, checker and math
source files. The Git revision names the observation's checkout HEAD; hashes separately bind the
selected file bytes and generated CXF, without claiming uncommitted bytes were committed at that HEAD
or binding unlisted transitive sources.

Each signal names its class, output and unique case path; reference CSV, original provenance and
generated CXF digests; original operation/recurrence rule and math-library provenance; proposed
Linux (at capture time) and conservative other-platform regimes; every time, oracle value and
engine value; and every exact mismatch index. Input/parameter provenance is available through the bound reference
CSV and generated CXF. Cases in the four existing suites are a checked projection of this inventory,
not a second manually maintained list. No oracle output is generated from engine output.

Words are `label:hhhhhhhhhhhhhhhh`, a lowercase, exactly 16-digit raw `u64` hexadecimal encoding.
Labels are `finite`, `+zero`, `-zero`, `+inf`, `-inf`, and `nan`; labels are validated against bits.
Raw NaN sign/payload is retained. Cross-cell exact comparison treats all NaNs as one class, as
the existing driver does; finite bits (including the sign of zero), signed infinities and time
bits are exact. Repeat captures within a cell compare **all raw bytes**, including NaN payloads.
No digest-only result substitutes for raw samples.

Unknown fields, wrong pins, missing/duplicate/unordered signals, changed provenance, incomplete
sample arrays, wrong labels and hidden mismatches refuse. Selected-source, lock, toolchain or inventory
changes invalidate the retained contract and need deliberate evidence review/refresh. CI cannot
admit checked-in observations. Capture always refuses to overwrite an existing artifact; the
historical-corpus refresh switch has been removed. Reference inventory validation is intentionally
separate from qualification: matching the old reference/input inventory does not admit an old
source map. Review every new observation rather than blessing changed engine output as correctness.

## Non-circular source binding and admission

[The source-digest list](../crates/oce-conformance/tests/strict_bits/evidence.rs#L157-L197) enumerates
exactly 35 selected files: the original locks/toolchain/build inputs,
four math implementations, four suites and three harness modules; plus `strict_bits.rs`,
`strict_bits/{evidence,matrix,controls}.rs`, the facade driver and its comparison module, exact and
aligned comparators, CSV/config/series/masking support and conformance module wiring, model value
encoding, the conformance manifest, CI workflow, nextest configuration and gate script.
The inventory/control tests require exactly this set and prove that changing any selected file
refuses admission even with unchanged raw samples. Specific mutations disable non-finite equality
and matrix topology checking; their source changes still refuse before receipt acceptance.

This selected boundary is **not the full compiled transitive facade closure**. The driver executes
through `oce_api::Engine`, but the map does not hash all `oce-api`, `oce-cxf`, registry/lowering or
other transitive implementation sources. Exact equality of its 35 digests establishes equality
only for those selected bytes, not current whole execution semantics or whole-executable identity.
Exact-head hosted native cells rerun the actual facade execution path per non-draft PR and catch
changes under the pinned corpus's stated exact-comparison rules. An unbound transitive source
change preserving all pinned outputs does not invalidate the historical raw result; neither the
receipt nor those reruns prove behavior on arbitrary inputs.

The receipt-binding order is **selected source bytes → captures → assembled matrix → receipt data**.
The checker hashes its own source bytes, which contain no expected source or matrix digest.
`receipts.json` is a closed, terminal data schema containing historical and optional current
Git/matrix identities; it contains no executable policy and is not hashed back into captures.
Current admission checks those reviewed identity values against actual raw data and current selected-source
digests. Final admission changes only receipt/fixture data and documentation, not bound checker
code. This avoids both a self-hash fixed point and a matrix-digest cycle; it is not a migration bypass.

The current receipt was admitted through the explicit admission test with `OCE_STRICT_MATRIX_DIR`
naming the download. It validated the selected source entries, reference/input data and the exact aggregate
before creating `qualified-linux/`; all eight retained files equal the downloaded bytes. The test
refuses CI and replacement of an existing directory. `linux/` and `corpus.json` remain unchanged
history. A bound-source change after the native run needs another native run; receipt-only admission
does not. No environment switch or historical-source exception bypasses current qualification.

## CI data flow and adjudication

The scoped `strict-bit-matrix` job runs on every non-draft development PR (and manual dispatch):
`ubuntu-latest` and `ubuntu-24.04-arm`, each in debug and release. It clears cached evidence, makes
two independent nextest process captures per cell at that PR's checked-out revision (each also
repeats the actual facade drive internally), compares their bytes, and runs the four suites plus
inventory, strict wiring and hostile controls.
Each cell uploads both complete raw captures even when a later strict test fails, retained 90 days.

`strict-bit-cross-arch` downloads all eight files and runs the otherwise-ignored native-only test.
The checker requires exactly the four native cells with first/repeat captures, matching Git and
selected-source provenance, the complete signal/sample inventory, and exact oracle **and** cross-cell
agreement. Missing data is failure, not a skipped signal. It emits `matrix.json`, containing all
eight raw captures and signal/sample/expected/actual mismatches, and retains it alongside the input
files even on a numerical disagreement. Success also requires every native cell's wiring and
mutation controls to pass; matching output files cannot mask a failed cell test. Synthetic matrix tests exercise each signal in every cell,
missing/extra/mislabeled cells, repeat drift, pin/provenance drift, signed zero and NaN class rules;
synthetic fixtures are never native evidence.

One-ULP mutations of every finite reference sample, every corpus signed zero, and relevant NaN/Inf
class/sign controls run through the facade driver. On Linux they also run through the actual
four-suite policy selector and assert the exact signal/sample failure; comparator-only unit tests
are not the sole evidence. Unmodified neighboring outputs stay green.

The successful native artifacts above are now checked in, rather than relying on a 90-day upload.
The explicit, ignored admission test verifies the downloaded aggregate digest, every canonical
capture, the zero-mismatch result and current selected-source equality before creating the retained
directory; it refuses replacement and cannot run in CI. It copies native evidence, never local
engine output. Ordinary retained validation is read-only and needs no environment opt-in.
Future divergence leaves the gate red with exact mismatch bits. Adjudicate the path before any
claim or regime change; no automatic downgrade, tolerance widening, bit-pattern acceptance,
formula rewrite or Tier-A regeneration is authorized. A changed selected source, pin or corpus needs a
new reviewed native receipt, not a rewritten historical observation or environment bypass.

The gate script runs the scoped tests in both codegen profiles locally, but cannot reproduce a
native cross-architecture result on one machine. The hosted matrix is separate from the existing
state-artifact determinism matrix; neither substitutes for the other. The existing required
`gate (light)` explicitly checks the new cross-cell job's success, including when that dependency
fails or skips, so the native check is not merely an optional status. GitHub delivery and changes
to branch protection remain the orchestrator's responsibility.

## Downstream notice

Whole-executable exactness inherits the least-qualified contributing path, target and input
domain. These 21 finite-corpus results do not qualify arbitrary compositions, arbitrary inputs or
a whole executable; unqualified paths/platforms still prevent such an inherited exactness claim.

This schema is compact, inspectable input for later Open Control Sim consumption, not a Sim policy
change or qualification. The supplied downstream inventory says Sim has no active oce-api dependency;
its matching libm pin is context only. Sim policy adoption remains M05-PR07. No Sim checkout, source,
dependency, tolerance policy or platform claim is changed here.
