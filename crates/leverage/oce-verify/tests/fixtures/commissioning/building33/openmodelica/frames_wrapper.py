#!/usr/bin/env python3
"""Write the OpenModelica wrapper that runs upstream's CoolingCoilValve on the oracle's frames.

    python3 -I frames_wrapper.py ../valve_oracle.txt > CoolingCoilValveFrames.mo

Reads only the input columns of `valve_oracle.txt` (time, TSup, TSupSet, TOut, uFanFee,
uFanSta), never its valve column, and writes a Modelica model that feeds them to
`Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve`, unmodified at the pin,
through one `CombiTimeTable` with constant segments: each frame's inputs are held until the next
frame, the engine's sample-and-hold, not interpolated. A Boolean column is written as 1 or 0 and
read back through `RealToBoolean` at 0.5. Each number is Python's `repr` of the oracle's value,
which Modelica reads back to the same double. Issue #787.
"""

import sys


def main(oracle):
    rows = []
    with open(oracle, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("row\t"):
                continue
            cells = line.rstrip("\n").split("\t")
            fan_sta = {"true": "1", "false": "0"}[cells[6]]
            rows.append("{}, {}, {}, {}, {}, {}".format(*cells[1:6], fan_sta))
    if not rows:
        raise SystemExit("no rows in {}".format(oracle))
    table = ";\n      ".join(rows)
    print("""model CoolingCoilValveFrames
  "Upstream's CoolingCoilValve on the {n} frames of valve_oracle.txt, each input held (issue #787)"
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
end CoolingCoilValveFrames;""".format(n=len(rows), table=table))


if __name__ == "__main__":
    main(sys.argv[1])
