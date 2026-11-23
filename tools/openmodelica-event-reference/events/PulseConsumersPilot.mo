model PulseConsumersPilot
  "Every event consumer behind CDL.Logical.Change of a signal that changes on consecutive frames, at 1 s frames with each input held (issue #800)"
  Modelica.Blocks.Sources.CombiTimeTable inputs(
    table=[
      0, 0, 1.5, 0.5, 0, 0, 0, 1;
      1, 0, -2.0, 0.25, 0, 0, 0, 1;
      2, 1, 3.25, 0.75, 1, 0, 1, 1;
      3, 0, -4.5, 0.125, 0, 0, 1, 1;
      4, 0, 5.0, 0.625, 0, 1, 1, 1;
      5, 1, -6.75, 0.375, 0, 1, 1, 1;
      6, 1, 7.5, 0.875, 0, 1, 1, 1;
      7, 0, -8.0, 0.0625, 0, 1, 0, 1;
      8, 1, 9.25, 0.5625, 0, 0, 0, 1;
      9, 0, -10.5, 0.3125, 0, 0, 0, 1],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: u (changes at 2, 3, 5, 7, 8, 9 s), uR, um, clr, lvl, b, x (Boolean columns 1 or 0)";
  Modelica.Blocks.Math.RealToBoolean uB(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean clrB(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean lvlB(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean bB(threshold=0.5);
  Modelica.Blocks.Math.RealToBoolean xB(threshold=0.5);
  Buildings.Controls.OBC.CDL.Logical.Sources.Constant fal(k=false);
  Buildings.Controls.OBC.CDL.Reals.Sources.Constant zer(k=0);
  Buildings.Controls.OBC.CDL.Reals.Sources.Constant one(k=1);
  Buildings.Controls.OBC.CDL.Logical.Change cha "true at each change of u";
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler sampler(y_start=7);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMax maxer;
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMovingMean mean(n=3);
  Buildings.Controls.OBC.CDL.Reals.IntegratorWithReset integ(k=1, y_start=0.5) "u = 0: the output is the last reset value";
  Buildings.Controls.OBC.CDL.Reals.PIDWithReset pid(
    controllerType=Buildings.Controls.OBC.CDL.Types.SimpleController.PI,
    k=1, Ti=1e20, xi_start=0.125, y_reset=0.5, yMax=10, yMin=-10)
    "integral held: a reset shows as y = y_reset";
  Buildings.Controls.OBC.CDL.Logical.Latch lat "clr true at 2 s only: the 3 s pulse sets it";
  Buildings.Controls.OBC.CDL.Logical.Toggle tog;
  Buildings.Controls.OBC.CDL.Logical.TimerAccumulating tim(t=100) "u true throughout: each reset restarts y at 0";
  Buildings.Controls.OBC.CDL.Integers.OnCounter cntNot(y_start=3);
  Buildings.Controls.OBC.CDL.Logical.Not not1;
  Buildings.Controls.OBC.CDL.Integers.OnCounter cntOr(y_start=3);
  Buildings.Controls.OBC.CDL.Logical.Or or1 "Or(Change(u), lvl)";
  Buildings.Controls.OBC.CDL.Integers.OnCounter cntPre(y_start=3) "a fallback cone: Or(Change(u), Pre(b))";
  Buildings.Controls.OBC.CDL.Logical.Or or2;
  Buildings.Controls.OBC.CDL.Logical.Pre pre1;
  Buildings.Controls.OBC.CDL.Logical.Change chaX "x true from the start: the initialisation pulse";
  Buildings.Controls.OBC.CDL.Logical.Latch latX;
  Buildings.Controls.OBC.CDL.Logical.Toggle togX;
equation
  connect(inputs.y[1], uB.u);
  connect(inputs.y[4], clrB.u);
  connect(inputs.y[5], lvlB.u);
  connect(inputs.y[6], bB.u);
  connect(inputs.y[7], xB.u);
  connect(uB.y, cha.u);
  connect(inputs.y[2], sampler.u);
  connect(cha.y, sampler.trigger);
  connect(inputs.y[2], maxer.u);
  connect(cha.y, maxer.trigger);
  connect(inputs.y[2], mean.u);
  connect(cha.y, mean.trigger);
  connect(zer.y, integ.u);
  connect(inputs.y[2], integ.y_reset_in);
  connect(cha.y, integ.trigger);
  connect(one.y, pid.u_s);
  connect(inputs.y[3], pid.u_m);
  connect(cha.y, pid.trigger);
  connect(cha.y, lat.u);
  connect(clrB.y, lat.clr);
  connect(cha.y, tog.u);
  connect(fal.y, tog.clr);
  connect(xB.y, tim.u);
  connect(cha.y, tim.reset);
  connect(cha.y, not1.u);
  connect(not1.y, cntNot.trigger);
  connect(fal.y, cntNot.reset);
  connect(cha.y, or1.u1);
  connect(lvlB.y, or1.u2);
  connect(or1.y, cntOr.trigger);
  connect(fal.y, cntOr.reset);
  connect(bB.y, pre1.u);
  connect(cha.y, or2.u1);
  connect(pre1.y, or2.u2);
  connect(or2.y, cntPre.trigger);
  connect(fal.y, cntPre.reset);
  connect(xB.y, chaX.u);
  connect(chaX.y, latX.u);
  connect(fal.y, latX.clr);
  connect(chaX.y, togX.u);
  connect(fal.y, togX.clr);
end PulseConsumersPilot;
