model PIDWithResetPilot
  "CDL.Reals.PIDWithReset driven at 1 s frames with each input held: two scenarios (issue #670)"
  Modelica.Blocks.Sources.CombiTimeTable initiallyTrueInputs(
    table=[
      0, 1, 0.75, 1;
      1, 1, 0.5, 1;
      2, 1, 0.25, 0;
      3, 1, 0, 1;
      4, 1, 0.625, 0;
      5, 1, 0.875, 0;
      6, 1, 0.375, 1;
      7, 1, 0.5, 1;
      8, 1, 0.25, 0;
      9, 1, 1, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u_s, u_m, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean initiallyTrueTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Reals.PIDWithReset initiallyTrue(
    controllerType=Buildings.Controls.OBC.CDL.Types.SimpleController.PI,
    k=1, Ti=1e20, xi_start=0.125, y_reset=0.5, yMax=10, yMin=-10)
    "the trigger true from the initial instant, falling and rising again";
  Modelica.Blocks.Sources.CombiTimeTable risingAfterStartInputs(
    table=[
      0, 1, 0.75, 0;
      1, 1, 0.5, 1;
      2, 1, 0.25, 0;
      3, 1, 0, 0;
      4, 1, 0.625, 1;
      5, 1, 0.875, 1;
      6, 1, 0.375, 0;
      7, 1, 0.5, 0;
      8, 1, 0.25, 1;
      9, 1, 1, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u_s, u_m, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean risingAfterStartTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Reals.PIDWithReset risingAfterStart(
    controllerType=Buildings.Controls.OBC.CDL.Types.SimpleController.PI,
    k=1, Ti=1e20, xi_start=0.125, y_reset=0.5, yMax=10, yMin=-10)
    "the control: the trigger false at the initial instant and rising one frame later";
equation
  connect(initiallyTrueInputs.y[1], initiallyTrue.u_s);
  connect(initiallyTrueInputs.y[2], initiallyTrue.u_m);
  connect(initiallyTrueInputs.y[3], initiallyTrueTrigger.u);
  connect(initiallyTrueTrigger.y, initiallyTrue.trigger);
  connect(risingAfterStartInputs.y[1], risingAfterStart.u_s);
  connect(risingAfterStartInputs.y[2], risingAfterStart.u_m);
  connect(risingAfterStartInputs.y[3], risingAfterStartTrigger.u);
  connect(risingAfterStartTrigger.y, risingAfterStart.trigger);
end PIDWithResetPilot;
