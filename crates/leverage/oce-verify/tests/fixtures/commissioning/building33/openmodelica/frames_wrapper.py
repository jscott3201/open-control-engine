#!/usr/bin/env python3
"""Write the OpenModelica wrapper that runs upstream's CoolingCoilValve on the oracle's frames.

    python3 -I frames_wrapper.py ../valve_oracle.txt > CoolingCoilValveFrames.mo
    python3 -I frames_wrapper.py ../valve_oracle.txt probe > CoolingCoilValveReenableProbe.mo

Reads only the input columns of `valve_oracle.txt` (time, TSup, TSupSet, TOut, uFanFee,
uFanSta), never its valve column, and writes a Modelica model that feeds them to
`Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve`, unmodified at the pin,
through one `CombiTimeTable` with constant segments: each frame's inputs are held until the next
frame, the engine's sample-and-hold, not interpolated. A Boolean column is written as 1 or 0 and
read back through `RealToBoolean` at 0.5. Each number is Python's `repr` of the oracle's value,
which Modelica reads back to the same double. Issue #787.

With `probe`, the model is `CoolingCoilValveReenableProbe`: the same frames with the supply air
`TSup` at 290 K instead of 288 K from 382 s on, the re-enable after the one-frame disable. The
enable falls at 377 s and rises at 382 s, so `Change` is true on two consecutive frames; the
probe makes the PI's error at 382 s differ from that at 377 s, so a reset at 382 s shows in the
valve signal (issue #800).
"""

PROBE_FROM = 382.0
PROBE_SUPPLY = 290.0


def frames(oracle, probe):
    """The oracle's input rows, (t, TSup, TSupSet, TOut, uFanFee, uFanSta) as text cells."""
    rows = []
    with open(oracle, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("row\t"):
                continue
            cells = line.rstrip("\n").split("\t")[1:7]
            if probe and float(cells[0]) >= PROBE_FROM:
                cells[1] = repr(PROBE_SUPPLY)
            rows.append(cells)
    if not rows:
        raise SystemExit("no rows in {}".format(oracle))
    return rows

import sys


def main(oracle, probe=False):
    rows = []
    for cells in frames(oracle, probe):
        fan_sta = {"true": "1", "false": "0"}[cells[5]]
        rows.append("{}, {}, {}, {}, {}, {}".format(*cells[:5], fan_sta))
    table = ";\n      ".join(rows)
    name = "CoolingCoilValveReenableProbe" if probe else "CoolingCoilValveFrames"
    what = "with TSup at 290 K from 382 s" if probe else "each input held"
    print("""model {name}
  "Upstream's CoolingCoilValve on the {n} frames of valve_oracle.txt, {what} (issue #787)"
  Modelica.Blocks.Sources.CombiTimeTable frames(
    table=[
      {table}],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: TSup, TSupSet, TOut, uFanFee, uFanSta (1 or 0)";
  Modelica.Blocks.Math.RealToBoolean fanSta(threshold=0.5)
    "The fan status column as a Boolean";
  Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve cooVal
    "Upstream's sequence, unmodified";
equation
  connect(frames.y[1], cooVal.TSup);
  connect(frames.y[2], cooVal.TSupSet);
  connect(frames.y[3], cooVal.TOut);
  connect(frames.y[4], cooVal.uFanFee);
  connect(frames.y[5], fanSta.u);
  connect(fanSta.y, cooVal.uFanSta);
end {name};""".format(n=len(rows), table=table, name=name, what=what))


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[2:] not in ([], ["probe"]):
        raise SystemExit("usage: frames_wrapper.py ORACLE [probe]")
    main(sys.argv[1], sys.argv[2:] == ["probe"])
