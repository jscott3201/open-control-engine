#!/usr/bin/env python3
"""Derive the bit goldens of the closed unit table `units/v1`.

    python3 -I derive_units.py units_v1.txt

This is the independent side of tests/unit_table.rs. It shares no code with the crate. For every
pair of the table (docs/design/commissioning.md, "Point mapping") it:

1. derives the pair's constants from base definitions in exact rational arithmetic (fractions),
   citing the source of each, and checks the decimal the crate writes against that derivation;
2. evaluates the pair at every sample input in the note's operation order with Python floats
   (IEEE 754 binary64, one rounding per operation, no fused multiply-add);
3. evaluates the same conversion a second way, as an exact rational from the base definitions
   (for a temperature, NIST SP 811's own form `T/K = (t/degF + 459.67) / 1.8`, not the note's),
   rounded once to binary64, and records how many units in the last place the two lie apart.

Column 4 of the output is the contract: the crate must return exactly those bits. Column 5 is
the correctly rounded value and column 6 the distance to it; they show what the note's order
costs and hold no test to more than what is written there.

The stated behaviour outside the arithmetic is restated here, not derived: a NaN or an infinity
is returned with its bits unchanged, and a finite input whose result is not finite is refused
(`overflow`). No test runs this script; run it by hand when the table changes, review the diff,
and say in the pull request which rows moved and why.
"""

import math
import struct
import sys
from fractions import Fraction

# ---------------------------------------------------------------------------------------------
# Base definitions, exact. Each is a definition, not a measurement.
# ---------------------------------------------------------------------------------------------

# NIST SP 811 (2008), Appendix B.8: the international foot is 0.3048 m and the inch 0.0254 m,
# exactly (the 1959 international yard and pound agreement; SP 811 prints both in bold, its mark
# for an exact factor).
FOOT_M = Fraction(3048, 10000)
INCH_M = Fraction(254, 10000)
# NIST SP 330 (2019), section 2.3.1 and appendix: the standard acceleration of free fall,
# g_n = 9.80665 m/s^2 exactly (3rd CGPM, 1901).
STANDARD_GRAVITY = Fraction(980665, 100000)
# NIST SP 811 (2008), Appendix B.8, footnote to "inch of water, conventional": the conventional
# units of water column take water at 1000 kg/m^3 exactly under standard gravity.
CONVENTIONAL_WATER_DENSITY = Fraction(1000)
# NIST SP 330 (2019), section 2.3.1: the Celsius temperature is t = T - T0, T0 = 273.15 K exactly.
KELVIN_AT_ZERO_CELSIUS = Fraction(27315, 100)
# NIST SP 811 (2008), Appendix B.9: T/K = (t/degF + 459.67) / 1.8 and t/degC = (t/degF - 32) / 1.8.
RANKINE_AT_ZERO_FAHRENHEIT = Fraction(45967, 100)
FAHRENHEIT_DEGREES_PER_KELVIN = Fraction(18, 10)
FAHRENHEIT_AT_ZERO_CELSIUS = Fraction(32)
# NIST SP 330 (2019), table 7: the SI prefix kilo is 10^3. NIST SP 811 (2008), Appendix B.8:
# the litre is 10^-3 m^3 exactly (12th CGPM, 1964).
KILO = Fraction(1000)
LITRE_M3 = Fraction(1, 1000)
# NIST SP 811 (2008), section 7.10.2: the symbol % stands for the number 0.01.
PERCENT = Fraction(1, 100)
SECONDS_PER_MINUTE = Fraction(60)

# NIST SP 811 (2008), Appendix B.8, "inch of water (60 degF)": 2.4884 E+02 Pa. A tabulated value
# to five significant digits, not a definition: water's density at 60 degF is a measured
# property, so no base definition derives it. Checked below only for plausibility.
INCH_OF_WATER_60F_PA_TABULATED = Fraction(24884, 100)
# Water at 60 degF (15.56 degC) is about 999.0 kg/m^3 (ASHRAE Handbook - Fundamentals, chapter 33,
# table of water properties); used only to show the tabulated value is the 60 degF inch.
WATER_DENSITY_60F_APPROX = Fraction(9990, 10)

# ---------------------------------------------------------------------------------------------
# The constants, derived, against the decimals the crate writes.
# ---------------------------------------------------------------------------------------------

PA_PER_KPA = KILO
PA_PER_INCH_OF_WATER = INCH_M * CONVENTIONAL_WATER_DENSITY * STANDARD_GRAVITY
M3S_PER_CFM = FOOT_M**3 / SECONDS_PER_MINUTE
M3S_PER_LITRE_PER_SECOND = LITRE_M3

# The decimal literal each constant is written as in the crate (src/commissioning/units.rs).
CRATE_LITERALS = {
    "PA_PER_KPA": ("1000.0", PA_PER_KPA),
    "PA_PER_INCH_OF_WATER": ("249.08891", PA_PER_INCH_OF_WATER),
    "PA_PER_INCH_OF_WATER_60F": ("248.84", INCH_OF_WATER_60F_PA_TABULATED),
    "M3S_PER_CFM": ("0.0004719474432", M3S_PER_CFM),
    "M3S_PER_LITRE_PER_SECOND": ("0.001", M3S_PER_LITRE_PER_SECOND),
    "KELVIN_AT_ZERO_CELSIUS": ("273.15", KELVIN_AT_ZERO_CELSIUS),
    "FAHRENHEIT_AT_ZERO_CELSIUS": ("32.0", FAHRENHEIT_AT_ZERO_CELSIUS),
    "PERCENT_DIVISOR": ("100.0", 1 / PERCENT),
}

# The design note's table writes the cfm factor as 0.00047194744832 beside "exact: 0.3048^3 / 60".
# The two disagree: the exact quotient is 0.0004719474432. The crate follows the stated
# derivation; this records the difference so a reader sees it.
NOTE_CFM_DIGITS = Fraction("0.00047194744832")


def check_constants():
    for name, (literal, exact) in CRATE_LITERALS.items():
        if Fraction(literal) != exact:
            sys.exit(f"{name}: the crate's {literal} is not the derivation {exact}")
    assert M3S_PER_CFM != NOTE_CFM_DIGITS
    # The 60 degF inch: water about 1 part in 1000 lighter than the conventional 1000 kg/m^3.
    derived = INCH_M * WATER_DENSITY_60F_APPROX * STANDARD_GRAVITY
    assert abs(derived - INCH_OF_WATER_60F_PA_TABULATED) < Fraction(1, 100), float(derived)


# ---------------------------------------------------------------------------------------------
# The pairs: the note's expression in its stated order, and an exact second derivation.
# ---------------------------------------------------------------------------------------------

FIVE_NINTHS = 5.0 / 9.0  # one correctly rounded division, as Rust folds `5.0 / 9.0`


def stated(source, target, x):
    """The note's expression for the pair, in its order, in binary64."""
    if (source, target) == ("degF", "K"):
        return (x - 32.0) * FIVE_NINTHS + 273.15
    if (source, target) == ("degC", "K"):
        return x + 273.15
    if (source, target) == ("degF", "degC"):
        return (x - 32.0) * FIVE_NINTHS
    if (source, target) == ("K", "degC"):
        return x - 273.15
    if (source, target) == ("kPa", "Pa"):
        return x * 1000.0
    if (source, target) == ("inH2O", "Pa"):
        return x * 249.08891
    if (source, target) == ("inH2O_60F", "Pa"):
        return x * 248.84
    if (source, target) == ("cfm", "m3/s"):
        return x * 0.0004719474432
    if (source, target) == ("L/s", "m3/s"):
        return x * 0.001
    if (source, target) in (("percent", "1"), ("%", "1")):
        return x / 100.0
    raise KeyError((source, target))


def exact(source, target, x):
    """The same conversion from the base definitions, an exact rational of the binary input."""
    q = Fraction(x)
    if (source, target) == ("degF", "K"):
        return (q + RANKINE_AT_ZERO_FAHRENHEIT) / FAHRENHEIT_DEGREES_PER_KELVIN
    if (source, target) == ("degC", "K"):
        return q + KELVIN_AT_ZERO_CELSIUS
    if (source, target) == ("degF", "degC"):
        return (q - FAHRENHEIT_AT_ZERO_CELSIUS) / FAHRENHEIT_DEGREES_PER_KELVIN
    if (source, target) == ("K", "degC"):
        return q - KELVIN_AT_ZERO_CELSIUS
    if (source, target) == ("kPa", "Pa"):
        return q * PA_PER_KPA
    if (source, target) == ("inH2O", "Pa"):
        return q * PA_PER_INCH_OF_WATER
    if (source, target) == ("inH2O_60F", "Pa"):
        return q * INCH_OF_WATER_60F_PA_TABULATED
    if (source, target) == ("cfm", "m3/s"):
        return q * M3S_PER_CFM
    if (source, target) == ("L/s", "m3/s"):
        return q * M3S_PER_LITRE_PER_SECOND
    if (source, target) in (("percent", "1"), ("%", "1")):
        return q * PERCENT
    raise KeyError((source, target))


PAIRS = [
    ("degF", "K"),
    ("degC", "K"),
    ("degF", "degC"),
    ("K", "degC"),
    ("kPa", "Pa"),
    ("inH2O", "Pa"),
    ("inH2O_60F", "Pa"),
    ("cfm", "m3/s"),
    ("L/s", "m3/s"),
    ("percent", "1"),
    ("%", "1"),
]

# ---------------------------------------------------------------------------------------------
# Sample inputs.
# ---------------------------------------------------------------------------------------------


def from_bits(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def to_bits(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


MAX = sys.float_info.max
SMALLEST_SUBNORMAL = from_bits(1)
MIN_POSITIVE = sys.float_info.min

# Every pair at these: the zeros, the extremes, the subnormals, the infinities and five NaNs
# (the quiet NaN Rust writes, a quiet payload, a signalling one, and both with the sign set).
COMMON = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    MAX,
    -MAX,
    MIN_POSITIVE,
    -MIN_POSITIVE,
    SMALLEST_SUBNORMAL,
    -SMALLEST_SUBNORMAL,
    math.inf,
    -math.inf,
]
NAN_BITS = [
    0x7FF8000000000000,
    0x7FF8000000000001,
    0x7FF0000000000001,
    0xFFF8000000000000,
    0xFFF4000000000000,
]
# A spread over the magnitudes a trend might hold, and well past them.
SPREAD = [
    -1.0e300,
    -123456.789,
    -273.15,
    -40.0,
    -1.0e-300,
    1.0e-300,
    0.1,
    3.0,
    72.5,
    1013.25,
    101.325,
    1.0e15,
    9007199254740993.0,  # 2^53 + 1 as written: binary64 holds 2^53
    1.0e300,
]
# Known points the note and the issue name, and the temperatures every reader checks.
KNOWN = {
    ("degF", "K"): [32.0, 212.0, -40.0, -459.67, 98.6, 55.0, 75.0],
    ("degC", "K"): [-40.0, 100.0, 25.0, 12.8],
    ("degF", "degC"): [32.0, 212.0, -40.0, -459.67, 98.6],
    ("K", "degC"): [273.15, 373.15, 0.0, 233.14999999999998, 293.15],
    ("kPa", "Pa"): [101.325, 1.5],
    ("inH2O", "Pa"): [2.0, 0.25, 1.5],
    ("inH2O_60F", "Pa"): [2.0, 0.25, 1.5],
    ("cfm", "m3/s"): [2118.88, 400.0, 10000.0],
    ("L/s", "m3/s"): [1000.0, 472.0],
    ("percent", "1"): [100.0, 50.0, 12.5, 33.3],
    ("%", "1"): [100.0, 50.0, 12.5, 33.3],
}


def overflow_edge(source, target):
    """The largest finite input whose result is finite, and the next one up; for factors > 1."""
    if stated(source, target, MAX) != math.inf:
        return []
    low, high = 1.0, MAX
    while math.nextafter(low, math.inf) < high:
        mid = from_bits((to_bits(low) + to_bits(high)) // 2)
        if math.isfinite(stated(source, target, mid)):
            low = mid
        else:
            high = mid
    return [low, high, -low, -high]


def fma_witnesses():
    """Inputs where fusing degF -> K's multiply and add would change the bits (Python 3.13+)."""
    found = []
    candidate = 33.0
    while len(found) < 3:
        unfused = (candidate - 32.0) * FIVE_NINTHS + 273.15
        fused = math.fma(candidate - 32.0, FIVE_NINTHS, 273.15)
        if to_bits(unfused) != to_bits(fused):
            found.append(candidate)
        candidate = math.nextafter(candidate, math.inf) + 0.37
    return found


# ---------------------------------------------------------------------------------------------
# The golden.
# ---------------------------------------------------------------------------------------------


def ulps_apart(a, b):
    def ordered(value):
        bits = to_bits(value)
        return -(bits & 0x7FFFFFFFFFFFFFFF) if bits >> 63 else bits

    return abs(ordered(a) - ordered(b))


def hex_bits(value):
    return f"0x{to_bits(value):016x}"


def rows_for(source, target):
    inputs = list(COMMON) + SPREAD + KNOWN[(source, target)] + overflow_edge(source, target)
    if (source, target) == ("degF", "K"):
        inputs += fma_witnesses()
    seen = set()
    for x in inputs:
        if to_bits(x) in seen:
            continue
        seen.add(to_bits(x))
        if math.isinf(x):
            yield (x, hex_bits(x), "-", "-")
            continue
        result = stated(source, target, x)
        if not math.isfinite(result):
            yield (x, "overflow", "-", "-")
            continue
        rounded = float(exact(source, target, x))
        yield (x, hex_bits(result), hex_bits(rounded), str(ulps_apart(result, rounded)))
    for bits in NAN_BITS:
        yield (from_bits(bits), f"0x{bits:016x}", "-", "-")


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    check_constants()
    lines = [
        "# units/v1 bit goldens, written by derive_units.py; never blessed, never edited by hand.",
        "# from<TAB>to<TAB>input bits<TAB>output bits or `overflow`<TAB>exact rounded once<TAB>ulps",
    ]
    for source, target in PAIRS:
        for x, output, rounded, ulps in rows_for(source, target):
            bits = to_bits(x)
            lines.append(f"{source}\t{target}\t0x{bits:016x}\t{output}\t{rounded}\t{ulps}")
    with open(sys.argv[1], "w", encoding="ascii", newline="\n") as out:
        out.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
