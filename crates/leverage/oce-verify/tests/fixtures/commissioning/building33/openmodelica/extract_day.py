#!/usr/bin/env python3
"""Take OpenModelica's `cooValSta.yCooVal` at the trends' 5 s instants of the first day.

    python3 -I extract_day.py <result.csv> <yCooVal_day.csv>

OpenModelica writes a row at every output instant and two rows at every event (before and
after), so an instant can appear more than once; the value kept is the last row written at
exactly that time, the value after any event there. The value is kept as OpenModelica printed
it. The output is `x,y`, one row per instant 0, 5, ..., 86400 s, so 17,281 rows.
"""

import csv
import sys


def main(result, output):
    with open(result, newline="") as handle:
        rows = list(csv.reader(handle))
    header = rows[0]
    time_column, value_column = header.index("time"), header.index("cooValSta.yCooVal")
    last = {}
    for row in rows[1:]:
        last[float(row[time_column])] = row[value_column]
    lines = ["x,y"]
    for k in range(0, 17281):
        t = 5.0 * k
        if t not in last:
            raise SystemExit("no row at exactly t = {}".format(t))
        lines.append("{},{}".format(k * 5, last[t]))
    with open(output, "w", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
