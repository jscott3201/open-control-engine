#!/usr/bin/env python3
"""Write the hostile inputs of the funnel differential fixture: `hostile/<case>/`.

    python3 -I capture/make_hostile.py .        # from tests/fixtures/funnel

Each case is a directory in the layout of funnel's own tests: `reference.csv`, `test.csv` (a
header line, then `x,y` rows) and `param.json` naming the two files and the tolerances. The
numbers are written with `repr`, the shortest text that reads back as the same double, so the
Rust test and the capture script parse identical values.

Everything is derived from integers and a fixed linear congruential generator: no clock, no
`random`, no environment. Running it again rewrites the same bytes. It imports nothing outside
the standard library and never imports pyfunnel; `capture.py` produces `expected/`.

The cases are the commissioning note's hostile set (decision 13): a constant series, a single
point, equal times, huge and tiny `x` magnitude and every tolerance kind alone, plus the shapes
the port's steps branch on (leading duplicates, loops at sharp peaks, the `1e-10` floor, negative
zero, noise), and series funnel refuses because their first or last `x` differ.
"""

import json
import math
import os
import sys


class Lcg:
    """Knuth's 64-bit linear congruential generator; `unit` is in [0, 1) on a 2^-53 grid."""

    def __init__(self, seed):
        self.state = seed & 0xFFFFFFFFFFFFFFFF

    def unit(self):
        self.state = (self.state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return (self.state >> 11) / 9007199254740992.0

    def between(self, low, high):
        return low + (high - low) * self.unit()


def interpolate(xs, ys, x):
    """`y` at `x` on the polyline, held outside it."""
    if x <= xs[0]:
        return ys[0]
    for k in range(1, len(xs)):
        if x <= xs[k]:
            if xs[k] == xs[k - 1]:
                return ys[k]
            return ys[k - 1] + (ys[k] - ys[k - 1]) * (x - xs[k - 1]) / (xs[k] - xs[k - 1])
    return ys[-1]


# A reference with every slope pattern the corner selection branches on: rises, falls, flats
# before and after a change, peaks, valleys, a zero crossing and zeros in both columns.
WAVE_X = [float(k) for k in range(21)]
WAVE_Y = [0.0, 1.0, 3.0, 2.0, 2.0, 2.0, 5.0, 4.0, 1.0, 1.0, 0.0,
          -2.0, -1.0, -1.0, 3.0, 3.5, 3.5, 0.5, 0.0, 0.0, 1.0]


def wave_test(seed, spread):
    """A test series for the wave: the reference at half steps, moved by up to `spread`."""
    lcg = Lcg(seed)
    xs = [0.0] + [k + 0.5 for k in range(20)] + [20.0]
    ys = [interpolate(WAVE_X, WAVE_Y, x) + lcg.between(-spread, spread) for x in xs]
    return xs, ys


def noisy(seed, count):
    """An irregularly sampled random walk."""
    lcg = Lcg(seed)
    xs, ys = [0.0], [lcg.between(-1.0, 1.0)]
    for _ in range(count - 1):
        xs.append(xs[-1] + lcg.between(0.05, 2.0))
        ys.append(ys[-1] + lcg.between(-1.0, 1.0))
    return xs, ys


def perturbed(xs, ys, seed, spread):
    """The same instants with every value moved by up to `spread`."""
    lcg = Lcg(seed)
    return list(xs), [y + lcg.between(-spread, spread) for y in ys]


def cases():
    out = {}

    def add(name, reference, test, **tolerances):
        assert name not in out, name
        out[name] = (reference, test, tolerances)

    # Every tolerance kind alone, on the wave.
    for name, tolerance in [
        ("atolx_alone", {"atolx": 0.3}),
        ("atoly_alone", {"atoly": 0.25}),
        ("rtolx_alone", {"rtolx": 0.02}),
        ("rtoly_alone", {"rtoly": 0.05}),
        ("ltolx_alone", {"ltolx": 0.05}),
        ("ltoly_alone", {"ltoly": 0.1}),
    ]:
        add(name, (WAVE_X, WAVE_Y), wave_test(655, 0.5), **tolerance)

    # No tolerance at all: every half-size is the 1e-10 floor.
    add("no_tolerance_identical", (WAVE_X, WAVE_Y), (WAVE_X, WAVE_Y))
    add("no_tolerance_moved", (WAVE_X, WAVE_Y), wave_test(656, 1e-9))
    # Tolerances that funnel's equality reads as zero are replaced by the floor.
    add("tolerance_below_floor", (WAVE_X, WAVE_Y), wave_test(657, 3e-10), atolx=5e-11, atoly=5e-11)

    # A constant series: the range is zero, so `rtol` falls back to the magnitude.
    constant_x = [float(k) for k in range(10)]
    constant_y = [21.5] * 10
    add("constant_series", (constant_x, constant_y),
        perturbed(constant_x, constant_y, 658, 0.08), rtolx=0.002, rtoly=0.002)
    # A constant zero series: the magnitude is zero as well, so the floor applies.
    zero_y = [0.0] * 10
    add("constant_zero_series", (constant_x, zero_y),
        (constant_x, [0.0, 5e-11, -5e-11, 2e-10, -2e-10, 1e-10, -1e-10, 0.0, 1e-3, -1e-3]),
        rtolx=0.01, rtoly=0.01)

    # A single point, with and without a tolerance, and against several test points.
    add("single_point", ([3.0], [2.0]), ([3.0], [2.05]), atolx=0.1, atoly=0.1)
    add("single_point_no_tolerance", ([3.0], [2.0]), ([3.0], [2.0000000002]))
    add("single_reference_point", ([5.0], [1.0]), ([5.0, 5.0, 5.0], [0.5, 1.0, 1.75]),
        atolx=0.25, atoly=0.25)

    # Equal times: vertical steps up and down, a three-point spike at one instant.
    step_x = [0.0, 1.0, 1.0, 2.0, 3.0, 3.0, 3.0, 4.0, 5.0, 5.0, 6.0]
    step_y = [0.0, 0.0, 1.0, 1.0, 1.0, 4.0, 2.0, 2.0, 2.0, 0.0, 0.0]
    add("equal_times", (step_x, step_y),
        ([0.0, 0.9, 1.0, 1.0, 1.1, 2.0, 2.95, 3.0, 3.0, 3.05, 4.0, 5.0, 5.0, 5.2, 6.0],
         [0.0, 0.02, 0.0, 1.0, 0.97, 1.04, 1.0, 1.0, 3.0, 2.0, 2.1, 2.0, 0.0, 0.3, 0.0]),
        atolx=0.05, atoly=0.05)
    add("equal_times_no_tolerance", (step_x, step_y), (step_x, step_y))

    # Duplicate points at the start, inside and at the end.
    duplicate_x = [0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 3.0, 4.0, 4.0, 4.0]
    duplicate_y = [1.0, 1.0, 1.0, 2.0, 0.5, 0.5, 0.5, 3.0, 3.0, 3.0]
    add("leading_duplicates", (duplicate_x, duplicate_y),
        ([0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 3.5, 4.0], [1.0, 1.6, 2.1, 1.2, 0.4, 0.5, 1.9, 3.0]),
        atolx=0.1, atoly=0.1)

    # Huge x: epoch milliseconds, and integers no double holds exactly.
    epoch_x = [1.7e12 + 300000.0 * k for k in range(21)]
    add("huge_x_magnitude", (epoch_x, WAVE_Y),
        ([1.7e12 + 300000.0 * x for x in wave_test(659, 0.3)[0]], wave_test(659, 0.3)[1]),
        atolx=150000.0, rtoly=0.01)
    beyond_x = [1e17 + 1024.0 * k for k in range(21)]
    add("x_beyond_exact_integers", (beyond_x, WAVE_Y),
        ([1e17 + 1024.0 * x for x in wave_test(660, 0.3)[0]], wave_test(660, 0.3)[1]),
        rtolx=0.01, rtoly=0.01)
    # A year of seconds with large values: the equality is relative on both axes.
    year_x = [3.15e7 + 900.0 * k for k in range(21)]
    year_y = [1e9 + 1e3 * y for y in WAVE_Y]
    add("large_values_both_axes", (year_x, year_y),
        ([3.15e7 + 900.0 * x for x in wave_test(661, 0.3)[0]],
         [1e9 + 1e3 * y for y in wave_test(661, 0.3)[1]]),
        atolx=300.0, ltoly=1e-7)

    # Tiny x: below, at and just above the magnitude where funnel starts normalising x.
    # The threshold is a magnitude of exactly 1e-5: normalised above it, not at it.
    for name, top in [
        ("tiny_x_magnitude", 5e-6),
        ("x_magnitude_at_threshold", 1e-5),
        ("x_magnitude_above_threshold", math.nextafter(1e-5, 1.0)),
    ]:
        def scaled(xs):
            return [top * (x / 20.0) for x in xs[:-1]] + [top]
        add(name, (scaled(WAVE_X), WAVE_Y),
            (scaled(wave_test(662, 0.3)[0]), wave_test(662, 0.3)[1]),
            atolx=top / 50.0, atoly=0.2)

    # Rectangles wider than the sample spacing: loops at every peak and valley.
    peak_x = [float(k) for k in range(11)]
    peak_y = [0.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 5.0, 5.0, 0.0]
    add("sharp_peak_wide_rectangles", (peak_x, peak_y),
        perturbed([0.25 * k for k in range(41)],
                  [interpolate(peak_x, peak_y, 0.25 * k) for k in range(41)], 663, 0.6),
        atolx=1.5, atoly=0.1)
    saw_x = [float(k) for k in range(41)]
    saw_y = [float(k % 2) for k in range(41)]
    add("sawtooth_wide_rectangles", (saw_x, saw_y),
        perturbed(saw_x, saw_y, 664, 0.7), atolx=2.5, atoly=0.05)
    add("sawtooth_narrow_rectangles", (saw_x, saw_y),
        perturbed(saw_x, saw_y, 665, 0.2), atolx=0.25, atoly=0.05)
    # A rising sawtooth under a local x tolerance: each tooth's rectangle reaches back over most
    # of the teeth before it, so every loop's search is long (the shape whose work is quadratic).
    rising_x = [1.0 + k for k in range(41)]
    rising_y = [0.01 * k + (k % 2) for k in range(41)]
    add("rising_sawtooth_local_tolerance", (rising_x, rising_y),
        perturbed(rising_x, rising_y, 673, 0.6), ltolx=0.9)

    # Negative x with local tolerances: the half-width shrinks to the floor at x = 0.
    negative_x = [x - 10.0 for x in WAVE_X]
    add("negative_x_local_tolerance", (negative_x, WAVE_Y),
        ([x - 10.0 for x in wave_test(666, 0.4)[0]], wave_test(666, 0.4)[1]),
        ltolx=0.05, ltoly=0.1)

    # Every x negative: the magnitude that normalises x is the absolute smallest, not the largest.
    add("negative_x", ([x - 30.0 for x in WAVE_X], WAVE_Y),
        ([x - 30.0 for x in wave_test(672, 0.4)[0]], wave_test(672, 0.4)[1]),
        atolx=0.3, rtoly=0.05)

    # Negative zero in both columns.
    add("negative_zero", ([-0.0, 1.0, 2.0, 3.0], [0.0, -0.0, 1.0, -0.0]),
        ([0.0, 1.0, 2.0, 3.0], [-0.0, 0.05, 1.0, 0.2]), atoly=0.1)

    # Noise: an irregular random walk under wide, narrow and local rectangles.
    walk = noisy(667, 240)
    add("noisy_wide_rectangles", walk, perturbed(*walk, 668, 0.8), rtolx=0.01, rtoly=0.01)
    add("noisy_narrow_rectangles", walk, perturbed(*walk, 669, 0.01), atolx=1e-3, atoly=1e-3)
    add("noisy_local_tolerance", walk, perturbed(*walk, 670, 0.4), ltolx=0.02, ltoly=0.05)
    add("noisy_every_tolerance", walk, perturbed(*walk, 671, 0.4),
        atolx=0.5, atoly=0.05, rtolx=0.002, rtoly=0.02, ltolx=0.004, ltoly=0.03)

    # Endpoints: funnel refuses series that do not start, or do not end, at the same x, and
    # decides "the same" by its own equality, relative above magnitude one.
    add("first_x_differs", (WAVE_X, WAVE_Y),
        ([0.001] + WAVE_X[1:], WAVE_Y), atolx=0.3, atoly=0.25)
    add("last_x_differs", (WAVE_X, WAVE_Y),
        (WAVE_X[:-1] + [20.001], WAVE_Y), atolx=0.3, atoly=0.25)
    add("both_ends_differ", (WAVE_X, WAVE_Y),
        ([0.5] + WAVE_X[1:-1] + [19.5], WAVE_Y), atolx=0.3, atoly=0.25)
    million_x = [1e6 * (x / 20.0) for x in WAVE_X]
    add("ends_equal_within_tolerance", (million_x, WAVE_Y),
        ([5e-11] + million_x[1:-1] + [1000000.00005], WAVE_Y), atolx=2e4, atoly=0.25)
    add("last_x_just_differs", (million_x, WAVE_Y),
        (million_x[:-1] + [1000000.00015], WAVE_Y), atolx=2e4, atoly=0.25)
    # The test's last x is equal to the reference's by funnel's equality and still lies beyond
    # the bounds' last corner: the bounds are held there, not extrapolated.
    # The wave ends rising, so the lower bound's last segment slopes and the upper one's is flat:
    # the test is below the band, where holding and extrapolating that segment differ.
    add("test_ends_beyond_the_bounds", (million_x, WAVE_Y),
        (million_x[:-1] + [1000000.00005], [y - 0.3 for y in WAVE_Y]), atolx=1e-5, atoly=0.25)

    # A run that is flat by funnel's equality and not by its slopes: wiggles of 5e-11 on a
    # millisecond grid change the slope at every point, and the corners they add are dropped
    # again because the level has not moved.
    wiggle_x = [0.001 * k for k in range(12)]
    wiggle_y = [1.0, 1.0, 1.0 + 5e-11, 1.0, 1.0 + 5e-11, 1.0 + 5e-11, 1.0, 1.0 - 5e-11, 1.0,
                2.0, 2.0 + 5e-11, 2.0]
    add("near_flat_wiggle", (wiggle_x, wiggle_y),
        (wiggle_x, [y + 0.02 * ((k % 3) - 1) for k, y in enumerate(wiggle_y)]),
        atolx=0.0002, atoly=0.01)

    # An identical series on a tick grid that leaves its own band: a step of two million, an x
    # tolerance of exactly one tick (so a corner of the bound sits on a sample) and no y
    # tolerance (so the band is only the 1e-10 floor high). Interpolating along the step rounds
    # by more than that. The same series a millionth as high stays inside.
    add("identical_large_step_no_y_tolerance",
        ([0.0, 60.0, 120.0], [2000000.0, 0.0, 0.0]),
        ([0.0, 60.0, 120.0], [2000000.0, 0.0, 0.0]), atolx=60.0)
    add("identical_small_step_no_y_tolerance",
        ([0.0, 60.0, 120.0], [2.0, 0.0, 0.0]),
        ([0.0, 60.0, 120.0], [2.0, 0.0, 0.0]), atolx=60.0)

    # Three more identical series that funnel reports outside their own band, each by the way
    # its equality merges corners or points, not by rounding. All have a y tolerance.
    # Three samples at one instant that do not rise or fall in order, with no x tolerance: the
    # rectangles are the 1e-10 floor wide, which funnel's equality cannot tell from zero once
    # x is a few units large, and the spike's corners collapse.
    add("identical_spike_at_one_instant",
        ([0.0, 100.0, 100.0, 100.0, 200.0], [0.0, 2.0, 0.0, 1.0, 0.0]),
        ([0.0, 100.0, 100.0, 100.0, 200.0], [0.0, 2.0, 0.0, 1.0, 0.0]), atoly=0.1)
    # A pulse whose two steps are each written as two samples at one instant, at x in the
    # millions, with no x tolerance: the same, for a step.
    pulse_x = [0.0, 1e6, 1e6, 2e6, 2e6, 3e6]
    pulse_y = [0.0, 0.0, 1.0, 1.0, 0.0, 0.0]
    add("identical_pulse_by_equal_times_at_large_x", (pulse_x, pulse_y), (pulse_x, pulse_y),
        atoly=0.1)
    # Epoch seconds in steps of a tenth: x over its step is 1.7e10, and funnel's equality,
    # 1e-10 relative, reads neighbouring instants as one.
    epoch_x = [1.7e9 + 0.1 * k for k in range(12)]
    add("identical_epoch_seconds_tenth_steps", (epoch_x, WAVE_Y[:12]), (epoch_x, WAVE_Y[:12]),
        atoly=0.1)

    # Three shapes a seeded search of random cases found, kept with the numbers it found them
    # at, because each takes a branch of the loop removal that no case above takes.
    # The two segments around a loop are parallel by funnel's equality: no corner is added.
    add("loop_between_parallel_segments",
        ([0.0, 1000.0, 1500.0], [1000000.0, 1000000.523871642, 999999.9542734216]),
        ([0.0, 1000.0, 1500.0], [1000000.0003761144, 1000000.5245320182, 999999.9554283033]),
        atolx=7513.223201215218, ltolx=0.0010462830632038287, ltoly=1.008424077201712,
        rtolx=0.0013205533361355549, rtoly=0.0005018068265039262)
    # A whole series at one instant: the corners of the upper bound share their x.
    add("series_at_one_instant",
        ([0.5, 0.5, 0.5], [0.0, -0.7334956518229883, -0.35824763736146026]),
        ([0.5, 0.5, 0.5], [0.0, -0.7334956518229883, -0.35824763736146026]),
        rtolx=0.1032121663602923, rtoly=0.05805394352408241)
    # The same at a huge x with a local tolerance: the corners of the lower bound share theirs.
    add("series_at_one_huge_instant",
        ([1700000000000.0] * 3, [1.0, 1.0000009102205005, 1.0000000344147626]),
        ([1700000000000.0] * 3, [0.9999999952094345, 1.0000009185694374, 1.0000000273935108]),
        atoly=2.384213967052036e-09, ltolx=0.02075672568409159, rtoly=0.0007550054411901956)
    return out


def write_series(path, header, series):
    xs, ys = series
    assert len(xs) == len(ys) and xs, path
    with open(path, "w", newline="\n") as handle:
        handle.write(header + "\n")
        for x, y in zip(xs, ys):
            handle.write("{!r},{!r}\n".format(x, y))


def main(root):
    hostile = os.path.join(root, "hostile")
    for name, (reference, test, tolerances) in sorted(cases().items()):
        directory = os.path.join(hostile, name)
        os.makedirs(directory, exist_ok=True)
        write_series(os.path.join(directory, "reference.csv"), "x,y", reference)
        write_series(os.path.join(directory, "test.csv"), "x,y", test)
        param = {"test": "test.csv", "reference": "reference.csv", "output": "results"}
        param.update(sorted(tolerances.items()))
        with open(os.path.join(directory, "param.json"), "w", newline="\n") as handle:
            handle.write(json.dumps(param) + "\n")
    print("wrote {} hostile cases".format(len(cases())))


if __name__ == "__main__":
    main(sys.argv[1])
