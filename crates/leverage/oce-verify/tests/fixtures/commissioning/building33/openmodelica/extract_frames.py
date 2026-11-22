#!/usr/bin/env python3
"""Take OpenModelica's `cooVal.yCooVal` at the frame instants of `valve_oracle.txt`.

    python3 -I extract_frames.py <result.csv> ../valve_oracle.txt <yCooVal_frames.csv>

OpenModelica writes a row at every output instant (every 1 s here) and two rows at every event
(before and after). The table's time events land a little after the frame instant (about
t * 1e-10 later, at most 4e-8 s on this run), so the rows of one instant are those within
1e-6 s of it, and the value kept is the last of them: the value after any event there, as
`extract_day.py` keeps it. The value is kept as OpenModelica printed it.

Each instant's held inputs, as the last row has them, must equal the oracle row's inputs, or
the script stops: the run saw the frames the engine sees. `reset` is 1 where the integrator
of `limPI` was reinitialised at the instant (`limPI.I.y` in the last row differs from the first
by more than 1e-6; between the two rows it otherwise moves by less than 1e-9), 0 elsewhere.
The output is `x,y,reset`, one row per frame, in the oracle's order, `x` as the oracle writes
the time. Issue #787.
"""

import csv
import sys

WINDOW = 1e-6
RESET = 1e-6
INPUTS = ("cooVal.TSup", "cooVal.TSupSet", "cooVal.TOut", "cooVal.uFanFee")


def main(result, oracle, output):
    with open(result, newline="") as handle:
        rows = list(csv.reader(handle))
    header = rows[0]
    column = {name: header.index(name) for name in header}
    data = [(float(row[0]), row) for row in rows[1:]]
    lines = ["x,y,reset"]
    with open(oracle, encoding="utf-8") as handle:
        frames = [line.rstrip("\n").split("\t") for line in handle if line.startswith("row\t")]
    for cells in frames:
        t = float(cells[1])
        near = [row for time, row in data if abs(time - t) <= WINDOW]
        if not near:
            raise SystemExit("no row within {} s of t = {}".format(WINDOW, t))
        first, last = near[0], near[-1]
        held = [float(last[column[name]]) for name in INPUTS]
        if held != [float(cell) for cell in cells[2:6]]:
            raise SystemExit("t = {}: held inputs {} differ from the oracle's".format(t, held))
        fan_sta = last[column["cooVal.uFanSta"]] == "1"
        if fan_sta != (cells[6] == "true"):
            raise SystemExit("t = {}: fan status differs from the oracle's".format(t))
        state = column["cooVal.limPI.I.y"]
        reset = abs(float(last[state]) - float(first[state])) > RESET
        lines.append("{},{},{}".format(cells[1], last[column["cooVal.yCooVal"]], int(reset)))
    with open(output, "w", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
