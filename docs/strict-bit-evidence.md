# Pinned strict-bit signal evidence

## Status and claim boundary

**Native Linux evidence is re-established on every pull request** (since
[GitHub #392](https://github.com/jscott3201/open-control-engine/issues/392)). The per-PR gate
captures the pinned corpus in four native cells — Linux x86_64 on `ubuntu-latest` and Linux aarch64
on `ubuntu-24.04-arm`, each in debug and release codegen — two independent processes per cell, and
a fan-in job compares all eight captures. The checks are exact equality with the unchanged Tier-A
references, byte-identical first and repeat captures within each cell, and equality of every
sample across all four cells. That per-PR matrix is the standing evidence for PC-037. Nothing is
admitted from stored digests: every run recomputes the result from the pull request's own
checkout, so there is no retained receipt to keep in step with the sources and no source-digest
binding.

The [machine-readable corpus](../crates/oce-conformance/tests/fixtures/strict_bits/corpus.json)
is the authoritative inventory of the **21 existing aligned-tolerance Real signal paths**.
It contains 161 samples: 101 finite (including 18 signed-zero samples), 52 NaNs and 8 signed
infinities. `corpus.json` retains the original **macOS aarch64 debug** observation. The four suites
use exact comparison for these 21 Linux Real signal cases. PC-037 is CURRENT only for this bounded
observed-output result: pinned rustc `1.97.1 (8bab26f4f 2026-07-14)` and libm `0.2.16`, baseline
repository codegen without custom `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`, Linux x86_64/aarch64
only. It is not a compiled-dependency-closure, whole-executable or arbitrary-input claim.

The local macOS debug/release tests still compare against the original observation. This is a
regression observation, **not macOS qualification**. macOS-arm64 remains explicitly
conservative/unqualified until M06-PR02; macOS and all other unqualified targets keep
`atoly=rtoly=ltoly=1e-12` in the four suites, with exact time/discrete comparison.
The per-PR macOS arm64 cell (`portable-snapshot-macos`) observes only that the portable engine-state
snapshot is byte-identical to Linux; it runs no strict-bit suite and says nothing about
transcendentals.

## Per-PR native matrix

`strict-bit-matrix` in [`pr-gate.yml`](../.github/workflows/pr-gate.yml) runs
[`scripts/ci/strict-bits-cell.sh`](../scripts/ci/strict-bits-cell.sh) once per cell. The script
refuses a host whose `uname -m` is not the cell's architecture, so a mislabeled runner cannot write
a capture under the wrong cell name, and it cross-compiles and emulates nothing. Each cell clears
cached evidence, makes two independent nextest process captures at the pull request's checkout
(each also repeats the actual facade drive internally), compares their bytes, and runs the four
suites plus inventory, strict wiring and hostile controls. Each cell uploads its two captures; the
x86_64 debug cell also uploads its built `strict_bits` test binary as a nextest archive.

`strict-bit-compare` downloads all eight captures and runs the otherwise-ignored
`matrix::native_linux_matrix_is_complete_and_exact` from that archive. It requires exactly the
four cells with first and repeat captures, every capture taken at this checkout's Git revision,
provenance, reference/input/CXF inventory and oracle bytes identical to a fresh collection in the
fan-in job, and exact agreement of every sample with both its Tier-A reference **and** the x86_64
debug capture. Missing data is a failure, not a skipped signal. It writes `matrix.json` — all eight
raw captures plus the comparison — even on a numerical disagreement, and on success also writes
`receipt-candidate.json`. Both are uploaded as the `strict-bit-matrix` artifact for 30 days.
Success of the fan-in also requires every cell's wiring and mutation controls to have passed;
matching output files cannot mask a failed cell test. `CI OK` needs all four cells and the fan-in.

**If a runner type is unavailable, the check fails closed.** An `ubuntu-24.04-arm` cell that
cannot get a runner stays queued, its fan-in never starts, and `CI OK` never reports success, so
branch protection keeps the pull request unmergeable. There is no fallback to emulation, no
skipped cell and no stored result that stands in for a missing run.

## Receipt log

[`receipts.json`](../crates/oce-conformance/tests/fixtures/strict_bits/receipts.json) is a log,
not a gate. `last_green_native_run` records the last green native matrix: run id, PR head and
merge SHAs, rustc, libm, the SHA-256 of each cell's first and repeat capture, and the `matrix.json`
SHA-256 — exactly the record the fan-in job writes as `receipt-candidate.json`, so updating the log
is pasting that file. `archives` names the two retained capture directories below with their run,
head, merge revision and matrix digest. The schema is closed: the retired `current`, `status`,
`pending` and source-digest fields are refused.

`receipts_log_records_a_green_native_run_on_exactly_the_four_native_cells` requires exactly the
four native Linux cells, byte-identical first and repeat digests, no two cells with the same
digest, well-formed SHAs and the current rustc/libm pins; when the logged run is a retained
archive, the record must equal the one rebuilt from those captures.
`receipts_log_refuses_absent_extra_mislabeled_and_unrepeated_cells` is its hostile control. No
test or job reads the log back to admit, skip or relax anything. The log is recorded today at the
native run 37382761120; the first per-PR native run is pasted in at the M1 closeout
([GitHub #404](https://github.com/jscott3201/open-control-engine/issues/404)).

## Retained native archives

Two sets of eight native captures from the GitHub-era `.github/workflows/ci.yml` stay checked in
as history. `retained_native_archives_reconstruct_their_recorded_matrix_digests` rebuilds each
set's `matrix.json` byte for byte from its canonical capture files and checks the recorded digest,
Git revision, sample count, reference inventory and zero mismatches;
`native_receipt_refuses_relabeling_and_raw_payload_reblessing` proves a relabeled revision or a
rewritten NaN payload refuses. Their captures carry the selected-source digest maps of their time
(17 and 35 entries); those maps are kept verbatim and are no longer compared with anything.

Each capture also records, per signal, the SHA-256 of the CXF document the conformance harness
built for it (`cxf_sha256`). Those documents bind the `S231` prefix in their `@context`, and
GitHub #527 changed the IRI the engine binds, so each of the 21 documents changed by that one
string. The corpus's 21 `cxf_sha256` values were rewritten with the ones a fresh capture by the
suite's own code recorded; no other field of `corpus.json` changed (no reference bit, input, time or signal name).
The archives were not touched: their `cxf_sha256` values are kept verbatim and are no longer
compared with the corpus, like their source maps, so both recorded matrix digests still
reconstruct. Every other inventory field of an archive still has to equal the corpus
(`an_archive_keeps_its_harness_document_digests_but_not_another_reference_inventory`). A fresh
collection and the eight per-PR captures are still compared with the corpus on `cxf_sha256`, so a
changed harness document cannot pass as the pinned input. The pull request that made the change
shows, for all 21 signals, that the SHA-256 of today's harness document with the IRI spelled the
earlier way equals the value both archives record.

### Run 37382761120 (`qualified-linux/`)

- [GitHub Actions run 37382761120](https://github.com/jscott3201/open-control-engine/actions/runs/37382761120)
  produced all eight native captures with the then-selected 35-file source map and a successful
  cross-cell comparison. It was collected for the workspace 0.2.0 version bump (GitHub #333).
- The run head was `f8dcaeb861fa18ef4c720b6fa2963b026f3a8300`. All captures preserve GitHub's
  actual synthetic PR merge checkout `fbae3b96b053836b916f603a0d86c53d30bce628`, not the run head
  or a later delivery commit. Each capture carries **35 source digests, 21 signals and 161 samples**.
- rustc: `1.97.1 (8bab26f4f 2026-07-14)`; libm: `0.2.16`; baseline repository codegen,
  without custom `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`.
- Uploaded assembled artifact archive digest:
  `sha256:0c94225cc2499e577cb597d0c36cbee138fc88bb65791872f1deb5c38e6b6576`.
- Downloaded `matrix.json` SHA-256:
  `78c5efaf4881977d306fc1f7a09c51f37cf897be338558d14aee9c2313d035d9`;
  its comparison is `{"Ok":[]}`. The retained test reconstructs this exact aggregate from the
  eight canonical capture files and verifies the digest, without storing a redundant ninth copy.
  Each per-cell upload (`strict-bits-<runner>-<codegen>`) is byte-identical to its two files in the
  assembled artifact.

| Native cell | SHA-256 of both first and repeat capture |
| --- | --- |
| Linux aarch64 debug | `6ed52dfdeaeb04d8c270943ec8bd594c7383aa5fc37cd8beff9446d675a78751` |
| Linux aarch64 release | `f97c6d8c257ab632a1685268a6a350bc46b55ebc5bd4c685b0d8b51ec9588442` |
| Linux x86_64 debug | `84d66c267e71f7f5e7ac02e7050fee99cf33ffca9b18bd81389fd7144e9368b2` |
| Linux x86_64 release | `81087ece58ea985d0d11cf79df78fefe7163b0180f2b8aa05d07902a5d6772d4` |

The collection run was intentionally **not a final green CI run**: at collection time the new
receipt had not been admitted. Each normal cell suite failed only its required receipt-admission
check (`source digest changed: Cargo.lock`), the cross-cell job failed only its subsequent
cell-success check, and `gate (light)` failed the strict-qualification prerequisite. Capture and
cross-cell numerical comparison succeeded; their artifacts are evidence of that bounded result, not
evidence that every hosted gate passed. It was admitted as the current receipt by data-only review
and stayed the admitted receipt until GitHub #392 retired the digest-bound admission model.

### Earlier receipts

Run [35494403523](https://github.com/jscott3201/open-control-engine/actions/runs/35494403523)
(head `d57e946126af76ed58ff10cef1e35ffbb627832f`, synthetic merge
`d16d69a49857ea9abd35a12643e83139ca5c6a6f`, `matrix.json` SHA-256
`3f6f9efe8e4f9980a790c1a0d8eba1143111a6ae9138f0d486cca841aa19b7fc`) was the previous current
receipt over the same 35-file boundary. Its eight captures were replaced in `qualified-linux/` by
the run above and remain readable in Git history; they are not separately pinned.

The [original Linux captures](../crates/oce-conformance/tests/fixtures/strict_bits/linux/) remain
unchanged historical evidence, pinned by the first `receipts.json` archive entry:

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

The archive test reconstructs that aggregate verbatim. The otherwise successful exact-head run
35493024355 used the same 17-source map. No old raw file, Git revision, source map or candidate
label is rewritten.

### The retired digest-bound receipt

From the first native capture until GitHub #392, admission worked the other way round: a retained
receipt was "current" only while a 35-file selected-source map (locks, toolchain, math sources,
suites, harness, checker, CI workflow, nextest configuration and gate script) matched the checkout
byte for byte, and any change to those files needed a new native capture, a manual download and a
data-only admission. On 2026-11-08 the receipt was put into a `recapture-pending` status for the
M1 refactor window (GitHub #387). Once both architectures ran natively on every PR, the owner
retired the binding (GitHub #405 decision 1): a recomputation from the PR's own sources supersedes
a comparison of stored digests with them, and it executes the transitive sources the 35-file map never
hashed. The source-digest list, the admission test, the pending status and the bound-source rule
were removed together; the captures and digests above are unchanged.


**libm 0.2.16 does NOT promise cross-architecture correctly rounded transcendentals; strict
identity is an empirical pinned observation only. std 1.97.1 likewise does not promise
deterministic transcendentals.** The versioned [libm documentation](https://docs.rs/libm/0.2.16/libm/)
and [Rust f64 precision contract](https://doc.rust-lang.org/1.97.1/std/primitive.f64.html)
do not justify upgrading a finite-corpus observation into an arbitrary-input guarantee.

In the engine those `libm` functions are reached only through the `math` modules of `oce-blocks`
and `oce-expr` (issue #399), so the bound implementation is named in one place per crate.
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
version, libm, Git revision, actual native OS/architecture/codegen cell and explicit qualification
limits. Captures taken before GitHub #392 also carry a `source_sha256` map of selected file
digests; new captures omit it, and the field is accepted only so the retained archives parse
verbatim. The Git revision names the observation's checkout HEAD.

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
sample arrays, wrong labels and hidden mismatches refuse. A rustc, libm or reference-inventory change
needs a deliberate pin or corpus review. Nothing admits checked-in observations as qualification:
capture always refuses to overwrite an existing artifact, and the historical-corpus refresh switch
has been removed. Review every new observation rather than blessing changed engine output as
correctness.

## Adjudication

Future divergence leaves the per-PR gate red with exact mismatch bits in the uploaded
`matrix.json`. Adjudicate the path before any claim or regime change; no automatic downgrade,
tolerance widening, bit-pattern acceptance, formula rewrite or Tier-A regeneration is authorized.

One-ULP mutations of every finite reference sample, every corpus signed zero, and relevant NaN/Inf
class/sign controls run through the facade driver. On Linux they also run through the actual
four-suite policy selector and assert the exact signal/sample failure; comparator-only unit tests
are not the sole evidence. Unmodified neighboring outputs stay green. Synthetic matrix tests
exercise each signal in every cell, missing/extra/mislabeled cells, repeat drift, one flipped bit
in a whole cell or only in its repeat, pin/provenance drift, signed zero and NaN class rules;
synthetic fixtures are never native evidence.

The gate script runs the scoped tests in both codegen profiles locally, but cannot reproduce a
cross-architecture result on one machine. The hosted matrix is separate from the state-artifact
determinism matrix; neither substitutes for the other. That matrix runs two debug cells per PR and
all four cells in the merge gate after each merge (issues #502 and #545). The strict-bit matrix did not
move: all four of its cells and the eight-capture comparison run on every pull request.

## Downstream notice

Whole-executable exactness inherits the least-qualified contributing path, target and input
domain. These 21 finite-corpus results do not qualify arbitrary compositions, arbitrary inputs or
a whole executable; unqualified paths/platforms still prevent such an inherited exactness claim.

This schema is compact, inspectable input for downstream consumers, not a downstream policy change
or qualification. No downstream checkout, source, dependency, tolerance policy or platform claim is
changed here.
