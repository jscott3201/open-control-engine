//! One-sentence descriptions of every registered class (GitHub #440).
//!
//! For the 132 classes with a vendored upstream source the sentence is the Modelica class
//! description string, verbatim (`block MultiSum "Sum of Reals, …"`): `oce-api`'s
//! `class_descriptions_match_vendored_sources` test holds each one equal to
//! `third_party/modelica-buildings-cdl`. `CDL.Logical.TrueHoldWithReset` (absent from current
//! Buildings) and the three reserved lowering identities carry engine-written sentences.
//! Descriptions are documentation, not behaviour: they enter neither the catalog JSON nor
//! [`crate::class_signature_hash`].

/// `(class_path, description)` in registry order.
pub(crate) static CLASS_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "CDL.Reals.Sources.Constant",
        "Output constant signal of type Real",
    ),
    ("CDL.Reals.Sources.CivilTime", "Civil time"),
    (
        "CDL.Reals.Sources.Pulse",
        "Generate pulse signal of type Real",
    ),
    ("CDL.Reals.Sources.Ramp", "Generate ramp signal"),
    ("CDL.Reals.Sources.Sin", "Generate sine signal"),
    (
        "CDL.Reals.Sources.CalendarTime",
        "Computes the unix time stamp and calendar time from the simulation time",
    ),
    (
        "CDL.Reals.Sources.TimeTable",
        "Table look-up with respect to time and linear or periodic extrapolation",
    ),
    ("CDL.Reals.Add", "Output the sum of the two inputs"),
    (
        "CDL.Reals.Subtract",
        "Output the difference of the two inputs",
    ),
    ("CDL.Reals.Multiply", "Output product of the two inputs"),
    (
        "CDL.Reals.Divide",
        "Output first input divided by second input",
    ),
    (
        "CDL.Reals.Sqrt",
        "Output the square root of the input (input >= 0 required)",
    ),
    ("CDL.Reals.Sin", "Output the sine of the input"),
    ("CDL.Reals.Cos", "Output the cosine of the input"),
    ("CDL.Reals.Tan", "Output the tangent of the input"),
    ("CDL.Reals.Asin", "Output the arc sine of the input"),
    ("CDL.Reals.Acos", "Output the arc cosine of the input"),
    ("CDL.Reals.Atan", "Output the arc tangent of the input"),
    (
        "CDL.Reals.Atan2",
        "Output atan(u1/u2) of the inputs u1 and u2",
    ),
    (
        "CDL.Reals.Exp",
        "Output the exponential (base e) of the input",
    ),
    (
        "CDL.Reals.Log",
        "Output the natural (base e) logarithm of the input (input > 0 required)",
    ),
    (
        "CDL.Reals.Log10",
        "Output the base 10 logarithm of the input (input > 0 required)",
    ),
    ("CDL.Reals.Average", "Output the average of its two inputs"),
    (
        "CDL.Reals.Modulo",
        "Output the remainder of first input divided by second input (~=0)",
    ),
    ("CDL.Reals.Round", "Round real number to given digits"),
    (
        "CDL.Reals.AddParameter",
        "Output the sum of an input plus a parameter",
    ),
    (
        "CDL.Reals.MultiplyByParameter",
        "Output the product of a gain value with the input signal",
    ),
    ("CDL.Reals.Abs", "Output the absolute value of the input"),
    ("CDL.Reals.Min", "Pass through the smallest signal"),
    ("CDL.Reals.Max", "Pass through the largest signal"),
    (
        "CDL.Reals.MultiMax",
        "Output the maximum element of the input vector",
    ),
    (
        "CDL.Reals.MultiMin",
        "Output the minimum element of the input vector",
    ),
    (
        "CDL.Reals.MultiSum",
        "Sum of Reals, y = k[1]*u[1] + k[2]*u[2] + ... + k[n]*u[n]",
    ),
    (
        "CDL.Reals.MatrixGain",
        "Output the product of a gain matrix with the input signal vector",
    ),
    (
        "CDL.Reals.MatrixMax",
        "Output vector of row- or column-wise maximum of the input matrix",
    ),
    (
        "CDL.Reals.MatrixMin",
        "Output vector of row- or column-wise minimum values",
    ),
    (
        "CDL.Reals.Sort",
        "Sort elements of input vector in ascending or descending order",
    ),
    ("CDL.Reals.Limiter", "Limit the range of a signal"),
    (
        "CDL.Reals.Line",
        "Output the value of the input x along a line specified by two points",
    ),
    (
        "CDL.Reals.Greater",
        "Output y is true, if input u1 is greater than input u2 with hysteresis",
    ),
    (
        "CDL.Reals.GreaterThreshold",
        "Output y is true, if input u is greater than threshold with hysteresis",
    ),
    (
        "CDL.Reals.Hysteresis",
        "Transform Real to Boolean signal with Hysteresis",
    ),
    (
        "CDL.Reals.Less",
        "Output y is true, if input u1 is less than input u2",
    ),
    (
        "CDL.Reals.LessThreshold",
        "Output y is true, if input u is less than threshold with hysteresis",
    ),
    ("CDL.Reals.Switch", "Switch between two Real signals"),
    (
        "CDL.Reals.IntegratorWithReset",
        "Output the integral of the input signal",
    ),
    (
        "CDL.Reals.Derivative",
        "Block that approximates the derivative of the input",
    ),
    (
        "CDL.Reals.LimitSlewRate",
        "Limit the increase or decrease rate of input",
    ),
    ("CDL.Reals.MovingAverage", "Block to output moving average"),
    ("CDL.Reals.Ramp", "Limit the changing rate of the input"),
    ("CDL.Reals.PID", "P, PI, PD, and PID controller"),
    (
        "CDL.Reals.PIDWithReset",
        "P, PI, PD, and PID controller with output reset",
    ),
    (
        "CDL.Psychrometrics.DewPoint_TDryBulPhi",
        "Block to compute the dew point temperature based on relative humidity",
    ),
    (
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "Block to compute the specific enthalpy based on relative humidity",
    ),
    (
        "CDL.Psychrometrics.WetBulb_TDryBulPhi",
        "Block to compute the wet bulb temperature based on relative humidity",
    ),
    (
        "CDL.Logical.Sources.Constant",
        "Output constant signal of type Boolean",
    ),
    (
        "CDL.Logical.Sources.Pulse",
        "Generate pulse signal of type Boolean",
    ),
    (
        "CDL.Logical.Sources.TimeTable",
        "Table look-up with respect to time with constant segments",
    ),
    ("CDL.Logical.And", "Logical 'and': y = u1 and u2"),
    ("CDL.Logical.Or", "Logical 'or': y = u1 or u2"),
    (
        "CDL.Logical.MultiAnd",
        "Logical MultiAnd, y = u[1] and u[2] and u[3] and ...",
    ),
    (
        "CDL.Logical.MultiOr",
        "Logical MultiOr, y = u[1] or u[2] or u[3] or ...",
    ),
    ("CDL.Logical.Not", "Logical not"),
    ("CDL.Logical.Nand", "Logical 'nand': y = not (u1 and u2)"),
    ("CDL.Logical.Nor", "Logical 'nor': y = not (u1 or u2)"),
    ("CDL.Logical.Xor", "Logical 'xor': y = u1 xor u2"),
    ("CDL.Logical.Switch", "Switch between two boolean signals"),
    (
        "CDL.Logical.Pre",
        "Breaks algebraic loops by adding a delay of the output without advancing time (y = pre(u): event iteration continues until u = pre(u))",
    ),
    (
        "CDL.Logical.Edge",
        "Output y is true, if the input u has a rising edge (y = edge(u))",
    ),
    (
        "CDL.Logical.Sources.SampleTrigger",
        "Generate sample trigger signal",
    ),
    ("CDL.Logical.Proof", "Verify two boolean inputs"),
    (
        "CDL.Logical.VariablePulse",
        "Generate boolean pulse with the width specified by input",
    ),
    (
        "CDL.Logical.FallingEdge",
        "Output y is true, if the input u has a falling edge (y = edge(not u))",
    ),
    (
        "CDL.Logical.Change",
        "Output y is true, if the input u has a rising or falling edge (y = change(u))",
    ),
    ("CDL.Logical.Latch", "Maintains a true signal until cleared"),
    (
        "CDL.Logical.Toggle",
        "Toggles output value whenever its input turns true",
    ),
    (
        "CDL.Logical.Timer",
        "Timer measuring the time from the time instant where the Boolean input became true",
    ),
    (
        "CDL.Logical.TimerAccumulating",
        "Accumulating timer that can be reset",
    ),
    (
        "CDL.Logical.TrueDelay",
        "Delay a rising edge of the input, but do not delay a falling edge",
    ),
    (
        "CDL.Logical.TrueFalseHold",
        "Block that holds an output signal for at least a specified duration",
    ),
    (
        "CDL.Logical.TrueHoldWithReset",
        "Hold a true output for at least a duration; the engine-local clr input resets the hold",
    ),
    (
        "urn:oce:lowering#PassThrough.Real",
        "Engine-reserved identity that forwards a Real boundary input straight to a boundary output",
    ),
    (
        "urn:oce:lowering#PassThrough.Integer",
        "Engine-reserved identity that forwards an Integer boundary input straight to a boundary output",
    ),
    (
        "urn:oce:lowering#PassThrough.Boolean",
        "Engine-reserved identity that forwards a Boolean boundary input straight to a boundary output",
    ),
    (
        "CDL.Conversions.BooleanToInteger",
        "Convert Boolean to Integer signal",
    ),
    (
        "CDL.Conversions.BooleanToReal",
        "Convert Boolean to Real signal",
    ),
    (
        "CDL.Conversions.IntegerToReal",
        "Convert Integer to Real signals",
    ),
    (
        "CDL.Conversions.RealToInteger",
        "Convert Real to Integer signal",
    ),
    (
        "CDL.Integers.Sources.Constant",
        "Output constant signal of type Integer",
    ),
    (
        "CDL.Integers.Sources.Pulse",
        "Generate pulse signal of type Integer",
    ),
    (
        "CDL.Integers.Sources.TimeTable",
        "Table look-up with respect to time with constant segments",
    ),
    ("CDL.Integers.Abs", "Output the absolute value of the input"),
    ("CDL.Integers.Add", "Output the sum of the two inputs"),
    (
        "CDL.Integers.Subtract",
        "Output the difference of the two inputs",
    ),
    ("CDL.Integers.Multiply", "Output product of the two inputs"),
    (
        "CDL.Integers.AddParameter",
        "Output the sum of an input plus a parameter",
    ),
    ("CDL.Integers.Max", "Pass through the largest signal"),
    ("CDL.Integers.Min", "Pass through the smallest signal"),
    (
        "CDL.Integers.MultiSum",
        "Sum of Integers, y = k[1]*u[1] + k[2]*u[2] + ... + k[n]*u[n]",
    ),
    ("CDL.Integers.Switch", "Switch between two integer signals"),
    (
        "CDL.Integers.Equal",
        "Output y is true, if input u1 is equal to input u2",
    ),
    (
        "CDL.Integers.Greater",
        "Output y is true, if input u1 is greater than input u2",
    ),
    (
        "CDL.Integers.GreaterThreshold",
        "Output y is true, if input u is greater than a threshold",
    ),
    (
        "CDL.Integers.GreaterEqual",
        "Output y is true, if input u1 is greater or equal than input u2",
    ),
    (
        "CDL.Integers.GreaterEqualThreshold",
        "Output y is true, if input u is greater or equal than a threshold",
    ),
    (
        "CDL.Integers.Less",
        "Output y is true, if input u1 is less than input u2",
    ),
    (
        "CDL.Integers.LessThreshold",
        "Output y is true, if input u is less than a threshold",
    ),
    (
        "CDL.Integers.LessEqual",
        "Output y is true, if input u1 is less or equal than input u2",
    ),
    (
        "CDL.Integers.LessEqualThreshold",
        "Output y is true, if input u is less or equal than a threshold",
    ),
    (
        "CDL.Integers.OnCounter",
        "Increment the output if the input switches to true",
    ),
    (
        "CDL.Integers.Change",
        "Output whether the Integer input changes values, increases or decreases",
    ),
    (
        "CDL.Integers.Stage",
        "Output total stages that should be enabled",
    ),
    (
        "CDL.Routing.BooleanExtractSignal",
        "Extract signals from a boolean input signal vector",
    ),
    (
        "CDL.Routing.BooleanExtractor",
        "Extract scalar signal out of boolean signal vector dependent on integer input index",
    ),
    (
        "CDL.Routing.BooleanScalarReplicator",
        "Boolean signal replicator",
    ),
    (
        "CDL.Routing.BooleanVectorFilter",
        "Filter a boolean vector based on a boolean mask",
    ),
    (
        "CDL.Routing.BooleanVectorReplicator",
        "Boolean vector signal replicator",
    ),
    (
        "CDL.Routing.IntegerExtractSignal",
        "Extract signals from an integer input signal vector",
    ),
    (
        "CDL.Routing.IntegerExtractor",
        "Extract scalar signal out of integer signal vector dependent on integer input index",
    ),
    (
        "CDL.Routing.IntegerScalarReplicator",
        "Integer signal replicator",
    ),
    (
        "CDL.Routing.IntegerVectorFilter",
        "Filter an integer vector based on a boolean mask",
    ),
    (
        "CDL.Routing.IntegerVectorReplicator",
        "Integer vector signal replicator",
    ),
    (
        "CDL.Routing.RealExtractSignal",
        "Extract signals from a real input signal vector",
    ),
    (
        "CDL.Routing.RealExtractor",
        "Extract scalar signal out of real signal vector dependent on integer input index",
    ),
    ("CDL.Routing.RealScalarReplicator", "Real signal replicator"),
    (
        "CDL.Routing.RealVectorFilter",
        "Filter a real vector of based on a boolean mask",
    ),
    (
        "CDL.Routing.RealVectorReplicator",
        "Real vector signal replicator",
    ),
    (
        "CDL.Discrete.FirstOrderHold",
        "First order hold of a sampled-data system",
    ),
    (
        "CDL.Discrete.Sampler",
        "Ideal sampler of a continuous signal",
    ),
    (
        "CDL.Discrete.TriggeredMax",
        "Output the maximum, absolute value of a continuous signal at trigger instants",
    ),
    (
        "CDL.Discrete.TriggeredMovingMean",
        "Triggered discrete moving mean of an input signal",
    ),
    (
        "CDL.Discrete.TriggeredSampler",
        "Triggered sampling of continuous signals",
    ),
    (
        "CDL.Discrete.UnitDelay",
        "Output the input signal with a unit delay",
    ),
    (
        "CDL.Discrete.ZeroOrderHold",
        "Output the input signal with a zero order hold",
    ),
    (
        "CDL.Utilities.Assert",
        "Print a warning message when input becomes false",
    ),
    ("CDL.Utilities.SunRiseSet", "Next sunrise and sunset time"),
];
