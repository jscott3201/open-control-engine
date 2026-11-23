package PulseReview
  "Issue 800 review round: OpenModelica semantics the design note rests on. TRANSCRIPTIONS of the
   vendored CDL equations (paths and lines in each block's comment), not the Buildings library,
   and not a conformance fixture."
  function round "Buildings.Utilities.Math.Functions.round, transcribed"
    input Real x;
    input Integer n;
    output Real y;
  algorithm
    y := if (x > 0) then floor(x*10^n + 0.5)/10^n else ceil(x*10^n - 0.5)/10^n;
  end round;
  block SampleTrigger "CDL/Logical/Sources/SampleTrigger.mo:4-30"
    parameter Real period;
    parameter Real shift = 0;
    output Boolean y;
  protected
    parameter Real t0(fixed = false);
  initial equation
    t0 = round(x = integer((time)/period)*period + mod(shift, period), n = 6);
  equation
    y = sample(t0, period);
  end SampleTrigger;
  block Change "CDL/Logical/Change.mo:4-15"
    parameter Boolean pre_u_start = false;
    input Boolean u(fixed = true, start = pre_u_start);
    output Boolean y;
  equation
    y = change(u);
  end Change;
  block Edge "CDL/Logical/Edge.mo:4-17"
    parameter Boolean pre_u_start = false;
    input Boolean u;
    output Boolean y;
  initial equation
    pre(u) = pre_u_start;
  equation
    y = edge(u);
  end Edge;
  block Pre "CDL/Logical/Pre.mo:13-17"
    parameter Boolean pre_u_start = false;
    input Boolean u;
    output Boolean y;
  initial equation
    pre(u) = pre_u_start;
  equation
    y = pre(u);
  end Pre;
  block Latch "CDL/Logical/Latch.mo:4-19"
    input Boolean u;
    input Boolean clr;
    output Boolean y;
  initial equation
    y = not clr and u;
  equation
    when {clr, u} then
      y = not clr and u;
    end when;
  end Latch;
  block Toggle "CDL/Logical/Toggle.mo:4-46"
    input Boolean u;
    input Boolean clr;
    output Boolean y;
  initial equation
    pre(y) = false;
    pre(u) = false;
    pre(clr) = false;
  equation
    when initial() then
      y = if clr then false else u;
    elsewhen (not clr) and change(u) and (pre(u) == false) and (pre(y) == false) then
      y = true;
    elsewhen (not clr) and change(u) and (pre(u) == false) and (pre(y) == true) then
      y = false;
    elsewhen (not clr) and change(u) and (pre(u) == true) then
      y = pre(y);
    elsewhen clr then
      y = false;
    end when;
  end Toggle;
  block TriggeredSampler "CDL/Discrete/TriggeredSampler.mo:4-22"
    parameter Real y_start = 0;
    input Real u;
    input Boolean trigger;
    output Real y;
  initial equation
    y = y_start;
  equation
    when trigger then
      y = u;
    end when;
  end TriggeredSampler;
  block TimerAccumulating "CDL/Logical/TimerAccumulating.mo:4-65"
    parameter Real t = 0;
    input Boolean u;
    input Boolean reset;
    output Real y;
    output Boolean passed;
  protected
    discrete Real entryTime;
    discrete Real yAcc;
  initial equation
    pre(u) = false;
    pre(reset) = false;
    pre(entryTime) = time;
    pre(passed) = t <= 0;
    yAcc = 0;
  equation
    when reset then
      entryTime = time;
      passed = t <= 0;
      yAcc = 0;
    elsewhen u then
      entryTime = time;
      passed = t <= yAcc;
      yAcc = pre(yAcc);
    elsewhen u and time >= t + pre(entryTime) - pre(yAcc) then
      passed = true;
      entryTime = pre(entryTime);
      yAcc = pre(yAcc);
    elsewhen not u then
      passed = pre(passed);
      entryTime = pre(entryTime);
      yAcc = pre(y);
    end when;
    y = if u then yAcc + time - entryTime else yAcc;
  end TimerAccumulating;
  block Counter "Integers.OnCounter with reset false: CDL/Integers/OnCounter.mo:16-25"
    input Boolean trigger;
    output Integer y;
  initial equation
    pre(y) = 0;
  equation
    when trigger then
      y = pre(y) + 1;
    end when;
  end Counter;
  model Pilot
    "u changes at 2, 3, 5, 7, 8, 9 s; b true on [2, 7); lvl35 from 3.5 s; x true from the start"
    Boolean u = (time >= 2 and time < 3) or (time >= 5 and time < 7) or (time >= 8 and time < 9);
    Boolean b = time >= 2 and time < 7;
    Boolean lvl35 = time >= 3.5;
    Boolean x = time < 100;
    Boolean fal = false;
    Real uR = 10 + floor(time + 0.5);
    SampleTrigger s1(period = 1);
    // review item 1: a sample through Not, and sample or level
    Counter notSample(trigger = not s1.y);
    Counter sampleOrLevel(trigger = s1.y or lvl35);
    TriggeredSampler samplerNotSample(y_start = 7, u = uR, trigger = not s1.y);
    // review item 2: Pre of a level beside a pulse
    Change chaU(u = u);
    Pre preB(u = b);
    Pre preU(u = u);
    Counter orChangePreB(trigger = chaU.y or preB.y);
    Counter norChangePreU(trigger = not (chaU.y or preU.y));
    // review item 3: non-primitive pulse leaves
    Pre preChaU(u = chaU.y);
    Counter orChangePreChange(trigger = chaU.y or preChaU.y);
    Edge edgU(u = u);
    Change chaEdg(u = edgU.y);
    Counter changeOfEdge(trigger = chaEdg.y);
    // review item 6: a second-iteration pulse and a first-iteration one
    Edge edgNotCha(u = not chaU.y);
    Counter andChangeEdgeNotChange(trigger = chaU.y and edgNotCha.y);
    // review item 4: Latch and Toggle reading the initialisation pulse
    Change chaX(u = x);
    Latch latX(u = chaX.y, clr = fal);
    Toggle togX(u = chaX.y, clr = fal);
    // review item 5: TimerAccumulating.reset fed by consecutive Change pulses
    TimerAccumulating timAcc(u = x, reset = chaU.y);
  end Pilot;
end PulseReview;
