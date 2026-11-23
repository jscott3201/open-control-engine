model MixedFallbackPilot
  "Two-input consumers with one input on a fallback cone and the other on the settled-value rule, and a Toggle whose clr holds negated samples (issue #800, second review of GitHub #805)"
  Modelica.Blocks.Sources.CombiTimeTable inputs(
    table=[
      0, 0, 0;
      1, 1, 1;
      2, 0, 0;
      3, 1, 1;
      4, 0, 1;
      5, 1, 1;
      6, 0, 1;
      7, 1, 1;
      8, 0, 1;
      9, 1, 1],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: a (changes every second from 1 s), b (changes at 1, 2 and 3 s, then true)";
  Modelica.Blocks.Math.RealToBoolean aB(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean bB(threshold=0.5);
  Buildings.Controls.OBC.CDL.Logical.Change chaA;
  Buildings.Controls.OBC.CDL.Logical.Not notChaA;
  Buildings.Controls.OBC.CDL.Logical.Pre preNotChaA;
  Buildings.Controls.OBC.CDL.Logical.Latch latA "u = Change(a), clr = Pre(Not(Change(a)))";
  Buildings.Controls.OBC.CDL.Logical.Change chaB;
  Buildings.Controls.OBC.CDL.Logical.Edge edgChaB;
  Buildings.Controls.OBC.CDL.Integers.OnCounter cntB(y_start=0) "trigger = Change(b), reset = Edge(Change(b))";
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s3(period=3, shift=0.5);
  Buildings.Controls.OBC.CDL.Logical.Not notS3;
  Buildings.Controls.OBC.CDL.Logical.Pre preB;
  Buildings.Controls.OBC.CDL.Logical.Toggle togC "u = Not(SampleTrigger(3 s, shift 0.5)), clr = Pre(b)";
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s15(period=1.5);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s2(period=2);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s3b(period=3);
  Buildings.Controls.OBC.CDL.Logical.Nor norS "Nor(SampleTrigger(2 s), SampleTrigger(3 s)): negated samples";
  Buildings.Controls.OBC.CDL.Logical.Toggle togD "u = SampleTrigger(1.5 s), clr = Nor(SampleTrigger(2 s), SampleTrigger(3 s))";
equation
  connect(inputs.y[1], aB.u);
  connect(inputs.y[2], bB.u);
  connect(aB.y, chaA.u);
  connect(chaA.y, notChaA.u);
  connect(notChaA.y, preNotChaA.u);
  connect(chaA.y, latA.u);
  connect(preNotChaA.y, latA.clr);
  connect(bB.y, chaB.u);
  connect(chaB.y, edgChaB.u);
  connect(chaB.y, cntB.trigger);
  connect(edgChaB.y, cntB.reset);
  connect(s3.y, notS3.u);
  connect(bB.y, preB.u);
  connect(notS3.y, togC.u);
  connect(preB.y, togC.clr);
  connect(s2.y, norS.u1);
  connect(s3b.y, norS.u2);
  connect(s15.y, togD.u);
  connect(norS.y, togD.clr);
end MixedFallbackPilot;
