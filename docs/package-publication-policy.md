# Package, feature, and publication policy

The [generated authority summary](authority-claims.md) indexes this owner without replacing it.

This document and
[`package-publication-ledger.json`](package-publication-ledger.json) are the normative contract for
workspace package support, `oce-api` feature selections, and release selection. The ledger is the
machine-readable inventory; this page defines what its classifications mean. The executable
validator is `scripts/package_policy/validate.py`.

No Open Control Engine crate is published to crates.io yet. A manifest marked publishable is only
eligible for a future registry release. Actual publication is deferred to release milestone M06 and
requires explicit owner authorization; this policy neither authorizes nor performs a publication.

## Authority and terminology

This policy is package-level authority. The separate
[public surface contract](public-surface-contract.md) and its blessed `oce-api` and `oce-store`
baselines remain the authority for Rust item names and signatures. Live Cargo metadata remains the
source of observed workspace membership, dependency edges, declared features, and manifest publish
flags. The validator requires those facts to agree with this ledger rather than copying dependency
edges into prose.

**Support status and publication status are different.** Publication is a registry-mechanics
decision: a package may need to exist on crates.io only because a supported package depends on it.
Support is the compatibility promise made to a host. In particular,
`implementation-dependency` packages are publishable without becoming independently supported
host-facing APIs.

The closed category vocabulary is:

| Category | Support meaning | Publication meaning |
| --- | --- | --- |
| `host-facade` | Primary supported embeddable host entry point. | Publishable. |
| `conditional-adapter-port` | Supported only under the documented database-free storage-port and adapter lifecycle contract. | Publishable. |
| `transitional-companion` | Supported companion while current consumers migrate toward facade-owned access. | Publishable. |
| `implementation-dependency` | Registry closure only; no independent host-facing support promise. | Publishable because the facade closure requires it. |
| `test-support` | Repository test infrastructure, not product surface. | Private. |
| `private-reference-adapter` | Verification/reference implementation, not a supported backend. | Private. |
| `verification-tooling` | Conformance and evidence tooling, not runtime product surface. | Private. |
| `reserved-panic-only` | Reserved name with incomplete behavior, including panic-only paths. | Private. |
| `experimental-reserved` | Experimental or reserved boundary without a supported implementation. | Private. |

## Closed package matrix

All 17 Cargo workspace members appear exactly once:

| Package | Category | Manifest publication | Contract |
| --- | --- | --- | --- |
| `oce-api` | `host-facade` | Publishable | Primary host facade. |
| `oce-bless` | `test-support` | `publish = false` | Golden-regeneration test support only. |
| `oce-blocks` | `transitional-companion` | Publishable | `catalog()` metadata is the supported companion contract. |
| `oce-conformance` | `verification-tooling` | `publish = false` | Verification harness, excluded from release selection. |
| `oce-cxf` | `implementation-dependency` | Publishable | Required by the facade closure; a direct downstream pin is a migration constraint, not promotion. |
| `oce-diag` | `implementation-dependency` | Publishable | Required by the facade closure. |
| `oce-docs` | `reserved-panic-only` | `publish = false` | Reserved document-generation seam; panic-only behavior is not releasable. |
| `oce-expr` | `implementation-dependency` | Publishable | Required by the facade closure. |
| `oce-extension` | `experimental-reserved` | `publish = false` | Reserved extension/FMI boundary with no supported implementation. |
| `oce-flatten` | `implementation-dependency` | Publishable | Required wired facade seam; no independent support promise. |
| `oce-graph` | `implementation-dependency` | Publishable | Required by the facade closure. |
| `oce-model` | `implementation-dependency` | Publishable | Required by the facade closure. |
| `oce-reference-wal-adapter` | `private-reference-adapter` | `publish = false` | Durability reference adapter, not a supported backend. |
| `oce-semantics` | `implementation-dependency` | Publishable | Required wired facade seam; deferred behavior is not independently promoted. |
| `oce-store` | `conditional-adapter-port` | Publishable | Database-free app-side adapter port and DTO contract. |
| `oce-store-mem` | `implementation-dependency` | Publishable | Unconditional in-memory implementation in the facade registry closure. |
| `oce-validate` | `implementation-dependency` | Publishable | Required by the facade closure. |

The resulting split is exactly 12 publishable and five private packages. The validator compares
every row with Cargo's live `publish` value, requires one row per workspace member, and rejects a
missing, extra, duplicate, or unknown classification.

## Closed `oce-api` feature matrix

`oce-api` declares only `default` and `mem`. The complete supported selection matrix is:

| Selection | Cargo spelling | Enabled `oce-api` features | Normal OCE dependency result |
| --- | --- | --- | --- |
| Default | no feature flags | `default`, `mem` | The shared 11-package closure, including `oce-store-mem`. |
| Explicit legacy memory spelling | `--no-default-features --features mem` | `mem` | Exactly the same closure. |
| No default features | `--no-default-features` | none | Exactly the same closure. |

`mem` is a **legacy no-op compatibility flag**. It does not make the in-memory backend optional and
is not evidence that disabling defaults removes that backend. `oce-store-mem` is an unconditional
normal dependency in all three selections, while `Engine<S: Store = MemStore>` supplies the default
type. The spelling remains until downstream manifests migrate; making it optional or removing it is
a later compatibility change, not part of this policy.

Any other feature name is unsupported and Cargo must reject it. Every supported selection must have
the exact same normal OCE closure and must not introduce a private workspace package, a database,
`tokio`, `async-std`, or unsafe code. The dependency checks live here in the package validator; the
workspace `unsafe_code = "forbid"` and crate-level `#![forbid(unsafe_code)]` checks remain the
compiler-backed unsafe controls.

## Registry closure and release selection

The future release workflow deliberately retains Cargo's workspace publication mechanism:

- `cargo publish --workspace --locked --dry-run` verifies the selected packages without uploading;
- `cargo publish --workspace --locked` exists only in the manually dispatched, environment-guarded
  publish job;
- Cargo skips the five members whose manifests say `publish = false`, selecting exactly the 12
  publishable members.

The intended order is leaf-to-facade, but no prose list owns that mutable order. The validator builds
the normal/build workspace graph from live Cargo metadata, rejects cycles and private-package
leakage, requires every publishable path dependency to carry a registry version, and derives a
deterministic topological order on each run. `oce-api`'s observed normal closure must equal the
ledger's closure under all three supported feature selections.

The release workflow still does not constitute authorization. A tag verifies only; actual registry
publication remains a manual dispatch behind the `release` environment and remains deferred until
M06 plus an explicit owner decision. Before that decision, package and publish commands are dry-run
evidence only. A dry-run never uploads.

### Immutable workflow approval

`.github/workflows/release.yml` is the readable execution definition. The literal SHA-256 in
`scripts/package_policy/release_workflow.py` is its approval witness, not a second execution grammar.
It approves the raw workflow at commit `2bab88acbc96862f1808b34d305b795f521b3614`: tag pushes run
verification only; publication is declared only in the manual-dispatch job with `needs: verify` and
`environment: release`. Only the publish step explicitly maps the registry token. A failed verify
job skips its dependent publish job under GitHub's
[job dependency rules](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds).
The declared environment follows GitHub's
[deployment environment semantics](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/control-deployments),
and Cargo's [publish dry-run](https://doc.rust-lang.org/cargo/commands/cargo-publish.html) does not upload.

**All bytes are closed.** The validator reads bytes and compares their SHA-256 to the fixed literal:
no YAML parsing, shell inference, decoding, or semantic normalization. Even comments, whitespace,
trailing bytes, CRLF conversion, and malformed encoding are rejected. The ledger retains only the
fixed workflow path and the 12/5 selection counts; it does not duplicate command or event grammar.
There is no configurable workflow/digest location and no runtime self-blessing or auto-bless mode.

An intentional future workflow change requires a reviewed workflow diff, a manual update to the
expected digest with a rationale for the newly approved execution, and updated independent byte
golden evidence. Demonstrate that the old approval rejects the new reference and that the new
approval rejects the old reference while accepting the new reference, repeatedly. Do not compute
the expected digest from whichever candidate the validator is currently checking.

This is a **drift guard preserving approved declared direct execution**, not a sandbox or a security
defense against coordinated edits to the workflow and checker. It does not verify the behavior of
invoked actions/scripts, GitHub environment protection configuration, required reviewers, or actual
secret placement. The workflow's environment comments describe intended setup, not evidence that
repository settings implement it. The checker and hostile controls run independently in CI and in
the local gate; they are not an externally protected checker. See [`.agents/gate.sh`](../.agents/gate.sh)
for the authoritative runnable gate, rather than a duplicate command inventory here.

## Downstream constraints and migration

Current consumer evidence sets a compatibility floor:

- Open Control Studio directly selects `oce-api` and `oce-blocks`, and its manifest uses
  `default-features = false, features = ["mem"]`. The legacy spelling stays until that manifest is
  migrated.
- Logic Studio directly pins `oce-api`, `oce-blocks`, and `oce-cxf`. Its active facade/catalog use
  keeps the catalog transitional companion supported. Its `oce-cxf` pin constrains migration but
  does not make `oce-cxf` an independently supported package.
- Verdant Watch uses `oce-api`, reinforcing the facade as the host-facing boundary.

No downstream repository changes under this policy. Direct implementation-package consumers must
migrate through coordinated downstream changes before an implementation package can become private
or disappear from the registry closure.

## Reversal before release freeze

Because nothing has been published, every classification remains reversible before release freeze,
but not silently. A reversal requires an owner-approved change to this policy and ledger, matching
manifest and workflow changes, a dependency-safe metadata result, all hostile controls and gates,
and any required downstream migration. Promoting an implementation package to independent support
also requires a stated public contract and compatibility evidence; changing a publishable package to
private requires first removing it from every publishable normal/build closure and coordinating any
direct consumers.

After first publication, registry history cannot be erased. Reversal then follows the release and
pre-1.0 compatibility process rather than pretending the package was never public.
