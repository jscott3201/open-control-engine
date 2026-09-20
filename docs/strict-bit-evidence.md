# Pinned strict-bit signal evidence

## Status and claim boundary

The [machine-readable corpus](../crates/oce-conformance/tests/fixtures/strict_bits/corpus.json)
is the authoritative inventory of the **21 existing aligned-tolerance Real signal paths**.
It contains 161 samples: 101 finite (including 18 signed-zero samples), 52 NaNs and 8 signed
infinities. Its checked-in observation is **macOS aarch64 debug**, not Linux evidence. All 21
observed paths agree with their unchanged Tier-A reference under the existing exact comparator.
The local debug/release tests compare against that observation; this is a regression observation,
**not macOS qualification**. macOS-arm64 remains explicitly conservative/unqualified until
M06-PR02, using the existing aligned band in the four suites.

Linux x86_64 and aarch64 debug/release are **exact candidates pending the hosted matrix**.
The four suites enforce the candidate exact comparator on those Linux targets, per inventoried
Real output, and the cross-cell job refuses qualification unless all eight captures agree.
This branch's local results do not establish that those hosted checks passed. PC-037 remains
FUTURE until the native evidence has run, been retained and received implementation acceptance.
Do not close issue #250 or describe a Linux promotion as empirically accepted based only on the
checked-in local capture. A green matrix is necessary, not permission to infer broader claims.

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
Linux and conservative other-platform regimes; every time, oracle value and engine value; and
every exact mismatch index. Input/parameter provenance is available through the bound reference
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
refresh the checked-in observation. The explicit local capture refresh facility writes only this
audit observation when opted in; it cannot refresh an oracle through that facility. Ordinary capture
refuses to overwrite an existing artifact. Review every observation diff rather than blessing a
changed engine result as correctness.

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

Before acceptance, admit the successful native artifacts to the checked-in delivery evidence and
review the per-signal result; the 90-day CI upload alone is not permanent evidence retention.
If any candidate diverges, the gate stays red and retains exact mismatch
bits. Adjudicate that path before promotion: retain its existing `atoly=rtoly=ltoly=1e-12` band
with the specific native mismatch/limitation, leaving time/discrete comparison exact. Do not widen
the band, silently accept a new bit pattern, rewrite a formula, or regenerate Tier-A output.
The initial inventory authorizes no divergent exception. Such an exception needs an explicit
reviewed evidence update, not an environment bypass.

The gate script runs the scoped tests in both codegen profiles locally, but cannot reproduce a
native cross-architecture result on one machine. The hosted matrix is separate from the existing
state-artifact determinism matrix; neither substitutes for the other. The existing required
`gate (light)` explicitly checks the new cross-cell job's success, including when that dependency
fails or skips, so the native check is not merely an optional status. GitHub delivery and changes
to branch protection remain the orchestrator's responsibility.

## Downstream notice

This schema is compact, inspectable input for later Open Control Sim consumption, not a Sim policy
change or qualification. The supplied downstream inventory says Sim has no active oce-api dependency;
its matching libm pin is context only. Sim policy adoption remains M05-PR07. No Sim checkout, source,
dependency, tolerance policy or platform claim is changed here.
