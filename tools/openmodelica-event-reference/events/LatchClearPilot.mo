model LatchClearPilot
  "Latch and Toggle whose clr carries a pulse or a sample while u fires in the second event iteration, and data inputs that carry a converted pulse (issue #800, review of GitHub #805)"
  Modelica.Blocks.Sources.CombiTimeTable inputs(
    table=[
      0, 0, 0;
      1, 0, 0;
      2, 1, 1;
      3, 1, 1;
      4, 1, 1;
      5, 0, 1;
      6, 0, 1;
      7, 0, 0;
      8, 0, 0;
      9, 0, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: a (changes at 2 and 5 s), b (changes with a at 2 s, alone at 7 s)";
  Modelica.Blocks.Math.RealToBoolean aB(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean bB(threshold=0.5);
  Buildings.Controls.OBC.CDL.Reals.Sources.Constant zer(k=0);
  Buildings.Controls.OBC.CDL.Reals.Sources.Constant one(k=1);
  Buildings.Controls.OBC.CDL.Logical.Change chaA;
  Buildings.Controls.OBC.CDL.Logical.Change chaB;
  Buildings.Controls.OBC.CDL.Logical.Not notChaA;
  Buildings.Controls.OBC.CDL.Logical.Edge edgA;
  Buildings.Controls.OBC.CDL.Logical.Not notEdgA;
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s2(period=2);
  Buildings.Controls.OBC.CDL.Logical.Not notS2;
  Buildings.Controls.OBC.CDL.Logical.Latch latA "u = Not(Change(a)), clr = Change(a)";
  Buildings.Controls.OBC.CDL.Logical.Toggle togA "u = Not(Change(a)), clr = Change(a)";
  Buildings.Controls.OBC.CDL.Logical.Latch latB "u = Not(Change(a)), clr = Change(b)";
  Buildings.Controls.OBC.CDL.Logical.Toggle togB "u = Not(Change(a)), clr = Change(b)";
  Buildings.Controls.OBC.CDL.Logical.Latch latC "u = Change(a), clr = Not(Edge(a))";
  Buildings.Controls.OBC.CDL.Logical.Toggle togC "u = Change(a), clr = Not(Edge(a))";
  Buildings.Controls.OBC.CDL.Logical.Latch latD "u = SampleTrigger(2 s), clr = Not(SampleTrigger(2 s)): a negated sample on clr";
  Buildings.Controls.OBC.CDL.Logical.Toggle togD "u = SampleTrigger(2 s), clr = Not(SampleTrigger(2 s)): a negated sample on clr";
  Buildings.Controls.OBC.CDL.Conversions.BooleanToReal pulR "a converted pulse as a data input";
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler samD(y_start=7) "u = BooleanToReal(Change(a)), trigger = Not(Change(a))";
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMax maxD;
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMovingMean meanD(n=3);
  Buildings.Controls.OBC.CDL.Reals.IntegratorWithReset integD(k=1, y_start=0.5);
  Buildings.Controls.OBC.CDL.Reals.PIDWithReset pidD(controllerType=Buildings.Controls.OBC.CDL.Types.SimpleController.PI, k=1, Ti=1e20, xi_start=0.125, y_reset=0.5, yMax=10, yMin=-10);
equation
  connect(inputs.y[1], aB.u);
  connect(inputs.y[2], bB.u);
  connect(aB.y, chaA.u);
  connect(bB.y, chaB.u);
  connect(chaA.y, notChaA.u);
  connect(aB.y, edgA.u);
  connect(edgA.y, notEdgA.u);
  connect(s2.y, notS2.u);
  connect(notChaA.y, latA.u);
  connect(chaA.y, latA.clr);
  connect(notChaA.y, togA.u);
  connect(chaA.y, togA.clr);
  connect(notChaA.y, latB.u);
  connect(chaB.y, latB.clr);
  connect(notChaA.y, togB.u);
  connect(chaB.y, togB.clr);
  connect(chaA.y, latC.u);
  connect(notEdgA.y, latC.clr);
  connect(chaA.y, togC.u);
  connect(notEdgA.y, togC.clr);
  connect(s2.y, latD.u);
  connect(notS2.y, latD.clr);
  connect(s2.y, togD.u);
  connect(notS2.y, togD.clr);
  connect(chaA.y, pulR.u);
  connect(pulR.y, samD.u);
  connect(notChaA.y, samD.trigger);
  connect(pulR.y, maxD.u);
  connect(notChaA.y, maxD.trigger);
  connect(pulR.y, meanD.u);
  connect(notChaA.y, meanD.trigger);
  connect(zer.y, integD.u);
  connect(pulR.y, integD.y_reset_in);
  connect(notChaA.y, integD.trigger);
  connect(one.y, pidD.u_s);
  connect(pulR.y, pidD.u_m);
  connect(notChaA.y, pidD.trigger);
end LatchClearPilot;
