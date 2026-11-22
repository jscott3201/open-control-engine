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

For each scenario (the one-block pilots' `initiallyTrue` and `risingAfterStart` instances, and
the single scenario of `OnCounterChangePilot` and `IntegratorWithResetRampPilot`) the script
writes a Modelica `#1` text table with the engine model's inputs, in port order, then `y`,
every value as Python's `repr` of OpenModelica's printed number (Booleans as 1.0 and 0.0). The frame instant,
not the event's time, is the table's time. Nothing here knows an expected output.
"""

import csv
import os
import sys

FRAMES = 10
WINDOW = 1e-6


def scenarios(slug, ports):
    """The two trigger scenarios of a one-block pilot: each instance's ports, then `y`."""
    return [
        (slug, stem, [(port, "{}.{}".format(instance, port)) for port in ports + ["y"]])
        for instance, stem in (("initiallyTrue", "initially_true"),
                               ("risingAfterStart", "rising_after_start"))
    ]


# Pilot -> [(table-name stem, file stem, [(column, OpenModelica result column)])]. The columns
# are the engine model's inputs in port order, then its output `y`.
PILOTS = {
    "OnCounterPilot": scenarios("integers_on_counter", ["trigger", "reset"]),
    "TriggeredSamplerPilot": scenarios("discrete_triggered_sampler", ["u", "trigger"]),
    "TriggeredMaxPilot": scenarios("discrete_triggered_max", ["u", "trigger"]),
    "IntegratorWithResetPilot": scenarios(
        "reals_integrator_with_reset", ["u", "y_reset_in", "trigger"]),
    "PIDWithResetPilot": scenarios("reals_pid_with_reset", ["u_s", "u_m", "trigger"]),
    # `Change` feeding `OnCounter`: the model's inputs are `Change.u` and `OnCounter.reset`.
    "OnCounterChangePilot": [("integers_on_counter", "change_pulses", [
        ("u", "consecutivePulsesChange.u"),
        ("reset", "consecutivePulses.reset"),
        ("y", "consecutivePulses.y"),
    ])],
    "IntegratorWithResetRampPilot": [("reals_integrator_with_reset", "ramp", [
        (port, "ramp." + port) for port in ("u", "y_reset_in", "trigger", "y")
    ])],
}


def main(pilot, result, output):
    with open(result, newline="") as handle:
        rows = list(csv.reader(handle))
    header = rows[0]
    data = [(float(row[0]), row) for row in rows[1:]]
    for slug, stem, columns in PILOTS[pilot]:
        index = [header.index(source) for _, source in columns]
        lines = []
        for k in range(FRAMES):
            near = [row for time, row in data if abs(time - k) <= WINDOW]
            if not near:
                raise SystemExit("{}: no row within {} s of t = {}".format(pilot, WINDOW, k))
            if k == 0 and any([row[i] for i in index] != [near[-1][i] for i in index]
                              for row in near):
                raise SystemExit("{}: {} changes during the initial instant".format(pilot, stem))
            values = [repr(float(near[-1][i])) for i in index]
            lines.append(" ".join([repr(float(k))] + values))
        name = "openmodelica_{}_{}".format(slug, stem)
        text = "#1\n# columns: time {}\ndouble {}({},{})\n{}\n".format(
            " ".join(column for column, _ in columns), name, FRAMES, len(columns) + 1,
            "\n".join(lines))
        path = os.path.join(output, "{}.canonical.csv".format(stem))
        with open(path, "w", newline="\n") as handle:
            handle.write(text)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
