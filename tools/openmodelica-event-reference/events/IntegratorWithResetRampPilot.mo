model IntegratorWithResetRampPilot
  "CDL.Reals.IntegratorWithReset integrating a nonzero, stepped u with two resets, at 1 s frames with each input held (issue #670)"
  Modelica.Blocks.Sources.CombiTimeTable rampInputs(
    table=[
      0, 0.5, 10, 0;
      1, 0.5, 10, 0;
      2, 0.5, 10, 0;
      3, 0.5, -4, 1;
      4, 0.5, -4, 0;
      5, 2.0, -4, 0;
      6, 2.0, -4, 0;
      7, 2.0, 6, 1;
      8, 2.0, 6, 0;
      9, 2.0, 6, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, y_reset_in, trigger (Boolean column 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean rampTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Reals.IntegratorWithReset ramp(k=1, y_start=0)
    "u = 0.5 then 2 from 5 s; resets at 3 s (to -4) and 7 s (to 6)";
equation
  connect(rampInputs.y[1], ramp.u);
  connect(rampInputs.y[2], ramp.y_reset_in);
  connect(rampInputs.y[3], rampTrigger.u);
  connect(rampTrigger.y, ramp.trigger);
end IntegratorWithResetRampPilot;
