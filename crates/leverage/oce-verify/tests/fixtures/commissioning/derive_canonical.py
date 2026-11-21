#!/usr/bin/env python3
"""Derive a canonical golden and its spec id for a commissioning spec or a point mapping.

    python3 -I derive_canonical.py <spec-or-mapping.json> <canonical-out.json>

This is the independent side of tests/commissioning/canonical_form.rs: a second implementation
of comment removal, of the canonical form (docs/design/commissioning.md, "Test-spec file") and
of FNV-1a-128 that shares no code with the crate. It writes the canonical bytes to the output
path and prints the id of those bytes (`spec:fnv1a128:<32 hex>`). No test runs it and nothing
blesses its output: run it by hand when the format changes in place, review the diff, and update
the id literals in the test.

What it does, for a document whose root is an object (a spec) or a list (a point mapping):

- removes `//` and `/* */` comments outside strings with one regular expression that reads a
  whole string, a whole line comment or a whole block comment at a time;
- reads the rest with `json.loads`, keeping an integer token and a real apart;
- gives a spec its `schema` tag when it has none;
- writes every object's keys sorted by `key.encode("utf-8")`, except the keys of a reference's
  `outputs` and `indicators` maps, which keep the order of the file: overrides apply in
  declaration order, so that order is part of the spec.

It trusts its input: it does not check the format's rules, a repeated key or a bound. The Rust
reader does; give it only documents that load.
"""

import json
import re
import sys

SCHEMA = "oce/commissioning-spec/v1"

# A string (kept), a line comment or a block comment (each blanked). The alternatives are tried
# in this order at each position, so a comment marker inside a string is part of the string.
TOKEN = re.compile(r'"(?:\\.|[^"\\])*"|//[^\n\r]*|/\*.*?\*/', re.S)


class Integer:
    """An integer token: no fraction and no exponent. `-0` is the integer zero."""

    def __init__(self, token):
        self.value = int(token)


class Real:
    """Any other number token, read with correct rounding."""

    def __init__(self, token):
        self.value = float(token)


def without_comments(text):
    def blank(match):
        token = match.group(0)
        if token.startswith('"'):
            return token
        return re.sub(r"[^\n]", " ", token)

    return TOKEN.sub(blank, text)


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


def write(value, depth, out, declared_order=False):
    """Write `value`; `declared_order` keeps an object's keys as the file has them."""
    pad = "  " * depth
    if value is None:
        out.append("null")
    elif isinstance(value, bool):
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
        keys = list(value) if declared_order else sorted(value, key=lambda k: k.encode("utf-8"))
        out.append("{")
        for index, key in enumerate(keys):
            out.append("\n" if index == 0 else ",\n")
            out.append(pad + "  " + spell_string(key) + ": ")
            write(value[key], depth + 1, out)
        out.append("\n" + pad + "}")
    else:
        raise TypeError(type(value))


def write_reference(reference, depth, out):
    """A reference: sorted keys, with `outputs` and `indicators` written in declared order."""
    pad = "  " * depth
    out.append("{")
    for index, key in enumerate(sorted(reference, key=lambda k: k.encode("utf-8"))):
        out.append("\n" if index == 0 else ",\n")
        out.append(pad + "  " + spell_string(key) + ": ")
        write(reference[key], depth + 1, out, declared_order=key in ("outputs", "indicators"))
    out.append("\n" + pad + "}")


def write_spec(spec, out):
    spec = dict(spec)
    spec.setdefault("schema", SCHEMA)
    out.append("{")
    for index, key in enumerate(sorted(spec, key=lambda k: k.encode("utf-8"))):
        out.append("\n" if index == 0 else ",\n")
        out.append("  " + spell_string(key) + ": ")
        if key == "references" and spec[key]:
            out.append("[")
            for position, reference in enumerate(spec[key]):
                out.append("\n" if position == 0 else ",\n")
                out.append("    ")
                write_reference(reference, 2, out)
            out.append("\n  ]")
        else:
            write(spec[key], 1, out)
    out.append("\n}")


def fnv1a128(data):
    digest = 0x6C62272E07BB014262B821756295C58D
    for byte in data:
        digest = ((digest ^ byte) * 0x0000000001000000000000000000013B) % (1 << 128)
    return digest


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    source, target = sys.argv[1], sys.argv[2]
    with open(source, encoding="utf-8", newline="") as handle:
        text = without_comments(handle.read())
    document = json.loads(text, parse_int=Integer, parse_float=Real)
    out = []
    if isinstance(document, dict):
        write_spec(document, out)
    else:
        write(document, 0, out)
    data = ("".join(out) + "\n").encode("ascii")
    with open(target, "wb") as handle:
        handle.write(data)
    print("spec:fnv1a128:%032x" % fnv1a128(data))


if __name__ == "__main__":
    main()
