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
| Files | 132 `.mo` class sources + `Buildings/legal.html` |
| Size | 768 KB |

Paths mirror upstream exactly, so verifying this directory is one command against a fresh clone
rather than a reading of 132 entries:

```bash
git clone https://github.com/lbl-srg/modelica-buildings /tmp/mb
git -C /tmp/mb checkout a131864e4c4df22ebcd52bb8da439de0087ac365
cd third_party/modelica-buildings-cdl && \
  find . -type f | while read -r f; do diff -q "$f" "/tmp/mb/$f" || echo "DIFFERS: $f"; done
```

The corpus is the 132 CDL classes reachable from the 46 G36 fixtures in
`crates/oce-cxf/tests/fixtures/g36/`, not the whole library.

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
