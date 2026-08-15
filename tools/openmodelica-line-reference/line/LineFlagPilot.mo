model LinePilot
  Modelica.Blocks.Sources.Constant x1Source(k=-2.0);
  Modelica.Blocks.Sources.Constant f1Source(k=1.25);
  Modelica.Blocks.Sources.Constant x2Source(k=2.0);
  Modelica.Blocks.Sources.Constant f2Source(k=3.25);
  Modelica.Blocks.Sources.TimeTable uSource(
    table=[0,-4; 60,-4; 60,-2; 120,-2; 120,0; 180,0; 180,2; 240,2; 240,4]);

  Buildings.Controls.OBC.CDL.Reals.Line both(limitBelow=true, limitAbove=true);
  Buildings.Controls.OBC.CDL.Reals.Line below(limitBelow=true, limitAbove=true);
  Buildings.Controls.OBC.CDL.Reals.Line above(limitBelow=false, limitAbove=true);
  Buildings.Controls.OBC.CDL.Reals.Line unlimited(limitBelow=false, limitAbove=false);

  output Real x1;
  output Real f1;
  output Real x2;
  output Real f2;
  output Real u;
  output Real yBoth;
  output Real yBelow;
  output Real yAbove;
  output Real yUnlimited;
equation
  connect(x1Source.y, both.x1);
  connect(x1Source.y, below.x1);
  connect(x1Source.y, above.x1);
  connect(x1Source.y, unlimited.x1);
  connect(f1Source.y, both.f1);
  connect(f1Source.y, below.f1);
  connect(f1Source.y, above.f1);
  connect(f1Source.y, unlimited.f1);
  connect(x2Source.y, both.x2);
  connect(x2Source.y, below.x2);
  connect(x2Source.y, above.x2);
  connect(x2Source.y, unlimited.x2);
  connect(f2Source.y, both.f2);
  connect(f2Source.y, below.f2);
  connect(f2Source.y, above.f2);
  connect(f2Source.y, unlimited.f2);
  connect(uSource.y, both.u);
  connect(uSource.y, below.u);
  connect(uSource.y, above.u);
  connect(uSource.y, unlimited.u);

  x1 = x1Source.y;
  f1 = f1Source.y;
  x2 = x2Source.y;
  f2 = f2Source.y;
  u = uSource.y;
  yBoth = both.y;
  yBelow = below.y;
  yAbove = above.y;
  yUnlimited = unlimited.y;
end LinePilot;
