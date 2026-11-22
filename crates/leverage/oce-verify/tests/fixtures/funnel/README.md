# The funnel differential fixture

The evidence that `oce-verify`'s `funnel` is LBNL funnel (commissioning note,
[`docs/design/commissioning.md`](../../../../../../docs/design/commissioning.md), "Envelope",
decisions 1, 12 and 13; issue #655). For every case here the port's five curves, printed with the
crate's `%.16g` writer, equal byte for byte the five files the pinned `pyfunnel` writes; and where
pyfunnel refuses a case, the port refuses it for the same reason.

| Pin | Value |
| --- | --- |
| funnel | [`lbl-srg/funnel`](https://github.com/lbl-srg/funnel) commit `a12fa96917ca73b78fcb6a17a1140b3a27a61e31` (tag `v2.1.1`) |
| pyfunnel | `2.1.1`, wheel `pyfunnel-2.1.1-py3-none-any.whl` |
| Wheel SHA-256 | `cf48c95a995b21bc16df16fbf1e3b350d95e2c8ea5c5d4c4a74a840fa334752a` |
| Library | `pyfunnel/lib/linux64/libfunnel.so` inside the wheel, SHA-256 `fbc3103f114b98ee9c6f82e8d0b104c00b64e4e7573ed3078579df2fc7df2928` |
| Platform | x86_64 Linux, glibc 2.41, CPython 3.12.16 ([`capture/provenance.json`](capture/provenance.json)) |

| Path | What it is | Written by |
| --- | --- | --- |
| `upstream/<case>/` | funnel's own 17 test directories at the pinned commit: the two input series, `param.json` and, where upstream has one, `readme.txt`, verbatim. Upstream's `results/` directories are not copied (see "Upstream's cases"). | nobody: copied from the pinned commit |
| `upstream/LICENSE.txt`, `upstream/COPYRIGHT.txt` | funnel's licence (BSD 3-clause with LBNL's enhancements clause) and copyright notice, beside the files copied from it. The same two texts cover the port itself, `src/funnel/`, which is a derived work of funnel's C: they are reproduced verbatim in `LICENSE-funnel` at the crate root. | nobody: copied |
| `hostile/<case>/` | 49 cases written for this repository: `reference.csv`, `test.csv`, `param.json`, in upstream's layout. | `capture/make_hostile.py` |
| `recorded/<case>/` | Recorded days: `building_33_day`, the Building 33 worked example's day (issue #755), the engine series of the transcribed `CoolingCoilValve` as `reference.csv` and the trended valve as `test.csv`, 17,281 points each at 5 s, written so that they read back to the run's bits, with `atolx` 1 and `rtoly` 0.01. | `tests/commissioning/building_33_day.rs`, blessed with the day's receipt (`OCE_BLESS_VERIFY=1`) |
| `expected/<case>/` | What pyfunnel made of each of the 67 cases: its five CSV files (`reference.csv`, `test.csv`, `lowerBound.csv`, `upperBound.csv`, `errors.csv`), or `refused.txt` with the one line its log gave. | `capture/capture.sh`, on x86_64 Linux |
| `printf/glibc.tsv` | glibc's `snprintf("%.16g")` of 4,792 doubles (bits, a tab, the text): the reference for the crate's `%.16g` writer. | `capture/capture.sh` |
| `capture/` | The capture: `capture.sh`, `capture.py`, `requirements.txt` (the wheel's hash pin), `make_hostile.py`, and `provenance.json`, the record of the run that wrote `expected/`. | by hand; `provenance.json` by `capture.sh` |
| `golden/<case>.txt` | The port's own five curves and figures for every case, as hex bits. | `OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify` |
| `.gitattributes` | Marks `upstream/` and `expected/` as bytes: four of upstream's inputs end their lines with CRLF, and the repository otherwise normalises text to LF. | by hand |

**No CI job runs Python, installs pyfunnel or reaches the network.** The tests read the checked-in
files: `tests/funnel_differential.rs` (the port against `expected/`), `tests/funnel_goldens.rs`
(the port against `golden/`) and `src/g16_tests.rs` (the writer against `printf/glibc.tsv`);
`tests/funnel_edges.rs`, `tests/funnel_refusals.rs` and `tests/aligned_grid.rs` read the inputs.
`expected/`, `printf/glibc.tsv` and the `hostile/` inputs are regenerated only by an issue that
says so (decision 13); no switch rewrites them.

## What the claim is, and is not

- **It is textual.** pyfunnel writes every number with C's `%.16g`, sixteen significant digits,
  and a double needs seventeen. Two curves that print the same can differ in their last bit, so
  "equal to pyfunnel" means "prints the same". The comparison has no numeric tolerance: one would
  hide a porting error. The port's exact bits are pinned separately, in `golden/`.
- **It is x86_64 arithmetic, because pyfunnel's own output depends on its platform.** The
  wheel's `linux64` library is built without fused multiply-add. Its macOS library, on arm64,
  contracts `a * b + c`: run on these 66 cases it prints 38 of the 306 expected files
  differently in the last digits, in 29 of the cases (measured for the pull request of issue
  #655). The port never contracts, on any target, so it matches the x86_64 Linux library
  everywhere.
- **Two of upstream's 17 cases are refusals, not curves.** At the pinned commit funnel refuses
  `fail2` and `fail6` ("Reference and test data maximum x values are different", which upstream's
  own test list expects), returns 1 and writes no curve. For those two, and for four
  hostile cases, `expected/<case>/refused.txt` holds the line, and the port must return
  `CompareError::MismatchedEndpoints` at the same end. So 60 cases are compared as curves
  and six as refusals.
- **`fail4` is compared as curves.** Upstream fails it by an unwritable output folder
  (`"output": "/results*|"`). The capture writes elsewhere, so its numbers, which are
  `success5`'s, are compared like any other case.
- **The Building 33 day is a set of its own, `recorded/`** (decision 13; issue #755). Its two
  inputs are the day's run of the worked example (`../commissioning/building33/`), the engine
  series as the reference and the trend as the test; the capture ran pyfunnel on them like any
  other case. Two tests compare it: `tests/funnel_differential.rs` runs the port on the two files,
  and `tests/commissioning/building_33_day.rs` holds the run's own five curves, `errors.csv`
  included, to pyfunnel's files, so the claim reaches the commissioning run itself. pyfunnel
  finds 817 of the 17,281 instants outside the band, as the run does. It is kept out of the
  case list the other envelope tests count and golden (`tests/support/funnel_cases.rs`,
  `RECORDED_CASES`). The capture for #755 rewrote the other 66 cases' files byte for byte as they
  were, and `provenance.json` gained the one case.
- **Where funnel defines no result, there is nothing to compare.** A local `x` tolerance above
  one can make funnel's loop removal read outside its arrays; pyfunnel then writes curves
  computed from whatever it read, or crashes. No such case is in `expected/`. The port returns
  `loop-removal-undefined`, and `tests/funnel_refusals.rs` has one.

## Upstream's cases

The last column compares the files upstream commits in each test directory with what the pinned
script wrote. **None is byte equal, and the oracle here is the pinned script, not those files.**
They were written by earlier versions (an older print format in several, an older tube in
`success9`, a funnel that did not yet refuse `fail2` and `fail6`), and upstream's own harness
reads the inputs through pandas, whose default number parser is one unit in the last place off
on some values; the capture reads them with Python's `float`, as pyfunnel's command line does.
Upstream's own test compares `errors.csv` alone, within a relative 1e-12.

| Case | Points (reference, test) | Tolerances | pyfunnel 2.1.1 | Upstream's committed `results/` |
| --- | --- | --- | --- | --- |
| `fail1` | 5041, 5041 | `rtolx` 0.002, `rtoly` 0.002 | 498 of 5041 instants outside, largest error 0.01912799999999654 | not equal: 0 of 5 files equal; largest relative difference 4e-07 |
| `fail2` | 5, 7 | `rtolx` 1e-06, `rtoly` 1e-06 | refuses: maximum x values are different | stale: funnel now refuses the case |
| `fail3` | 10, 10 | `atolx` 0.1, `atoly` 0.1 | 5 of 10 instants outside, largest error 0.05128405906224476 | not equal: 3 of 5 files equal; largest relative difference 4e-16 |
| `fail4` | 4, 8 | `atolx` 0.1, `atoly` 0.01 | inside the band | none committed |
| `fail5` | 4, 4 | `atolx` 1e-06, `atoly` 0.099 | 1 of 4 instants outside, largest error 0.0009900000000010456 | not equal: 0 of 5 files equal; largest relative difference 4e-07 |
| `fail6` | 5, 3 | `rtolx` 1e-06, `rtoly` 1e-06 | refuses: maximum x values are different | stale: funnel now refuses the case |
| `should_succeed1` | 4, 4 | `atolx` 0.01, `atoly` 0.01 | 1 of 4 instants outside, largest error 1 | not equal: 0 of 5 files equal; the others hold the same numbers in other text |
| `success1` | 21, 21 | `rtolx` 0.002, `rtoly` 0.002 | inside the band | not equal: 0 of 5 files equal; largest relative difference 6e-16 |
| `success2` | 31, 31 | `rtolx` 1e-06, `rtoly` 1e-06 | inside the band | not equal: 2 of 5 files equal; largest relative difference 8e-16 |
| `success3` | 31, 31 | `atolx` 0.01, `atoly` 1e-06 | inside the band | not equal: 3 of 5 files equal; largest relative difference 7e-16 |
| `success4` | 4, 5 | `atolx` 0.1, `atoly` 1e-06 | inside the band | not equal: 0 of 5 files equal; the others hold the same numbers in other text |
| `success5` | 4, 8 | `atolx` 0.1, `atoly` 0.01 | inside the band | not equal: 0 of 5 files equal; the others hold the same numbers in other text |
| `success6` | 10, 10 | `atolx` 0.001, `atoly` 0.001, `rtolx` 0.001, `rtoly` 0.001 | 5 of 10 instants outside, largest error 0.01760551878923822 | not equal: 0 of 5 files equal; largest relative difference 3e-16 |
| `success7` | 101, 101 | `atolx` 0.001, `atoly` 0.001 | inside the band | not equal: 0 of 5 files equal; largest relative difference 4e-16 |
| `success8` | 10, 10 | `atolx` 0.0001, `atoly` 0.0001, `ltolx` 0.001, `ltoly` 0.001 | 8 of 10 instants outside, largest error 0.005717472612315734 | not equal: 0 of 5 files equal; largest relative difference 6e-16 |
| `success9` | 101, 101 | `ltolx` 0.001, `ltoly` 0.001 | inside the band | not equal: 0 of 5 files equal; largest relative difference 1e-03 |
| `success10` | 31, 31 | none | inside the band | not equal: 2 of 5 files equal; largest relative difference 6e-16 |

"Inside the band" and "outside" are pyfunnel's `errors.csv`, read as text. A directory's name
says whether upstream expects its test to pass, which is another matter: `success6` and
`success8` reproduce their stored errors, which are not zero.

## The hostile cases

Written by `capture/make_hostile.py` from integers and a fixed linear congruential generator, so
running it again writes the same bytes. They are the commissioning note's hostile set (a constant
series, a single point, equal times, huge and tiny `x` magnitude, every tolerance kind alone), the
shapes the port's steps branch on, and series funnel refuses by their ends. Three of them
(`loop_between_parallel_segments`, `series_at_one_instant`, `series_at_one_huge_instant`) were
found by a seeded search of random cases and are kept with the numbers it found them at, because
each takes a branch of the loop removal that no other case takes. Two are one series compared
with itself on a tick grid under an `x` tolerance of one tick and no `y` tolerance:
`identical_large_step_no_y_tolerance`, a step of two million, which pyfunnel reports outside its
own band by `2.328306436538696e-10` (the band is only the `1e-10` floor high and the
interpolation rounds by more), and `identical_small_step_no_y_tolerance`, the same series a
millionth as high, which is inside. Three more are identical series that funnel reports
outside their own band by whole units of `y`, with a `y` tolerance:
`identical_spike_at_one_instant` and `identical_pulse_by_equal_times_at_large_x` (several samples
at one instant with no `x` tolerance, where the `1e-10` floor's width is below what funnel's
equality can tell from zero) and `identical_epoch_seconds_tenth_steps` (`x` over 1e10 times its
step, where that equality reads neighbouring instants as one).

| Case | Points (reference, test) | Tolerances | pyfunnel 2.1.1 |
| --- | --- | --- | --- |
| `atolx_alone` | 21, 22 | `atolx` 0.3 | 12 of 22 instants outside |
| `atoly_alone` | 21, 22 | `atoly` 0.25 | 12 of 22 instants outside |
| `both_ends_differ` | 21, 21 | `atolx` 0.3, `atoly` 0.25 | refuses: minimum x values are different |
| `constant_series` | 10, 10 | `rtolx` 0.002, `rtoly` 0.002 | 5 of 10 instants outside |
| `constant_zero_series` | 10, 10 | `rtolx` 0.01, `rtoly` 0.01 | 4 of 10 instants outside |
| `ends_equal_within_tolerance` | 21, 21 | `atolx` 20000.0, `atoly` 0.25 | inside the band |
| `equal_times` | 11, 15 | `atolx` 0.05, `atoly` 0.05 | 2 of 15 instants outside |
| `equal_times_no_tolerance` | 11, 11 | none | 1 of 11 instants outside |
| `first_x_differs` | 21, 21 | `atolx` 0.3, `atoly` 0.25 | refuses: minimum x values are different |
| `huge_x_magnitude` | 21, 22 | `atolx` 150000.0, `rtoly` 0.01 | 7 of 22 instants outside |
| `identical_epoch_seconds_tenth_steps` | 12, 12 | `atoly` 0.1 | 7 of 12 instants outside |
| `identical_large_step_no_y_tolerance` | 3, 3 | `atolx` 60.0 | 1 of 3 instants outside |
| `identical_pulse_by_equal_times_at_large_x` | 6, 6 | `atoly` 0.1 | 1 of 6 instants outside |
| `identical_small_step_no_y_tolerance` | 3, 3 | `atolx` 60.0 | inside the band |
| `identical_spike_at_one_instant` | 5, 5 | `atoly` 0.1 | 1 of 5 instants outside |
| `large_values_both_axes` | 21, 22 | `atolx` 300.0, `ltoly` 1e-07 | 4 of 22 instants outside |
| `last_x_differs` | 21, 21 | `atolx` 0.3, `atoly` 0.25 | refuses: maximum x values are different |
| `last_x_just_differs` | 21, 21 | `atolx` 20000.0, `atoly` 0.25 | refuses: maximum x values are different |
| `leading_duplicates` | 10, 8 | `atolx` 0.1, `atoly` 0.1 | inside the band |
| `loop_between_parallel_segments` | 3, 3 | `atolx` 7513.223201215218, `rtolx` 0.0013205533361355549, `rtoly` 0.0005018068265039262, `ltolx` 0.0010462830632038287, `ltoly` 1.008424077201712 | inside the band |
| `ltolx_alone` | 21, 22 | `ltolx` 0.05 | 10 of 22 instants outside |
| `ltoly_alone` | 21, 22 | `ltoly` 0.1 | 17 of 22 instants outside |
| `near_flat_wiggle` | 12, 12 | `atolx` 0.0002, `atoly` 0.01 | 6 of 12 instants outside |
| `negative_x` | 21, 22 | `atolx` 0.3, `rtoly` 0.05 | 1 of 22 instants outside |
| `negative_x_local_tolerance` | 21, 22 | `ltolx` 0.05, `ltoly` 0.1 | 4 of 22 instants outside |
| `negative_zero` | 4, 4 | `atoly` 0.1 | 1 of 4 instants outside |
| `no_tolerance_identical` | 21, 21 | none | inside the band |
| `no_tolerance_moved` | 21, 22 | none | 14 of 22 instants outside |
| `noisy_every_tolerance` | 240, 240 | `atolx` 0.5, `atoly` 0.05, `rtolx` 0.002, `rtoly` 0.02, `ltolx` 0.004, `ltoly` 0.03 | inside the band |
| `noisy_local_tolerance` | 240, 240 | `ltolx` 0.02, `ltoly` 0.05 | 34 of 240 instants outside |
| `noisy_narrow_rectangles` | 240, 240 | `atolx` 0.001, `atoly` 0.001 | 197 of 240 instants outside |
| `noisy_wide_rectangles` | 240, 240 | `rtolx` 0.01, `rtoly` 0.01 | 44 of 240 instants outside |
| `rising_sawtooth_local_tolerance` | 41, 41 | `ltolx` 0.9 | 19 of 41 instants outside |
| `rtolx_alone` | 21, 22 | `rtolx` 0.02 | 10 of 22 instants outside |
| `rtoly_alone` | 21, 22 | `rtoly` 0.05 | 10 of 22 instants outside |
| `sawtooth_narrow_rectangles` | 41, 41 | `atolx` 0.25, `atoly` 0.05 | 10 of 41 instants outside |
| `sawtooth_wide_rectangles` | 41, 41 | `atolx` 2.5, `atoly` 0.05 | 24 of 41 instants outside |
| `series_at_one_huge_instant` | 3, 3 | `atoly` 2.384213967052036e-09, `rtoly` 0.0007550054411901956, `ltolx` 0.02075672568409159 | 2 of 3 instants outside |
| `series_at_one_instant` | 3, 3 | `rtolx` 0.1032121663602923, `rtoly` 0.05805394352408241 | inside the band |
| `sharp_peak_wide_rectangles` | 11, 41 | `atolx` 1.5, `atoly` 0.1 | 11 of 41 instants outside |
| `single_point` | 1, 1 | `atolx` 0.1, `atoly` 0.1 | inside the band |
| `single_point_no_tolerance` | 1, 1 | none | 1 of 1 instants outside |
| `single_reference_point` | 1, 3 | `atolx` 0.25, `atoly` 0.25 | 2 of 3 instants outside |
| `test_ends_beyond_the_bounds` | 21, 21 | `atolx` 1e-05, `atoly` 0.25 | 21 of 21 instants outside |
| `tiny_x_magnitude` | 21, 22 | `atolx` 1.0000000000000001e-07, `atoly` 0.2 | 12 of 22 instants outside |
| `tolerance_below_floor` | 21, 22 | `atolx` 5e-11, `atoly` 5e-11 | 7 of 22 instants outside |
| `x_beyond_exact_integers` | 21, 22 | `rtolx` 0.01, `rtoly` 0.01 | 16 of 22 instants outside |
| `x_magnitude_above_threshold` | 21, 22 | `atolx` 2.0000000000000004e-07, `atoly` 0.2 | 12 of 22 instants outside |
| `x_magnitude_at_threshold` | 21, 22 | `atolx` 2.0000000000000002e-07, `atoly` 0.2 | 12 of 22 instants outside |

## The recorded day

| Case | Points (reference, test) | Tolerances | pyfunnel 2.1.1 |
| --- | --- | --- | --- |
| `building_33_day` | 17281, 17281 | `atolx` 1.0, `rtoly` 0.01 | 817 of 17281 instants outside, largest error 0.02086470974525789 |

## Running the capture

`capture.sh` needs x86_64 Linux, `python3` with `venv`, and network access to the package index
for the one hash-pinned wheel; it refuses to run anywhere else. It was run for the pull request of
issue #655 in the official `python:3.12-slim` container image
(`sha256:5e2bdb1b0d542d5039cffb8181c65930093949194a65c365dfb1b0fe13a4fa5b`) for `linux/amd64`,
emulated on an arm64 host; the emulation executes x86_64 floating point exactly. It was run again
for the pull request of issue #755, the same way and by the same image digest, to add
`recorded/building_33_day`; Python 3.12.16 and glibc 2.41 again.

```bash
# From this directory. <funnel> is lbl-srg/funnel checked out at the pinned commit.
python3 -I capture/make_hostile.py .          # only when the hostile set changes
docker run --rm --platform linux/amd64 \
  -v "$PWD":/fixture -v <funnel>:/funnel:ro -w /fixture \
  python@sha256:5e2bdb1b0d542d5039cffb8181c65930093949194a65c365dfb1b0fe13a4fa5b \
  bash capture/capture.sh /funnel
```

Refresh `recorded/building_33_day/`'s two inputs first, if the day's run changed, with
`OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify --test commissioning -E 'test(/building_33_day/)'`.

`capture.py` fails if a file under `upstream/` is not upstream's byte for byte, if pyfunnel is
not 2.1.1 or loads another library, or if a case returns anything but curves or one of funnel's
two endpoint refusals. It writes no path, host name, user name or time. After a capture: review
the diff of `expected/`; re-bless `golden/` only if the port changed; and update the case counts
in `tests/support/funnel_cases.rs`, `tests/funnel_differential.rs` and `tests/aligned_grid.rs`,
the parity goldens under `../aligned_grid/` if a case of at most 41 reference points was added,
and the tables here.

## Vendored files

| Fixture | Upstream path | SHA-256 |
| --- | --- | --- |
| `upstream/LICENSE.txt` | `LICENSE.txt` | `70afbe986806a28367fb8caf971b8bd1360e1fe0d3714923f0a32df331ea1caf` |
| `upstream/COPYRIGHT.txt` | `COPYRIGHT.txt` | `29ecb160593f83643482f3e51f4a26f1715c4a98e159a3dcc0b38c44cb600d77` |
| `upstream/fail1/param.json` | `tests/fail1/param.json` | `2e3398c03c9eb4ae92cad7a689886708611595059b9fd445ce2d71e22f85c2ac` |
| `upstream/fail1/simulated.csv` | `tests/fail1/simulated.csv` | `d4e9f55a68aba8b58b32af6004ebcb5a07b8e782681e77ba3e2cd2e0ff5b50ee` |
| `upstream/fail1/trended.csv` | `tests/fail1/trended.csv` | `b2a75c6a72788e30cfc9e0cc8bae087e6c123f08ab17adb204a126a172be1dcc` |
| `upstream/fail2/param.json` | `tests/fail2/param.json` | `95d0371ec10abfc60c73a4a246c17df9b08abd1bc6f9edc8a0f0c43f55146f8f` |
| `upstream/fail2/readme.txt` | `tests/fail2/readme.txt` | `483c65b40c341fa00dbc2b4f63adca4c1af82df1cef377a8b6bec984c972921b` |
| `upstream/fail2/simulated.csv` | `tests/fail2/simulated.csv` | `085f84664a413fefe3e64719958ddb20b4aa930c46704c4641cc43002d669d08` |
| `upstream/fail2/trended.csv` | `tests/fail2/trended.csv` | `998df83543340928a6a151f688934cbb027e2236dda697ad57ae07e1e6e356dc` |
| `upstream/fail3/param.json` | `tests/fail3/param.json` | `f6578d114efd56b8b03f028070787817529e2f32017e0ec65ec6363c14d6e6c3` |
| `upstream/fail3/readme.txt` | `tests/fail3/readme.txt` | `a9d98176043a93ee7b8aaba545d0604e93ab9fc554bc1e1453fb424fc7905555` |
| `upstream/fail3/simulated.csv` | `tests/fail3/simulated.csv` | `db26c6a30ad5f935968f6b8fbd3dfc7f5455fb6c86970d57455c42971a8e8186` |
| `upstream/fail3/trended.csv` | `tests/fail3/trended.csv` | `800d3329f1a12e8e965017cf59c5e8801fb0974ca3170e0101f26c4e3105ff34` |
| `upstream/fail4/param.json` | `tests/fail4/param.json` | `47eb7b849a9efbc1192697d27dc8452fe574bc0c42323bd2940edea0c7f3d85c` |
| `upstream/fail4/readme.txt` | `tests/fail4/readme.txt` | `81f308c6c31cc3aab34d07188c5ba34c61bf28dd3602e9e1e75d38b8497f9d4d` |
| `upstream/fail4/simulated.csv` | `tests/fail4/simulated.csv` | `a91fe65bcd76ac8d3595cbbcc6e028a952c59be22d52b7e8ba2d3825670c3a56` |
| `upstream/fail4/trended.csv` | `tests/fail4/trended.csv` | `e78314c264cf97d76ddd9efe71129778b522bdf7965796f1ccd425ef9935da70` |
| `upstream/fail5/param.json` | `tests/fail5/param.json` | `b2d37974ed6e039c1f03ba1fc94be1b7a26ff03a26bd7eb7fffdfbe6af819664` |
| `upstream/fail5/readme.txt` | `tests/fail5/readme.txt` | `4af03a60532141a6bf91af31f79f2f4fd014559d43045a50cabc952081f9fa97` |
| `upstream/fail5/simulated.csv` | `tests/fail5/simulated.csv` | `ff2dc67e9a6d9398b4374b9b113ff8d0d9c016da025d618a7881d6c97ae2e44a` |
| `upstream/fail5/trended.csv` | `tests/fail5/trended.csv` | `f80d8de3629b65aa873327c342273c946066ec8ebfc02e4819dc5ec5021307fe` |
| `upstream/fail6/param.json` | `tests/fail6/param.json` | `95d0371ec10abfc60c73a4a246c17df9b08abd1bc6f9edc8a0f0c43f55146f8f` |
| `upstream/fail6/readme.txt` | `tests/fail6/readme.txt` | `1b93f6e04c1e5eaa41b88c86c7f2500324df7a6d4963cd3c6fafc5c4e6fdea9f` |
| `upstream/fail6/simulated.csv` | `tests/fail6/simulated.csv` | `2a4e88c11ab96aad8ca21787687f1648e9a4e5616866a92cf4c9c7857ddfa199` |
| `upstream/fail6/trended.csv` | `tests/fail6/trended.csv` | `0c116c468f2d21ed1d75d4574535c9565036366411b7ba2dfa231b72c300cde7` |
| `upstream/should_succeed1/param.json` | `tests/should_succeed1/param.json` | `444052df886d39fed7794f32aa7091bd582e1f53e48a54d09c9542f06b371d19` |
| `upstream/should_succeed1/readme.txt` | `tests/should_succeed1/readme.txt` | `b52dd08a00d22ad1cb56d9fbc4cb68be5d2e2c44435ee0c1c36f977aaae603ab` |
| `upstream/should_succeed1/simulated.csv` | `tests/should_succeed1/simulated.csv` | `24805e500eaba509edb1ab9127d500016ca54586ba210042333f2d820df511ad` |
| `upstream/should_succeed1/trended.csv` | `tests/should_succeed1/trended.csv` | `bc4c5d622251d32f827a28254334c42ec527548a800251a54859727f298ef0d8` |
| `upstream/success1/param.json` | `tests/success1/param.json` | `2e3398c03c9eb4ae92cad7a689886708611595059b9fd445ce2d71e22f85c2ac` |
| `upstream/success1/simulated.csv` | `tests/success1/simulated.csv` | `f5443c9f754c5196f6681c645fac2a59849a79b88087701b5a14c4c3ab63274e` |
| `upstream/success1/trended.csv` | `tests/success1/trended.csv` | `1b2858f34bb45ffbee80c46fcf00610fef7536e3e283b0b356866543136f02f1` |
| `upstream/success2/param.json` | `tests/success2/param.json` | `b0f45b194b0313f6c7ff60c117d21755bc072bfe89b1916952ee2d7376d80d97` |
| `upstream/success2/readme.txt` | `tests/success2/readme.txt` | `7953baf466d52e10af883c54a00b33770fcb441ea027ec85f4a3f0496ab6e7f5` |
| `upstream/success2/simulated.csv` | `tests/success2/simulated.csv` | `e88bcb65b205c366ee17cfb879abb141c9e74ed378954d1d58cfe5fe5ca53207` |
| `upstream/success2/trended.csv` | `tests/success2/trended.csv` | `c25ba4258f626e124880df0b72c01ce2a475a5eb5c23e67816fb0acd6b4c5c18` |
| `upstream/success3/param.json` | `tests/success3/param.json` | `551ab7272aec4caa3d51bce59e44a598e45cc9e964e464f4aef836247696709a` |
| `upstream/success3/readme.txt` | `tests/success3/readme.txt` | `967ee5d6ac80409c85b7485a7d8a30832b408fd0610d10caff05e72b6e885244` |
| `upstream/success3/simulated.csv` | `tests/success3/simulated.csv` | `83cfb6cb2d531b4bd210af5c69f8d3577a8c6225d83ebf9f1f54fd693bfa4417` |
| `upstream/success3/trended.csv` | `tests/success3/trended.csv` | `84f67f33072e6c738d985d54bd657ec8dd5038c070ae62d57019f4f5925c86cd` |
| `upstream/success4/param.json` | `tests/success4/param.json` | `64c94a22ff6014b52344c25dd7b0b3a0fdf2217c3ccffd594cd7783bb01e2928` |
| `upstream/success4/readme.txt` | `tests/success4/readme.txt` | `97f2cbc0159d1d775b42de55668d2e6fcc7adad59083e2bfddabd1e502c65059` |
| `upstream/success4/simulated.csv` | `tests/success4/simulated.csv` | `157fe70a448b60e33ab4ebc29865b8111d8e5a1a55c37c290897286ac441aad6` |
| `upstream/success4/trended.csv` | `tests/success4/trended.csv` | `e78314c264cf97d76ddd9efe71129778b522bdf7965796f1ccd425ef9935da70` |
| `upstream/success5/param.json` | `tests/success5/param.json` | `0f99b11b81a270f1b3b2fca8403a8a3b5a25b33aee6f1f9ef34986c9ec0a07ad` |
| `upstream/success5/readme.txt` | `tests/success5/readme.txt` | `97f2cbc0159d1d775b42de55668d2e6fcc7adad59083e2bfddabd1e502c65059` |
| `upstream/success5/simulated.csv` | `tests/success5/simulated.csv` | `a91fe65bcd76ac8d3595cbbcc6e028a952c59be22d52b7e8ba2d3825670c3a56` |
| `upstream/success5/trended.csv` | `tests/success5/trended.csv` | `e78314c264cf97d76ddd9efe71129778b522bdf7965796f1ccd425ef9935da70` |
| `upstream/success6/param.json` | `tests/success6/param.json` | `28b108f2b3d20c9d6337e43ff785424039e908b789de969d1b2457d56d229de7` |
| `upstream/success6/readme.txt` | `tests/success6/readme.txt` | `abd936015d908824c4d6150b4817b266e685f417111126d4834a91f09778b8c2` |
| `upstream/success6/simulated.csv` | `tests/success6/simulated.csv` | `e4ee7183b5e607ae7d61abc2b2d0a14ea2ae2e92e6845e456a5b23374c58c3f9` |
| `upstream/success6/trended.csv` | `tests/success6/trended.csv` | `8ccf842152cae1a77eae2b632de4c0bbe1c6d46559e810995e7f0556a33ef83c` |
| `upstream/success7/param.json` | `tests/success7/param.json` | `e00b8087939af719b3d6cce06bc4f7092ec298326fb8b44b305e8671c55c2678` |
| `upstream/success7/readme.txt` | `tests/success7/readme.txt` | `8d9505099ccffa1808402932d68383e10a67d44017fe2a91aff73de9410fa11f` |
| `upstream/success7/reference.csv` | `tests/success7/reference.csv` | `2a4bd32d4acbdea98b468f2d5ab11b95dbacbccc1b7781a563f0dd574438101a` |
| `upstream/success7/test.csv` | `tests/success7/test.csv` | `2a4bd32d4acbdea98b468f2d5ab11b95dbacbccc1b7781a563f0dd574438101a` |
| `upstream/success8/param.json` | `tests/success8/param.json` | `53ebd7e66368e21ad08064b9c56cb1aef02bbd4ec39dcfddee9adb574ce2f248` |
| `upstream/success8/readme.txt` | `tests/success8/readme.txt` | `563cec6ad091543fc43f0fb68a98c3dd8798531438cad9d2c4182d0a84939f19` |
| `upstream/success8/simulated.csv` | `tests/success8/simulated.csv` | `98b7fa04e6bab98e6270cfbcf7ab3bd964747214a0baa56a1137aebcbf6b781c` |
| `upstream/success8/trended.csv` | `tests/success8/trended.csv` | `8ccf842152cae1a77eae2b632de4c0bbe1c6d46559e810995e7f0556a33ef83c` |
| `upstream/success9/param.json` | `tests/success9/param.json` | `ef295517bc6123cb50e88bc09cb6a26b1e8362a69804b152ef73394bdfe41559` |
| `upstream/success9/readme.txt` | `tests/success9/readme.txt` | `f1ce43fa025ca5611ee7695bd045befec9b0618dc77dade240b932fb07c15648` |
| `upstream/success9/reference.csv` | `tests/success9/reference.csv` | `2a4bd32d4acbdea98b468f2d5ab11b95dbacbccc1b7781a563f0dd574438101a` |
| `upstream/success9/test.csv` | `tests/success9/test.csv` | `2a4bd32d4acbdea98b468f2d5ab11b95dbacbccc1b7781a563f0dd574438101a` |
| `upstream/success10/param.json` | `tests/success10/param.json` | `e2235ac338c8080498ee8bcb57f30855cc630c56226094d7ae23ef594a0d944d` |
| `upstream/success10/readme.txt` | `tests/success10/readme.txt` | `5fe51b497426426d1c0941c9d386db6ee39511a4dce27946dd9283aa5a6e35b3` |
| `upstream/success10/simulated.csv` | `tests/success10/simulated.csv` | `e88bcb65b205c366ee17cfb879abb141c9e74ed378954d1d58cfe5fe5ca53207` |
| `upstream/success10/trended.csv` | `tests/success10/trended.csv` | `c25ba4258f626e124880df0b72c01ce2a475a5eb5c23e67816fb0acd6b4c5c18` |
