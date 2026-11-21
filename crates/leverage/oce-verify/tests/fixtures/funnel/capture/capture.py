#!/usr/bin/env python3
"""Capture the reference outputs of the funnel differential fixture from the pinned pyfunnel.

    python -I capture/capture.py <fixture root> <funnel checkout> <wheel sha256>

Run through `capture.sh`, which builds the environment this file assumes: x86_64 Linux and
`pyfunnel==2.1.1` installed from its hash-pinned wheel. It writes, under the fixture root:

- `expected/<case>/`: for every case of `upstream/` and `hostile/`, the five CSV files pyfunnel
  writes, byte for byte as its C library wrote them; or, for a case funnel refuses (it returns 1
  and writes no curve), the one file `refused.txt` holding the line its log gave;
- `printf/glibc.tsv`: C's `snprintf("%.16g")` of a fixed list of doubles, from the same libc the
  library prints with;
- `capture/provenance.json`: what produced them.

`<funnel checkout>` is `lbl-srg/funnel` at the pinned commit. Nothing is run from it: it is read
to prove the vendored inputs are upstream's bytes and to record, case by case, whether the
captured outputs equal the `results/` upstream commits.

Inputs are read the way pyfunnel's own command line reads them (`csv.reader`, `float`), with one
difference: exactly the first row is the header, and any other row that is not two numbers is an
error here, where the command line skips it silently.

No path, host name, user name or time is written anywhere.
"""

import contextlib
import csv
import ctypes
import hashlib
import io
import json
import math
import os
import platform
import shutil
import struct
import sys
import tempfile

FUNNEL_COMMIT = "a12fa96917ca73b78fcb6a17a1140b3a27a61e31"
PYFUNNEL_VERSION = "2.1.1"
WHEEL = "pyfunnel-2.1.1-py3-none-any.whl"
LIBRARY = "pyfunnel/lib/linux64/libfunnel.so"
OUTPUTS = ["reference.csv", "test.csv", "lowerBound.csv", "upperBound.csv", "errors.csv"]
TOLERANCES = ["atolx", "atoly", "ltolx", "ltoly", "rtolx", "rtoly"]
REFUSED = "refused.txt"
# The two refusals funnel's comparison makes before it builds anything (compare.c).
REFUSALS = [
    "Error: Reference and test data minimum x values are different.",
    "Error: Reference and test data maximum x values are different.",
]
UPSTREAM_CASES = (
    ["fail{}".format(k) for k in range(1, 7)]
    + ["should_succeed1"]
    + ["success{}".format(k) for k in range(1, 11)]
)


def sha256(path):
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def read_series(path):
    xs, ys = [], []
    with open(path, newline="") as handle:
        rows = list(csv.reader(handle))
    for number, row in enumerate(rows[1:], start=2):
        if len(row) != 2:
            raise SystemExit("{}: row {} has {} fields".format(path, number, len(row)))
        xs.append(float(row[0]))
        ys.append(float(row[1]))
    return xs, ys


def same_bytes(left, right):
    with open(left, "rb") as a, open(right, "rb") as b:
        return a.read() == b.read()


def capture_case(pyfunnel, directory, expected):
    """Run one case and write its `expected/` directory; the refusal line, or None."""
    with open(os.path.join(directory, "param.json")) as handle:
        param = json.load(handle)
    reference = read_series(os.path.join(directory, param["reference"]))
    test = read_series(os.path.join(directory, param["test"]))
    shutil.rmtree(expected, ignore_errors=True)
    os.makedirs(expected)
    with tempfile.TemporaryDirectory() as scratch:
        output = os.path.join(scratch, "results")
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            code = pyfunnel.compareAndReport(
                reference[0], reference[1], test[0], test[1], outputDirectory=output,
                **{name: param.get(name) for name in TOLERANCES}
            )
        written = sorted(os.listdir(output))
        if code == 1 and not written:
            # pyfunnel prints its status line, then the library's log, and deletes the log.
            lines = [line for line in printed.getvalue().splitlines() if line]
            if len(lines) != 2 or lines[1] not in REFUSALS:
                raise SystemExit("{}: pyfunnel refused with {!r}".format(directory, lines))
            with open(os.path.join(expected, REFUSED), "w", newline="\n") as handle:
                handle.write(lines[1] + "\n")
            return lines[1]
        if code != 0 or written != sorted(OUTPUTS):
            raise SystemExit("{}: pyfunnel returned {} and wrote {}".format(
                directory, code, written))
        for name in OUTPUTS:
            shutil.copyfile(os.path.join(output, name), os.path.join(expected, name))
    return None


def upstream_record(checkout, root, case):
    """Prove the vendored inputs are upstream's and compare with its committed `results/`."""
    theirs = os.path.join(checkout, "tests", case)
    ours = os.path.join(root, "upstream", case)
    for name in sorted(os.listdir(ours)):
        if not same_bytes(os.path.join(ours, name), os.path.join(theirs, name)):
            raise SystemExit("upstream/{}/{} is not upstream's file".format(case, name))
    results = os.path.join(theirs, "results")
    if not os.path.isdir(results):
        return "none committed"
    if os.path.exists(os.path.join(root, "expected", case, REFUSED)):
        return "committed, but pyfunnel refuses the case"
    differing = [
        name for name in OUTPUTS
        if not same_bytes(os.path.join(results, name), os.path.join(root, "expected", case, name))
    ]
    return "equal" if not differing else "differs: " + ", ".join(differing)


def printf_values():
    """The doubles of the `%.16g` golden, as bit patterns: landmarks, then a fixed random walk."""
    landmarks = [
        0.0, -0.0, 1.0, -1.0, 0.1, 0.5, 1.0 / 3.0, 2.0 / 3.0, 0.1 + 0.2, 100.0, 123456.789,
        # The switch to exponent notation below 1e-4, with the doubles around it.
        1e-4, math.nextafter(1e-4, 0.0), math.nextafter(1e-4, 1.0), 9.5e-5, 1e-5,
        math.nextafter(1e-5, 0.0), math.nextafter(1e-5, 1.0), 1.5e-5, 0.00012345678901234567,
        # The switch to exponent notation at 1e16, with the doubles around it.
        1e15, 9999999999999998.0, 1e16, 10000000000000002.0, 1e17, 1.2345678901234568e17,
        9007199254740992.0, 9007199254740994.0, 1152921504606846976.0, 999999999999999.9,
        # Seventeen-digit values the sixteenth digit rounds: exact ties (to even) and near ties.
        1234567890123456.5, 1234567890123457.5, 2251799813685247.5, 4503599627370495.5,
        5.9604644775390625e-08, 1234567890123456.2, 1234567890123456.8,
        0.30000000000000004, 0.7999999999999999, 2.675, 1e23, 1e22, 1e21, 8.41e21,
        math.nextafter(1.0, 0.0), math.nextafter(1.0, 2.0), math.nextafter(10.0, 0.0),
        # Subnormals, the smallest normal and the largest double.
        5e-324, 1e-310, 2.2250738585072009e-308, 2.2250738585072014e-308,
        1.7976931348623157e308, -1.7976931348623157e308,
        # Exponent digits: two at least, three when needed.
        1e-7, 1e9, 1e100, 1.5e300, 1e-100, -2.5e-7,
        # Values of the fixtures' own shape.
        3700000030.72, 1.7e12, 54000.0, 0.008935670100205906,
        math.inf, -math.inf,
    ]
    bits = [struct.unpack("<Q", struct.pack("<d", value))[0] for value in landmarks]
    bits += [0x7FF8000000000000, 0xFFF8000000000000]  # nan, -nan
    # Powers of ten whose nearest double is not the power: the rounding decides the exponent.
    for power in range(-320, 309):
        bits.append(struct.unpack("<Q", struct.pack("<d", float("1e{}".format(power))))[0])
    state = 655
    for _ in range(4096):
        state = (state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        bits.append(state)
    return bits


def capture_printf(root):
    libc = ctypes.CDLL(None)
    libc.snprintf.restype = ctypes.c_int
    buffer = ctypes.create_string_buffer(64)
    lines = []
    for bits in printf_values():
        value = struct.unpack("<d", struct.pack("<Q", bits))[0]
        written = libc.snprintf(buffer, ctypes.c_size_t(64), b"%.16g", ctypes.c_double(value))
        assert 0 < written < 64, bits
        lines.append("{:016x}\t{}\n".format(bits, buffer.value.decode("ascii")))
    os.makedirs(os.path.join(root, "printf"), exist_ok=True)
    with open(os.path.join(root, "printf", "glibc.tsv"), "w", newline="\n") as handle:
        handle.writelines(lines)
    return len(lines)


def main(root, checkout, wheel_sha256):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise SystemExit("capture runs on x86_64 Linux, not {} {}".format(
            platform.system(), platform.machine()))
    import pyfunnel
    from pyfunnel import core

    if pyfunnel.__version__ != PYFUNNEL_VERSION:
        raise SystemExit("pyfunnel {} is installed, not {}".format(
            pyfunnel.__version__, PYFUNNEL_VERSION))
    library = core._get_lib_path("funnel")
    package = os.path.dirname(os.path.dirname(os.path.abspath(pyfunnel.__file__)))
    if os.path.relpath(library, package) != LIBRARY:
        raise SystemExit("pyfunnel loads an unexpected library")

    hostile = sorted(os.listdir(os.path.join(root, "hostile")))
    if sorted(os.listdir(os.path.join(root, "upstream"))) != sorted(
            UPSTREAM_CASES + ["COPYRIGHT.txt", "LICENSE.txt"]):
        raise SystemExit("upstream/ is not funnel's 17 cases and its two licence files")
    shutil.rmtree(os.path.join(root, "expected"), ignore_errors=True)
    cases = {}
    for case in UPSTREAM_CASES:
        refusal = capture_case(pyfunnel, os.path.join(root, "upstream", case),
                               os.path.join(root, "expected", case))
        cases[case] = {"set": "upstream", "return_code": 0 if refusal is None else 1,
                       "upstream_results": upstream_record(checkout, root, case)}
    for case in hostile:
        if case in cases:
            raise SystemExit("hostile/{} repeats an upstream name".format(case))
        refusal = capture_case(pyfunnel, os.path.join(root, "hostile", case),
                               os.path.join(root, "expected", case))
        cases[case] = {"set": "hostile", "return_code": 0 if refusal is None else 1}
    for name in ["COPYRIGHT.txt", "LICENSE.txt"]:
        if not same_bytes(os.path.join(root, "upstream", name), os.path.join(checkout, name)):
            raise SystemExit("upstream/{} is not upstream's file".format(name))

    provenance = {
        "funnel_commit": FUNNEL_COMMIT,
        "pyfunnel_version": pyfunnel.__version__,
        "wheel": WHEEL,
        "wheel_sha256": wheel_sha256,
        "library": LIBRARY,
        "library_sha256": sha256(library),
        "platform": {
            "system": platform.system(),
            "machine": platform.machine(),
            "libc": " ".join(platform.libc_ver()),
            "python": platform.python_version(),
        },
        "printf_values": capture_printf(root),
        "cases": cases,
    }
    with open(os.path.join(root, "capture", "provenance.json"), "w", newline="\n") as handle:
        json.dump(provenance, handle, indent=2, sort_keys=True)
        handle.write("\n")
    print("captured {} cases and {} printf values".format(len(cases), provenance["printf_values"]))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
