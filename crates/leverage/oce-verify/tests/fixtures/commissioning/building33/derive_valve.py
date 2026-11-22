#!/usr/bin/env python3
"""Derive CoolingCoilValve's valve signal, frame by frame, for inputs that take every branch.

    python3 -I derive_valve.py > valve_oracle.txt

Run by hand from this directory (the README beside it has the command); no test runs it. It
shares no code with the crate, the engine or the authored `cooling_coil_valve.oce`. The sequence
is written out here from the vendored Modelica sources, read directly:

- `CoolingCoilValve.mo` (Buildings.Utilities.Plotters.Examples.BaseClasses) for the wiring and
  the parameter bindings, evaluated in f64 in the order the source writes them;
- the CDL blocks it instantiates, under `third_party/modelica-buildings-cdl/Buildings/Controls/
  OBC/CDL/`: `Reals/PIDWithReset.mo` (PI: `e = revAct/r*u_s - revAct/r*u_m`, `yP = k*e`,
  `yUnl = yP + I`, `y = min(yMax, max(yMin, yUnl))`, `der(I) = k/Ti*(e - (yUnl - y)/(k*Ni))`,
  and on a rising `trigger` `I := y_reset - yP`), `Reals/Line.mo` (`b = (f2-f1)/(x2-x1)`,
  `a = f2 - b*x2`, `y = a + b*min(x2, max(x1, u))`), `Reals/Hysteresis.mo`
  (`y = not pre(y) and u > uHigh or pre(y) and u >= uLow`, `pre(y)` false at the start),
  `Logical/Change.mo` (`y = change(u)`, `pre(u)` false at the start), `Logical/And.mo`,
  `Reals/Min.mo`, `Reals/Multiply.mo`, `Conversions/BooleanToReal.mo` and
  `Reals/Sources/Constant.mo`.

Time is discrete, as the engine's is: each row of the input table is one frame. The integrator
is the engine's declared discretisation, its documented tick convention for a PI controller
(forward Euler; a frame emits from the state before it, then the state advances by the gain
times the frame's integrand times the time since the previous frame, zero at the first frame;
a reset on a rising trigger is applied as the state advances, so it shows at the next frame).
That convention is the engine's choice, and this derivation checks the transcription (the
wiring, the parameters, every branch), not the choice: the OpenModelica comparison in the
README is where the continuous model is compared.

The inputs are synthetic and in CDL units (K, the fan feedback as a fraction, as the port
takes it), chosen so that the valve signal goes through every branch the README lists: off
before the outdoor air first passes the cut-off, the PI controller active, the hysteresis
holding below the cut-off, the supply-air high limit cutting the signal (and the line clamped
at its lower end), the fan feedback hysteresis holding and then disabling, the fan status
disabling, the outdoor air below upstream's `uLow`, and each re-enable resetting the
integrator. Irregular frame steps exercise the time step.

Each output row is the frame's time, its five inputs, the valve signal as 16 hex digits of
bits and as `repr`, and the branch label. The test compares the engine with it bit for bit.
The arithmetic here is written as the CDL sources write it (`1/(k*Ni)` times the anti-windup
error, `a + b*xLim` for the line); on these inputs the engine gives the same bits, which a
different grouping of the same arithmetic could break by an ulp without being wrong. If that
happens, the README says what to do: compare within a stated bound and record the largest
difference, never re-derive from the engine.
"""

import struct

# CoolingCoilValve.mo's parameters, its bindings in its order.
ALC_PROP_K = 1 / 100
ALC_INT_K = 0.5 / 100
ALC_K_UNIT_CONV = 9 / 5
T_OUT_COO_CUT = 50 * (5 / 9) - 32 * (5 / 9) + 273.15
T_OUT_DELTA = 2 * (5 / 9) - 32 * (5 / 9) + 273.15
FAN_FEE_CUT = 15 / 100
FAN_FEE_DELTA = 10 / 100
T_SUP_HIGH_LIM = 50 * (5 / 9) - 32 * (5 / 9) + 273.15
T_SUP_HIG_LIM = 42 * (5 / 9) - 32 * (5 / 9) + 273.15
INTERVAL = 15.0
REVERSE_ACTING = False
K = ALC_PROP_K * ALC_K_UNIT_CONV
TI = K * INTERVAL / (ALC_INT_K * ALC_K_UNIT_CONV)
U_MAX = 1.0
U_MIN = 0.0
# PIDWithReset's defaults that the instance does not set, and the ones it does.
R = 1.0
NI = 0.9
Y_MAX = 1.0
Y_MIN = 0.0
Y_RESET = 0.0
REV_ACT = 1.0 if REVERSE_ACTING else -1.0


def bits(value):
    return "{:016x}".format(struct.unpack("<Q", struct.pack("<d", value))[0])


def scenario():
    """(t, TSup, TSupSet, TOut, uFanFee, uFanSta, branch) rows."""
    rows = []
    t = 0.0

    def add(n, step, t_sup, t_sup_set, t_out, fan_fee, fan_sta, branch):
        nonlocal t
        for _ in range(n):
            rows.append((t, t_sup, t_sup_set, t_out, fan_fee, fan_sta, branch))
            t += step

    add(5, 5.0, 288.0, 285.0, 280.0, 0.6, True, "off: the outdoor air has not passed the cut-off")
    add(12, 5.0, 288.0, 285.0, 290.0, 0.6, True, "pi: enabled, unlimited")
    add(3, 2.0, 287.0, 285.0, 290.0, 0.6, True, "pi: enabled, a 2 s step")
    add(2, 13.0, 284.0, 285.0, 290.0, 0.6, True, "pi: enabled, a 13 s step, error negative")
    add(5, 5.0, 288.0, 285.0, 270.0, 0.6, True, "pi: the hysteresis holds below the cut-off")
    add(6, 5.0, 281.0, 230.0, 290.0, 0.6, True, "limited: the supply-air high limit cuts the PI")
    add(3, 5.0, 277.0, 230.0, 290.0, 0.6, True, "limited: the line clamped at its lower end")
    add(4, 5.0, 288.0, 285.0, 290.0, 0.1, True, "pi: the fan feedback hysteresis holds")
    add(3, 5.0, 288.0, 285.0, 290.0, 0.03, True, "off: the fan feedback below its uLow")
    add(3, 5.0, 288.0, 285.0, 290.0, 0.12, True, "off: the fan feedback below its uHigh")
    add(4, 5.0, 288.0, 285.0, 290.0, 0.2, True, "pi: re-enabled, the integrator reset")
    add(3, 5.0, 288.0, 285.0, 290.0, 0.2, False, "off: the fan status false")
    add(4, 5.0, 288.0, 285.0, 290.0, 0.2, True, "pi: re-enabled by the fan status")
    add(3, 5.0, 288.0, 285.0, 20.0, 0.2, True, "off: the outdoor air below upstream's uLow")
    add(4, 5.0, 288.0, 285.0, 290.0, 0.2, True, "pi: re-enabled by the outdoor air")
    return rows


def main():
    t_out_low = T_OUT_COO_CUT - T_OUT_DELTA
    fan_low = FAN_FEE_CUT - FAN_FEE_DELTA
    # Line higLim: (x1, f1) = (TSupMin.k, yCooValMin.k), (x2, f2) = (TSupMax.k, yCooValMax.k).
    x1, f1, x2, f2 = T_SUP_HIG_LIM, U_MIN, T_SUP_HIGH_LIM, U_MAX
    slope = (f2 - f1) / (x2 - x1)
    intercept = f2 - slope * x2
    t_out_thr = fan_thr = False  # Hysteresis pre(y) at the start
    enable_before = False  # Change pre(u) at the start
    trigger_before = False  # the integrator's `when trigger` edge
    integral = 0.0  # IntegratorWithReset y_start = xi_start = 0
    t_before = None
    out = ["# derived by derive_valve.py from the vendored Modelica sources; never edited by hand",
           "# row: t, TSup, TSupSet, TOut, uFanFee, uFanSta, yCooVal bits, yCooVal, branch"]
    for t, t_sup, t_sup_set, t_out, fan_fee, fan_sta, branch in scenario():
        # Enable: the two hysteresis blocks, the two And blocks.
        t_out_thr = (not t_out_thr and t_out > T_OUT_COO_CUT) or (t_out_thr and t_out >= t_out_low)
        fan_thr = (not fan_thr and fan_fee > FAN_FEE_CUT) or (fan_thr and fan_fee >= fan_low)
        enable = (t_out_thr and fan_thr) and fan_sta
        change = enable != enable_before
        enable_before = enable
        # PI, emitting from the integrator state before this frame.
        e = REV_ACT / R * t_sup_set - REV_ACT / R * t_sup
        y_p = K * e
        y_unl = y_p + integral
        y_pi = min(Y_MAX, max(Y_MIN, y_unl))
        # Downstream: BooleanToReal, Multiply, Line, Min.
        product = y_pi * (1.0 if enable else 0.0)
        limit = intercept + slope * min(x2, max(x1, t_sup))
        valve = min(product, limit)
        out.append("row\t{!r}\t{!r}\t{!r}\t{!r}\t{!r}\t{}\t{}\t{!r}\t{}".format(
            t, t_sup, t_sup_set, t_out, fan_fee, "true" if fan_sta else "false",
            bits(valve), valve, branch))
        # Advance the integrator: forward Euler over the time since the previous frame, or the
        # reset on a rising trigger.
        dt = 0.0 if t_before is None else t - t_before
        if change and not trigger_before:
            integral = Y_RESET - y_p
        else:
            integrand = e - (1 / (K * NI)) * (y_unl - y_pi)
            integral = integral + K / TI * integrand * dt
        trigger_before = change
        t_before = t
    print("\n".join(out))


if __name__ == "__main__":
    main()
