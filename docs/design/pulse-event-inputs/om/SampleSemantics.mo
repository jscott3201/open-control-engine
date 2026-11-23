package SampleSemantics "Issue 800: CDL SampleTrigger into TriggeredSampler / a counter (equations transcribed)"
  function round "Buildings.Utilities.Math.Functions.round, transcribed"
    input Real x;
    input Integer n;
    output Real y;
  algorithm
    y := if (x > 0) then floor(x*10^n + 0.5)/10^n else ceil(x*10^n - 0.5)/10^n;
  end round;
  block SampleTrigger "Logical/Sources/SampleTrigger.mo:4-30"
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
  block TriggeredSampler "Discrete/TriggeredSampler.mo:16-22"
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
  block Counter
    input Boolean trigger;
    output Integer y;
  initial equation
    pre(y) = 0;
  equation
    when trigger then
      y = pre(y) + 1;
    end when;
  end Counter;
  model Pilot "u = 10 + k on (k - 0.5, k + 0.5]: changes mid-frame, so each 1 s frame instant sees 10 + k"
    Real u = 10 + floor(time + 0.5);
    SampleTrigger p05(period = 0.5);
    SampleTrigger p1(period = 1);
    SampleTrigger p2(period = 2);
    SampleTrigger p14(period = 1.4);
    SampleTrigger p1s(period = 1, shift = 0.5);
    TriggeredSampler s05(y_start = 7, u = u, trigger = p05.y);
    TriggeredSampler s1(y_start = 7, u = u, trigger = p1.y);
    TriggeredSampler s2(y_start = 7, u = u, trigger = p2.y);
    TriggeredSampler s14(y_start = 7, u = u, trigger = p14.y);
    TriggeredSampler s1s(y_start = 7, u = u, trigger = p1s.y);
    Counter c1(trigger = p1.y);
    Counter c14(trigger = p14.y);
    Counter c1s(trigger = p1s.y);
  end Pilot;
end SampleSemantics;
