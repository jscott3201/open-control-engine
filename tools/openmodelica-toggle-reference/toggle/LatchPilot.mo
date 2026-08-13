model TogglePilot
  Modelica.Blocks.Sources.BooleanTable uSource(
    table={30,90,150,210,270,390,450,510},
    startValue=true);
  Modelica.Blocks.Sources.BooleanTable clrSource(
    table={310,350,390,430},
    startValue=false);
  Buildings.Controls.OBC.CDL.Logical.Latch dut;

  output Boolean u;
  output Boolean clr;
  output Boolean y;
equation
  connect(uSource.y, dut.u);
  connect(clrSource.y, dut.clr);
  u = uSource.y;
  clr = clrSource.y;
  y = dut.y;
end TogglePilot;
