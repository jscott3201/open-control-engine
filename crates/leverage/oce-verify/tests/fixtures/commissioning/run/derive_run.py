#!/usr/bin/env python3
"""Derive the verdicts of the run fixture's oracle case, independently of the crate.

    python3 -I derive_run.py <fixture-directory> > oracle.txt
    python3 -I derive_run.py <fixture-directory> change-of-value > change_of_value/oracle.txt

The second form derives the change-of-value cases (a reference with no sampling; the design
note's "As landed: the change-of-value comparison", ruled by the owner on 2026-11-22): every
case directory under change_of_value/ (spec.json, main.csv, limit.csv, outputs.csv, fan.csv),
each output's instants (the window's ends, the series' and its indicator's sample times, the
uncovered stretches' ends, a Real output's funnel corners snapped onto a series instant within
4 eps (m + w)), the step series, the largest error over each stretch read from inside it at both
ends, the masked runs as stretches, the trend's last sample at or before stop, and the ids of the
golden receipts there. Only the funnel: aligned-grid with no sampling is refused. It is
tests/commissioning/run_change_of_value.rs's independent side.

This is the independent side of tests/commissioning/run_receipts.rs: a second implementation,
sharing no code with the crate, of what docs/design/commissioning.md says a commissioning run
does to the `plant` sequence of tests/fixtures/subjects/plant.cxf.json, for the reference of
oracle_spec.json, once under each envelope:

- the unit table's three rows the fixture uses (degC -> K, degC_delta -> K, percent -> 1);
- the frames at the union of the two input sources' times, a frame refused when a source has no
  row there and no hold (or a hold older than its maxAge), and the uncovered stretches: from a
  refused frame's time up to the next executed frame, or to stop (decision 28);
- the sequence itself, small enough to state: TSet = T + dT, enOut = en, nOut = n, limOut = lim,
  sampled at the executed frames;
- the comparison instants: start + k * s while at or before stop, then stop when the grid does
  not reach it; each series held there, the last sample at or before (decision 14);
- the indicator fanSta.y on limOut (the glob plant.lim*), held and false before its first sample
  (decision 2); a masked instant first, then an uncovered one, else compared; both series zeroed
  where not compared;
- the funnel envelope (the note's "Envelope" steps 1 to 5, funnel's equality, x normalised by its
  magnitude) for a case with no loop to remove, which the script checks and refuses otherwise;
  and the aligned-grid envelope of oce-conformance (summed terms, a grid of every reference x
  and rectangle edge); either read at the instants only (decision 32), the engine the reference;
- a Boolean or Integer output compared exactly, a mismatch an exceedance of 1 (decision 8);
- the report's figures (the note's output table).

It also recomputes the `receipt_id` of each golden receipt under receipts/: FNV-1a-128 over the
receipt's canonical text with its `receipt_id` line and its `timings` block cut out (and the
comma the line before them then no longer needs), `receipt:fnv1a128:<32 hex>`.

Every float is written as the 16 hex digits of its bits, a NaN as `nan`. No test runs this
script and nothing blesses its output: run it by hand when the fixture changes and review the
diff of oracle.txt.
"""

import csv
import json
import struct
import sys
from pathlib import Path


def bits(x):
    if x != x:
        return "nan"
    return struct.pack(">d", x).hex()


def read_csv(path):
    with open(path, newline="") as handle:
        rows = list(csv.reader(handle))
    return [dict(zip(rows[0], row)) for row in rows[1:]]


def boolean(text):
    return {"true": True, "false": False, "1": True, "0": False}[text]


def kelvin(celsius):
    return celsius + 273.15


def ratio(percent):
    return percent / 100.0


def held_index(times, t):
    index = None
    for i, x in enumerate(times):
        if x <= t:
            index = i
    return index


# Funnel, from the design note's "Envelope" section.
def equ(a, b):
    magnitude = max(abs(a), abs(b))
    return abs(a - b) <= 1e-10 * (magnitude if magnitude > 1.0 else 1.0)


def larger(a, b):
    return a if a > b else b


def sign(v):
    return 1 if v > 0 else (-1 if v < 0 else 0)


def funnel(reference, test, tol):
    lower, upper, hw, hh = funnel_bounds(reference, tol)
    return errors_at(lower, upper, test), hw, hh


def funnel_bounds(reference, tol):
    """The funnel's lower and upper bound around `reference`, and the half-sizes' extremes."""
    rx = [p[0] for p in reference]
    ry = [p[1] for p in reference]
    min_x = max_x = rx[0]
    min_y = max_y = ry[0]
    for v in rx:
        min_x, max_x = (v if v < min_x else min_x), (v if v > max_x else max_x)
    for v in ry:
        min_y, max_y = (v if v < min_y else min_y), (v if v > max_y else max_y)
    range_x, range_y = max_x - min_x, max_y - min_y
    mag_x, mag_y = larger(max_x, abs(min_x)), larger(max_y, abs(min_y))

    def sizes(values, atol, rtol, ltol, rng, mag):
        out = []
        for v in values:
            size = larger(larger(atol, rtol * rng), ltol * abs(v))
            if equ(size, 0.0):
                if not equ(rtol, 0.0):
                    size = larger(size, rtol * mag)
                if equ(size, 0.0):
                    size = larger(size, 1e-10)
            out.append(size)
        return out

    hx = sizes(rx, tol["atolx"], tol["rtolx"], tol["ltolx"], range_x, mag_x)
    hy = sizes(ry, tol["atoly"], tol["rtoly"], tol["ltoly"], range_y, mag_y)
    norm = mag_x > 1e-5
    xn = [v / mag_x for v in rx] if norm else rx[:]
    hxn = [v / mag_x for v in hx] if norm else hx[:]

    def slope(i):
        rise = ry[i + 1] - ry[i]
        d = sign(rise)
        if not equ(xn[i + 1], xn[i]):
            return d, rise / (xn[i + 1] - xn[i])
        return d, (1e15 if d > 0 else -1e15)

    def outline(upper):
        level = (lambda i: ry[i] + hy[i]) if upper else (lambda i: ry[i] - hy[i])
        towards = 1 if upper else -1

        def corner(edge, i):
            return ((xn[i] - hxn[i]) if edge == "L" else (xn[i] + hxn[i]), level(i))

        def interior(before, after):
            rising = before != -1 and after != -1
            falling = before != 1 and after != 1
            valley = before == -1 and after == 1
            if upper:
                if rising:
                    return "L"
                if falling:
                    return "R"
                return "LR" if not valley else "RL"
            if rising:
                return "R"
            if falling:
                return "L"
            return "LR" if valley else "RL"

        count = len(xn)
        corners = []
        first = 0
        while first + 1 < count and equ(xn[first], xn[first + 1]) and equ(ry[first], ry[first + 1]):
            first += 1
        corners.append(corner("L", first))
        if first + 1 < count:
            before, slope_before = slope(first)
            if before == -towards:
                corners.append(corner("R", first))
            for i in range(first + 1, count - 1):
                if equ(xn[i], xn[i + 1]) and equ(ry[i], ry[i + 1]):
                    continue
                after, slope_after = slope(i)
                if not equ(slope_before, slope_after):
                    for edge in interior(before, after):
                        corners.append(corner(edge, i))
                    extremum = before * after == -1
                    n = len(corners)
                    last = corners[-1][1]
                    if equ(level(i + 1), last):
                        if extremum and n >= 3 and equ(corners[n - 3][1], last):
                            del corners[n - 2:]
                        elif not extremum and n >= 2 and equ(corners[n - 2][1], last):
                            del corners[n - 1:]
                before, slope_before = after, slope_after
            if before == towards:
                corners.append(corner("L", count - 1))
        corners.append(corner("R", count - 1))
        for a, b in zip(corners, corners[1:]):
            if b[0] < a[0]:
                sys.exit("derive_run.py: a loop to remove; this oracle covers loop-free cases only")
        return [((x * mag_x) if norm else x, y) for x, y in corners]

    return outline(False), outline(True), (min(hx), max(hx)), (min(hy), max(hy))


def errors_at(lower_bound, upper_bound, test):
    """Funnel's step 5: both bounds interpolated at each test x, and the error there."""

    def bound_at(bound, xs):
        values = []
        end = 1
        for x in xs:
            if x > bound[-1][0]:
                values += [bound[-1][1]] * (len(xs) - len(values))
                break
            while bound[end][0] < x and end + 1 < len(bound):
                end += 1
            x0, y0 = bound[end - 1]
            x1, y1 = bound[end]
            values.append(y0 + (((y1 - y0) / (x1 - x0)) * (x - x0)) if not equ((x1 - x0) * (x - x0), 0.0) else y0)
        return values

    lower = bound_at(lower_bound, [p[0] for p in test])
    upper = bound_at(upper_bound, [p[0] for p in test])
    errors = []
    for (_, y), low, up in zip(test, lower, upper):
        errors.append(low - y if y < low else (y - up if y > up else 0.0))
    return errors


# The aligned-grid envelope, from oce-conformance's funnel.rs as the design note describes it.
def value_at(curve, x):
    if x <= curve[0][0]:
        return curve[0][1]
    for k in range(1, len(curve)):
        if curve[k][0] >= x:
            x0, y0 = curve[k - 1]
            x1, y1 = curve[k]
            if x == x1 or x1 == x0:
                return y1
            return y0 + (x - x0) / (x1 - x0) * (y1 - y0)
    return curve[-1][1]


def aligned_grid(reference, test, tol):
    lower, upper, hw, hh = aligned_grid_bounds(reference, tol)
    errors = []
    for x, y in test:
        low, high = value_at(lower, x), value_at(upper, x)
        above, below = y - high, y - low
        errors.append((above if above > 0 else 0.0) - (below if below < 0 else 0.0))
    return errors, hw, hh


def aligned_grid_bounds(reference, tol):
    """The aligned-grid lower and upper bound around `reference`, and the half-sizes' extremes."""
    xs = [p[0] for p in reference]
    ys = [p[1] for p in reference]
    range_x, range_y = max(xs) - min(xs), max(ys) - min(ys)
    rects = []
    for x, y in reference:
        w = tol["atolx"] + tol["rtolx"] * range_x + tol["ltolx"] * abs(x)
        h = tol["atoly"] + tol["rtoly"] * range_y + tol["ltoly"] * abs(y)
        rects.append((x, y, w, h))
    grid = sorted({v for x, _, w, _ in rects for v in (x - w, x, x + w)})
    lower, upper = [], []
    for g in grid:
        y = value_at(reference, g)
        low, high = y, y
        for x, ry, w, h in rects:
            if x - w <= g <= x + w:
                low, high = min(low, ry - h), max(high, ry + h)
        lower.append((g, low))
        upper.append((g, high))
    return lower, upper, (min(r[2] for r in rects), max(r[2] for r in rects)), (
        min(r[3] for r in rects), max(r[3] for r in rects))


def report(name, kind, times, fates, exceedances, extra, stretches=False, last=None):
    out = [f"output\t{name}", f"type\t{kind}"]
    compared = [k for k, f in enumerate(fates) if f == "compared"]
    out.append(f"counts\t{len(compared)}\t{fates.count('masked')}\t{fates.count('uncovered')}")
    failing = [k for k in compared if exceedances[k] > 0]
    status = "no_data" if not compared else ("fail" if failing else "pass")
    out.append(f"status\t{status}")
    if status == "fail":
        worst = max(exceedances[k] for k in compared)
        at = next(k for k in compared if exceedances[k] == worst)
        out.append(f"worst\t{bits(times[at])}\t{bits(worst)}")
        out.append(f"first\t{bits(times[failing[0]])}")
    total = 0.0
    for k in failing:
        if k + 1 < len(times):
            total += times[k + 1] - times[k]
    out.append(f"time\t{bits(total)}")
    if last is not None:
        out.append(f"last_sample\t{bits(last)}")
    run = None
    for t, f in zip(times, fates):
        if f == "masked":
            run = (run[0], t) if run else (t, t)
        elif run:
            # By change of value a run ends where its last stretch ends: this instant.
            out.append(f"masked_interval\t{bits(run[0])}\t{bits(t if stretches else run[1])}")
            run = None
    if run:
        out.append(f"masked_interval\t{bits(run[0])}\t{bits(run[1])}")
    out.extend(extra)
    for t, e in zip(times, exceedances):
        out.append(f"error\t{bits(t)}\t{bits(e)}")
    return out


FNV_OFFSET = 0x6C62272E07BB014262B821756295C58D
FNV_PRIME = 0x0000000001000000000000000000013B


def fnv1a128(data):
    value = FNV_OFFSET
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) % (1 << 128)
    return value


def receipt_id(text):
    """The id of a canonical receipt: its `receipt_id` and `timings` cut out."""
    kept, skipping = [], False
    for line in text.split("\n"):
        if line.startswith('  "receipt_id": '):
            continue
        if line == '  "timings": {':
            skipping = True
            continue
        if skipping:
            if line.startswith("  }"):
                skipping = False
            continue
        kept.append(line)
    closing = kept.index("}")
    kept[closing - 1] = kept[closing - 1].rstrip(",")
    return "receipt:fnv1a128:%032x" % fnv1a128("\n".join(kept).encode("ascii"))


def case(root, envelope):
    main = read_csv(root / "oracle_main.csv")
    limit = read_csv(root / "oracle_limit.csv")
    outputs = read_csv(root / "oracle_outputs.csv")
    fan = read_csv(root / "fan.csv")
    start, stop, step, max_age = 0.0, 33.0, 5.0, 5.0
    main_times = [float(r["time"]) for r in main]
    limit_times = [float(r["time"]) for r in limit]
    frames = sorted(set(main_times) | set(limit_times))
    executed, stretches, open_from = [], [], None
    for t in frames:
        has_main = t in main_times
        last_limit = held_index(limit_times, t)
        has_limit = last_limit is not None and t - limit_times[last_limit] <= max_age
        if has_main and has_limit:
            executed.append(t)
            if open_from is not None and t > open_from:
                stretches.append((open_from, t))
            open_from = None
        elif open_from is None:
            open_from = t
    if open_from is not None:
        stretches.append((open_from, None))

    def engine_at(t, column):
        row = main[held_index(main_times, t)]
        lim = limit[held_index(limit_times, t)]
        return {
            "TSet": kelvin(float(row["Supply Temp"])) + float(row["Offset"]),
            "enOut": boolean(row["Enable"]),
            "nOut": int(row["Stage"]),
            "limOut": ratio(float(lim["Limit"])),
        }[column]

    trend_times = [float(r["time"]) for r in outputs]
    trend = {
        "TSet": [kelvin(float(r["Setpoint"])) for r in outputs],
        "enOut": [boolean(r["Enabled"]) for r in outputs],
        "nOut": [int(r["Staged"]) for r in outputs],
        "limOut": [ratio(float(r["Limited"])) for r in outputs],
    }
    fan_times = [float(r["time"]) for r in fan]
    fan_values = [boolean(r["Fan Status"]) for r in fan]
    instants, k = [], 0
    while start + k * step <= stop:
        instants.append(start + k * step)
        k += 1
    if instants[-1] < stop:
        instants.append(stop)

    def uncovered(t):
        return any(t >= a and (b is None or t < b) for a, b in stretches)

    tolerances = {
        "TSet": {"atolx": 1.0, "atoly": 0.05, "rtolx": 0.0, "rtoly": 0.0, "ltolx": 0.0, "ltoly": 0.0},
        "limOut": {"atolx": 1.0, "atoly": 0.01, "rtolx": 0.0, "rtoly": 0.0, "ltolx": 0.0, "ltoly": 0.0},
    }
    lines = [f"case\t{envelope}"]
    for name in ["TSet", "enOut", "limOut", "nOut"]:
        fates = []
        for t in instants:
            value = False
            for x, v in zip(fan_times, fan_values):
                if x <= t:
                    value = v
            if name == "limOut" and not value:
                fates.append("masked")
            elif uncovered(t):
                fates.append("uncovered")
            else:
                fates.append("compared")
        # The engine's series is its executed frames, held.
        engine = [engine_at(executed[held_index(executed, t)], name) for t in instants]
        test_values = [trend[name][held_index(trend_times, t)] for t in instants]
        if name in tolerances:
            reference = [(t, v if f == "compared" else 0.0) for t, v, f in zip(instants, engine, fates)]
            test = [(t, v if f == "compared" else 0.0) for t, v, f in zip(instants, test_values, fates)]
            compare = funnel if envelope == "funnel" else aligned_grid
            errors, hw, hh = compare(reference, test, tolerances[name])
            shown = [e if f != "uncovered" else float("nan") for e, f in zip(errors, fates)]
            extra = [f"half_width\t{bits(hw[0])}\t{bits(hw[1])}", f"half_height\t{bits(hh[0])}\t{bits(hh[1])}"]
            extra += [f"masked_exceedance\t{bits(t)}\t{bits(e)}" for t, e, f in zip(instants, errors, fates) if f == "masked" and e > 0]
            lines += report(name, "real", instants, fates, shown, extra)
        else:
            exceedances = [
                (1.0 if a != b else 0.0) if f == "compared" else (0.0 if f == "masked" else float("nan"))
                for a, b, f in zip(engine, test_values, fates)
            ]
            lines += report(name, "discrete", instants, fates, exceedances, [])
    return lines


# The change-of-value comparison (a reference with no sampling; the design note's "As landed: the
# change-of-value comparison", ruled by the owner on 2026-11-22), derived from its statement.
def held_at(times, t):
    """The index of the last sample at or before t (the last of equal times), or None."""
    found = None
    for i, x in enumerate(times):
        if x <= t:
            found = i
    return found


def steps(instants, values):
    """A series held at instants as funnel points: the held value carried to each change first."""
    points = []
    for k, (t, v) in enumerate(zip(instants, values)):
        if k > 0 and v != values[k - 1]:
            points.append((t, values[k - 1]))
        points.append((t, v))
    return points


def read_bound(bound, t, side):
    """The bound at t from the stretch after t ("right") or the one before it ("left")."""
    xs = [p[0] for p in bound]
    ys = [p[1] for p in bound]
    if side == "right":
        at_or_before = [i for i, x in enumerate(xs) if x <= t]
        if not at_or_before:
            return ys[0]
        j = at_or_before[-1]
        if xs[j] == t or j == len(xs) - 1:
            return ys[j]
        return ys[j] + (((ys[j + 1] - ys[j]) / (xs[j + 1] - xs[j])) * (t - xs[j]))
    at_or_after = [i for i, x in enumerate(xs) if x >= t]
    if not at_or_after:
        return ys[-1]
    j = at_or_after[0]
    if xs[j] == t or j == 0:
        return ys[j]
    return ys[j - 1] + (((ys[j] - ys[j - 1]) / (xs[j] - xs[j - 1])) * (t - xs[j - 1]))


def outside(y, low, up):
    return low - y if y < low else (y - up if y > up else 0.0)


def change_case(directory):
    spec = json.loads((directory / "spec.json").read_text())
    ref = spec["references"][0]
    start = float(ref["window"]["start"])
    stop = float(ref["window"]["stop"])
    if ref.get("envelope", "funnel") != "funnel":
        sys.exit("derive_run.py: aligned-grid with no sampling is refused (sampling-required)")
    role = ref.get("funnelReference", "engine")
    max_age = float(ref["inputs"][1]["hold"]["maxAge"])
    tolerance_base = {name: 0.0 for name in ["atolx", "atoly", "rtolx", "rtoly", "ltolx", "ltoly"]}
    tolerance_base.update({k: float(v) for k, v in spec["tolerances"].items()})
    main = read_csv(directory / "main.csv")
    limit = read_csv(directory / "limit.csv")
    outputs = read_csv(directory / "outputs.csv")
    fan = read_csv(directory / "fan.csv")
    main_times = [float(r["time"]) for r in main]
    limit_times = [float(r["time"]) for r in limit]
    frames = sorted(set(main_times) | set(limit_times))
    executed, stretches, open_from = [], [], None
    for t in frames:
        last_limit = held_at(limit_times, t)
        has_limit = last_limit is not None and t - limit_times[last_limit] <= max_age
        if t in main_times and has_limit:
            executed.append(t)
            if open_from is not None and t > open_from:
                stretches.append((open_from, t))
            open_from = None
        elif open_from is None:
            open_from = t
    if open_from is not None:
        stretches.append((open_from, None))
    ends = sorted([a for a, _ in stretches] + [b for _, b in stretches if b is not None])

    def engine_value(t, name):
        row = main[held_at(main_times, t)]
        lim = limit[held_at(limit_times, t)]
        return {
            "TSet": kelvin(float(row["Supply Temp"])) + float(row["Offset"]),
            "enOut": boolean(row["Enable"]),
            "nOut": int(row["Stage"]),
            "limOut": ratio(float(lim["Limit"])),
        }[name]

    trend_times = [float(r["time"]) for r in outputs]
    trend = {
        "TSet": [kelvin(float(r["Setpoint"])) for r in outputs],
        "enOut": [boolean(r["Enabled"]) for r in outputs],
        "nOut": [int(r["Staged"]) for r in outputs],
        "limOut": [ratio(float(r["Limited"])) for r in outputs],
    }
    fan_times = [float(r["time"]) for r in fan]
    fan_values = [boolean(r["Fan Status"]) for r in fan]
    engine_series = {name: [engine_value(t, name) for t in executed]
                     for name in ["TSet", "enOut", "nOut", "limOut"]}
    lines = [f"case\t{directory.name}"]
    for name in ["TSet", "enOut", "limOut", "nOut"]:
        real = name in ("TSet", "limOut")
        kind = "real" if real else "discrete"
        gated = name == "limOut"
        times = set([start, stop]) | set(executed) | set(trend_times) | set(ends)
        if gated:
            times |= set(fan_times)
        instants = sorted(t for t in times if start <= t <= stop)
        engine_at = [held_at(executed, t) for t in instants]
        trend_at = [held_at(trend_times, t) for t in instants]
        if engine_at[0] is None or trend_at[0] is None:
            lines += report(name, kind, [], [], [], [])
            continue
        fates = []
        for t in instants:
            on = True
            if gated:
                index = held_at(fan_times, t)
                on = index is not None and fan_values[index]
            if not on:
                fates.append("masked")
            elif any(t >= a and (b is None or t < b) for a, b in stretches):
                fates.append("uncovered")
            else:
                fates.append("compared")
        # The trend's sample held at stop: its last at or before stop.
        last = trend_times[trend_at[-1]]
        engine = [engine_series[name][i] for i in engine_at]
        test_values = [trend[name][i] for i in trend_at]
        if not real:
            exceedances = [
                (1.0 if a != b else 0.0) if f == "compared" else (0.0 if f == "masked" else float("nan"))
                for a, b, f in zip(engine, test_values, fates)
            ]
            lines += report(name, kind, instants, fates, exceedances, [], stretches=True, last=last)
            continue
        engine = [v if f == "compared" else 0.0 for v, f in zip(engine, fates)]
        test_values = [v if f == "compared" else 0.0 for v, f in zip(test_values, fates)]
        if "compared" not in fates:
            lines += report(name, kind, instants, fates, [float("nan")] * len(instants), [], stretches=True,
                            last=last)
            continue
        reference, test = (engine, test_values) if role == "engine" else (test_values, engine)
        tolerances = dict(tolerance_base)
        if name == "TSet":
            tolerances["atoly"] = 0.05
        lower, upper, hw, hh = funnel_bounds(steps(instants, reference), tolerances)
        # A corner within 4 eps (m + w) of a series instant lies on it (m the x magnitude, w the
        # largest half-width), the nearest instant, the earlier of two equally near.
        magnitude = stop if stop > abs(start) else abs(start)
        radius = 4.0 * (2.0 ** -52) * (magnitude + hw[1])

        def snap(x):
            nearest = min(instants, key=lambda t: (abs(x - t), t))
            return nearest if abs(x - nearest) <= radius else x

        lower = [(snap(x), y) for x, y in lower]
        upper = [(snap(x), y) for x, y in upper]
        corners = {x for x, _ in lower} | {x for x, _ in upper}
        pieces = sorted(set(instants) | {x for x in corners if start < x < stop})
        piece_fates, piece_test = [], []
        for t in pieces:
            k = held_at(instants, t)
            piece_fates.append(fates[k])
            piece_test.append(test[k])
        errors = []
        for k, (t, y) in enumerate(zip(pieces, piece_test)):
            last_error = outside(y, read_bound(lower, t, "left"), read_bound(upper, t, "left"))
            if k + 1 < len(pieces):
                first_error = outside(y, read_bound(lower, t, "right"), read_bound(upper, t, "right"))
                end = pieces[k + 1]
                end_error = outside(y, read_bound(lower, end, "left"), read_bound(upper, end, "left"))
                errors.append(end_error if end_error > first_error else first_error)
            else:
                errors.append(last_error)
        shown = [e if f != "uncovered" else float("nan") for e, f in zip(errors, piece_fates)]
        extra = [f"half_width\t{bits(hw[0])}\t{bits(hw[1])}", f"half_height\t{bits(hh[0])}\t{bits(hh[1])}"]
        extra += [f"masked_exceedance\t{bits(t)}\t{bits(e)}"
                  for t, e, f in zip(pieces, errors, piece_fates) if f == "masked" and e > 0]
        lines += report(name, kind, pieces, piece_fates, shown, extra, stretches=True, last=last)
    return lines


def main(directory, mode=None):
    root = Path(directory)
    if mode == "change-of-value":
        lines = []
        for case_directory in sorted(p for p in (root / "change_of_value").iterdir() if p.is_dir()):
            lines += change_case(case_directory)
        for golden in sorted((root / "change_of_value").glob("*.receipt.json")):
            lines.append(f"receipt\t{golden.name}\t{receipt_id(golden.read_text(encoding='ascii'))}")
        sys.stdout.write("# derive_run.py over the run fixture's change-of-value cases\n")
        sys.stdout.write("\n".join(lines) + "\n")
        return
    lines = case(root, "funnel") + case(root, "aligned-grid")
    for golden in sorted((root / "receipts").glob("*.receipt.json")):
        lines.append(f"receipt\t{golden.name}\t{receipt_id(golden.read_text(encoding='ascii'))}")
    sys.stdout.write("# derive_run.py over the run fixture's oracle case\n")
    sys.stdout.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:3])
