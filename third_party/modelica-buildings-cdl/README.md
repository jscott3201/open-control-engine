# Vendored: Modelica Buildings Library — CDL block sources

Third-party source, copied verbatim. **Nothing here is open-control code, and nothing here is
compiled.** These files are read as *data* by three input-hygiene audits (see "Who reads it").

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
| Files | 176 `.mo` class sources + `Buildings/legal.html` |
| Vendored tree size | About 3.46 MB (`Buildings/` 1.34 MB; generated `cxf/` 2.11 MB) |

The always-on local integrity check is the `structural oracle (input hygiene)` step of
`bash .agents/gate.sh`. Its hash manifest checks every expected path and byte in both directions,
including the repo-authored README and generated CXF documents.

For a separate upstream-fidelity spot-check, derive the `Buildings/**` comparison set from the
Git index. Its count must agree with the hash manifest's `upstream-buildings` bucket:

```bash
repo_root=$(git rev-parse --show-toplevel)
[ -d /tmp/modelica-buildings/.git ] ||
  git clone https://github.com/lbl-srg/modelica-buildings /tmp/modelica-buildings
git -C /tmp/modelica-buildings checkout a131864e4c4df22ebcd52bb8da439de0087ac365
compared=0
drifted=0
while IFS= read -r tracked_path; do
  relative=${tracked_path#third_party/modelica-buildings-cdl/}
  compared=$((compared + 1))
  if ! cmp -- "$repo_root/$tracked_path" "/tmp/modelica-buildings/$relative"; then
    drifted=$((drifted + 1))
  fi
done < <(git -C "$repo_root" ls-files -- 'third_party/modelica-buildings-cdl/Buildings/**')
printf 'compared=%s (must equal upstream-buildings count 177) drifted=%s (must be 0)\n' \
  "$compared" "$drifted"
test "$compared" -eq 177 && test "$drifted" -eq 0
```

This spot-check deliberately verifies fidelity rather than completeness: the upstream tree is a
superset of the vendored cone, while additions and deletions in the cone are caught by the
bidirectional manifest gate.

The corpus contains the 132 elementary CDL classes reachable from the 46 G36 fixtures plus the
44 G36 and type classes having a mirror-pathed document under `cxf/`. This exact criterion keeps
the structural oracle's class resolution and conditional declarations locally reproducible
without vendoring the whole library. The upstream spot-check covers both `.mo` source sets.

## Generated CXF structural oracle

`cxf/` contains 44 machine translations of the vendored Modelica sources. They were generated
with `modelica-json` commit `85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb` (master, cloned
2026-07-28) under Node v26.5.0, from the Buildings commit recorded above. The translations are
derived copies under the same upstream license; they are test data and are not compiled or
packaged in an `oce-*` crate.

For each of the 31 upstream classes named by the structural-oracle class manifest
`crates/oce-cxf/tests/fixtures/g36/structural_oracle_manifest.json`, generation used:

```bash
MODELICAPATH=<directory-containing-Buildings> node app.js \
  -f <absolute-path-to-class.mo> -o cxf -m cdl -d <output-directory> -p -l error
```

To verify the checked-in translations, check out the two recorded pins, regenerate all manifest
classes into a temporary directory with that command, and compare the trees:

```bash
repo_root=$(git rev-parse --show-toplevel)
[ -d /tmp/modelica-buildings/.git ] ||
  git clone https://github.com/lbl-srg/modelica-buildings /tmp/modelica-buildings
git -C /tmp/modelica-buildings checkout a131864e4c4df22ebcd52bb8da439de0087ac365
[ -d /tmp/modelica-json/.git ] ||
  git clone https://github.com/lbl-srg/modelica-json /tmp/modelica-json
git -C /tmp/modelica-json checkout 85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb
rm -rf /tmp/regenerated-cxf
mkdir /tmp/regenerated-cxf
cd /tmp/modelica-json
# From this directory, run the command above for every structural-oracle class-manifest entry,
# with MODELICAPATH=/tmp/modelica-buildings and output under /tmp/regenerated-cxf.
diff -r "$repo_root/third_party/modelica-buildings-cdl/cxf" /tmp/regenerated-cxf
```

The 44 checked-in documents were byte-compared with a fresh regeneration at exactly those pins
on 2026-07-28.

## Why the files are unmodified

They are deliberately **not** stripped down to interface declarations, even though the consuming
test reads only connector declarations and much of the roughly 1.34 MB `.mo` corpus is
documentation and annotations.

Stripping is itself a derivation step, and a derivation step can be wrong. The extractor that
produced the checked-in table this directory replaces accumulated five defects before it was
correct — it leaked a nested block's connectors into its parent, miscounted `end if;` as a class
close, missed classes spelled `model` rather than `block`, parsed documentation prose as a class
opening, and computed array-ness per class instead of per declaration. A pre-stripped corpus would
have baked any one of those into the vendored data, where review cannot see it, and would have
broken the upstream-fidelity comparison above.

## Who reads it

Three input-hygiene audits, and nothing else:

- `crates/oce-cxf/tests/fixture_port_order.rs` derives each class's port declaration
  order from these sources at test time and checks that every fixture lists its ports in
  that order.
- `crates/oce-cxf/tests/fixture_structural_oracle.rs` reads the `cxf/` documents as the
  structural oracle and these `.mo` sources for class existence and conditional-component
  declarations when resolving each fixture's specialization.
- The `third_party_manifest` verifier hosted by
  `crates/oce-cxf/tests/fixture_structural_oracle.rs` hashes every byte of every
  manifest-listed file and rejects additions, deletions, and non-whitelisted file types. Unlike
  the declaration-only scanner, it covers the complete vendored bytes.

See `TESTING.md` for what those gates do and do not prove.

## Pin-advance policy

Pins advance only when a needed class is absent at the current pin, the upstream subtree SHA
diverges, or a CDL specification revision is one we intend to conform to. Advances are
event-driven only, never on cadence. A pin-advance PR must, in one PR, run
`bash .agents/gate.sh full` and re-bless the hash manifest, its independently pinned tree-SHA
constant, and affected catalog fingerprints, including this README's commit row. The full gate is
required because the G36 half of the pin guard is release-gate-only: a PR that edits the G36
provenance commit field passes the light gate green.

`.agents/revendor-upstream.sh` is the reporter of record — run it before and after any advance; it
verifies local bytes, upstream fidelity by blob OID, and cone drift, and it never re-blesses
anything.

Any byte change anywhere under this vendored tree — including to this README — requires both
re-blessing the hash manifest and re-deriving and deliberately hand-editing the tree-SHA constant.
