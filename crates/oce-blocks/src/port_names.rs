//! Declared CDL port names for every block class whose interface is fixed-arity.
//!
//! # Why the registry needs names at all
//!
//! A CXF document lists a block's ports as an array, and the resolver takes position from array
//! order. Nothing downstream checks a port's identity: the arity guard compares counts, and
//! `oce-validate`'s `check_ports_dir` compares each position's kind. A transposition between two
//! ports of the **same kind** passes both and silently computes a different answer — `CDL.Reals.PID`
//! with `u_s`/`u_m` swapped inverts the control action.
//!
//! That is not a hypothetical. The reference CDL toolchain, `modelica-json`, renders a class's
//! connectors sorted **alphabetically by label** rather than in declaration order: its own
//! `CDL.Reals.PID` lists `u_m` before `u_s`, where the Modelica source declares `u_s` first. Eight
//! classes reorder under an alphabetical sort without changing their kind sequence, so the kind
//! check cannot see them — `Reals.PID`, `Reals.Derivative`, `Reals.Line`, `Logical.Latch`,
//! `Logical.Toggle`, `Logical.Proof`, `Logical.TimerAccumulating`, and `Integers.OnCounter`.
//!
//! Names give the resolver something to bind against, so a document that orders its ports
//! differently can be **wired correctly** rather than rejected. Ordering is a renderer's choice;
//! identity is not.
//!
//! # What is deliberately absent
//!
//! [`port_names`] returns `None` for 29 of the 133 registered classes, and absence means "bind
//! positionally, as before" — never "this class has no ports":
//!
//! - **25 width-driven classes.** Upstream declares one array connector (`u[nin]`) where this
//!   engine carries N flattened scalars, so there is no 1:1 name correspondence to record.
//! - **The three `Sources.TimeTable` classes.** Upstream declares an array output `y[nout]` while
//!   the registry carries a single output, so naming that port `y` would assert a scalar interface
//!   upstream does not have.
//! - **`CDL.Logical.TrueHoldWithReset`.** It is the one registered class with no vendored upstream
//!   source and it appears in no fixture, so nothing in this repository could contradict a name
//!   written for it. Unfalsifiable data is what the port-order audit exists to eliminate, so it
//!   gets none.
//!
//! # What checks these names, and what does not
//!
//! `crates/oce-cxf/tests/fixture_port_order.rs` compares this table against port names derived from
//! vendored upstream Modelica source, and the 104 entries here are exactly the classes that audit
//! can reach. Be precise about what that buys: these names were transcribed from CDL, so the
//! comparison detects **drift** between the two artifacts — it is not an independent oracle, and it
//! would not catch a name that was wrong in both places from the start.
//!
//! The names earn their keep operationally instead. Once the resolver binds by name, every fixture
//! and every conformance golden exercises them: a wrong name stops matching the document that uses
//! the real one, and the failure is loud.

/// The declared CDL port names of one block class, in upstream declaration order.
///
/// Index correspondence is the whole contract: `inputs[i]` names the port at input position `i` of
/// the same class's [`BlockSignature`](crate::BlockSignature), and likewise for `outputs`.
#[derive(Clone, Copy, Debug)]
pub struct ClassPortNames {
    /// Canonical class path, e.g. `"CDL.Reals.PID"`.
    pub class_path: &'static str,
    /// Input port names in declaration order (index = port index).
    pub inputs: &'static [&'static str],
    /// Output port names in declaration order (index = port index).
    pub outputs: &'static [&'static str],
}

/// Sorted by `class_path`, which is how it is reviewed; lookup does not depend on the order.
static PORT_NAMES: &[ClassPortNames] = &[
    ClassPortNames {
        class_path: "CDL.Conversions.BooleanToInteger",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Conversions.BooleanToReal",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Conversions.IntegerToReal",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Conversions.RealToInteger",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.FirstOrderHold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.Sampler",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.TriggeredMax",
        inputs: &["u", "trigger"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.TriggeredMovingMean",
        inputs: &["u", "trigger"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.TriggeredSampler",
        inputs: &["u", "trigger"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.UnitDelay",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Discrete.ZeroOrderHold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Abs",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Add",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.AddParameter",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Change",
        inputs: &["u"],
        outputs: &["y", "up", "down"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Equal",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Greater",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.GreaterEqual",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.GreaterEqualThreshold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.GreaterThreshold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Less",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.LessEqual",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.LessEqualThreshold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.LessThreshold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Max",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Min",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Multiply",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.OnCounter",
        inputs: &["trigger", "reset"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Sources.Constant",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Sources.Pulse",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Stage",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Subtract",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Integers.Switch",
        inputs: &["u1", "u2", "u3"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.And",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Change",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Edge",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.FallingEdge",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Latch",
        inputs: &["u", "clr"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Nand",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Nor",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Not",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Or",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Pre",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Proof",
        inputs: &["u_s", "u_m"],
        outputs: &["yLocFal", "yLocTru"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Sources.Constant",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Sources.Pulse",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Sources.SampleTrigger",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Switch",
        inputs: &["u1", "u2", "u3"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Timer",
        inputs: &["u"],
        outputs: &["y", "passed"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.TimerAccumulating",
        inputs: &["u", "reset"],
        outputs: &["y", "passed"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Toggle",
        inputs: &["u", "clr"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.TrueDelay",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.TrueFalseHold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.VariablePulse",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Logical.Xor",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Psychrometrics.DewPoint_TDryBulPhi",
        inputs: &["TDryBul", "phi"],
        outputs: &["TDewPoi"],
    },
    ClassPortNames {
        class_path: "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        inputs: &["TDryBul", "phi"],
        outputs: &["h"],
    },
    ClassPortNames {
        class_path: "CDL.Psychrometrics.WetBulb_TDryBulPhi",
        inputs: &["TDryBul", "phi"],
        outputs: &["TWetBul"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Abs",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Acos",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Add",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.AddParameter",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Asin",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Atan",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Atan2",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Average",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Cos",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Derivative",
        inputs: &["k", "T", "u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Divide",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Exp",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Greater",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.GreaterThreshold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Hysteresis",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.IntegratorWithReset",
        inputs: &["u", "y_reset_in", "trigger"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Less",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.LessThreshold",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.LimitSlewRate",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Limiter",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Line",
        inputs: &["x1", "f1", "x2", "f2", "u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Log",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Log10",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Max",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Min",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Modulo",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.MovingAverage",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Multiply",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.MultiplyByParameter",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.PID",
        inputs: &["u_s", "u_m"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.PIDWithReset",
        inputs: &["u_s", "u_m", "trigger"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Ramp",
        inputs: &["u", "active"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Round",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sin",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sources.CalendarTime",
        inputs: &[],
        outputs: &["year", "month", "day", "hour", "minute", "weekDay"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sources.CivilTime",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sources.Constant",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sources.Pulse",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sources.Ramp",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sources.Sin",
        inputs: &[],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Sqrt",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Subtract",
        inputs: &["u1", "u2"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Switch",
        inputs: &["u1", "u2", "u3"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Reals.Tan",
        inputs: &["u"],
        outputs: &["y"],
    },
    ClassPortNames {
        class_path: "CDL.Utilities.Assert",
        inputs: &["u"],
        outputs: &[],
    },
    ClassPortNames {
        class_path: "CDL.Utilities.SunRiseSet",
        inputs: &[],
        outputs: &["nextSunRise", "nextSunSet", "sunUp"],
    },
];

/// Declared port names for `class_path`, or `None` when the class binds positionally.
///
/// `None` is a routine answer, not an error — see the module header for the 29 classes that return
/// it and why. Linear scan, matching [`crate::lookup`]; it runs once per block instance at BUILD,
/// never on the tick.
pub fn port_names(class_path: &str) -> Option<&'static ClassPortNames> {
    PORT_NAMES.iter().find(|e| e.class_path == class_path)
}

/// Every class carrying declared port names. Ordered as authored.
pub fn all_port_names() -> &'static [ClassPortNames] {
    PORT_NAMES
}
