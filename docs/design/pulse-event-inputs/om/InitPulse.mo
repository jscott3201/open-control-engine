model InitPulse
  "Issue 800: does a Change or Edge true at initialisation fire a when at t = 0? Built-in operators,
   a scratch-quality semantics probe, not a conformance fixture"
  Boolean u = time < 100;
  Boolean cha(start = false, fixed = false);
  Boolean edg;
  Boolean uc(fixed = true, start = false) = u;
  Boolean ue = u;
  Integer nCha, nEdg, nSam;
initial equation
  pre(ue) = false;
  pre(nCha) = 0;
  pre(nEdg) = 0;
  pre(nSam) = 0;
equation
  cha = change(uc);
  edg = edge(ue);
  when cha then
    nCha = pre(nCha) + 1;
  end when;
  when edg then
    nEdg = pre(nEdg) + 1;
  end when;
  when sample(0, 1) then
    nSam = pre(nSam) + 1;
  end when;
end InitPulse;
