model OnCounterChangePilot
  "CDL.Integers.OnCounter counting CDL.Logical.Change pulses of a signal that changes on consecutive frames, at 1 s frames with each input held (issue #800)"
  Modelica.Blocks.Sources.CombiTimeTable consecutivePulsesInputs(
    table=[
      0, 0, 0;
      1, 0, 0;
      2, 1, 0;
      3, 0, 0;
      4, 0, 0;
      5, 1, 0;
      6, 1, 0;
      7, 0, 0;
      8, 1, 0;
      9, 0, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u (the changing signal), reset (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean consecutivePulsesU(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean consecutivePulsesReset(threshold=0.5);
  Buildings.Controls.OBC.CDL.Logical.Change consecutivePulsesChange
    "True at each change of u: changes at 2 and 3 s, 7 and 8 s, 8 and 9 s are one frame apart";
  Buildings.Controls.OBC.CDL.Integers.OnCounter consecutivePulses(y_start=3)
    "Counts the Change pulses";
equation
  connect(consecutivePulsesInputs.y[1], consecutivePulsesU.u);
  connect(consecutivePulsesInputs.y[2], consecutivePulsesReset.u);
  connect(consecutivePulsesU.y, consecutivePulsesChange.u);
  connect(consecutivePulsesChange.y, consecutivePulses.trigger);
  connect(consecutivePulsesReset.y, consecutivePulses.reset);
end OnCounterChangePilot;
