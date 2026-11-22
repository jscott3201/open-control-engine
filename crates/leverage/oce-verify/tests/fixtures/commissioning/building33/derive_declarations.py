#!/usr/bin/env python3
"""Read the vendored CoolingCoilValve.mo and write what it declares, for the transcription check.

    python3 -I derive_declarations.py <CoolingCoilValve.mo> > declarations.txt

Run by hand from this directory (the README beside it has the command); no test runs it. It
reads the Modelica source, not the authored `cooling_coil_valve.oce`, and shares no code with
the crate or the authoring layer, so `tests/commissioning/building_33_transcription.rs`, which
holds the emitted CXF and the engine that loads it to this file, checks the transcription
against the model and not against itself.

What it reads, and how:

- every `parameter` declaration: its type as written, its `unit` (a `final unit=` modifier, or
  the unit and quantity `Modelica.Units.SI.Temperature` and `.Time` stand for), its `min` and
  `max`, and its binding, evaluated in f64 by a closed evaluator over Python's `ast` (numbers,
  `+ - * /`, unary minus, parentheses and the names of parameters declared above it; anything
  else is an error), in the operation order the source writes;
- every connector of `Buildings.Controls.OBC.CDL.Interfaces`: direction, type, `unit`,
  `quantity`, `displayUnit`, `min`, `max`;
- every other component of `Buildings.Controls.OBC.CDL`: its class and each modifier, a
  modifier's value evaluated the same way (a lone name is also kept as the name it is);
- every `connect(a, b)`, its two ends with their whitespace removed, as an unordered pair.

The file starts with the source's FNV-1a-128 digest, so a test can tell that it was derived
from the bytes in the tree. Reals are written as their 16 hex digits of bits and as `repr`.
"""

import ast
import re
import struct
import sys

FNV_OFFSET = 0x6C62272E07BB014262B821756295C58D
FNV_PRIME = 0x0000000001000000000000000000013B


def fnv1a128(data):
    value = FNV_OFFSET
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) % (1 << 128)
    return "{:032x}".format(value)


def bits(value):
    return "{:016x}".format(struct.unpack("<Q", struct.pack("<d", value))[0])


def evaluate(text, scope):
    """The f64 value of a Modelica binding of numbers, + - * /, unary minus and known names."""

    def walk(node):
        if isinstance(node, ast.Expression):
            return walk(node.body)
        if isinstance(node, ast.Constant) and type(node.value) in (int, float):
            return float(node.value)
        if isinstance(node, ast.Name):
            if node.id not in scope:
                raise SystemExit("unknown name {!r} in {!r}".format(node.id, text))
            return scope[node.id]
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return -walk(node.operand)
        if isinstance(node, ast.BinOp):
            left, right = walk(node.left), walk(node.right)
            if isinstance(node.op, ast.Add):
                return left + right
            if isinstance(node.op, ast.Sub):
                return left - right
            if isinstance(node.op, ast.Mult):
                return left * right
            if isinstance(node.op, ast.Div):
                return left / right
        raise SystemExit("unsupported expression {!r}".format(text))

    return walk(ast.parse(" ".join(text.split()), mode="eval"))


def modifiers(text):
    """`final a=1, b = c` as an ordered list of (name, value text)."""
    result = []
    for part in text.split(","):
        part = part.strip()
        if not part:
            continue
        name, value = part.split("=", 1)
        name = name.replace("final", "").strip()
        result.append((name, value.strip()))
    return result


SI_TYPES = {
    "Modelica.Units.SI.Temperature": ("K", "ThermodynamicTemperature"),
    "Modelica.Units.SI.Time": ("s", "Time"),
}


def main(path):
    raw = open(path, "rb").read()
    text = raw.decode("utf-8").replace("\r\n", "\n")
    # Up to `equation`: declarations; after it: the connections. Annotations are dropped by
    # stopping each declaration at its description string or its `annotation`.
    head, equations = text.split("\nequation\n", 1)
    out = ["# derived from CoolingCoilValve.mo by derive_declarations.py; never edited by hand",
           "source\tfnv1a128:{}".format(fnv1a128(raw))]
    scope = {}
    for match in re.finditer(
            r"parameter\s+([\w.]+)\s+(\w+)\s*(?:\(([^()]*)\))?\s*=\s*([^\"]+?)\s*\"", head, re.S):
        kind, name, mods, binding = match.groups()
        attrs = dict(modifiers(mods or ""))
        unit, quantity = SI_TYPES.get(kind, (attrs.get("unit", "").strip('"'), ""))
        if kind == "Boolean":
            value = {"true": "true", "false": "false"}[binding.strip()]
            scope[name] = value
            out.append("param\t{}\t{}\tunit=\tquantity=\tmin=\tmax=\tvalue={}".format(
                name, kind, value))
            continue
        value = evaluate(binding, scope)
        scope[name] = value
        out.append("param\t{}\t{}\tunit={}\tquantity={}\tmin={}\tmax={}\tvalue={}\t{}".format(
            name, kind, unit, quantity, attrs.get("min", ""), attrs.get("max", ""),
            bits(value), repr(value)))
    for match in re.finditer(
            r"Buildings\.Controls\.OBC\.CDL\.Interfaces\.(Real|Boolean)(Input|Output)\s+(\w+)"
            r"\s*(?:\(([^()]*)\))?", head, re.S):
        kind, direction, name, mods = match.groups()
        attrs = {key: value.strip('"') for key, value in modifiers(mods or "")}
        out.append("port\t{}\t{}\t{}\tunit={}\tquantity={}\tdisplayUnit={}\tmin={}\tmax={}".format(
            "in" if direction == "Input" else "out", name, kind, attrs.get("unit", ""),
            attrs.get("quantity", ""), attrs.get("displayUnit", ""), attrs.get("min", ""),
            attrs.get("max", "")))
    for match in re.finditer(
            r"Buildings\.Controls\.OBC\.CDL\.((?!Interfaces\.)[\w.]+)\s+(\w+)\s*(?:\(([^()]*)\))?",
            head, re.S):
        cls, name, mods = match.groups()
        out.append("block\t{}\tBuildings.Controls.OBC.CDL.{}".format(name, cls))
        for key, value in modifiers(mods or ""):
            if value in ("true", "false"):
                out.append("assign\t{}\t{}\t{}".format(name, key, value))
            elif value in scope and isinstance(scope[value], str):
                out.append("assign\t{}\t{}\t{}\tref={}".format(name, key, scope[value], value))
            else:
                number = evaluate(value, scope)
                ref = value if re.fullmatch(r"\w+", value) and value in scope else ""
                out.append("assign\t{}\t{}\t{}\t{}\tref={}".format(
                    name, key, bits(number), repr(number), ref))
    pairs = []
    for match in re.finditer(r"connect\(([^,]+),([^)]+)\)", equations):
        ends = sorted(re.sub(r"\s+", "", end) for end in match.groups())
        pairs.append("connect\t{}\t{}".format(*ends))
    out.extend(sorted(pairs))
    sys.stdout.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main(sys.argv[1])
