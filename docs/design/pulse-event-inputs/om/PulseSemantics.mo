package PulseSemantics "Issue 800: how Modelica fires when-clauses on pulses (CDL equations transcribed, no libraries)"
  block Counter "OnCounter without reset: Integers/OnCounter.mo:16-25 with reset=false"
    input Boolean trigger;
    output Integer y;
  initial equation
    pre(y) = 0;
  equation
    when trigger then
      y = pre(y) + 1;
    end when;
  end Counter;
  block ChangeB "Logical/Change.mo:4-15"
    parameter Boolean pre_u_start = false;
    input Boolean u(fixed = true, start = pre_u_start);
    output Boolean y;
  equation
    y = change(u);
  end ChangeB;
  block EdgeB "Logical/Edge.mo"
    parameter Boolean pre_u_start = false;
    input Boolean u;
    output Boolean y;
  initial equation
    pre(u) = pre_u_start;
  equation
    y = edge(u);
  end EdgeB;
  block PreB "Logical/Pre.mo"
    parameter Boolean pre_u_start = false;
    input Boolean u;
    output Boolean y;
  initial equation
    pre(u) = pre_u_start;
  equation
    y = pre(u);
  end PreB;
  block LatchB "Logical/Latch.mo:14-19"
    input Boolean u;
    input Boolean clr;
    output Boolean y;
  initial equation
    y = not clr and u;
  equation
    when {clr, u} then
      y = not clr and u;
    end when;
  end LatchB;
  model Pilot
    "u changes at 2, 3, 5, 7, 8, 9 s (the OnCounterChangePilot pattern); lvl true on [4, 8)"
    Boolean u = (time >= 2 and time < 3) or (time >= 5 and time < 7) or (time >= 8 and time < 9);
    Boolean lvl = time >= 4 and time < 8;
    Boolean lvl2 = time >= 3.5 and time < 100;
    ChangeB cha(u = u);
    // 1. direct: reference count of changes
    Counter direct(trigger = cha.y);
    // 2. Not-routed pulse
    Counter notRouted(trigger = not cha.y);
    // 3. And(pulse, level)
    Counter andLevel(trigger = cha.y and lvl);
    // 4. Or(pulse, level)
    Counter orLevel(trigger = cha.y or lvl);
    // 5. Or of the same pulse twice (two pulses at one instant)
    ChangeB cha2(u = u);
    Counter orSame(trigger = cha.y or cha2.y);
    // 6. user-built change: Xor(u, Pre(u))
    PreB preU(u = u);
    Counter xorPre(trigger = (u and not preU.y) or (not u and preU.y));
    // 7. user-built rising edge: And(u, Not(Pre(u)))
    Counter andNotPre(trigger = u and not preU.y);
    // 8. Edge(Not(Change(u))): a pulse cascade of depth 2
    EdgeB edgNot(u = not cha.y);
    Counter edgeNotChange(trigger = edgNot.y);
    // 9. Edge of a pulse: Edge(Change(u))
    EdgeB edgCha(u = cha.y);
    Counter edgeChange(trigger = edgCha.y);
    // 10. Pre of a pulse
    PreB preCha(u = cha.y);
    Counter prePulse(trigger = preCha.y);
    // 11. sample() at 0.5 s, 0.7 s and 1.4 s (frames are 1 s): count of when firings
    Counter s05(trigger = sample(0, 0.5));
    Counter s07(trigger = sample(0, 0.7));
    Counter s14(trigger = sample(0, 1.4));
    // 12. Or(pulse, level) where the level rises one instant after a pulse (Or(Edge, level) shape)
    Counter orLevel2(trigger = cha.y or lvl2);
    // 13. Latch with u = pulse, clr = And(pulse, not hold): TrimAndRespond's lat with a hold
    Boolean hold = time >= 6.5 and time < 100;
    LatchB lat(u = sample(0, 0.5), clr = sample(0, 0.5) and not hold);
  end Pilot;
end PulseSemantics;
