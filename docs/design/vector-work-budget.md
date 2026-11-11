# Bounding a vector run's work

**Design note for issue #561, written before any code, and ruled by the owner on 2026-11-11 (see
[Rulings](#rulings)).** It bounds the total work
one `run_vectors` call does: the instants it compares, the engine loads it makes at bind and while
scoring mutants, and the frames it runs, weighted by the size of the document that runs them. Part
4 (the `oce verify` verb, #522) is blocked on it, because the verb puts the run in front of files
fetched from elsewhere. The [vectors note](vectors.md) is the format and run this note bounds. A
test reads its text, so its amendment ships with the implementing PR, which closes #561; this note
stays documentation only. Nothing here is built.

## What is bounded today, and what is not

`VerifyLimits` bounds each dimension of a set on its own: 4,096 scenarios, 4,096 expectations a
scenario, 16,777,216 ticks, 256 mutants, 64 MiB of mutant bytes together, and 8 MiB a document
(`oce_api::MAX_CXF_BYTES`). One product is bounded: `max_total_frames`, 16,777,216 frames counted
`(N + 1) * scenarios * (1 + mutants)`. Every other product of those limits is unbounded, and three
are large.

With `S` the subject's bytes, `M` the mutants' bytes together, `D = S + M`, `m` the listed mutants,
`s` the scenarios, `N + 1` the ticks, `w` the scenarios with a non-empty effective edit list, `e`
the distinct such lists, and `I_j` the instants scenario `j` compares (the ticks inside each of its
windows, plus one per diagnostic expectation), one run does this:

| Work | Count | Bounded by |
| --- | --- | --- |
| Subject loads at bind | `1 + w`: the bind load, then one trial per scenario with edits | nothing beyond `s` |
| Mutant loads at bind | `m * (1 + e)`: one validation load, then one trial per distinct list | nothing beyond `m`, `e` |
| Subject loads in the run | `s`: a fresh engine per scenario | nothing beyond `s` |
| Mutant loads while scoring | at most `m * s`: each mutant until a scenario fails on it | nothing beyond `m`, `s` |
| Frames | `(N + 1) * s * (1 + m)` | `max_total_frames` |
| Compared instants | at most `(1 + m) * sum(I_j)` | nothing beyond the frame budget times 4,096 |

The frame budget counts frames, but a frame's cost grows with the document that runs it, and a load
costs in proportion to its bytes. So the frame budget does not bound time either.

## Measured costs

A throwaway probe, not committed, timed loads, frames and compared instants through `oce_api` and
`run_vectors`. It ran on an Apple M5, one thread, in the workspace's `release` profile (the CLI's
build) and its `dev` profile (the test profile). The documents were:
- the two in-repo library rules (AHU-0016, 5,985 bytes; AHU-0004, 9,946 bytes);
- three 1.4 to 2 KB test subjects;
- banks of `Reals.Hysteresis` blocks fed from one input, 10 to 9,902 blocks, up to 8,375,327 bytes;
- the densest packing tried: chains of `Logical.Not` with short identifiers and no whitespace, 393
  bytes a block, up to 4,002,656 bytes.

| Unit | `release` | `dev` |
| --- | --- | --- |
| A load, per document byte | 12 to 30 ns (a 6 KB rule: 94 µs; 8.4 MB: 0.25 s) | 96 to 190 ns (6 KB: 0.63 ms; 8.4 MB: 1.42 s) |
| A frame, per document byte | 0.015 to 0.022 ns (AHU-0016: 0.09 µs a frame) | 0.11 to 0.132 ns (AHU-0016: 0.72 µs) |
| A compared instant | 1.0 ns | 10.8 ns |

Five checks that the model holds:
- **Loads dominate a short clock.** 256 one-tick scenarios on an 835,333-byte bank took 250 loads'
  worth of time in `release` and 258 in `dev`, against 257 loads predicted.
- **The load formula is exact.** 8 mutants and 16 scenarios of 31 ticks predict 153 loads; those
  loads were 96 % (`dev`) to 98 % (`release`) of the run's time.
- **Bytes are a fair proxy for frame cost.** The rule, the banks and the chains fall within 1.5x
  of each other per byte-frame.
- **A load has no fixed floor worth counting.** Load cost is linear in bytes down to the 1.4 KB
  subjects (15 ns a byte in `release`), so counting loads adds nothing to counting their bytes.
- **No small document blows up its frames.** A `Logical.MultiAnd` whose `nin` (2, 1,000 or
  1,000,000) exceeds the one port it writes did not load, so an array-size parameter does not
  multiply a frame's work beyond what the document writes. The facade also caps any document at
  4,194,304 state words.

## The worst case today

Each case below is reachable within today's limits by files far under their byte bounds: the
vector file stays under 1 MiB in each, a quarter of its 4 MiB bound. A subject that passes, so
that the mutants are scored, needs only one checked tick a scenario.

| Work | Worst case | `release` | `dev` |
| --- | --- | --- | --- |
| Instants | 4,096 whole-clock windows on 16,777,216 ticks: 6.9 x 10^10 | 69 s | 12 min |
| Load bytes | an 8 MiB subject and eight 8 MiB mutants, 4,096 one-tick scenarios each with its own edit: 72 MiB of documents, each loaded 8,193 times, 576 GiB | 5.2 h | 33 h |
| Frame bytes | 16,777,216 frames, each on an 8 MiB document: 1.4 x 10^14 | 52 min | 5.2 h |
| Frames | 16,777,216 on a 6 KB rule | 1.5 s | 12 s |

## The design

**Four budgets, each in its natural unit, each one product, each arithmetic on what binding
already knows.** The frame budget stays. Three join it:

| `VerifyLimits` field | Counts | Default | Refusal |
| --- | --- | --- | --- |
| `max_total_frames` (kept) | `(N + 1) * s * (1 + m)` | 2^24 = 16,777,216 | `too-many-frames` |
| `max_total_instants` | `(1 + m) * sum(I_j)` | 2^28 = 268,435,456 | `too-many-instants` |
| `max_total_load_bytes` | `D * (1 + e + s)` | 2^30 = 1,073,741,824 | `too-many-load-bytes` |
| `max_total_frame_bytes` | `(N + 1) * s * D` | 2^40 = 1,099,511,627,776 | `too-many-frame-bytes` |

`max_total_instants` is the issue's own proposal, at the default it suggested.

- **Every count is an upper bound.** Like the frame budget, each counts every mutant as if it ran
  every scenario, and every window as if no frame were refused. A run inside its budgets does
  exactly the work it does today.
- **Overflow.** The products are computed in `u128`, so none overflows: the largest, frame bytes,
  is below 2^24 ticks x 2^12 scenarios x 2^64 bytes.
- **`M` is the bytes supplied.** It counts every mutant's bytes as the caller hands them in. A path
  the set does not list is refused later as `mutant-unlisted`, and the 64 MiB mutant byte bound is
  checked later with the other mutant checks.
- **Load bytes, term by term:** `D * (1 + e + s)` is the subject's and each mutant's bind load,
  their trials and their scenario runs. It assumes the subject's trials are made once per distinct
  list, as the mutants' already are (ruling 7). Before that change, the count was
  `S * (1 + w + s) + M * (1 + e + s)`.

**Where they are checked: one budget step at bind, before any load but the subject's first.**
Distinct edit lists are counted on resolved parameter paths, so `e` exists only after the subject's
bind load and its names resolve. That one load is already bounded by the 8 MiB document limit,
0.25 s in `release`. The bind order becomes:
1. load the subject;
2. check the boundary ports, then the names and values (parameter keys included);
3. check the budgets, in table order (frames, instants, load bytes, frame bytes), the first one
   exceeded winning;
4. run the parameter trials;
5. validate the mutants.

This moves the frame budget ahead of the subject's trials. Since the owner's ruling of 2026-11-11 it
runs after them and before the mutants. An over-budget set whose edits the subject also refuses
then gets `too-many-frames`, not `param-refused` (ruling 6).

**The refusal.** Each budget is its own `VerifyErrorKind`, inserted after `TooManyFrames` in
`VERIFY_ERROR_CODES`. Its subject is `scenarios` and its detail names the count and the limit,
as the total frame budget's `too-many-frames` does today. Every one is exit 2 with no receipt.

**The receipt records nothing.** A budget never changes a receipt's bytes: a run inside its budgets
writes the receipt it would write with no budget, and a run outside them writes none. Two runs of
one subject, set and mutants under different budgets therefore share a `receipt_id` or refuse.
That is unlike the record caps, which do change the bytes (#563). The receipt already records the
frames and instants that ran (`totals.frames`, `totals.instants`).

**Raise or lower.** As decision 35 states for every limit: a host may lower any budget, to zero
included; a value above the default is read as the default, so no caller can raise one. The CLI
exposes none of them, as it exposes no limit today. Raising a default is a source change, made in
place before 1.0.0, with this note's arithmetic redone (ruling 5).

## The defaults, against the corpus and the clock

The corpus is the sets that have their rule beside them: 137 library bundles from a local clone of
`open-control-library` at `d90f63b`, plus the two in-repo copies (139 sets). None has a parameter
edit, a diagnostic expectation or a mutant. The in-repo mutant fixture
(`crates/leverage/oce-verify/tests/fixtures/AHU-0016/`) has three. "256 mutants" is the corpus's
largest set with 256 mutants the size of its rule, the most `max_mutants` allows.

| Budget | Default | Corpus max | 256 mutants of it | AHU-0016 fixture | Worst at the default, `release` / `dev` |
| --- | --- | --- | --- | --- | --- |
| Frames | 16,777,216 | 23,337 (CHW-0002) | 5,997,609 | 868 | 1.5 s / 12 s |
| Instants | 268,435,456 | 41,488 (CHW-0002) | 10,662,416 | 864 | 0.27 s / 2.9 s |
| Load bytes | 1,073,741,824 | 847,757 (HW-0011) | 217,873,549 | 204,944 | 32 s / 3.4 min |
| Frame bytes | 1,099,511,627,776 | 426,133,620 (CHW-0002) | 109,516,340,340 | 5,559,106 | 24 s / 2.4 min |

**Headroom.** The corpus's own sets sit 700x to 6,500x under each default. With 256 mutants each,
the margin is at least 2.8x on frames (already the case today), 4.9x on load bytes, 10x on frame
bytes and 25x on instants.

**The worst at a default** uses the slowest rate measured for that unit: 30 ns a byte for a load
and 0.022 ns a byte-frame in `release`, 190 ns and 0.132 ns in `dev`. **All four together** cost
at most about one minute in `release` and six in `dev`, against hours today.

**A large real sequence.** The largest sequence in the tree is the G36 cooling-only controller,
418,985 bytes.
- With 30 scenarios of one day (1,441 ticks), it fits up to 59 mutants: frame bytes is the binding
  budget, 1,441 x 30 x 418,985 x 60 = 1.09 x 10^12.
- With a week's clock (10,081 ticks), it fits 7.

That is the intent: a long clock on a large document with many mutants is exactly the hours-long
run a hostile file would ask for.

## What the budgets leave bounded elsewhere

- **Parameter passes.** Each trial and each run applies its edits inside one halt, in passes, and
  a pass that applies nothing refuses. The work per edit list stays within a constant of the load
  before it:
  - **An attempt is cheap.** `Engine::set_param` checks one edit against its class's
    `param_rules()`. Each rule reads the edited value and, for the three cross-parameter rules
    (`RealLessOrEqual`, `IntegerProductLessOrEqualConstant`, `RealTimesIntegerInclusiveRange`),
    one other parameter of the same block. A table element is checked alone.
  - **The edits are few.** An edit list names each parameter once (`param-path-duplicate`), and
    holds at most 512 edits: two maps of 256.
  - **The passes are few.** A later pass can only admit an edit that a cross-parameter rule ties to
    one admitted before it. So the passes number at most the longest chain of such rules in one
    class: two for `Reals.Hysteresis` (`uLow <= uHigh`). The implementing PR computes the catalog's
    longest chain from `param_rules()` and pins it with a test, rather than trusting this sentence.
  - **One resume per apply.** The single resume that follows re-instantiates the blocks once.
- **Diagnostic matching and trace memory.** Both are bounded per scenario by the replay's
  `max_trace_bytes`. A run holds one trace at a time unless the caller sets
  `VerifyOptions::keep_traces`, which the CLI never does; a caller who keeps traces holds every
  scenario's trace.
- **The vector file** is read once under its 4 MiB bound. The receipt is written under its record
  caps.

## Alternatives

The record of why the design is shaped as it is. Each rejected alternative stays here.

- **A. Separate budgets in natural units** (the design above; ruling 1).
- **B. One work budget**, with each unit weighted by a cost constant: a nanosecond-equivalent
  score. Rejected: the constants are this machine's and this build's. Any engine speed-up would
  make them a stale contract, and a refusal would name a score rather than the dimension a writer
  can change.
- **C. A load count** (`max_total_loads`) in place of load bytes. Rejected: an 8 MiB document costs
  4,000 times a 2 KB one to load. Load cost is linear in bytes with no fixed floor worth counting,
  so a count either refuses small subjects early or lets large ones run for hours.
- **D. Lower the existing limits** (scenarios, mutants, expectations, ticks) until every product
  is small. Rejected: bounding a product through its factors takes the factors down by more than
  the corpus can spare. For example, a minute of load bytes on 8 MiB documents with eight mutants
  allows about a dozen scenarios with edits; the corpus already uses up to 28 scenarios.
- **E. A wall-clock deadline or a cancel hook.** Rejected as the bound: the same files would pass
  on one machine and refuse on another, and the refusal would not be reproducible. A host keeps
  its own process timeout. A cooperative cancel hook could be a later API addition beside the
  budgets.
- **F. The note states the worst case and bounds nothing** (the issue's other route). Rejected:
  the worst case is hours in `release`, behind a verb meant for fetched files.
- **G. Do less work rather than bound it.** Load each document once and rewind it per scenario
  through `Engine::checkpoint` and `Engine::restore_checkpoint`, a process-local image the facade
  offers for deterministic branching. Loads would fall from `(1 + m) * (1 + e + s)` to `1 + m`.
  This complements the budgets rather than replacing them: frames and instants still need bounds,
  and the vectors note's fresh load per scenario is its isolation guarantee. A rewound engine
  running bit-identically to a freshly loaded one, parameters included, is not a contract the
  facade states today. It is #569, after part 4 (ruling 10).

## Rulings

The owner ruled on 2026-11-11, on GitHub #568. Questions 1, 3, 4, 6 and 8 were ruled directly. The
rest were accepted as recommended, with no objection. Each ruling names what it rejected.

1. **Separate budgets, each a simple product checked at bind** (A). Rejected: one weighted work
   budget (B), whose constants would be one machine's contract; bounding nothing (F).
2. **Loads are counted in bytes** (`max_total_load_bytes`). Rejected: a load count, alone or beside
   the bytes (C), since load cost is linear in bytes with no fixed floor.
3. **`max_total_frame_bytes` is added beside `max_total_frames`.** Rejected: replacing the frame
   count, which still bounds the per-frame work that does not scale with the document; leaving
   frames unweighted, a 52-minute case in `release`.
4. **The defaults are 2^28 instants, 2^30 load bytes and 2^40 frame bytes,** with frames unchanged
   at 2^24: about one minute in `release` at worst. Rejected: the tighter set (2^26, 2^29, 2^38,
   about 25 s) and the looser one (2^30, 2^32, 2^42, about 4 min).
5. **Lower-only, the default being the maximum,** as decision 35 states for every limit. Rejected:
   a `VerifyLimits::DEFAULT` under a higher `MAX`, and an uncapped opt-out for trusted hosts.
6. **One budget step, after the subject's first bind load and its name resolution, before every
   other load.** `too-many-frames` therefore wins over `param-refused`. Rejected: checking before
   any load, which over-counts edit lists written rather than resolved; and checking after the
   subject's trials, which leaves 4,097 subject loads (32 GiB) unbounded.
7. **The subject's trials are deduplicated** by distinct effective edit list, as the mutants'
   already are. The first scenario in file order that holds a refused list is still the one named,
   so no refusal or receipt changes. This amends part 2's reading 8 of the vectors note. Rejected:
   one trial per scenario with edits.
8. **One code per budget:** `too-many-instants`, `too-many-load-bytes` and `too-many-frame-bytes`,
   after `too-many-frames` in `VERIFY_ERROR_CODES`. Rejected: one `too-much-work` code with a
   `cause`.
9. **The receipt records none of the budgets.** A budget never changes a receipt's bytes. Rejected:
   recording the budgets in force, which would give identical runs different ids; recording the
   loads made, an implementation detail that ruling 10's follow-up would change.
10. **Engine reuse through checkpoints is a follow-up, after part 4:** #569. Rejected: folding it
    into the implementing PR, or dropping it.

## What the implementing PR changes

One PR in `crates/leverage/oce-verify`, which closes #561, changes these, with nothing outside the
crate but documents:
- **Code:**
  - `src/limits.rs`: the three fields and their maxima;
  - `src/bind.rs`: the budget step, and the deduplicated subject trials (ruling 7);
  - `src/verify_error.rs`: three kinds, the code list and the module documentation.
- **Tests:**
  - each budget accepted at its bound and refused one past it;
  - the order between the budgets, and against `param-refused` and the mutant codes;
  - a `u128` product past `u64`;
  - the corpus shapes inside every default.
- **No golden moves**, because a budget changes no receipt's bytes.
- **Documents:**
  - `docs/design/vectors.md`: binding steps 4 to 6, the error-code table, decision 35, the API
    sketch, part 2's readings 8 and 9, and part 3's readings 1 and 6;
  - the crate README's bounds table;
  - the CHANGELOG.

Part 4 then lands on a run whose worst case is minutes, not hours.
