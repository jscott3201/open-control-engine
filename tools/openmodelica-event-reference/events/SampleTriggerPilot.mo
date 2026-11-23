model SampleTriggerPilot
  "CDL.Logical.Sources.SampleTrigger at periods below, at and above the 1 s frame step, into OnCounter, TriggeredSampler, Latch, TrimAndRespond's hold latch and TriggeredMovingMean, from an instant at t = 0 (issue #800)"
  Modelica.Blocks.Sources.CombiTimeTable inputs(
    table=[
      0.0, 2.5, 0;
      0.5, 3.5, 0;
      1.5, -1.25, 0;
      2.5, 4.75, 1;
      3.5, -6.5, 1;
      4.5, 8.0, 0;
      5.5, -9.25, 0;
      6.5, 10.5, 0;
      7.5, -11.75, 0;
      8.5, 13.0, 0],
    smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments,
    extrapolation=Modelica.Blocks.Types.Extrapolation.HoldLastPoint)
    "Columns: uR, clr (Boolean 1 or 0); frame k's values hold from half a frame before it";
  Modelica.Blocks.Math.RealToBoolean clrB(threshold=0.5);
  Buildings.Controls.OBC.CDL.Logical.Sources.Constant fal(k=false);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s05(period=0.5);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s1(period=1);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s2(period=2);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s14(period=1.4);
  Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger s1h(period=1, shift=0.5) "instants half way between frames";
  Buildings.Controls.OBC.CDL.Integers.OnCounter c05(y_start=0);
  Buildings.Controls.OBC.CDL.Integers.OnCounter c1(y_start=0);
  Buildings.Controls.OBC.CDL.Integers.OnCounter c2(y_start=0);
  Buildings.Controls.OBC.CDL.Integers.OnCounter c14(y_start=0);
  Buildings.Controls.OBC.CDL.Integers.OnCounter c1h(y_start=0);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler t05(y_start=7);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler t1(y_start=7);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler t2(y_start=7);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler t14(y_start=7);
  Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler tNot(y_start=7) "Not(SampleTrigger(1 s)): never samples in Modelica";
  Buildings.Controls.OBC.CDL.Logical.Not not1;
  Buildings.Controls.OBC.CDL.Logical.Latch lat "u = SampleTrigger(1 s), clr true at 3 and 4 s";
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMovingMean mean(n=3) "behind SampleTrigger(1 s), from its instant at t = 0";
  Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter neg(k=-1) "-uR: a negative input at t = 0";
  Buildings.Controls.OBC.CDL.Discrete.TriggeredMax maxer "behind SampleTrigger(1 s): initial equation y = u, then the t = 0 instant";
  Buildings.Controls.OBC.CDL.Reals.Sources.Constant zer(k=0);
  Buildings.Controls.OBC.CDL.Reals.Sources.Constant one(k=1);
  Buildings.Controls.OBC.CDL.Logical.Sources.Constant tru(k=true);
  Buildings.Controls.OBC.CDL.Reals.IntegratorWithReset integ(k=1, y_start=0.5) "u = 0: the output is the last reset value";
  Buildings.Controls.OBC.CDL.Reals.PIDWithReset pid(
    controllerType=Buildings.Controls.OBC.CDL.Types.SimpleController.PI,
    k=1, Ti=1e20, xi_start=0.125, y_reset=0.5, yMax=10, yMin=-10)
    "integral held: a reset shows as y = y_reset";
  Buildings.Controls.OBC.CDL.Logical.TimerAccumulating tim(t=100) "u true throughout: each instant resets y to 0";
  Buildings.Controls.OBC.CDL.Logical.Toggle tog "u = SampleTrigger(1 s)";
  Buildings.Controls.OBC.CDL.Logical.Not notHol "G36 TrimAndRespond's hold latch: the clr column stands for truHol.y";
  Buildings.Controls.OBC.CDL.Logical.And notHolAndTic;
  Buildings.Controls.OBC.CDL.Logical.Latch latHol "u = SampleTrigger(1 s), clr = not hold and SampleTrigger(1 s)";
equation
  connect(inputs.y[2], clrB.u);
  connect(s05.y, c05.trigger);
  connect(s1.y, c1.trigger);
  connect(s2.y, c2.trigger);
  connect(s14.y, c14.trigger);
  connect(s1h.y, c1h.trigger);
  connect(fal.y, c05.reset);
  connect(fal.y, c1.reset);
  connect(fal.y, c2.reset);
  connect(fal.y, c14.reset);
  connect(fal.y, c1h.reset);
  connect(inputs.y[1], t05.u);
  connect(inputs.y[1], t1.u);
  connect(inputs.y[1], t2.u);
  connect(inputs.y[1], t14.u);
  connect(inputs.y[1], tNot.u);
  connect(s05.y, t05.trigger);
  connect(s1.y, t1.trigger);
  connect(s2.y, t2.trigger);
  connect(s14.y, t14.trigger);
  connect(s1.y, not1.u);
  connect(not1.y, tNot.trigger);
  connect(s1.y, lat.u);
  connect(clrB.y, lat.clr);
  connect(inputs.y[1], mean.u);
  connect(s1.y, mean.trigger);
  connect(inputs.y[1], neg.u);
  connect(neg.y, maxer.u);
  connect(s1.y, maxer.trigger);
  connect(zer.y, integ.u);
  connect(inputs.y[1], integ.y_reset_in);
  connect(s1.y, integ.trigger);
  connect(one.y, pid.u_s);
  connect(inputs.y[1], pid.u_m);
  connect(s1.y, pid.trigger);
  connect(tru.y, tim.u);
  connect(s1.y, tim.reset);
  connect(s1.y, tog.u);
  connect(fal.y, tog.clr);
  connect(clrB.y, notHol.u);
  connect(notHol.y, notHolAndTic.u1);
  connect(s1.y, notHolAndTic.u2);
  connect(s1.y, latHol.u);
  connect(notHolAndTic.y, latHol.clr);
end SampleTriggerPilot;
