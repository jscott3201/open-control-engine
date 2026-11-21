#!/usr/bin/env python3
"""Derive the oracle of the three-file trend fixture, sharing no code with the crate.

    python3 -I derive_trends.py trends_oracle.txt

This is the independent side of tests/commissioning/trend_oracle.rs. From ahu.mos, status.csv,
limits.csv and points.json beside it, as the spec.json beside it declares them, it writes:

1. `digest` rows: FNV-1a-128 of each file's bytes, computed here with Python integers;
2. `frame` rows: the merged timeline and cells, by the rule the issue states and the design
   note records: the frames are the union of the sources' times; at an instant where a source
   has k rows, its i-th row fills the i-th frame there, and there are as many frames as the
   source with the most rows has; a source with no i-th row is missing there;
3. `replay` rows: what replay feeds the engine at each frame, by replay's missing-sample policy
   (docs/design/replay.md, "Missing samples"): a missing input of a source with `hold.maxAge`
   takes the last sample observed for that input when `t - t_sample <= maxAge` (observations on
   refused rows count), otherwise, or with no hold, the frame is refused.

Each file is parsed here for its own shape only (it is not a general reader): the matrix named
`ahu` of ahu.mos, two csv files with a header row. Units are converted with the design note's
operations in its order, in Python's binary64 (degF to K: (x - 32) * (5 / 9) + 273.15; degC to
K: x + 273.15; percent to 1: x / 100). Instants are converted to whole and fractional seconds
from the origin with integer arithmetic. A Real is written as the 16 hex digits of its bits.

No test runs this script; run it by hand when the fixture changes and review the diff.
"""

import re
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ORIGIN_UTC_SECONDS = None  # set from the spec below


def fnv1a128(data):
    h = 0x6C62272E07BB014262B821756295C58D
    prime = 0x0000000001000000000000000000013B
    for byte in data:
        h = ((h ^ byte) * prime) % (1 << 128)
    return "fnv1a128:%032x" % h


def bits(x):
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def instant_seconds(text):
    """Seconds since the epoch of an RFC 3339 instant, as (whole, fraction digits)."""
    m = re.fullmatch(r"(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d):(\d\d)(\.\d+)?(Z|[+-]\d\d:\d\d)", text)
    year, month, day, hour, minute, second = (int(g) for g in m.groups()[:6])
    fraction = m.group(7) or ""
    zone = m.group(8)
    # Days from the civil date, by counting (a fixture's dates are all in 2018).
    days = 0
    for y in range(1970, year):
        days += 366 if (y % 4 == 0 and y % 100 != 0) or y % 400 == 0 else 365
    lengths = [31, 29 if year % 4 == 0 else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    days += sum(lengths[: month - 1]) + day - 1
    offset = 0
    if zone != "Z":
        sign = -1 if zone[0] == "-" else 1
        offset = sign * (int(zone[1:3]) * 3600 + int(zone[4:6]) * 60)
    whole = days * 86400 + hour * 3600 + minute * 60 + second - offset
    nanos = int((fraction[1:] + "000000000")[:9]) if fraction else 0
    return whole, nanos


def model_seconds(text, origin):
    whole, nanos = instant_seconds(text)
    o_whole, o_nanos = origin
    total = (whole - o_whole) * 1_000_000_000 + (nanos - o_nanos)
    w, r = divmod(total, 1_000_000_000)
    return float(w) + float(r) / 1e9


def read_ahu():
    text = (HERE / "ahu.mos").read_text()
    body = text.split("double ahu(6, 3)")[1]
    body = "\n".join(line.split("#")[0] for line in body.split("\n"))
    numbers = [float(tok) for tok in re.split(r"[\s,;]+", body) if tok]
    assert len(numbers) == 18
    rows = [numbers[i : i + 3] for i in range(0, 18, 3)]
    times = [r[0] + 0.0 for r in rows]
    ahu = [(r[1] - 32.0) * (5.0 / 9.0) + 273.15 for r in rows]
    zone = [r[2] + 273.15 for r in rows]
    return times, {"ahu.T": ahu, "zone.T": zone}


def read_status():
    lines = (HERE / "status.csv").read_text().splitlines()[1:]
    times, count, enable = [], [], []
    for line in lines:
        t, c, e = line.split(",")
        times.append(float(t))
        count.append(None if c == "" else int(float(c)))
        enable.append({"true": True, "1": True, "false": False, "0": False}[e])
    return times, {"n": count, "b": enable}


def read_limits(origin):
    lines = (HERE / "limits.csv").read_text().splitlines()[1:]
    times, limit = [], []
    for line in lines:
        # The third column, `Spare`, is not mapped: the reader ignores it.
        t, l, _ = line.split(",")
        times.append(model_seconds(t, origin))
        limit.append(None if l == "NA" else float(l) / 100.0)
    return times, {"lim": limit}


def main(out):
    origin = instant_seconds("2018-06-07T00:00:00-07:00")
    sources = [
        ("ahu.mos", 10.0, *read_ahu()),
        ("status.csv", 30.0, *read_status()),
        ("limits.csv", None, *read_limits(origin)),
    ]
    union = sorted(set(t for _, _, times, _ in sources for t in times))
    frames = []  # (t, {input: value or None})
    for t in union:
        counts = [sum(1 for x in times if x == t) for _, _, times, _ in sources]
        for k in range(max(counts)):
            cells = {}
            for _, _, times, columns in sources:
                rows = [i for i, x in enumerate(times) if x == t]
                for name, values in columns.items():
                    cells[name] = values[rows[k]] if k < len(rows) else None
            frames.append((t, cells))
    hold = {}
    for _, max_age, _, columns in sources:
        for name in columns:
            hold[name] = max_age
    names = sorted(hold)

    def show(name, value):
        if value is None:
            return "missing"
        if isinstance(value, bool):
            return "true" if value else "false"
        if isinstance(value, int):
            return str(value)
        return bits(value)

    lines = ["# written by derive_trends.py; do not edit"]
    for name, _, _, _ in sources:
        lines.append("digest\t%s\t%s" % (name, fnv1a128((HERE / name).read_bytes())))
    lines.append("columns\t" + "\t".join(names))
    for index, (t, cells) in enumerate(frames):
        lines.append(
            "frame\t%d\t%s\t" % (index, bits(t)) + "\t".join(show(n, cells[n]) for n in names)
        )
    last = {}  # input -> (time, value) of the last observed sample
    for index, (t, cells) in enumerate(frames):
        fed, status = {}, "executed"
        for n in names:
            if cells[n] is not None:
                fed[n] = cells[n]
            elif hold[n] is not None and n in last and t - last[n][0] <= hold[n]:
                fed[n] = last[n][1]
                status = "substituted" if status == "executed" else status
            else:
                status = "refused"
        for n in names:
            if cells[n] is not None:
                last[n] = (t, cells[n])
        shown = ["-" if status == "refused" else show(n, fed[n]) for n in names]
        lines.append("replay\t%d\t%s\t" % (index, status) + "\t".join(shown))
    Path(out).write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1])
