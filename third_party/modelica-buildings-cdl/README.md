# Vendored: Modelica Buildings Library — CDL block sources

Third-party source, copied verbatim. **Nothing here is open-control code, and nothing here is
compiled.** These files are read as *data* by one test.

## License

Modelica Buildings Library, Copyright (c) 1998-2026 Modelica Association, IBPSA, The Regents of
the University of California through Lawrence Berkeley National Laboratory, and contributors.

The license is a **revised 3-clause BSD license with one added paragraph** covering contributed
enhancements. Its full text is vendored verbatim alongside the sources at
[`Buildings/legal.html`](Buildings/legal.html) — the same path it occupies upstream. Source
redistribution is permitted provided the copyright notice, the conditions, and the disclaimer are
retained, which is what that file does.

open-control itself is MIT OR Apache-2.0. This directory does not change that: it is neither
linked nor distributed as part of any published `oce-*` crate. It sits outside every crate root,
so `cargo package` does not see it.

## What was copied

| | |
| --- | --- |
| Upstream | https://github.com/lbl-srg/modelica-buildings |
| Commit | `a131864e4c4df22ebcd52bb8da439de0087ac365` |
| Fetched | 2026-07-27 |
| Files | 176 `.mo` class sources + `Buildings/legal.html` |
| Size | 1.6 MB |

Paths mirror upstream exactly, so verifying this directory is one command against a fresh clone
rather than a reading of 132 entries:

```bash
git clone https://github.com/lbl-srg/modelica-buildings /tmp/mb
git -C /tmp/mb checkout a131864e4c4df22ebcd52bb8da439de0087ac365
cd third_party/modelica-buildings-cdl && \
  find . -type f | while read -r f; do diff -q "$f" "/tmp/mb/$f" || echo "DIFFERS: $f"; done
```

The corpus contains the 132 elementary CDL classes reachable from the 46 G36 fixtures plus the
44 G36 and type classes having a mirror-pathed document under `cxf/`. This exact criterion keeps
the structural oracle's class resolution and conditional declarations locally reproducible
without vendoring the whole library. The one-command pin verification above covers both sets.

## Generated CXF structural oracle

`cxf/` contains 44 machine translations of the vendored Modelica sources. They were generated
with `modelica-json` commit `85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb` (master, cloned
2026-07-28) under Node v26.5.0, from the Buildings commit recorded above. The translations are
derived copies under the same upstream license; they are test data and are not compiled or
packaged in an `oce-*` crate.

For each of the 31 upstream classes named by
`crates/oce-cxf/tests/fixtures/g36/structural_oracle_manifest.json`, generation used:

```bash
MODELICAPATH=<directory-containing-Buildings> node app.js \
  -f <absolute-path-to-class.mo> -o cxf -m cdl -d <output-directory> -p -l error
```

To verify the checked-in translations, check out the two recorded pins, regenerate all manifest
classes into a temporary directory with that command, and compare the trees:

```bash
git -C /tmp/modelica-buildings checkout a131864e4c4df22ebcd52bb8da439de0087ac365
git -C /tmp/modelica-json checkout 85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb
diff -r third_party/modelica-buildings-cdl/cxf /tmp/regenerated-cxf
```

The 44 checked-in documents were byte-compared with a fresh regeneration at exactly those pins
on 2026-07-28.

## Why the files are unmodified

They are deliberately **not** stripped down to interface declarations, even though the consuming
test reads only connector declarations and 768 KB is mostly documentation and annotations.

Stripping is itself a derivation step, and a derivation step can be wrong. The extractor that
produced the checked-in table this directory replaces accumulated five defects before it was
correct — it leaked a nested block's connectors into its parent, miscounted `end if;` as a class
close, missed classes spelled `model` rather than `block`, parsed documentation prose as a class
opening, and computed array-ness per class instead of per declaration. A pre-stripped corpus would
have baked any one of those into the vendored data, where review cannot see it, and would have
broken the `diff` above that makes this directory checkable at all.

## Who reads it

`crates/oce-cxf/tests/fixture_port_order.rs`, and nothing else. It derives each class's port
declaration order from these sources at test time and checks that every fixture lists its ports in
that order. See `TESTING.md` for what that gate does and does not prove.
