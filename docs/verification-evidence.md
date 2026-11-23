# Verification and evidence

The [generated authority summary](authority-claims.md) mechanically inventories raw provenance
records only. Evidence independence and semantic coverage remain review-only, not inferred counts.

This page is for someone deciding whether to trust this engine near real equipment. It answers one
question: what has actually been proven about the Open Control Engine, and what has not.

Three questions are worth asking of any system that claims to be verified. What can tell it that it
is wrong? Is that thing independent of the system it is judging? And will it tell you which checks
it is not running? This page answers them in that order, and every count on it can be reproduced
from a clone with `find` and `grep`.

The short version first, because it is the part that matters most: four named cases have been
executed through OpenModelica 1.25.1 against pinned Buildings and MSL sources. Nand covers all four
two-input Boolean states; Toggle covers one exact stateful event schedule with initially true input,
repeated rises, and clear priority; Line covers four limit modes across five finite input regions.
Reliefs covers one seven-state exact-bit case for a composed G36 leaf. The global Tier-3 report
remains skipped; no full-sequence, arbitrary Real, general tolerance, solver, or cross-architecture
raw-byte claim follows from these cases. A fifth OpenModelica run, of upstream's Building 33
verification model over one day, is a within-a-bound comparison for one commissioning example and
none of these ([below](#commissioning-on-recorded-data-the-building-33-day)). Single runs, not
repeated, check the event timing of five event-triggered classes exactly
([below](#event-timing-against-openmodelica)) and upstream's Building 33 valve sequence on its 78
synthetic frames, exact at every engine reset. Two of those runs found the engine and OpenModelica
apart where a pulse is true on two consecutive frames; the settled-value rule of issue #800
resolved both, and seven more pilots in nine runs, against the pinned Buildings classes, check that
rule ([below](#pulses-on-event-inputs-against-openmodelica)), with ten registered differences.

`CDL.Logical.Pre` is outside those executed-reference claims. The engine's fixed
[HostTick v1 profile](execution-profile.md) delays `Pre` by one HostTick transition rather than one
same-time Modelica event iteration. The profile is covered by exact engine contract tests, not
by an OpenModelica oracle, and no broader stateful conformance claim follows from it.

---

## What can tell this engine it is wrong?

Six evidence layers here are called "tests". They prove different things, and the most
visible of them proves nothing about correctness at all.

| Layer | Artifact | Count | Independent of the engine? |
| --- | --- | --- | --- |
| Tier-2 determinism goldens | `crates/oce-conformance/tests/fixtures/golden/g36_traces/` | 46 traces + 46 `.prov.json` | **No** — engine self-output, by construction |
| Tier-A source references | `tools/golden-gen/goldens/` | 400 provenance records, 398 signal goldens | Yes — CI-enforced code-dependency firewall |
| Tier-A HostTick profile references | `tools/golden-gen/goldens/G36/`, `tools/golden-gen/goldens/CDL/Logical/Pre/` | 22 signal goldens: 20 G36, 2 standalone `Pre` | Independent implementation of the engine profile; not a Modelica oracle |
| Structural oracle | `third_party/modelica-buildings-cdl/cxf/` | 45 vendored translations (44 G36 and the Building 33 sequence); 31 comparable fixtures | Yes — an independent translation of the same upstream source |
| Tier-1 per-block oracle comparisons | `crates/oce-conformance/tests/per_block_*.rs` | 16 suites; 285 CDL signal goldens: 264 existing exact plus 21 exact on qualified Linux, unchanged 1e-12 aligned band on unqualified targets; [bounded receipt](strict-bit-evidence.md) | Yes — Tier-A generator is outside the engine workspace |
| Scoped Tier-3 cross-implementation differentials | `crates/oce-conformance/tests/fixtures/open_modelica/` | 4 named cases: 2 Boolean, 1 finite Real matrix, and 1 composed G36 leaf; global report skipped | Yes — pinned OpenModelica and Buildings execution |
| Event timing against OpenModelica | `crates/oce-conformance/tests/fixtures/open_modelica/{integers_on_counter,discrete_triggered_*,reals_*_with_reset}/` | 5 classes, 2 ten-frame scenarios each, exact; an integrating ramp; 1 registered divergence (consecutive pulses); one run per pilot | Yes — pinned OpenModelica and Buildings execution, one run |

### Tier-2 determinism goldens — they catch drift, not wrongness

Each of the 46 ASHRAE Guideline 36 fixtures carries a committed whole-sequence trace and a sidecar
provenance record. All 46 records say the same two things:

```json
{ "tier": "2",
  "source": "engine self-output (determinism snapshot); NOT a correctness oracle",
  "depends_on_oce_blocks": true }
```

That is not a caveat added by this page; it is a field in every one of the 46 files, and it is the
whole meaning of the layer. These goldens were produced by running this engine and committing what
it printed. If the engine computes a sequence wrongly and keeps computing it wrongly, all 46 pass
forever. They detect one thing well: that a code change moved a number that was not supposed to
move. Call that drift detection, and do not call it correctness.

Each record also carries a `content_sha256` binding it to the bytes of its CSV, checked per PR by
`crates/oce-cxf/tests/golden_provenance/mod.rs`. That guard is honest about its own limit in its
first paragraph: editing a CSV together with its digest passes by design. It detects drift between
two checked-in artifacts, not fabrication of both.

### Tier-A references — 422 records behind a code-dependency firewall

The reference layer is generated by `tools/golden-gen`, a crate deliberately held **off the
workspace**. The repository's workspace is `members = ["crates/*"]` (`Cargo.toml:24`), and
`tools/golden-gen/Cargo.toml` declares an empty `[workspace]` table so the root workspace cannot
absorb it. Its entire dependency list is `libm`, `ryu`, and `serde_json` — no `oce-*` crate.

That is enforced mechanically rather than by convention.
`.github/scripts/check-golden-gen-anti-tautology.sh` runs `cargo metadata` over the generator and
fails if any package named `oce-*` appears anywhere in its dependency graph. It fails closed: a
`cargo metadata` error, or output that does not contain the `golden-gen` package itself, is a
failure rather than a pass. It runs as its own CI job and again inside `.agents/gate.sh`.

The layer contains **422 Tier-A provenance records**, every one of them recording `"tier": "A"` and
`"depends_on_oce_blocks": false` — counts verified across the tree, with zero records carrying
`true`. Of these, 400 records check source semantics and 22 check HostTick v1:

- **287 CDL records** — 285 per-signal goldens, plus two fold-time provenance-only records with no
  CSV beside them (`goldens/CDL/Types/types.prov.json`, pinning enum ordinals, and
  `goldens/CDL/Constants/constants.prov.json`).
- **135 G36 sequence signal goldens**, spanning all 46 fixtures; two of them belong to a 1/16 s
  FreezeProtection scenario that reuses that fixture.

**420 of those are signal goldens — 399 with existing exact comparisons and 21 inventoried
accepted Linux exact cases, still aligned-tolerance on unqualified platforms.** The
[per-PR native matrix](strict-bit-evidence.md#per-pr-native-matrix) re-establishes them on every
pull request: Linux x86_64/aarch64 × debug/release on native runners, two byte-identical runs per
cell, 21 signals and 161 samples per run, zero exact mismatches against Tier-A and across cells,
recomputed through `oce_api::Engine` from the PR's own checkout. No stored receipt admits or
skips anything; `receipts.json` only logs the last green native run. PC-037 is CURRENT only for
this pinned corpus. macOS-arm64 remains
unqualified until M06-PR02; neither libm mathematical correctness nor arbitrary-input or whole-engine
exactness follows. No Sim policy changes. The retained capture archives from runs 35492290613 and
37382761120 are reconstructed history, not current qualification.
The 285 CDL signals are compared by the 16
`crates/oce-conformance/tests/per_block_*.rs` suites. All but one go through a shared harness that drives each
block through the frozen facade, asserts the comparison is unmasked, and asserts
`compared_points == reference.n_rows` so a zero-row comparison cannot pass vacuously. The
exception is `per_block_utilities_assert.rs`: `CDL.Utilities.Assert` has no output connector, so it
drives a standalone Assert CXF through the same facade and compares each frame's Warning
diagnostics (count, source, message, time bits) with the golden's `warning` column, after
asserting the golden's row count, exactly. Of the other 15 suites, 12 run `ComparisonMode::Exact`
with zero tolerances (`crates/oce-conformance/tests/block_harness/mod.rs`), and four select each of their 21
libm-dependent Real goldens through the inventory: exact on the two qualified Linux targets,
`ComparisonMode::AlignedTolerance` at the unchanged 1e-12 band elsewhere:
`per_block_reals_transcendental.rs`, `per_block_reals_sources_transcendental.rs`,
`per_block_psychrometrics.rs`, and `per_block_utilities.rs` — with
`per_block_reals_sources_transcendental.rs` counted in both, because its two `CalendarTime`
cases compare exactly while its single `Sin` case is inventoried. Boolean outputs in the aligned
suites still compare by bits even in that mode (`crates/oce-conformance/src/aligned.rs:214`), so
all 285 CDL goldens are exact on qualified Linux targets, versus 264 exact and 21 aligned elsewhere.
The 135 G36 signals are compared by 23 `*_funnel.rs` and four `*_oracle.rs` per-fixture suites in the same
directory. Their recorded comparison regimes tally exactly: 104 `Value::bit_eq` f64, 19 exact
encoded integer, 12 exact 0.0/1.0.

The semantic claim is narrower than the 420-comparison count. **398 signal goldens check CDL /
Buildings source semantics**: 377 existing exact and 21 native-matrix-qualified Linux cases (conservative
aligned-tolerance on unqualified platforms). The remaining **22 exact signals** check HostTick v1:
20 G36 signals belong to `Generic.TimeSuppression`, `CoolingOnly.Controller`, and `ReliefFanGroup`,
and two are the standalone `CDL.Logical.Pre` goldens (`execution_profile: "HostTick v1"`, with a
`diverges_from_modelica_event_iteration` note). Those references are independent of `oce-blocks`,
but their `CDL.Logical.Pre` recurrences implement HostTick v1 and are labeled as profile checks
rather than Modelica event-iteration oracles.

Three provenance fields narrow what a G36 signal golden is evidence of. A script can read all three
from every G36 record:

- **`sequence_derivation`.** The 10 signals of the three fixture-only sequences
  (`ahu_supply_air_temp_reset`, `ahu_economizer`, `vav_single_zone`) are labelled
  `fixture-local-composition`. These fixtures borrow a pattern from a G36 sequence but use
  fixture-local inputs that the G36 `.mo` files do not have. Their goldens check CDL block semantics
  over the fixture topology, and they cite only those CDL block sources. They are not answer keys
  for any G36 sequence.
- **`diverges_from_g36_intent`.** Twelve signals come from fixtures that faithfully copy pinned
  Buildings behaviour contradicting the stated G36 intent:
  - `multizone_vav_economizer_enable` and the single-damper economizer controller reference follow
    `Enable.mo`'s `Or` high-limit wiring, which enables the economizer only above the cutoff;
  - `multizone_vav_supply_temperature` follows `SupplyTemperature.mo`'s cool-down/setup mode swap.
    Its golden drives mode 1 only, so it never exercises the swapped modes: it is labelled
    because its fixture carries the swap, not because its trace shows it. Separately from the
    `.mo`, its CXF fixture writes the nested trim-and-respond parameters as literals where the
    `.mo` binds them to the outer parameters. The literals equal the outer values today.
  - every reference built on `Generic/TrimAndRespond.mo` — `trim_and_respond_have_hol_false`,
    `multizone_vav_supply_fan` (its pressure reset; `supply_fan_status` carries the note only
    because the label is per scenario) and `multizone_vav_supply_temperature` — follows the
    block's graph, not its documentation. The documentation says the setpoint is `iniSet` while
    the device is off, but `iniSet` reaches the sampled `UnitDelay` only at a `samplePeriod`
    instant. After an off window that holds no sample instant, the reset resumes from the stale
    pre-trip value once the re-armed delay elapses. The standalone golden pins both sides of
    that boundary (GitHub issue #354).

  The economizer goldens agree with the pinned `.mo`, not with Guideline 36.
- **`frame_rate_dependence`.** Eight signals are driven by a `Ti = 0.5 s` PI loop. The generator
  integrates that loop with the forward-Euler recurrence the engine also uses, so their values
  change with the frame step:
  - at a 60 s frame (`k·dt/Ti = 120`, saturating every frame):
    `multizone_vav_freeze_protection/heating_coil_command`, plus `outdoor_damper_min_limit`,
    `outdoor_damper_command` and `return_damper_command` of the single-damper economizer
    controller reference;
  - at a 1 s frame (`k·dt/Ti = 2`): `return_fan_speed` of
    `multizone_vav_return_fan_airflow_tracking`, and `relief_damper_command`,
    `discharge_pressure_setpoint` and `return_fan_speed` of
    `multizone_vav_return_fan_direct_pressure`.

  They check that shared discretization, not continuous-time `.mo` behaviour.

  The criterion is `k·dt/Ti ≥ 2`. At that ratio forward Euler applied to the integrator's own
  time constant has an amplification factor of −1 or below, so the discrete recurrence no longer
  tracks the continuous PI.

  PI references below the threshold use the same recurrence, but at their golden's step they are
  treated as frame-step-insensitive:

  | Reference | `k·dt/Ti` |
  | --- | --- |
  | `cooling_only_dampers` | 0.2 |
  | `multizone_vav_freeze_protection_fine_step` | 0.125 |
  | `cooling_only_controller` | 0.1 |
  | `vav_single_zone` | 0.0125 |
  | `thermal_zones_control_loops` | ≈0.0067 |
  | `multizone_vav_supply_fan` | ≈0.0017 |
  | `multizone_vav_economizer_limits_common` | ≈4.2e-4 |
  | `multizone_vav_supply_signals` | ≈8.3e-5 |

  No scenario falls in `[0.2, 2)`.

  `multizone_vav_freeze_protection_fine_step` is the continuous-time counterpart of the labelled
  FreezeProtection heating-coil column. It runs the same fixture at a 1/16 s frame over 330 s
  (5,281 rows), long enough for the 300 s stage timers, and steps the mixed-air and supply-air
  temperatures so both heating-coil PI controllers ramp, saturate, leave saturation and hold.
  The engine matches it bit-exactly
  (`crates/oce-conformance/tests/g36_freeze_protection_funnel.rs`). Separately,
  `crates/oce-conformance/tests/g36_freeze_protection_heating_coil_continuous_time.rs` solves the
  `CDL/Reals/PID.mo` PI with anti-windup in closed form. That solution is piecewise linear and
  exponential, and it is exact here because every input breakpoint lies on the frame grid. The
  suite requires every row of the golden to stay within a per-row forward-Euler bound. The
  bound adds up one-frame truncation errors taken from the closed form, carried through the
  Euler map's Lipschitz constant: 1 when unsaturated, `1 − dt/(Ti·Ni)` when saturated. The
  largest bound is ≈ 0.0099 and the largest observed error is ≈ 0.0049. The bound is sharp: right
  after a mode corner the error reaches it to within rounding. Solving the closed form with `Ti`
  off by 1% puts some rows about 10⁶ times past their bound, and `Ni` off by 5% or `yMax = 0.95`
  is also rejected. The same oracle departs from the 60 s column by a full 1.0, which is why that
  column keeps its label. A third check shows forward Euler converging to the closed form at
  first order as the step shrinks, at `k = 1` and at `k = 0.37`.

  The fine-step stage column must match a stage schedule derived from FreezeProtection.mo, and
  that schedule, not the golden, picks which PI each row is checked against.

  One limit: FreezeProtection.mo has `k = 1`, so these goldens cannot distinguish the
  anti-windup gain `1/(k·Ni)` from `k/Ni` or `1/Ni`. The `k ≠ 1` form is pinned by the CDL PID
  Tier-A goldens (`k = 0.37`, saturating at `yMax = 0.42`), which do separate the three forms.
  The engine matches them exactly in `crates/oce-conformance/tests/per_block_reals_dynamics.rs`
  (`reals_dynamics_blocks_match_exact_oracle`, cases `reals_pid_pi_recurrence` and
  `reals_pid_with_reset_pi_recurrence`). No `oce-blocks` unit test exercises anti-windup with
  `k ≠ 1`.

  `crates/oce-conformance/tests/g36_reference_frame_rate_labels.rs` recomputes each ratio from the
  fixture's resolved PI parameters and the golden's time grid. It traces which outputs each
  at-threshold PI reaches and requires the labels to match exactly. A new integrating PI
  scenario fails that suite until someone reviews it.

`crates/oce-conformance/tests/g36_reference_provenance_labels.rs` pins the derivation and
divergence labels. It also checks each note against the vendored source or fixture it describes.

"Bit-exact" here has a precise definition, in `crates/oce-conformance/src/exact.rs`: finite Reals
compare by IEEE-754 bits, NaN compares equal to any NaN payload, and each signed infinity compares
only to itself. Integer and Boolean cells compare their encoded values exactly.

The L1 funnel band is an **additional** layer applied over 103 of the 104 Real G36 outputs, never the
primary check. The fine-step FreezeProtection heating-coil command is compared exactly and
against the continuous-time oracle, but is not routed through the band. Boolean and Integer outputs are deliberately kept off it, because the funnel is
type-blind and a band could otherwise admit a value between two discrete levels
(`crates/oce-conformance/tests/g36_funnel_band/policy.rs:17-21`).

### The structural oracle — it bounds fixture fidelity, not block behavior

Every conformance test in the workspace derives from the same 46 catalog fixture documents. The
47th CXF document, `member_list_interface.jsonld`, is a resolver contract fixture and has no
conformance trace. A structurally wrong catalog fixture therefore fails nothing — it makes the
entire suite validate the wrong sequence, consistently and permanently. Neither goldens nor oracles
can see that, because both are computed from the fixture.

The check that can is `crates/oce-cxf/tests/fixture_structural_oracle.rs`. It compares each fixture
against `modelica-json`'s independent CXF translation of the same upstream G36 class — 44 `.jsonld`
documents vendored under `third_party/modelica-buildings-cdl/cxf/`, at Buildings commit
`a131864e4c4df22ebcd52bb8da439de0087ac365` and modelica-json commit
`85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb`, pinned and byte-checked by a hash manifest in both
directions. The comparison flattens the oracle's composite hierarchy, resolves every conditional
against the fixture's own parameter values on both sides, canonicalizes array instances and vector
ports, and compares instances and undirected edges — counting orientation flips separately, since
CXF §8.2 admits either endpoint as the `isConnectedTo` subject.

The verdict table is itself a committed golden
(`crates/oce-cxf/tests/fixtures/golden/structural_oracle_verdicts.txt`), and its VERDICTS line
reads:

```text
VERDICTS: EXACT=30 EXACT-XFOLD=1   EXCLUDED=15
```

So: **30 EXACT plus 1 EXACT-XFOLD over the 31 comparable fixtures.** The XFOLD case is
`multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21`, where one
constant-folded subtree (`ecoHigLim`, 146 reference instances against 0 of ours) is excluded and
named in the golden. The 15 excluded fixtures are listed with reasons and are **never counted as
passes**: three are this repository's own compositions with no upstream class, and twelve are
parameter-specialized reductions of `AirEconomizerHighLimits` that are structurally unverifiable by
design.

State the limit next to the result: this compares **graph structure only**. It contains no
numerics and executes no engine code path. It bounds how faithfully the fixture corpus represents
upstream G36. It says nothing whatsoever about whether a block computes the right number.

### Tier 1 and the global Tier-3 report

This is not a footnote. It is the boundary of everything above.

When conformance report assembly succeeds, it emits five tiers. Tier 1 and Tier 3 are hard-coded as
skipped on every successful path:

- **Tier 1** — per-block "same response" against the Buildings library. Skipped with the summary
  `"per-block Buildings-oracle comparison is the per-block corpus, not a full-sequence run"`
  (`report.rs:131-136`). A per-block corpus does exist — the `per_block_*.rs` suites described
  above — but it compares against re-derived references, not against Buildings executed output, and
  it is not wired into the tier report.
- **Tier 3** — the global row remains skipped because the report cannot represent partial external
  coverage (`report.rs:138-143`). The separate Nand, Toggle, Line, and Reliefs tests do not enter the
  report.

The scoped Nand fixture retains two byte-identical raw OMC runs, one semantic And control, strict
raw-to-canonical projection, and the exact facade comparison. Toggle retains two byte-identical raw
runs, a one-token Latch control, and the same class of projection and facade checks at explicit event
instants. Both are native `linux/arm64` cases.

Line retains two repeat-identical native runs on each of `linux/arm64` and `linux/amd64`, with both
platform manifests and configs under the shared OCI index. Its canonical bytes match across the two
architectures. Four closed views drive the public facade at the emitted timestamp bits and compare
OpenModelica and engine outputs against an independent 40-cell expected-bit table. Each architecture
retains keep-first canonical output and inspection metadata. The structural sentinel executes the
explicit keep-first path again from retained raw input, compares both artifacts byte-for-byte, and
derives input-schedule mismatches at rows 2, 4, 6, and 8. This projection control is not a
facade-comparator control, and this stateless block can remain green when given internally consistent
pre-event rows. The external limit-flag change, swapped output mapping, and four arithmetic reference
mutants fail through the facade comparator at pinned rows. This proves only the fixed finite matrix;
raw cross-architecture bytes,
other Line inputs, non-finite values, signed zero, subnormals, and solver behavior remain outside it.

Reliefs also retains two repeat-identical native runs on each architecture and compares one strict
keep-last canonical table across them. The seven retained rows are the initial state followed by the
first complete five-input tuple change. The facade drives those tuples at the emitted timestamp bits
and reads only the declared `yOutDam` and `yRetDam` roots; topology checks bind those roots to the
internal Min and Max drivers. All 14 output cells are compared exactly against an independent bit
table. A parameter-only `uOutDamMax` change, swapped root mapping, a nonexistent root, keep-first
projection, and inconsistent final limits exercise separate failure or overwrite paths. The final
limit control fixes all 21 raw input tuples plus the seven selected tuples, timestamps, and source
rows before checking the overwritten outputs. This is one composed G36 leaf at one parameterization,
with no claim about other G36 classes or parameters.

Each native architecture record binds every checkout file used through native artifact publication,
including workflow, sandbox helpers, OCI metadata, wrappers, and canonicalizer tool inputs. Assembly
and final-manifest generation happen after native publication, so their scripts are bound by the
final artifact manifest rather than represented as native generator inputs.

Reliefs records one immutable generation contract in
`crates/oce-cxf/tests/open_modelica_reliefs_reference/generation-revision.json`. Its revision is the
exact checkout observed while the candidate native artifacts ran, not a reachability requirement.
During generation, each record must equal checkout `HEAD`. The candidate assembler requires both
native records to share that observation and the complete generator-input digest map, verifies the
current exact input bytes, and emits the candidate contract and manifest together. Retained
validation uses those committed bytes and does not inspect Git history. Zero, stale, unrelated, or
rehash-substituted observations still fail the fixed contract.

The Line and Reliefs workflows install Rust 1.97.1 and Python 3.13.7 for host-side artifact
processing and record compiler, Cargo, Python, and architecture identities in each native log. MSL
materialization uses `git archive` with the explicit `Modelica/package.mo -export-subst` attribute
override. For the pinned source, both the committed and materialized `Modelica/package.mo` SHA-256 values are
`c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191`; separate fields preserve the
boundary even though the bytes are equal. Buildings uses no local attribute override, and its
committed and materialized file hashes are also recorded separately.

All four regeneration paths disable container networking. The retained Line and Reliefs workflows
are manual only after evidence capture; normal CI validates committed evidence and does not run
Docker. No Dymola, Spawn, FMI, whole-sequence, or Integer external case exists. These four cases
cannot make the engine-wide report pass.

The Reliefs manual workflow produces and verifies one candidate native artifact per architecture,
then runs the same two-architecture assembler used for ratification and uploads its candidate
manifest and contract. Its green status proves that those workflow outputs enter the assembler; it
does not say that the committed fixture is fresh. Admission still requires committing the emitted
contract and assembled evidence, followed by retained-graph validation. Repository Python
entrypoints disable bytecode writes so ignored `__pycache__` files cannot pollute a generation
checkout. That Python validator is POSIX-only; the equivalent Rust validator remains the
cross-platform retained-evidence check and uses handle metadata on Windows.

Regeneration assumes a trusted host account, checkout, Docker client, Git and shell tools, and
executable search path. Rust and Python artifact tools are pinned and recorded; that does not turn
the rest of the host into sandbox inputs. The recorded sandbox limits the OpenModelica container;
it does not defend against another process running as the invoking user.

### Event timing against OpenModelica

Five classes apply an event on a Boolean input: `Integers.OnCounter`, `Discrete.TriggeredSampler`,
`Discrete.TriggeredMax`, `Reals.IntegratorWithReset` and `Reals.PIDWithReset`. The
[execution profile](execution-profile.md#events-on-a-boolean-input) says none fires at frame 0
for a trigger already true and each shows its event at the frame where its condition rises (issue
#768). Every other reference for that rule (hand derivations, golden-gen, the Building 33
derivation) implements the engine's discretisation; OpenModelica does not.

**What ran.** A pilot model per class (`tools/openmodelica-event-reference/events/`) drives two
instances at 1 s frames from tables with constant segments, so each input is held as the engine
holds it: the trigger true from the initial instant, falling and rising again, and the control,
the trigger false at the initial instant and rising at frame 1. Inputs change at the trigger's
rising frames too (the sampled value, the reset value, the PI's error), so the comparison sees
which frame's inputs an event takes. OpenModelica 1.25.1 ran each pilot once, in the
digest-pinned container the other cases use, against the same Buildings and MSL commits, under
cvode at tolerance 1e-6; `canonicalize.py` keeps, at each frame instant, the last row written
within 1e-6 s of it, the values after any event there. `regenerate.sh` repeats the run; a
provenance record per class binds the pilot, the raw result and both tables by SHA-256.

**What it shows.** At all ten frames of all ten scenarios the engine, driven through the facade
with the tables' inputs, equals OpenModelica exactly
(`crates/oce-conformance/tests/open_modelica_event_triggers.rs`): every output is exact in both
tools, because the integrator's input is 0 and the PI's integral time is 1e20 s, so its output
is the proportional part plus the held or reset integral. OpenModelica fires no `when` at the
initial instant: the one row it writes at t = 0 holds the initial-equation values. A
recurrence of each class under the rule equals OpenModelica's output at every frame; the two
readings the rule replaced, an event at frame 0 and an event shown one frame late, each differ
from it in every class, and a reference rewritten under either fails the facade comparison at the
first frame it changes.

A sixth pilot integrates a nonzero, stepped input through `IntegratorWithReset` with two resets
(`open_modelica_integrator_ramp.rs`). At its uniform 1 s frames, with inputs held between them, the
engine trails OpenModelica by exactly one frame of the input held at the last reset or at the start
(0.5, then 2), equals it at each reset frame, and on 1/16, 1/32 and 1/64 s frames the offset halves
with the step. That one-frame lag is the same after a reset as after the start because the reset
frame takes no Euler step (decision 4 of the [reset timing note](design/reset-timing.md)); a reset
frame that took its step fails the test. OpenModelica shows the lag is uniform at a uniform step; it
does not show the convention is the only right one. At uneven steps the difference after the reset
(or start) at frame `r` is `k * [sum over j of u(j) * (dt(j) - dt(j+1)) - u(r) * dt(r+1)]`, the sum
over the frames after `r`, not one frame of a single input.

A seventh pilot counts, with `OnCounter`, the `Change` pulses of a signal that changes on
consecutive frames (`open_modelica_event_pulses.rs`). OpenModelica counts each change (the count
reaches 9); so does the engine since the settled-value rule of issue #800. Before it the engine
counted one per run of consecutive changes (to 6): `DVG-000002`, now resolved, keeps that count as
its evidence. **The test runs nightly, not per PR:** it is in `oce-conformance`'s `conformance`
binary, which no per-PR job runs.

### Pulses on event inputs against OpenModelica

Seven pilots in nine runs (`tools/openmodelica-event-reference/events/PulseConsumersPilot.mo`,
`SampleTriggerPilot.mo`, `SampleTriggerOffGridPilot.mo`, and from the review of GitHub #805
`InitNegatedPilot.mo` and `InitMixturesPilot.mo`, each with an off-grid copy, and
`LatchClearPilot.mo`, and from its second review `MixedFallbackPilot.mo`; `regenerate.sh ...
pulses`) run the
**pinned Buildings classes**, not transcriptions, in the same container, at 1 s frames with each
input held, and `crates/oce-conformance/tests/open_modelica_pulse_events.rs` drives the engine with
the same blocks wired the same way (issue #800, design note
[Event inputs fed by one-frame pulses](design/pulse-event-inputs.md)). Every compared signal equals
OpenModelica at every frame, bit for bit, except a timer within 1e-6 (OpenModelica's table changes
land about 1e-10 s after each frame) and a mean within 1e-12 (OpenModelica prints 16 significant
digits), and ten registered known divergences:

| Shape | Frame 0 | Agreement |
| --- | --- | --- |
| `Change(u)`, changes at 2, 3, 5, 7, 8, 9 s, into `TriggeredSampler`, `TriggeredMax`, `TriggeredMovingMean`, `IntegratorWithReset`, `PIDWithReset`, `Latch.u`, `Toggle.u`, `TimerAccumulating.reset` | no event | every frame; each consecutive change is an event |
| `Not(Change(u))`, `Or(Change(u), level)` into `OnCounter` | no event | every frame (OpenModelica 9 and 7 counts) |
| `Latch`, `Toggle` behind `Change(x)`, `x` true from the start | `y = 1` (the initial equation reads the initialisation pulse) | every frame |
| `SampleTrigger` at 1 and 2 s from an instant at t = 0 into `OnCounter`, `TriggeredSampler`, `TriggeredMax` (negative `u`), `IntegratorWithReset`, `PIDWithReset`, `TimerAccumulating.reset`, `Toggle`, `Latch` | the instant fires: a sample, `max(u, abs(u))`, a reset, a toggle | every frame |
| `TriggeredMovingMean(n = 3)` behind `SampleTrigger(1 s)` | samples twice: `initial()` and the instant | every frame (2.8333 at 1 s) |
| G36 `TrimAndRespond`'s hold latch: `Latch(u = SampleTrigger(1 s), clr = And(Not(hold), SampleTrigger(1 s)))`, hold true at 3 and 4 s | `y = 0` | every frame: latched at 3 and 4 s, where the rising edge never fired |
| `Not(SampleTrigger(1 s))` into `TriggeredSampler` | no event | never samples, as in OpenModelica |
| `SampleTrigger(1 s, shift 0.5 s)` into `OnCounter`; `SampleTrigger(1.4 s)` into `OnCounter` | — | every frame |
| `SampleTrigger(1 s)` from an off-grid start (frames at 0.25 + k s) into `OnCounter`, `TriggeredSampler`, `Latch` | no instant, no event, `Latch` false | every frame |
| `Or(Change(u), Pre(b))` into `OnCounter` (a fallback cone) | — | `DVG-000003`: 4 against 6 counts from 8 s |
| `SampleTrigger(0.5 s)` into `OnCounter` (two instants a frame) | — | `DVG-000004`: 10 against 19 |
| `SampleTrigger(1.4 s)` into `TriggeredSampler` | — | `DVG-000005`: the frame's `u`, not the instant's, at 2, 5 and 9 s |
| `Pre(SampleTrigger(1 s))` into `OnCounter`, off-grid start | — | `DVG-000006`: 1 against 9 |
| `Not(Change(x))`, `Not(Edge(x))` with `x(0)` true and false, `Not(SampleTrigger(1 s))`, on and off the grid, into every consumer | `x(0) = true`: fires (count 1, a sample, a reset, `Latch` and `Toggle` true); `x(0) = false` and `Not(SampleTrigger)`: nothing | every frame, but `IntegratorWithReset` and `PIDWithReset` behind `Not(SampleTrigger)` (`DVG-000010`: OpenModelica reinitialises when `sample()` falls after each instant) and two timers off the grid (`DVG-000011`: a reset between frames) |
| `Or`, `Xor` of `Change(x1)` (an initialisation pulse) and `SampleTrigger(1 s)`; `And(Not(Change(x1)), SampleTrigger)`; `And(Not(Change(x1)), Not(SampleTrigger))`; a three-leaf mixture; `Or(Not(Change(x1)), level)`; on and off the grid, into `OnCounter`, `Latch`, `Toggle` | `i -> w -> s` | every frame |
| `Or(Not(Change(x1)), Pre(false))` at frame 0 (a fallback cone) | OpenModelica fires | `DVG-000007`: the engine does not (frame 0 only) |
| `Latch`, `Toggle` with `u = Not(Change(a))` and `clr = Change(a)` or `Change(b)`, and `u = Change(a)`, `clr = Not(Edge(a))` | — | every frame |
| `Latch`, `Toggle` with `u = SampleTrigger(2 s)`, `clr = Not(SampleTrigger(2 s))` | — | `DVG-000008`: OpenModelica keeps the latch set, the engine clears it a frame after each instant |
| Two-input consumers with one input on a fallback cone (`Latch(u = Change(a), clr = Pre(Not(Change(a))))`, `OnCounter(trigger = Change(b), reset = Edge(Change(b)))`, `Toggle(u = Not(SampleTrigger(3 s, shift 0.5)), clr = Pre(b))`) | — | every frame for the first two (the block keeps the rising edge on both inputs); the `Toggle` `DVG-000012` at frame 1, the `Pre` a frame late, as on every engine |
| `BooleanToReal(Change(a))` as the data input of `TriggeredSampler`, `TriggeredMax`, `TriggeredMovingMean`, `IntegratorWithReset`, `PIDWithReset`, trigger `Not(Change(a))` | — | `TriggeredMax`, `TriggeredMovingMean` every frame; the others `DVG-000009` (the block keeps the rising edge, one frame after OpenModelica's second iteration) |

They are the limits the owner ruled on (decision 2): a cone with a `Pre` keeps the rising edge,
several instants in one frame fire once (with a once-only warning), every event inside a frame
interval is shown at the frame with the frame's values, and, from the review of GitHub #805, a
negated sample on a `Latch`'s `clr`, a data input carrying a converted pulse, and OpenModelica's
reinit on a negated sample's fall. Every pilot column was also run on the engine before this
change: none agrees with OpenModelica there and not here. They are documented in the
[execution profile](execution-profile.md#conformance-boundary). These are single runs on
`linux/arm64`; the engine side is checked bit for bit, the OpenModelica side by digest.

**What these runs do not show:** `PIDWithReset`'s integration (the Building 33 frames below
compare it), `PIDWithReset` with a derivative, other parameters, another tool or tool version, and
repeat-identical or cross-architecture runs: each pilot ran once, on `linux/arm64`, and none of
the four cases' OCI, projection-mutation or two-run checks apply. Two regenerations wrote
byte-identical raw results, tables, logs (omc's timing lines are dropped) and records.

### Commissioning on recorded data: the Building 33 day

The commissioning verifier (OBC §12; [design note](design/commissioning.md)) has one worked
example on recorded data: OBC §12.6's cooling-coil valve sequence against one day of LBNL
Building 33's trends (issue #755). Its receipt is a golden
(`crates/leverage/oce-verify/tests/fixtures/commissioning/building33/receipts/day.receipt.json`),
and `oce verify --commission` prints it byte for byte.

**What ran.** The sequence is a `CoolingCoilValve` **authored from the vendored model, not
vendored**: upstream's modelica-json translation does not load (issue #760), so by the owner's
ruling the example runs from a transcription of the vendored `.mo`, written in the oce text form
and emitted with `oce emit`. **The verdict is about that transcription, not about upstream's
translation.** The trends are the six vendored files, read in place. The spec is the design
note's worked spec unchanged: the window 0 to 86,400 s from 2018-06-07 00:00 PDT, the funnel
built around the engine series, no parameter edit. Of its tolerances, `atolx` 1 s and `rtoly`
0.01 are §12.6's (Fig. 12.12's caption: 1 s in time, 1 % in y); `atoly` 0, `rtolx` 0 and the 5 s
comparison grid are the design note's choices, 5 s being the trend interval.

| Output | Status | Compared | Masked | Uncovered | Worst exceedance | First exceedance | Time outside the band |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `yCooVal` | **fail** | 17,281 | 0 | 0 | 0.020864709745257892 at 65,965 s | 61,770 s | 4,085 s (817 instants, 61,770 to 67,755 s) |

The band is 1 s wide and 0.01 high everywhere (`half_width` 1.0, `half_height` 0.01): 1 % of the
engine series' range, which spans 0 to 1 on this day. Outside 61,770 to 67,755 s, about 17:09 to
18:49 local time, the trended valve is inside the band at every instant. The receipt's `frames`
block (`executed` 2, `substituted` 75,566, `refused` 15) counts the whole replay, every frame the
trend files hold to 493,800 s, not the compared day; the 15 refused frames lie at 485,400 to
493,800 s, after the supply-air trend ends. Its `model` field names upstream's example model, as
the spec writes it; only its `content_id` identifies the transcription that ran.

**What the figures rest on.** Four checks, each from outside the authored file
([fixture README](../crates/leverage/oce-verify/tests/fixtures/commissioning/building33/README.md)):

- *Structure*: every parameter value, connector (type, unit, quantity, display unit, bounds),
  block class, modifier and the 21 connections of the emitted CXF and the loaded engine equal
  what a script read from the `.mo` with its own parser.
- *Behaviour*: over 78 synthetic frames that take every branch the valve signal can show (PI
  active, its upper saturation under the supply-air high limit and its lower saturation with the
  anti-windup acting, the line clamped below, the outdoor-air and fan-feedback hysteresis, the
  fan status, each reset, and a one-frame disable whose re-enable, on the next frame, is a second
  `Change` event and a second reset), the engine's valve signal equals a derivation written from the `.mo`
  and the vendored CDL block sources, bit for bit, under the engine's discrete-time convention.
  One modifier cannot show in the valve signal for any input, the line's upper clamp
  (`limitAbove`), because the PI's output never exceeds `yMax = 1`, the line's `f2`; only the
  structural check sees it. Eleven mutants of the transcription (a swapped connection, a wrong
  gain, a dropped limit, a lower PI limit below zero, reverse acting, …) are each caught by the
  derivation; the day's receipt alone catches five, because on this day the high limit never
  cuts the signal and the enable is on throughout.
- *The continuous model*: OpenModelica 1.25.1, in the digest-pinned container the other
  OpenModelica cases use, simulated upstream's own `ControlsVerification_CoolingCoilValve`,
  unmodified at the pin, over the same day (cvode, tolerance 1e-6, trends interpolated linearly).
  The engine's signal stays within **0.0034** of it at every 5 s instant (largest at 67,375 s),
  well inside the band. This is one run on one architecture (`linux/arm64`), compared within a
  bound, and none of the exact-bit, repeat-identical or two-architecture properties of the four
  cases above is claimed for it.
- *The continuous model at the resets* (issue #787): the same container ran upstream's
  `CoolingCoilValve` on the 78 synthetic frames, each frame's inputs held. It reinitialises the
  integrator at all nine changes of the enable, **and so does the engine**; at those the engine
  equals it to 1e-12, as it does wherever the enable is off or the line cuts the PI (32 frames).
  The engine before issue #768 was 0.09, 0.045, 0.018 and 0.018 away at the four re-enables. The
  ninth, 382 s, is a re-enable one frame after a disable: `Change` is true at both frames, and
  under the settled-value rule of issue #800 each is an event. Rerun with the supply air at 290 K
  from 382 s, the engine reads 0, 0 and 0.015 at 382, 387 and 392 s against OpenModelica's 0,
  0.015 and 0.030: the reset, then the one frame of integral action every engine reset trails by.
  Before the rule it read 0.036, 0.051 and 0.066 (`DVG-000001`, resolved). After each
  re-enable OpenModelica leads by one frame of integral action, 0.009, as after the start: the
  reset frame's skipped Euler step. At the frames that carry the integral the difference reaches
  0.0277 (187 s), above the day's 0.01 band; that figure is a pin, not a bound, and at those
  uneven 2 to 13 s steps the engine integrates each frame's error over the interval before it,
  error beyond forward Euler's that refining a uniform grid cannot separate. On a uniform grid of
  1/16, 1/32 and 1/64 s the largest difference halves with each halving of the step, to 7.2e-5,
  always at 187 s, and every reset is exact (the 382 s reset included, the two frames being apart
  there). The stated bound #787 asked for holds only on those grids, and it is a sanity bound;
  the halving and its location are the tests.
- *The envelope*: the day's five funnel curves, `errors.csv` included, print byte for byte as
  the pinned `pyfunnel==2.1.1` printed them for the same two series on x86_64 Linux, the
  Building 33 day of the funnel differential (decision 13).

**What OBC shows for the same example.** OBC §12.6's text says only that the computed and the trended signals
"show good agreement, which is also demonstrated by verifying the time series with the funnel
software", under the same tolerances. Its Fig. 12.12, as we read the published SVG
(`funnel_valve_control.svg`, read 2026-11-22), plots the error over about 0 to 315,700 s: it is
zero everywhere except about
62,000 to 68,000 s on the first day, where it reaches about 0.02 (read from the plotted path; the
axis tops out at 0.02). Our verdict differs from OBC's prose, not from OBC's figure. Run over
OBC's whole window, 0 to 315,710 s (upstream's `StopTime`), the same spec gives the same worst
instant and 4,120 s outside the band, 35 s more than the first day alone. And OpenModelica's run of
upstream's own model, put through the same funnel against the same trend, leaves the band from
the same first instant, 61,770 s, to 67,755 s (834 instants, worst 0.0234 at 67,335 s). We
attribute the `fail` to the recorded controller's departure from the CDL sequence that afternoon,
which §12.6 puts down to the ALC controller's discrete time step and its proprietary anti-windup,
and not to the transcription. The basis: the transcription is at most 0.0034 from OpenModelica's
run of upstream's model, which cannot account for an excess of 0.02 beyond a band 0.01 high, and
that OpenModelica run fails over the same stretch. No tolerance was tuned; with the trend as the
funnel's reference instead, the day fails too (worst 0.0210 at 65,965 s).

**What an honest reader may conclude:**

- the transcribed `CoolingCoilValve`, run by this engine, reproduces upstream's model as
  OpenModelica runs it within 0.0034 over this day, and takes every branch its valve signal can
  show as the `.mo` and the CDL sources say, under the engine's discrete-time convention: each
  engine reset shows at the frame of its event, where OpenModelica's run of upstream's model on
  the same frames agrees to 1e-12, a re-enable one frame after a disable included (issue #800);
- under OBC §12.6's own tolerances, LBNL Building 33's recorded valve signal on 2018-06-07 leaves
  the funnel for about 68 minutes of the late afternoon, by at most 0.021, and is inside it the
  rest of the day, which is what OBC's Fig. 12.12 shows as we read the published SVG.

**What they may not conclude:**

- anything about upstream's modelica-json translation, which does not load;
- anything about another building, another day, another controller or another sequence: this is
  recorded data from one air handler on one day;
- that the tolerances are right for commissioning in general: 1 s and 1 % are §12.6's choice for
  this example, and a band 1 % high is tight for a signal a controller computes with a different
  integrator;
- that the fan feedback hysteresis is checked on this data. `uFanFee` declares no unit, so by the
  owner's ruling the mapping gives it none on either side and the raw percent (60.9 to 72.5)
  reaches a port whose thresholds are 0.05 and 0.15, where upstream's model scales it by 0.01
  first. The hysteresis is on either way here; on a feedback between 0.15 % and 15 % the two
  would disagree;
- that the outdoor-air cut-off is checked on this data. Upstream converts its 2 degF hysteresis
  delta as an absolute temperature (`TOutDelta` 256.48 K), so the cut-off's lower threshold is
  26.67 K and, once on, it never turns off; the transcription keeps upstream's value. The day's
  outdoor air (48.89 to 65.68 degF) starts above the 50 degF cut-off and never falls below 48 degF,
  so the enable is on all day under upstream's delta and under a corrected one alike, and a test
  finds every figure the same with the corrected threshold;
- that the indicator was exercised: `cooValSta.uFanSta` masked nothing, because the fan was
  enabled all day (masking keeps OBC §12.7.2's zero fill, #745).

**What CI executes.** The day runs in the leverage lane with no slow-test override: about 0.8 s
and a 36 MB peak for one run of the library in a debug build, about 0.7 s and 36 MB for the
`oce` binary, and 0.1 s and 33 MB for a release `oce`, which prints the same receipt bytes. The
replay runs every frame the trend files hold, 75,583 frames to 493,800 s, and compares only the
window's instants; over OBC's whole window, 0 to 315,710 s, the debug binary takes 1.2 s and
44 MB. In a parallel run of both crates' suites (debug), a test that runs the day once takes
0.7 to 1.2 s, and the longest, five mutants of a day each, about 6 s, against the `ci` profile's
60 s slow-test mark. No CI job runs Python, Docker, pyfunnel or
OpenModelica: the derivations, the capture and the OpenModelica run are scripts checked in beside
their outputs, and the tests read the outputs.

---

## Is the thing that judges it independent of it?

The answer differs by layer.

**Tier-2: no, and it never claimed to be.** The reference *is* prior engine output. Its provenance
records say so in a field a script can read.

**Tier-A: yes at the code level, mechanically enforced, with one honest caveat.** The firewall
guarantees the generator cannot import the implementation under test, so a Tier-A golden can never
be a replay of `oce-blocks`. What the firewall cannot guarantee is *derivational* independence.
`tools/golden-gen/src/main.rs:9-11` states the caveat itself: some exact oracles share a pinned math
kernel or restate the same documented recurrence the engine uses. Where that is true, a Tier-A pass
is evidence about plumbing and transcription of a shared formula — not an independent check that the
formula is right. A mechanical shared-kernel detector is filed as follow-up work and does not exist
today, so treat the boundary between "independently derived" and "independently transcribed" as
un-audited per class.

**The structural oracle: yes.**
`modelica-json` is an LBL tool, not one of ours, translating upstream `.mo` sources this project did
not author, at a pinned commit whose bytes are gated. It is also the narrowest claim: structure
only, over 31 of 46 fixtures.

**The scoped OpenModelica cases: yes at execution and source boundaries.** A digest-pinned image
executes the pinned Buildings `Nand`, `Toggle`, `Line`, and G36 `Reliefs` classes with inputs supplied
by MSL sources. The wrappers contain no expected output. Each comparison remains a discrepancy
detector rather than an oracle verdict: analytical evidence comes first in adjudication, and a
mismatch cannot change a golden, tolerance, or report status. Their scope is four Boolean pairs for
Nand, one event schedule for Toggle, one finite four-mode/five-region matrix for Line, and one
seven-state exact-bit case for the composed Reliefs leaf.

One more thing an evaluator should weigh: independence of the oracle does not make the *comparison*
independent of when it runs. See the next section.

---

## Will it tell you which checks it is not running?

Yes, and it does so in the place where it is hardest to ignore — the end of every gate run.

### The gate prints what it does not cover

`.agents/gate.sh` finishes by printing a literal block headed `NOT COVERED BY THIS SCRIPT — a green
run here does not prove these pass`, listing:

- **The cross-architecture determinism matrix**: per PR, and again nightly, four native cells,
  x86_64 on `ubuntu-latest` and aarch64 on `ubuntu-24.04-arm`, each in debug and release codegen,
  with a fan-in that compares across architectures and debug against release. One machine cannot
  reproduce it; CI is the only place it runs. In its `determinism` and `full` modes the script
  does run the subset under release codegen on the local host: one cell of the four, and no
  comparison.
- **The strict-bit raw-output matrix** over four native cells on every PR (both architectures,
  debug and release), two captures each, and its
  eight-capture comparison. Local synthetic controls and macOS observations are not native Linux
  evidence.
- **`cargo deny check advisories`.** It needs network access and a writable advisory database, so it
  runs in `advisories.yml` and in the release gate's cargo-deny job instead.
- **That these commands still match `pr-gate.yml`.** Nothing verifies that mechanically. An attempt was
  made and withdrawn; the dormant `.github/workflows/ci.yml:293-321` records why — every design either compared
  argv strings, which `RUSTFLAGS=--cap-lints=allow` leaves byte-identical while neutering clippy, or
  reimplemented enough of `if:` / `needs:` / matrix semantics to become its own untested gate. CI
  does *execute* the script, but only after a merge: `gate (full)` in the release gate, nightly on
  the `development` tip and on release PRs. No per-PR job runs it, so before a merge the author's
  own run is the only one; the script says outright that the CI run is coverage, not parity.

Every run but a `full` one adds further lines: it did not run the workspace test suite or the
doctests, because the per-PR gate does not run them either, and a `docs`, `leverage` or `light`
run ran nothing under release codegen. The report also names which row of the
[lane-matched local proof](ci-and-the-gate.md#the-local-proof) the run was.

### The CI split, read in the dangerous direction

CI is dev-light and release-heavy. The per-PR gate into `development` runs the state-determinism
subset for **`oce-api`, `oce-blocks`, and `oce-expr`** — the `determinism-matrix` job
and the corresponding steps inside the gate script, in four native cells (x86_64 on
`ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, each in debug and release codegen). The cells emit
populated portable and target-bound engine-state vectors. The `determinism-compare` fan-in requires
the portable bytes to match across architectures and the
target-bound bytes to differ, and both to match across debug and release codegen within an
architecture. Each architecture then parses the other's target-bound snapshot and
requires `restore_state` to return the target-domain refusal.

The release-codegen cells of that matrix and the debug-against-release comparisons are per-PR
evidence again (issue #556). From issue #502 they ran only after the merge, because a release
cell took 11 to 18 minutes; once the integration tests of `oce-api`, `oce-author`, `oce-cxf` and
`oce-conformance` were built as a few binaries each (GitHub #546, #551, #553, #554) a release
cell took about three minutes and they came back. The nightly release gate runs the same four
cells again. The strict-bit matrix is separate and has run under release codegen on every pull
request throughout, in two of its four cells.

Separately, the scoped **`oce-conformance` strict-bit subset runs per-PR**: `strict_bits` and the
four affected per-block suite binaries, in four native Linux cells (x86_64 and aarch64, debug and
release), two independent
process captures per cell and a fail-closed eight-capture comparison. That [per-PR native matrix](strict-bit-evidence.md)
covers the 21 formerly banded Real cases; Linux exact comparison is not a macOS qualification.

The remainder of `oce-conformance` still follows release/full-gate coverage. The complete set of
420 reference comparisons — 399 existing exact plus 21 exact on qualified Linux, unchanged 1e-12
aligned elsewhere — runs in the full local gate and release workflow (nightly against
`development`, release PRs, pushes to `main`, and manual dispatch; not on each merge since
issue #545). Two `oce-cxf` input-hygiene audits also run per-PR:
fixture port order and the structural oracle, including the vendored-tree and Tier-2 digest guards.
**A change outside these named subsets can show every check green without running its own tests.**
A green PR is not evidence that the rest of a changed crate passed.

One more disclosure worth knowing before you read a PR's checks: `CI OK` is the single status
that summarizes the per-PR gate. Confirm it reported, not merely that nothing is red.

Full detail is in [CI and the gate](ci-and-the-gate.md).

### Coverage, stated as a fraction rather than a claim

Per-block Tier-A goldens cover **132 of the 133 CDL classes** in the registry. The registry pins 136
entries — 133 CDL classes plus 3 reserved internal lowering classes
(`crates/oce-blocks/src/registry/manifest_tests.rs:21`) — and the golden tree contains exactly 132
distinct CDL block `class_path` values, once the two non-block fold-time records are set aside.

The one without a per-block oracle, and why:

| Class | Status |
| --- | --- |
| `CDL.Logical.Sources.Constant` | indirect G36-sequence evidence only |

Two of the covered classes carry a narrower golden than an output-port source check:

| Class | What its golden pins |
| --- | --- |
| `CDL.Logical.Pre` | the HostTick v1 one-transition projection (first tick emits `pre_u_start`, later ticks the previous input); no Modelica event-iteration oracle |
| `CDL.Utilities.Assert` | a `warning` observation column (one Warning record per evaluation with `u` false) compared against completed-frame diagnostics; it has no output port |

"Indirect G36-sequence evidence" means the class is exercised inside sequences whose outputs are
oracle-compared, so a gross error would likely surface — but nothing pins that class's behavior in
isolation against Modelica semantics.
Which classes exist and what "supported" means for sequences is in
[CDL coverage](cdl-coverage.md).

---

## What this adds up to

If you are evaluating this engine, the defensible summary is:

- Determinism is **tested**, on two architectures, per PR, against committed goldens, in debug
  and release codegen, with debug compared against release.
- Agreement with CDL / Buildings **source semantics** is bounded by 398 oracle comparisons: 377
  existing exact plus 21 exact on qualified Linux and at the unchanged 1e-12 aligned band on
  unqualified targets. The [retained strict-bit evidence](strict-bit-evidence.md) applies only to
  the pinned corpus/toolchain. References behind the code-dependency firewall cover 132 of 133
  classes; the scoped strict-bit subset runs per-PR, while the remainder needs the release/full gate.
- HostTick v1 agreement is checked separately by 22 exact signals: 20 G36 signals across the
  three `Pre`-dependent fixtures and two standalone `CDL.Logical.Pre` goldens. Those are profile checks, not Modelica event-iteration evidence.
- Fixture fidelity is bounded structurally against an independent LBL translation of upstream
  sources, per PR, for 31 of 46 fixtures.
- Agreement with an **executed Buildings implementation** is bounded for one exhaustive
  `CDL.Logical.Nand` Boolean case, one stateful `CDL.Logical.Toggle` schedule, one finite
  `CDL.Reals.Line` matrix, and one composed G36 Reliefs leaf. Global Tier 3 remains skipped.

The gate will tell you which broader checks it skipped. The scoped OMC artifacts do not change those
disclosures.

See also: [Testing standard](../TESTING.md) for the bar every change is held to, and
[CI and the gate](ci-and-the-gate.md) for what runs when.
