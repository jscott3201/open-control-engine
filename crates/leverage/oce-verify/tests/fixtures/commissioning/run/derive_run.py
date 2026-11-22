#!/usr/bin/env python3
"""Derive the verdicts of the run fixture's oracle case, independently of the crate.

    python3 -I derive_run.py <fixture-directory> > oracle.txt

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

    lower = bound_at(outline(False), [p[0] for p in test])
    upper = bound_at(outline(True), [p[0] for p in test])
    errors = []
    for (_, y), low, up in zip(test, lower, upper):
        errors.append(low - y if y < low else (y - up if y > up else 0.0))
    return errors, (min(hx), max(hx)), (min(hy), max(hy))


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
    errors = []
    for x, y in test:
        low, high = value_at(lower, x), value_at(upper, x)
        above, below = y - high, y - low
        errors.append((above if above > 0 else 0.0) - (below if below < 0 else 0.0))
    return errors, (min(r[2] for r in rects), max(r[2] for r in rects)), (
        min(r[3] for r in rects), max(r[3] for r in rects))


def report(name, kind, times, fates, exceedances, extra):
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
    run = None
    for t, f in zip(times, fates):
        if f == "masked":
            run = (run[0], t) if run else (t, t)
        elif run:
            out.append(f"masked_interval\t{bits(run[0])}\t{bits(run[1])}")
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


def main(directory):
    root = Path(directory)
    lines = case(root, "funnel") + case(root, "aligned-grid")
    for golden in sorted((root / "receipts").glob("*.receipt.json")):
        lines.append(f"receipt\t{golden.name}\t{receipt_id(golden.read_text(encoding='ascii'))}")
    sys.stdout.write("# derive_run.py over the run fixture's oracle case\n")
    sys.stdout.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1])
