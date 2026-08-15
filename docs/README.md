# Documentation

Reference documentation for Open Control Engine. The [project README](../README.md) is the
front door; these pages are the detail behind it.

## Start here

| Page | Read it when you want to know |
| --- | --- |
| [Architecture](architecture.md) | How the engine is layered, where the CDL §7.17 seam sits, what each of the 17 crates owns, and the platform and MSRV policy |
| [Execution profile](execution-profile.md) | Why each host tick is one state transition, and where `CDL.Logical.Pre` differs from Modelica same-time event iteration |
| [Verification and evidence](verification-evidence.md) | What has actually been proven about this engine, what has not, and which checks are deliberately not running |
| [CDL coverage](cdl-coverage.md) | Whether *your* sequence runs — which classes and G36 sequences are supported, and what "supported" is defined to mean |
| [CXF round trip](cxf-round-trip.md) | What export guarantees, and the conditions under which it silently drops part of your model |
| [CXF composite subset](cxf-composite-subset.md) | The normative contract, if you are writing a tool that emits CXF for this engine |
| [Host responsibilities](host-responsibilities.md) | What safety behavior you must implement yourself, before wiring the engine to equipment |
| [CI and the gate](ci-and-the-gate.md) | What runs when, and what a green check does and does not prove |
| [Benchmarks](benchmarks.md) | Measured `Engine::tick()` throughput, recorded per run with the commit and host that produced it |

## Elsewhere in the repository

| Document | Purpose |
| --- | --- |
| [`README.md`](../README.md) | Project front door: what this is, who it is for, and how to try it |
| [`TESTING.md`](../TESTING.md) | The testing standard every change is held to. Read before writing a test |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | How to work on the repository |
| [`SECURITY.md`](../SECURITY.md) | Reporting, threat model, and the known hardening limit |
| [`CHANGELOG.md`](../CHANGELOG.md) | Notable changes |

## Two pages worth reading even if you are only evaluating

**[Verification and evidence](verification-evidence.md)** is the honest accounting. Six evidence
layers in this repository are called "tests" and they prove different things — one of
them proves nothing about correctness at all. That page says which is which, names the two global
report tiers that are not wired, and bounds the separate three-case OpenModelica evidence.

**[Host responsibilities](host-responsibilities.md)** is the one to read before anything
touches a physical output. The engine implements no fail-safe policy of its own, by design,
and that page is the checklist of what your host layer therefore has to do.

## A note on these documents

Claims here cite `file:line` wherever they are checkable, so you can verify rather than
trust. Where something is unverified, these pages say so rather than rounding up — several
of them were written specifically to correct claims that had drifted out of date.

Nothing here describes a working directory that is not in the repository. If a page cites a
path, you have that path in your clone.
