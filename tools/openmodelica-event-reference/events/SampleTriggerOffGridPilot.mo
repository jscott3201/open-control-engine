model SampleTriggerOffGridPilot
  "CDL.Logical.Sources.SampleTrigger(period = 1 s) in a run that starts off its grid, at 0.25 s, frames every 1 s from there (issue #800)"
  Modelica.Blocks.Sources.CombiTimeTable inputs(
    table=[
      0.25, 2.5, 0;
      0.75, 3.5, 0;
      1.75, -1.25, 0;
      2.75, 4.75, 1;
      3.75, -6.5, 1;
      4.75, 8.0, 0;
      5.75, -9.25, 0;
      6.75, 10.5, 0;
      7.75, -11.75, 0;
      8.75, 13.0, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: uR, clr (Boolean 1 or 0); frame k's values hold from half a frame before it";
  Modelica.Blocks.Math.RealToBoolean clrB(threshold=0.5);
  Buildings.Controls.OBC.CDL.Logical.Sources.Constant fal(k=false);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s1(period=1) "the run starts at 0.25 s: its first instant after the start is 1 s";
  Buildings.Controls.OBC.CDL.Integers.OnCounter c1(y_start=0);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler t1(y_start=7);
  Buildings.Controls.OBC.CDL.Logical.Latch lat "u = SampleTrigger(1 s): the initial equation reads it false";
  Buildings.Controls.OBC.CDL.Logical.Pre pre1 "Pre(SampleTrigger(1 s)): a fallback cone";
  Buildings.Controls.OBC.CDL.Integers.OnCounter cPre(y_start=0);
equation
  connect(inputs.y[2], clrB.u);
  connect(s1.y, c1.trigger);
  connect(fal.y, c1.reset);
  connect(inputs.y[1], t1.u);
  connect(s1.y, t1.trigger);
  connect(s1.y, lat.u);
  connect(fal.y, lat.clr);
  connect(s1.y, pre1.u);
  connect(pre1.y, cPre.trigger);
  connect(fal.y, cPre.reset);
end SampleTriggerOffGridPilot;
