model NandPilot
  Modelica.Blocks.Sources.BooleanTable u1Source(
    table={120},
    startValue=false);
  Modelica.Blocks.Sources.BooleanTable u2Source(
    table={60,120,180},
    startValue=false);
  Buildings.Controls.OBC.CDL.Logical.And dut;

  output Boolean u1;
  output Boolean u2;
  output Boolean y;
equation
  connect(u1Source.y, dut.u1);
  connect(u2Source.y, dut.u2);
  u1 = u1Source.y;
  u2 = u2Source.y;
  y = dut.y;
end NandPilot;
