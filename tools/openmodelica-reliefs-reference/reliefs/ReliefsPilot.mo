model ReliefsPilot
  Modelica.Blocks.Sources.Constant uOutDamMinSource(k=0.25);
  Modelica.Blocks.Sources.Constant uOutDamMaxSource(k=0.875);
  Modelica.Blocks.Sources.Constant uRetDamMinSource(k=0.125);
  Modelica.Blocks.Sources.Constant uRetDamMaxSource(k=0.75);
  Modelica.Blocks.Sources.TimeTable uTSupSource(
    table=[0,-0.5; 60,-0.5; 60,-0.25; 120,-0.25; 120,-0.125;
      180,-0.125; 180,0; 240,0; 240,0.125; 300,0.125; 300,0.25;
      360,0.25; 360,0.5; 420,0.5]);

  Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.Reliefs mod(
    uMin=-0.25,
    uMax=0.25,
    uOutDamMax=0.0,
    uRetDamMin=0.0);

  output Real uTSup;
  output Real uOutDam_min;
  output Real uOutDam_max;
  output Real uRetDam_min;
  output Real uRetDam_max;
  output Real yOutDam;
  output Real yRetDam;
equation
  connect(uTSupSource.y, mod.uTSup);
  connect(uOutDamMinSource.y, mod.uOutDam_min);
  connect(uOutDamMaxSource.y, mod.uOutDam_max);
  connect(uRetDamMinSource.y, mod.uRetDam_min);
  connect(uRetDamMaxSource.y, mod.uRetDam_max);
  uTSup = uTSupSource.y;
  uOutDam_min = uOutDamMinSource.y;
  uOutDam_max = uOutDamMaxSource.y;
  uRetDam_min = uRetDamMinSource.y;
  uRetDam_max = uRetDamMaxSource.y;
  yOutDam = mod.yOutDam;
  yRetDam = mod.yRetDam;
end ReliefsPilot;
