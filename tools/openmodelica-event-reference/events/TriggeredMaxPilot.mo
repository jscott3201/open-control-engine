model TriggeredMaxPilot
  "CDL.Discrete.TriggeredMax driven at 1 s frames with each input held: two scenarios (issue #670)"
  Modelica.Blocks.Sources.CombiTimeTable initiallyTrueInputs(
    table=[
      0, -2, 1;
      1, -1, 1;
      2, -5, 0;
      3, 3, 1;
      4, -4, 0;
      5, 2, 1;
      6, -6, 0;
      7, 1, 0;
      8, 8, 1;
      9, -7, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean initiallyTrueTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMax initiallyTrue
    "the trigger true from the initial instant, falling and rising again";
  Modelica.Blocks.Sources.CombiTimeTable risingAfterStartInputs(
    table=[
      0, -2, 0;
      1, 1, 1;
      2, -5, 0;
      3, 3, 0;
      4, 4, 1;
      5, 2, 1;
      6, -6, 0;
      7, 1, 1;
      8, 0.5, 0;
      9, -7, 1],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u, trigger (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean risingAfterStartTrigger(threshold=0.5);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMax risingAfterStart
    "the control: the trigger false at the initial instant and rising one frame later";
equation
  connect(initiallyTrueInputs.y[1], initiallyTrue.u);
  connect(initiallyTrueInputs.y[2], initiallyTrueTrigger.u);
  connect(initiallyTrueTrigger.y, initiallyTrue.trigger);
  connect(risingAfterStartInputs.y[1], risingAfterStart.u);
  connect(risingAfterStartInputs.y[2], risingAfterStartTrigger.u);
  connect(risingAfterStartTrigger.y, risingAfterStart.trigger);
end TriggeredMaxPilot;
