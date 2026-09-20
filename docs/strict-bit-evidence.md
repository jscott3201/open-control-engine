# Pinned strict-bit signal evidence

## Status and claim boundary

**Native-evidence staging:** the historical Linux result below remains immutable, but is not a
qualification of the strengthened checker. [Receipt data](../crates/oce-conformance/tests/fixtures/strict_bits/receipts.json)
currently has `current: null`. The ordinary retained-qualification test deliberately fails with
`PENDING_NATIVE_EVIDENCE` until a new native matrix binds the complete checker sources. This is
not a green delivery head. The exact Linux comparison policy and conservative other-target band
remain active; no tolerance fallback is enabled while admission is pending.

The [machine-readable corpus](../crates/oce-conformance/tests/fixtures/strict_bits/corpus.json)
is the authoritative inventory of the **21 existing aligned-tolerance Real signal paths**.
It contains 161 samples: 101 finite (including 18 signed-zero samples), 52 NaNs and 8 signed
infinities. `corpus.json` retains the original **macOS aarch64 debug** observation. Separately,
the [eight native Linux captures](../crates/oce-conformance/tests/fixtures/strict_bits/linux/)
are accepted permanent evidence for **Linux x86_64/aarch64 × debug/release**, two independent
process runs per cell. Every run contains all 21 signals and 161 samples, with **zero mismatches**
against the unchanged Tier-A references and across cells under the exact comparator. Every
first/repeat pair is byte-identical, including NaN payloads. PC-037 records this bounded historical
result; current-source qualification is pending as stated above. The four suites use exact
comparison for these 21 Linux Real signal cases.

The local macOS debug/release tests still compare against the original observation. This is a
regression observation, **not macOS qualification**. macOS-arm64 remains explicitly
conservative/unqualified until M06-PR02; macOS and all other unqualified targets keep
`atoly=rtoly=ltoly=1e-12` in the four suites, with exact time/discrete comparison.

## Accepted native receipt

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
  its comparison is `{"Ok":[]}`. The retained test reconstructs this exact aggregate from
  the eight canonical capture files and verifies that digest, avoiding a ninth duplicate of
  the raw data. The upload archive digest is a provenance locator, not a locally rebuilt archive.

The historical receipt test reconstructs that aggregate verbatim. Its 17-source map omitted
semantic capture/comparison/admission code, so it cannot satisfy the current 35-source admission
contract. The otherwise successful exact-head run 35493024355 used the same incomplete source-map
implementation; it is not a substitute for new checker-complete captures. No old raw file, Git
revision, source map or candidate label is rewritten to imply those sources were captured then.

The active current-qualification test requires a separately admitted receipt and all four cells,
two runs per cell, 21 signals and 161 samples per run, zero mismatches, byte-exact repeats, capture
identity and aggregate integrity. It compares the complete source map and current reference,
provenance, input/CXF and time inventory. The synthetic checkout SHA is **not required to equal a
later HEAD** or exist in local history. A historical receipt cannot bypass a missing or changed
current source; source equality, not Git equality or matching raw outputs alone, governs applicability.

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
limits and SHA-256 of the locks, toolchain, fixture definitions, harness and math source files.
The Git revision names the observation's checkout HEAD; local uncommitted definitions are bound
by the source digests and generated CXF digest, not falsely described as committed at that HEAD.

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
sample arrays, wrong labels and hidden mismatches refuse. Source, lock, toolchain or inventory
changes invalidate the retained contract and need deliberate evidence review/refresh. CI cannot
admit checked-in observations. Capture always refuses to overwrite an existing artifact; the
historical-corpus refresh switch has been removed. Reference inventory validation is intentionally
separate from qualification: matching the old reference/input inventory does not admit an old
source map. Review every new observation rather than blessing changed engine output as correctness.

## Non-circular source binding and admission

`evidence::source_digests` enumerates 35 bound files: the original locks/toolchain/build inputs,
four math implementations, four suites and three harness modules; plus `strict_bits.rs`,
`strict_bits/{evidence,matrix,controls}.rs`, the facade driver and its comparison module, exact and
aligned comparators, CSV/config/series/masking support and conformance module wiring, model value
encoding, the conformance manifest, CI workflow, nextest configuration and gate script.
The inventory/control tests require the complete set and prove that changing every bound file
refuses admission even with unchanged raw samples. Specific mutations disable non-finite equality
and matrix topology checking; their source changes still refuse before receipt acceptance.

The dependency order is **source bytes → captures → assembled matrix → receipt data**.
The checker hashes its own source bytes, which contain no expected source or matrix digest.
`receipts.json` is a closed, terminal data schema containing historical and optional current
Git/matrix identities; it contains no executable policy and is not hashed back into captures.
Current admission checks those reviewed identity values against actual raw data and current source
digests. Final admission changes only receipt/fixture data and documentation, not bound checker
code. This avoids both a self-hash fixed point and a matrix-digest cycle; it is not a migration bypass.

For this staging head, CI captures still run first and upload both processes per cell. The normal
cell test step is deliberately red only at the pending current-receipt test (other failures still
require repair). The cross-cell comparison can assemble `matrix.json` with `{"Ok":[]}` using the
complete current source map, independently of receipt admission. Its subsequent cell-success step
and dependent `gate (light)` remain red until admission. Upload-on-failure retains the candidate
matrix and eight inputs. Do not weaken or skip these checks to make the staging head green.

After the stable checker stage is committed and run natively, review all eight captures and the
aggregate SHA-256, set `receipts.json.current` to the honest captured Git revision and matrix hash,
and run the explicit admission test with `OCE_STRICT_MATRIX_DIR` naming the download. It validates
current sources and reference/input data before creating `qualified-linux/`; it refuses CI and
replacement of an existing directory. Keep `linux/` and `corpus.json` as unchanged history. A code
change after that native run needs another native run; a receipt-only admission does not.

## CI data flow and adjudication

The scoped `strict-bit-matrix` job runs on every non-draft development PR (and manual dispatch):
`ubuntu-latest` and `ubuntu-24.04-arm`, each in debug and release. It clears cached evidence, makes
two independent nextest process captures per cell (each also repeats the facade drive internally),
compares their bytes, and runs the four suites plus inventory, strict wiring and hostile controls.
Each cell uploads both complete raw captures even when a later strict test fails, retained 90 days.

`strict-bit-cross-arch` downloads all eight files and runs the otherwise-ignored native-only test.
The checker requires exactly the four native cells with first/repeat captures, matching Git and
source provenance, the complete signal/sample inventory, and exact oracle **and** cross-cell
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
capture, the zero-mismatch result and current source applicability before creating the retained
directory; it refuses replacement and cannot run in CI. It copies native evidence, never local
engine output. Ordinary retained validation is read-only and needs no environment opt-in.
Future divergence leaves the gate red with exact mismatch bits. Adjudicate the path before any
claim or regime change; no automatic downgrade, tolerance widening, bit-pattern acceptance,
formula rewrite or Tier-A regeneration is authorized. A changed source, pin or corpus needs a
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
