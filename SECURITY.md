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
| Connection collection (resolve Step 9) | time and memory linear in the edge list: each distinct endpoint IRI classified once, one shared diagnostic subject per endpoint | `crates/oce-cxf/src/resolve/connection_endpoints.rs` |
| Composite lowering (connection orientation, boundary walk, lowered edge list) | time and memory linear in the document's edge list and IRI bytes: each distinct endpoint IRI classified once, one shared allocation per lowered IRI | `crates/oce-cxf/src/resolve/composite_orientation/endpoints.rs` |

Each count or byte bound returns a typed diagnostic rather than aborting. The connection-collection
and composite-lowering rows are complexity guarantees, not limits; see [Untrusted CXF remains untrusted input](#untrusted-cxf-remains-untrusted-input).
`#![forbid(unsafe_code)]` is set in all 18 workspace members (the 12 engine crates, the firmware image and the leverage crates
`oce-replay`, `oce-author`, `oce-vocab`, `oce-verify` and `oce-cli`, whose `oce` binary root carries it too), so the usual memory-safety
classes are out of scope by construction.

The `oce` binary (`crates/leverage/oce-cli`) is a local command-line tool with no listener. It
checks an input file's size from its metadata before reading it and reads every input through a
bounded reader capped at the form's limit (4 MiB, `oce_author::MAX_TEXT_BYTES` and
`MAX_WIRE_BYTES`) — `crates/leverage/oce-cli/src/input.rs`. Its file writes (`fmt --write`,
`emit -o`) go through a uniquely named `create_new` temporary file beside the target and an atomic
rename, so a planted link is never followed and an interrupted write never leaves a half-written
file — `crates/leverage/oce-cli/src/commands/atomic_write.rs`. Stdin is bounded in bytes, not in
time: a pipe that never closes keeps the process waiting.

## Untrusted CXF remains untrusted input

Composite boundary resolution is iterative and bounded independently from composite nesting. A
path may enter 64 non-top boundary nodes, and the complete document may cause 65,536 target
examinations or 8 MiB of aggregate target-IRI bytes within boundary walks. Attempting the next hop,
examination, or byte returns a `MalformedDocument` diagnostic; the resolver does not build a
partial flattened graph. Resource-limit diagnostics omit the attempted target subject so refusal
does not add another attacker-controlled IRI copy. Direct leaf wiring does not consume the
boundary-work budgets.

Connection collection after lowering classifies each distinct endpoint IRI once and shares one
diagnostic-subject allocation per endpoint, so a long source fanned out to many targets costs time
and memory linear in the document, not fan-out × IRI length. The source of a derivation-skipped,
re-anchored synthesized connection is not charged against the byte budget, and it no longer needs
to be: a 10 MB document with a 1 MiB such source and 65,535 targets is refused in under 0.1 s of
release CPU, where it took about a minute before (GitHub issue #479).

Composite lowering, which runs before connection collection, does the same (GitHub issue #480). It
interns each distinct endpoint IRI once per document and keys its per-edge work (orientation,
duplicate suppression, the boundary walk and the erased-relation check) on dense ids; owners,
composites and parents are `@graph` positions rather than copied IRIs; and every lowered target is
one shared allocation per distinct IRI, so a swapped fan-out does not copy its long target once per
edge. The diagnostic sort keys each subject allocation once. A leaf port with a 1 MiB `@id` wired
65,535 times to a top boundary port (a document of about 10 MB) now imports to its ordinary
diagnostics in under 0.1 s of release CPU (about 1 s in debug) and about 110 MB of peak memory;
before, it took more than three minutes of release CPU, and the swapped variant copied the IRI once
per edge (about 64 GiB). What remains is JSON parsing and the passes linear in the document.

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

`cargo deny check bans licenses sources` runs in the engine lane of `.agents/gate.sh` and as its
own CI job on every pull request that changes anything outside the documentation, the root
`README.md` and `crates/leverage/`. `cargo deny check advisories` runs on a daily schedule, in the nightly release
gate and on every release PR — it is excluded from the per-PR script because it needs network access and
a writable advisory database.

`third_party/` vendors upstream Modelica Buildings CDL sources and modelica-json CXF
translations **verbatim and uncompiled**. They are read as data by input-hygiene audits, never
built, and the tree sits outside every crate root so `cargo package` never ships it. A
byte-level hash manifest gates that tree against silent modification.
