# Security

Open Control Engine executes control sequences that drive real building equipment. A wrong
result is a physical hazard, not a failed request. This page states what the engine defends
against, what it deliberately does not, and where to report a problem.

## Reporting a vulnerability

Report privately through [GitHub Security Advisories] on this repository rather than opening
a public issue, and please include the CXF document or input that reproduces it if you can
share one.

[GitHub Security Advisories]: https://github.com/jscott3201/open-control-engine/security/advisories/new

The project is pre-1.0 and maintained by a small team, so there is no formal response-time
commitment. What is committed to: an acknowledgement that names what was understood, and a
public disclosure that describes the actual defect rather than a euphemism for it.

## Threat model

The engine is a **library**. It has no `main`, no daemon, no network listener, and no async
runtime, so it exposes no network attack surface of its own. Process lifecycle, transport,
TLS, authentication, authorization, and multi-tenancy all belong to the host application.

That leaves one interesting boundary: **the CXF document and the parameter values a host
feeds in**. Everything below concerns that boundary.

Also relevant, and easy to overlook: the engine reads no wall clock and holds no ambient
state, so identical inputs produce identical outputs. Determinism is a security property
here as much as a correctness one — an incident can be replayed exactly.

## What is bounded

Verified in the tree, with the enforcing site named so you can check rather than trust:

| Input | Bound | Enforced at |
| --- | --- | --- |
| Binding-expression nesting | 64 (`MAX_NESTING_DEPTH`) | parser entry, the completed AST, and again in `eval()` — `crates/oce-expr/src/parse.rs`, `crates/oce-expr/src/lib.rs` |
| Binding-expression size | 4096 nodes (`MAX_EXPR_NODES`) | parser construction — `crates/oce-expr/src/parse.rs` |
| Composite *nesting* depth | 64 (`MAX_COMPOSITE_NESTING_DEPTH`) | CXF import — `crates/oce-cxf/src/resolve/composite.rs` |
| Composite boundary path | 64 non-top `isConnectedTo` hops | iterative CXF lowering — `crates/oce-cxf/src/resolve/composite.rs` |
| Composite boundary work | 65,536 target examinations and 8 MiB of aggregate target-IRI bytes per document | iterative CXF lowering — `crates/oce-cxf/src/resolve/composite.rs` |

Each returns a typed diagnostic rather than aborting. `#![forbid(unsafe_code)]` is set in
all 17 crates, so the usual memory-safety classes are out of scope by construction.

## Untrusted CXF remains untrusted input

Composite boundary resolution is iterative and bounded independently from composite nesting. A
path may enter 64 non-top boundary nodes, and the complete document may cause 65,536 target
examinations or 8 MiB of aggregate target-IRI bytes within boundary walks. Attempting the next hop,
examination, or byte returns a `MalformedDocument` diagnostic; the resolver does not build a
partial flattened graph. Resource-limit diagnostics omit the attempted target subject so refusal
does not copy an attacker-controlled IRI. Direct leaf wiring does not consume the boundary-work
budgets.

The library does not impose a byte limit before JSON deserialization. A host accepting CXF from an
untrusted source must still cap document size and resource use before calling the loader. Keep
untrusted loading outside the process that is actively commanding equipment when your threat model
requires process isolation.

The boundary limits narrow the accepted CXF subset; they are engine safety policy, not CDL
semantics. Below the limits, canonical target order and duplicate path multiplicity are preserved
so single-assignment validation sees the same graph as before.

## What the engine will not do for you

The engine implements **no fail-safe policy of its own**, and that is a design decision with
safety consequences your host must answer for. In particular, a sample stages the same way
whether its `PointStatus` is `Ok`, `Fault`, `Stale`, or `Uninitialized`, and a missing sample
is not an error — the connector holds its previous value indefinitely, so a dead sensor is
indistinguishable from a steady one.

Staleness limits, fault reactions, and safe-state fallback belong above the engine. See
[Host responsibilities](docs/host-responsibilities.md) before wiring anything to a physical
output.

## Dependencies

`cargo deny check bans licenses sources` runs unconditionally in `.agents/gate.sh`, which CI
executes on every pull request. `cargo deny check advisories` runs on a daily schedule and on
every release PR — it is excluded from the per-PR script because it needs network access and
a writable advisory database.

`third_party/` vendors upstream Modelica Buildings CDL sources and modelica-json CXF
translations **verbatim and uncompiled**. They are read as data by input-hygiene audits, never
built, and the tree sits outside every crate root so `cargo package` never ships it. A
byte-level hash manifest gates that tree against silent modification.
