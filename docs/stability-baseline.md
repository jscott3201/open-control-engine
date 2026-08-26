# Stability baseline and downstream pin ledger

[`stability-baseline-2026-08-26.json`](stability-baseline-2026-08-26.json) is a dated,
machine-readable evidence snapshot. It records exact Open Control Engine refs and topology,
package/toolchain facts, tag versus GitHub Release state, issue and pull-request inventories,
repository-policy unknowns, Studio product/program identities, and downstream OCE pin states.

This is **evidence, not authority**. Live repository source and repository policy remain
authoritative. Moving branch heads do not invalidate this historical capture, and a later capture
must be added as a new dated artifact with an explicit supersession note; this file must never be
silently rewritten as though its old observations were current.

## Identity boundaries

The Studio entries deliberately keep three identities separate:

1. `open-control-studio` product source is the reviewed behavior.
2. The selected program locator is the immutable program revision named by that product source.
3. The program repository head is newer planning work and does not define product behavior until a
   reviewed product locator change selects it.

Likewise, the Library's declared `ENGINE_PIN` remains its exact commit even though that commit's OCE
tree equals the captured OCE development tree. Tree equivalence never substitutes for commit
identity. Sim and cxf-json use `none_at_revision`, which is intentionally different from `UNKNOWN`.

## Provenance and supersession

Primary Git source is named in each artifact entry by repository, exact revision, and path. The OCE
package claims come from `Cargo.toml`, `rust-toolchain.toml`, and
`crates/oce-api/Cargo.toml` at the captured development commit. Studio pins come from `Cargo.toml`
and its selected locator from `docs/roadmap/README.md` at the captured product commit. Library uses
`ENGINE_PIN`; Sim uses its Cargo manifests and `ocs-bridge` source; cxf-json uses all Cargo
manifests. Dated GitHub observations name the API endpoint used for compare, tag, release,
issue, and pull-request evidence.

The local-only 2026-08-25 planning input at
`_spec/open-control-engine-2026-08-25/CURRENT-BASELINE.md` is absent from clean checkouts. Its relevant
stale statements are quoted in the JSON rather than treated as authority: Studio product main was
`c756d320aa495ddd66630f0987202c6d852f27f5`, its locator was
`5bc001ff9dbf980e5fb7f106e2d5eb9329e842c3`, and the program head was
`5dd50c97472020142503ec8fdb79823fc0e56c78`. The 2026-08-26 capture records the refreshed values
without modifying that historical input.

Branch protection and ruleset state is `UNKNOWN`: the available API evidence was
authorization-limited, so no absence of protection is inferred. One concrete closure step remains:
a repository administrator must inspect **Settings → Rules → Rulesets** and **Settings → Branches**
and record every active rule applying to `development` and `main`. Issue program owners are also
`UNKNOWN`; empty assignee lists do not permit inferring an owner from an issue author.

## Deterministic verification

The standard-library-only tool performs no network access and never compares captured refs with
today's moving branch heads:

```text
python3 tools/stability_baseline/baseline.py --check
python3 tools/stability_baseline/baseline.py --check --source open-control-engine=.
```

`--check` rejects duplicate/extra fields, malformed or shortened SHAs, noncanonical serialization,
changed exact facts, pin/tree mismatches, conflated Studio identities, missing issue/PR distinctions,
and no-pin/unknown substitution. The source option additionally verifies exact local Git objects,
trees, divergence, and source files by commit. It does not fetch, so source access is explicit and
historical verification cannot accidentally depend on mutable heads.

Downstream exact source can be checked from explicit local clones, individually or together:

```text
python3 tools/stability_baseline/baseline.py --check \
  --source open-control-studio=PATH \
  --source open-control-studio-program=PATH \
  --source open-control-library=PATH \
  --source open-control-sim=PATH \
  --source cxf-json=PATH
```

Regeneration is deterministic and review-only:

```text
python3 tools/stability_baseline/baseline.py --write
python3 -m unittest discover -s tools/stability_baseline -p 'test_*.py' -v
```

Regenerate only when reviewing this dated artifact itself. A new observation date belongs in a new
artifact rather than rewriting historical evidence.
