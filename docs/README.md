# Documentation

Reference documentation for Open Control Engine. The [project README](../README.md) is the
front door; these pages are the detail behind it.

## Start here

| Page | Read it when you want to know |
| --- | --- |
| [Product contract](product-contract.md) | Versioned executable-CXF/HostTick requirements, domain-owner delegations, limitations, evidence and explicitly future outcomes |
| [Authority claims and supersession](authority-claims.md) | The generated cross-domain summary, checked versus review-only boundaries, source-owner update procedure, and non-exhaustive supersession map, from the [index](authority-claims.json) |
| [Architecture](architecture.md) | How the engine is layered, where the CDL §7.17 seam sits, what each of the 12 engine crates, the firmware image and the leverage crates (the `oce` CLI included) owns, and the platform and MSRV policy |
| [Compiled-schedule blob](compiled-schedule-blob.md) | What an `OCEB` blob carries from a `std` host to a `no_std` target, what it leaves behind, how it is versioned, and what the decoder trusts and refuses |
| [Execution profile](execution-profile.md) | Why each host tick is one state transition, and where `CDL.Logical.Pre` differs from Modelica same-time event iteration |
| [Canonical replay record](replay-record.md) | Bounded per-accepted-frame exact bytes, typed refusals and host-owned ordered replay with separate snapshot sidecars |
| [Verification and evidence](verification-evidence.md) | What has actually been proven about this engine, what has not, and which checks are deliberately not running |
| [CDL coverage](cdl-coverage.md) | Whether *your* sequence runs — which classes and G36 sequences are supported, and what "supported" is defined to mean |
| [CXF round trip](cxf-round-trip.md) | What export guarantees, and the conditions under which it silently drops part of your model |
| [CXF composite subset](cxf-composite-subset.md) | The normative contract, if you are writing a tool that emits CXF for this engine |
| [Host responsibilities](host-responsibilities.md) | What safety behavior you must implement yourself, before wiring the engine to equipment |
| [CI and the gate](ci-and-the-gate.md) | What runs when, and what a green check does and does not prove |
| [Benchmarks](benchmarks.md) | Current complete-frame allocation/latency harness and historical throughput, qualified by the commit and host that produced it |
| [Facade migration](facade-migration.md) | Frame-only execution migration, removed pre-release names, Warning-only diagnostics and preserved host boundaries |

## Design notes

`docs/design/` holds one-page design notes, written and reviewed as a draft PR before a thinly
specified work item is built. A note records the decisions, the rejected alternatives and the
questions its reviewer ruled on; the contracts above remain the authority for shipped behavior.

| Note | Decides |
| --- | --- |
| [Compiled-schedule blob (OCEB)](design/compiled-schedule-blob.md) | The byte format a `std` host emits after load and a `no_std` target decodes and executes: layout, class signature hash, integrity, versioning and size budget |
| [Authoring layer (M2)](design/authoring-layer.md) | How a person or an agent writes a sequence without JSON-LD: the typed model, its text and wire forms, the validator and its finding codes, the deterministic CXF emitter and the `oce` CLI |
| [Historical replay API](design/replay.md) | How recorded or scripted inputs drive the engine frame by frame: slot-indexed frame sources, the missing-sample policy, the columnar trace and its size per frame, and the resumable cursor behind Python and the browser worker |
| [Commissioning verifier (CDL §12)](design/commissioning.md) | How `oce-verify`'s tolerance mode checks a real controller's trends against the engine per CDL §12: the test-spec and point-mapping files, unit conversion, the funnel envelope port, indicators, the report and the receipt, with a worked Building 33 example |
| [Structural diff (`oce-diff`)](design/diff.md) | How two revisions of a sequence are compared: authored names as block identity with a bounded rename pass, the closed change categories, site parameters apart from the graph, the serde `Diff` and its text and Markdown renderings, and the CXF and blob adapters, with three worked examples |
| [Canonical content id](design/content-id.md) | What one canonical content id covers: the loaded flat graph with names inside and namespaces stripped, its `OCECANON` byte encoding, SHA-256 and the `oce:canon1:` format, the surfaces it replaces on `ExportReport`, the blob header and the compatibility descriptor, and the mapping table for the library re-stamp, with a worked byte dump |
| [Importer (`oce-author`)](design/importer.md) | How library CXF and the frozen prototype's v0 graph JSON enter the authoring model: sequence parameters of an enumeration type, bounds on boundary declarations, library constants as member values, what stays refused with a closed `ImportError`, names and the library re-stamp, and the corrected gate-item-1 coverage |

## Elsewhere in the repository

| Document | Purpose |
| --- | --- |
| [`README.md`](../README.md) | Project front door: what this is, who it is for, and how to try it |
| [`TESTING.md`](../TESTING.md) | The testing standard every change is held to. Read before writing a test |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | How to work on the repository |
| [`SECURITY.md`](../SECURITY.md) | Reporting, threat model, and the known hardening limit |
| [`oce` CLI](../crates/leverage/oce-cli/README.md) | The `oce` command-line tool: its commands, the exit codes and the JSON report contract an agent can branch on |
| [`CHANGELOG.md`](../CHANGELOG.md) | Notable changes |

## Two pages worth reading even if you are only evaluating

**[Verification and evidence](verification-evidence.md)** is the honest accounting. Six evidence
layers in this repository are called "tests" and they prove different things — one of
them proves nothing about correctness at all. That page says which is which, names the two global
report tiers that are not wired, and bounds the separate four-case OpenModelica evidence.

**[Host responsibilities](host-responsibilities.md)** is the one to read before anything
touches a physical output. The engine implements no fail-safe policy of its own, by design,
and that page is the checklist of what your host layer therefore has to do.

## A note on these documents

Claims here cite `file:line` wherever they are checkable, so you can verify rather than
trust. Where something is unverified, these pages say so rather than rounding up — several
of them were written specifically to correct claims that had drifted out of date.

Current source links resolve in a clone. Explicit historical locators in the
[supersession map](authority-claims.md#representative-supersession-map--non-exhaustive) are inert
text, not clone prerequisites or current authority.
