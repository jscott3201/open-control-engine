model OnCounterPilot
  "CDL.Integers.OnCounter driven at 1 s frames with each input held: two scenarios (issue #670)"
  Modelica.Blocks.Sources.CombiTimeTable initiallyTrueInputs(
    table=[
      0, 1, 0;
      1, 1, 0;
      2, 0, 0;
      3, 1, 0;
      4, 1, 0;
      5, 0, 0;
      6, 1, 0;
      7, 0, 1;
      8, 0, 0;
      9, 1, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: trigger, reset (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean initiallyTrueTrigger(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean initiallyTrueReset(threshold=0.5);
  Buildings.Controls.OBC.CDL.Integers.OnCounter initiallyTrue(y_start=3)
    "the trigger true from the initial instant, falling and rising again";
  Modelica.Blocks.Sources.CombiTimeTable risingAfterStartInputs(
    table=[
      0, 0, 0;
      1, 1, 0;
      2, 1, 0;
      3, 0, 0;
      4, 1, 0;
      5, 0, 0;
      6, 0, 1;
      7, 1, 1;
      8, 0, 0;
      9, 1, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: trigger, reset (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean risingAfterStartTrigger(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean risingAfterStartReset(threshold=0.5);
  Buildings.Controls.OBC.CDL.Integers.OnCounter risingAfterStart(y_start=3)
    "the control: the trigger false at the initial instant and rising one frame later";
equation
  connect(initiallyTrueInputs.y[1], initiallyTrueTrigger.u);
  connect(initiallyTrueTrigger.y, initiallyTrue.trigger);
  connect(initiallyTrueInputs.y[2], initiallyTrueReset.u);
  connect(initiallyTrueReset.y, initiallyTrue.reset);
  connect(risingAfterStartInputs.y[1], risingAfterStartTrigger.u);
  connect(risingAfterStartTrigger.y, risingAfterStart.trigger);
  connect(risingAfterStartInputs.y[2], risingAfterStartReset.u);
  connect(risingAfterStartReset.y, risingAfterStart.reset);
end OnCounterPilot;
