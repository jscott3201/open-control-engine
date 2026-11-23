"""Print, for each OpenModelica result here, the instants at which each counter fired.

Run: python3 -I fires.py [DIR]   (DIR defaults to this directory; prints results.txt)
A counter is an Integer `<name>.y` that a `when <condition> then y = pre(y) + 1` drives; the
result rows at an event instant come in pairs (before and after), so a fire is a row whose count
exceeds the previous row's. Real and Boolean outputs are printed at each whole second, last row.
"""

import csv
import sys
from pathlib import Path

COUNTERS = {
    "PulseSemantics.Pilot_res.csv": ["direct", "notRouted", "andLevel", "orLevel", "orSame",
                                     "xorPre", "andNotPre", "edgeNotChange", "edgeChange",
                                     "prePulse", "orLevel2", "s05", "s07", "s14"],
    "SampleSemantics.Pilot_res.csv": ["c1", "c14", "c1s"],
    "InitPulse_res.csv": [],
    "PulseReview.Pilot_res.csv": ["notSample", "sampleOrLevel", "orChangePreB", "norChangePreU",
                                  "orChangePreChange", "changeOfEdge", "andChangeEdgeNotChange"],
}
VALUES = {
    "PulseSemantics.Pilot_res.csv": ["lat.y"],
    "SampleSemantics.Pilot_res.csv": ["s05.y", "s1.y", "s2.y", "s14.y", "s1s.y"],
    "InitPulse_res.csv": ["nCha", "nEdg", "nSam"],
    "PulseReview.Pilot_res.csv": ["samplerNotSample.y", "latX.y", "togX.y", "timAcc.y"],
}


def main():
    here = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
    for name, counters in COUNTERS.items():
        rows = list(csv.reader((here / name).open()))
        head, body = rows[0], rows[1:]
        print(f"== {name}")
        for c in counters:
            i = head.index(f"{c}.y")
            fires, prev = [], None
            for row in body:
                v = int(float(row[i]))
                if prev is not None and v > prev:
                    fires.append(f"{float(row[0]):g}")
                prev = v
            print(f"{c}: {len(fires)} fires at {', '.join(fires) or '-'}")
        for v in VALUES[name]:
            i = head.index(v)
            last = {}
            for row in body:
                t = float(row[0])
                if abs(t - round(t)) < 1e-6:
                    last[round(t)] = float(row[i])
            print(f"{v} at 0, 1, 2, ... s (last row): {', '.join(f'{x:g}' for x in last.values())}")


main()
