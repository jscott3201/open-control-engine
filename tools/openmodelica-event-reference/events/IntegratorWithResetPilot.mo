model IntegratorWithResetPilot
  "CDL.Reals.IntegratorWithReset driven at 1 s frames with each input held: two scenarios (issue #670)"
  Modelica.Blocks.Sources.CombiTimeTable initiallyTrueInputs(
    table=[
      0, 0, 10, 1;
      1, 0, 20, 1;
      2, 0, 30, 0;
      3, 0, 40, 1;
      4, 0, 50, 1;
      5, 0, 60, 0;
      6, 0, 70, 0;
      7, 0, 80, 1;
      8, 0, 90, 0;
      9, 0, 100, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, y_reset_in, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean initiallyTrueTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Reals.IntegratorWithReset initiallyTrue(k=1, y_start=1.5)
    "the trigger true from the initial instant, falling and rising again";
  Modelica.Blocks.Sources.CombiTimeTable risingAfterStartInputs(
    table=[
      0, 0, 10, 0;
      1, 0, 20, 1;
      2, 0, 30, 0;
      3, 0, 40, 0;
      4, 0, 50, 1;
      5, 0, 60, 0;
      6, 0, 70, 1;
      7, 0, 80, 1;
      8, 0, 90, 0;
      9, 0, 100, 1],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, y_reset_in, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean risingAfterStartTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Reals.IntegratorWithReset risingAfterStart(k=1, y_start=1.5)
    "the control: the trigger false at the initial instant and rising one frame later";
equation
  connect(initiallyTrueInputs.y[1], initiallyTrue.u);
  connect(initiallyTrueInputs.y[2], initiallyTrue.y_reset_in);
  connect(initiallyTrueInputs.y[3], initiallyTrueTrigger.u);
  connect(initiallyTrueTrigger.y, initiallyTrue.trigger);
  connect(risingAfterStartInputs.y[1], risingAfterStart.u);
  connect(risingAfterStartInputs.y[2], risingAfterStart.y_reset_in);
  connect(risingAfterStartInputs.y[3], risingAfterStartTrigger.u);
  connect(risingAfterStartTrigger.y, risingAfterStart.trigger);
end IntegratorWithResetPilot;
