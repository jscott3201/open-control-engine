model TriggeredSamplerPilot
  "CDL.Discrete.TriggeredSampler driven at 1 s frames with each input held: two scenarios (issue #670)"
  Modelica.Blocks.Sources.CombiTimeTable initiallyTrueInputs(
    table=[
      0, 10, 1;
      1, 11, 1;
      2, 12, 0;
      3, 13, 1;
      4, 14, 0;
      5, 15, 0;
      6, 16, 1;
      7, 17, 1;
      8, 18, 0;
      9, 19, 1],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean initiallyTrueTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler initiallyTrue(y_start=7)
    "the trigger true from the initial instant, falling and rising again";
  Modelica.Blocks.Sources.CombiTimeTable risingAfterStartInputs(
    table=[
      0, 10, 0;
      1, 11, 1;
      2, 12, 1;
      3, 13, 0;
      4, 14, 1;
      5, 15, 0;
      6, 16, 0;
      7, 17, 0;
      8, 18, 1;
      9, 19, 1],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean risingAfterStartTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler risingAfterStart(y_start=7)
    "the control: the trigger false at the initial instant and rising one frame later";
equation
  connect(initiallyTrueInputs.y[1], initiallyTrue.u);
  connect(initiallyTrueInputs.y[2], initiallyTrueTrigger.u);
  connect(initiallyTrueTrigger.y, initiallyTrue.trigger);
  connect(risingAfterStartInputs.y[1], risingAfterStart.u);
  connect(risingAfterStartInputs.y[2], risingAfterStartTrigger.u);
  connect(risingAfterStartTrigger.y, risingAfterStart.trigger);
end TriggeredSamplerPilot;
