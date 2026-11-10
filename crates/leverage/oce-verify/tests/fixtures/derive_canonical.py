#!/usr/bin/env python3
"""Derive a canonical golden and its spec id for an oce/vectors/v1 document.

    python3 -I derive_canonical.py <vectors.json> <canonical-out.json>

This is the independent side of tests/canonical_form.rs: a second implementation of the
canonical form (docs/design/vectors.md, "Canonical form") and of FNV-1a-128 that shares no code
with the crate. It writes the canonical bytes to the output path and prints the spec id. No test
runs it and nothing blesses its output: run it by hand when the format changes in place, review
the diff, and update the id literals in the test.

It trusts its input: it does not check the format's rules, a repeated key or a bound. The Rust
reader does; give it only documents that load.
"""

import json
import re
import sys


class Integer:
    """An integer token: no fraction and no exponent. `-0` is the integer zero."""

    def __init__(self, token):
        self.value = int(token)


class Real:
    """Any other number token, read with correct rounding."""

    def __init__(self, token):
        self.value = float(token)


def spell_real(value):
    """Shortest round-trip digits; plain decimal for zero and for 1e-4 <= |x| < 1e16."""
    if value == 0.0:
        return "-0.0" if str(value).startswith("-") else "0.0"
    shortest = repr(value)
    exponent_form = re.match(r"^(-?[0-9.]+)e([+-])(\d+)$", shortest)
    if exponent_form:
        mantissa, sign, exponent = exponent_form.groups()
        return mantissa + "e" + ("-" if sign == "-" else "") + str(int(exponent))
    assert "." in shortest, shortest
    return shortest


def spell_string(text):
    out = ['"']
    for character in text:
        if character == '"':
            out.append('\\"')
        elif character == "\\":
            out.append("\\\\")
        elif character == "\n":
            out.append("\\n")
        elif " " <= character <= "~":
            out.append(character)
        else:
            units = character.encode("utf-16-be")
            for index in range(0, len(units), 2):
                out.append("\\u%04x" % int.from_bytes(units[index : index + 2], "big"))
    out.append('"')
    return "".join(out)


def write(value, depth, out):
    pad = "  " * depth
    if isinstance(value, bool):
        out.append("true" if value else "false")
    elif isinstance(value, Integer):
        out.append(str(value.value))
    elif isinstance(value, Real):
        out.append(spell_real(value.value))
    elif isinstance(value, str):
        out.append(spell_string(value))
    elif isinstance(value, list):
        if not value:
            out.append("[]")
            return
        out.append("[")
        for index, item in enumerate(value):
            out.append("\n" if index == 0 else ",\n")
            out.append(pad + "  ")
            write(item, depth + 1, out)
        out.append("\n" + pad + "]")
    elif isinstance(value, dict):
        if not value:
            out.append("{}")
            return
        out.append("{")
        for index, key in enumerate(sorted(value, key=lambda k: k.encode("utf-8"))):
            out.append("\n" if index == 0 else ",\n")
            out.append(pad + "  " + spell_string(key) + ": ")
            write(value[key], depth + 1, out)
        out.append("\n" + pad + "}")
    else:
        raise TypeError(type(value))


def fnv1a128(data):
    digest = 0x6C62272E07BB014262B821756295C58D
    for byte in data:
        digest = ((digest ^ byte) * 0x0000000001000000000000000000013B) % (1 << 128)
    return digest


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    source, target = sys.argv[1], sys.argv[2]
    with open(source, encoding="utf-8") as handle:
        document = json.load(handle, parse_int=Integer, parse_float=Real)
    out = []
    write(document, 0, out)
    data = ("".join(out) + "\n").encode("ascii")
    with open(target, "wb") as handle:
        handle.write(data)
    print("spec:fnv1a128:%032x" % fnv1a128(data))


if __name__ == "__main__":
    main()
