#!/usr/bin/env python3
"""Project one pilot's OpenModelica result onto its frames, one canonical table per scenario.

    python3 -I canonicalize.py <PilotName> <result.csv> <output directory>

The pilots (issue #670) drive each class at frames 0, 1, ..., 9 s through a held table.
OpenModelica writes a row at every output instant and two rows at every event (before and
after), and the table's event for frame k lands a little after k s (about k * 2e-10 s on these
runs). The rows of frame k are those within 1e-6 s of k; the row kept is the last of them, the
values after any event at the frame, as the Building 33 extraction keeps them. If OpenModelica
writes more than one row at t = 0, they must agree on every kept column, or the script stops:
frame 0 is compared with the initialisation values.

For each scenario instance (`initiallyTrue`, `risingAfterStart`) the script writes a Modelica
`#1` text table with the instance's inputs, in the engine port order, then `y`, every value as
Python's `repr` of OpenModelica's printed number (Booleans as 1.0 and 0.0). The frame instant,
not the event's time, is the table's time. Nothing here knows an expected output.
"""

import csv
import os
import sys

FRAMES = 10
WINDOW = 1e-6
PILOTS = {
    "OnCounterPilot": ("integers_on_counter", ["trigger", "reset"]),
    "TriggeredSamplerPilot": ("discrete_triggered_sampler", ["u", "trigger"]),
    "TriggeredMaxPilot": ("discrete_triggered_max", ["u", "trigger"]),
    "IntegratorWithResetPilot": ("reals_integrator_with_reset", ["u", "y_reset_in", "trigger"]),
    "PIDWithResetPilot": ("reals_pid_with_reset", ["u_s", "u_m", "trigger"]),
}
SCENARIOS = (("initiallyTrue", "initially_true"), ("risingAfterStart", "rising_after_start"))


def main(pilot, result, output):
    slug, ports = PILOTS[pilot]
    with open(result, newline="") as handle:
        rows = list(csv.reader(handle))
    header = rows[0]
    data = [(float(row[0]), row) for row in rows[1:]]
    for instance, scenario in SCENARIOS:
        columns = ports + ["y"]
        index = [header.index("{}.{}".format(instance, column)) for column in columns]
        lines = []
        for k in range(FRAMES):
            near = [row for time, row in data if abs(time - k) <= WINDOW]
            if not near:
                raise SystemExit("{}: no row within {} s of t = {}".format(pilot, WINDOW, k))
            if k == 0 and any([row[i] for i in index] != [near[-1][i] for i in index]
                              for row in near):
                raise SystemExit("{}: {} changes during the initial instant".format(pilot, instance))
            values = [repr(float(near[-1][i])) for i in index]
            lines.append(" ".join([repr(float(k))] + values))
        name = "openmodelica_{}_{}".format(slug, scenario)
        text = "#1\n# columns: time {}\ndouble {}({},{})\n{}\n".format(
            " ".join(columns), name, FRAMES, len(columns) + 1, "\n".join(lines))
        path = os.path.join(output, "{}.canonical.csv".format(scenario))
        with open(path, "w", newline="\n") as handle:
            handle.write(text)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
