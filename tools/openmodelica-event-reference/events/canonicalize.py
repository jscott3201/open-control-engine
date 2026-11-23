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


# The pulse-event pilots of issue #800: pilot -> (table-name stem, file stem, first frame time,
# [(column, OpenModelica result column)]). Columns are the engine model's inputs, then its
# outputs. Frame k is at `start + k` s. Unlike the pilots above, frame 0 keeps the LAST row at the
# start, the values after the start's event iteration: a sample instant at the start fires a
# `when` there (decision 3 of docs/design/pulse-event-inputs.md), and `Latch` and `Toggle` read
# the initialisation pulse of a `Change`.
PULSE_PILOTS = {
    "PulseConsumersPilot": ("pulse_events", "consumers", 0.0, [
        ("u", "uB.y"), ("uR", "inputs.y[2]"), ("um", "inputs.y[3]"), ("clr", "clrB.y"),
        ("lvl", "lvlB.y"), ("b", "bB.y"), ("x", "xB.y"),
        ("sampler", "sampler.y"), ("maxer", "maxer.y"), ("mean", "mean.y"),
        ("integ", "integ.y"), ("pid", "pid.y"), ("lat", "lat.y"), ("tog", "tog.y"),
        ("tim", "tim.y"), ("cntNot", "cntNot.y"), ("cntOr", "cntOr.y"), ("cntPre", "cntPre.y"),
        ("latX", "latX.y"), ("togX", "togX.y"),
    ]),
    "SampleTriggerPilot": ("pulse_events", "sample_triggers", 0.0, [
        ("uR", "inputs.y[1]"), ("clr", "clrB.y"),
        ("c05", "c05.y"), ("c1", "c1.y"), ("c2", "c2.y"), ("c14", "c14.y"), ("c1h", "c1h.y"),
        ("t05", "t05.y"), ("t1", "t1.y"), ("t2", "t2.y"), ("t14", "t14.y"), ("tNot", "tNot.y"),
        ("lat", "lat.y"), ("mean", "mean.y"), ("maxer", "maxer.y"), ("integ", "integ.y"),
        ("pid", "pid.y"), ("tim", "tim.y"), ("tog", "tog.y"), ("latHol", "latHol.y"),
    ]),
    "SampleTriggerOffGridPilot": ("pulse_events", "sample_trigger_off_grid", 0.25, [
        ("uR", "inputs.y[1]"), ("clr", "clrB.y"),
        ("c1", "c1.y"), ("t1", "t1.y"), ("lat", "lat.y"), ("cPre", "cPre.y"),
    ]),
    "InitNegatedPilot": ("pulse_events", "init_negated", 0.0, [
        ("x1", "x1B.y"), ("x0", "x0B.y"), ("uR", "inputs.y[3]"), ("cntC1", "cntC1.y"),
        ("latC1", "latC1.y"), ("togC1", "togC1.y"), ("samC1", "samC1.y"), ("maxC1", "maxC1.y"),
        ("meanC1", "meanC1.y"), ("integC1", "integC1.y"), ("pidC1", "pidC1.y"),
        ("timC1", "timC1.y"), ("cntE1", "cntE1.y"), ("latE1", "latE1.y"), ("togE1", "togE1.y"),
        ("samE1", "samE1.y"), ("cntC0", "cntC0.y"), ("latC0", "latC0.y"), ("togC0", "togC0.y"),
        ("samC0", "samC0.y"), ("maxC0", "maxC0.y"), ("meanC0", "meanC0.y"),
        ("integC0", "integC0.y"), ("pidC0", "pidC0.y"), ("timC0", "timC0.y"), ("cntE0", "cntE0.y"),
        ("latE0", "latE0.y"), ("togE0", "togE0.y"), ("samE0", "samE0.y"), ("cntS", "cntS.y"),
        ("latS", "latS.y"), ("togS", "togS.y"), ("samS", "samS.y"), ("maxS", "maxS.y"),
        ("meanS", "meanS.y"), ("integS", "integS.y"), ("pidS", "pidS.y"), ("timS", "timS.y"),
    ]),
    "InitNegatedOffGridPilot": ("pulse_events", "init_negated_off_grid", 0.25, [
        ("x1", "x1B.y"), ("x0", "x0B.y"), ("uR", "inputs.y[3]"), ("cntC1", "cntC1.y"),
        ("latC1", "latC1.y"), ("togC1", "togC1.y"), ("samC1", "samC1.y"), ("maxC1", "maxC1.y"),
        ("meanC1", "meanC1.y"), ("integC1", "integC1.y"), ("pidC1", "pidC1.y"),
        ("timC1", "timC1.y"), ("cntE1", "cntE1.y"), ("latE1", "latE1.y"), ("togE1", "togE1.y"),
        ("samE1", "samE1.y"), ("cntC0", "cntC0.y"), ("latC0", "latC0.y"), ("togC0", "togC0.y"),
        ("samC0", "samC0.y"), ("maxC0", "maxC0.y"), ("meanC0", "meanC0.y"),
        ("integC0", "integC0.y"), ("pidC0", "pidC0.y"), ("timC0", "timC0.y"), ("cntE0", "cntE0.y"),
        ("latE0", "latE0.y"), ("togE0", "togE0.y"), ("samE0", "samE0.y"), ("cntS", "cntS.y"),
        ("latS", "latS.y"), ("togS", "togS.y"), ("samS", "samS.y"), ("maxS", "maxS.y"),
        ("meanS", "meanS.y"), ("integS", "integS.y"), ("pidS", "pidS.y"), ("timS", "timS.y"),
    ]),
    "InitMixturesPilot": ("pulse_events", "init_mixtures", 0.0, [
        ("x1", "x1B.y"), ("cntChaSam", "cntChaSam.y"), ("latChaSam", "latChaSam.y"),
        ("togChaSam", "togChaSam.y"), ("cntNotSam", "cntNotSam.y"), ("latNotSam", "latNotSam.y"),
        ("togNotSam", "togNotSam.y"), ("cntXorSam", "cntXorSam.y"), ("latXorSam", "latXorSam.y"),
        ("togXorSam", "togXorSam.y"), ("cntNotTru", "cntNotTru.y"), ("latNotTru", "latNotTru.y"),
        ("togNotTru", "togNotTru.y"), ("cntNotFal", "cntNotFal.y"), ("latNotFal", "latNotFal.y"),
        ("togNotFal", "togNotFal.y"), ("cntNotPre", "cntNotPre.y"), ("latNotPre", "latNotPre.y"),
        ("togNotPre", "togNotPre.y"), ("cntNotNot", "cntNotNot.y"), ("latNotNot", "latNotNot.y"),
        ("togNotNot", "togNotNot.y"), ("cntThree", "cntThree.y"), ("latThree", "latThree.y"),
        ("togThree", "togThree.y"), ("cntXorNot", "cntXorNot.y"), ("latXorNot", "latXorNot.y"),
        ("togXorNot", "togXorNot.y"),
    ]),
    "InitMixturesOffGridPilot": ("pulse_events", "init_mixtures_off_grid", 0.25, [
        ("x1", "x1B.y"), ("cntChaSam", "cntChaSam.y"), ("latChaSam", "latChaSam.y"),
        ("togChaSam", "togChaSam.y"), ("cntNotSam", "cntNotSam.y"), ("latNotSam", "latNotSam.y"),
        ("togNotSam", "togNotSam.y"), ("cntXorSam", "cntXorSam.y"), ("latXorSam", "latXorSam.y"),
        ("togXorSam", "togXorSam.y"), ("cntNotTru", "cntNotTru.y"), ("latNotTru", "latNotTru.y"),
        ("togNotTru", "togNotTru.y"), ("cntNotFal", "cntNotFal.y"), ("latNotFal", "latNotFal.y"),
        ("togNotFal", "togNotFal.y"), ("cntNotPre", "cntNotPre.y"), ("latNotPre", "latNotPre.y"),
        ("togNotPre", "togNotPre.y"), ("cntNotNot", "cntNotNot.y"), ("latNotNot", "latNotNot.y"),
        ("togNotNot", "togNotNot.y"), ("cntThree", "cntThree.y"), ("latThree", "latThree.y"),
        ("togThree", "togThree.y"), ("cntXorNot", "cntXorNot.y"), ("latXorNot", "latXorNot.y"),
        ("togXorNot", "togXorNot.y"),
    ]),
    "LatchClearPilot": ("pulse_events", "latch_clear", 0.0, [
        ("a", "aB.y"), ("b", "bB.y"), ("latA", "latA.y"), ("togA", "togA.y"), ("latB", "latB.y"),
        ("togB", "togB.y"), ("latC", "latC.y"), ("togC", "togC.y"), ("latD", "latD.y"),
        ("togD", "togD.y"), ("samD", "samD.y"), ("maxD", "maxD.y"), ("meanD", "meanD.y"),
        ("integD", "integD.y"), ("pidD", "pidD.y"),
    ]),
}


def pulse_main(pilot, result, output):
    with open(result, newline="") as handle:
        rows = list(csv.reader(handle))
    header = rows[0]
    data = [(float(row[0]), row) for row in rows[1:]]
    slug, stem, start, columns = PULSE_PILOTS[pilot]
    index = [header.index(source) for _, source in columns]
    lines = []
    for k in range(FRAMES):
        frame = start + k
        near = [row for time, row in data if abs(time - frame) <= WINDOW]
        if not near:
            raise SystemExit("{}: no row within {} s of t = {}".format(pilot, WINDOW, frame))
        values = [repr(float(near[-1][i])) for i in index]
        lines.append(" ".join([repr(frame)] + values))
    name = "openmodelica_{}_{}".format(slug, stem)
    text = "#1\n# columns: time {}\ndouble {}({},{})\n{}\n".format(
        " ".join(column for column, _ in columns), name, FRAMES, len(columns) + 1,
        "\n".join(lines))
    with open(os.path.join(output, "{}.canonical.csv".format(stem)), "w", newline="\n") as handle:
        handle.write(text)


def main(pilot, result, output):
    if pilot in PULSE_PILOTS:
        pulse_main(pilot, result, output)
        return
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
