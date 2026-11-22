<!-- Title: type(scope): what it does — imperative. One PR may close several related issues; fix rounds push to this branch. -->

Closes #

## What and why

<!-- Two paragraphs at most. Link the design note if one exists. -->

## Gate

```text
<!-- Paste the TAIL of what you ran first-hand on the final head: fmt, clippy and rustdoc on the
     changed crates, their nextest suites and doctests, and the evidence the issue asks for. Not a
     summary: say which commands ran. The gate script's row for this PR's paths is batched (it runs
     on `development` after every few merges and is recorded on the milestone's tracking issue),
     so say that it is. Owed on this PR itself: a pin advance, .agents/gate.sh, .config/nextest.toml
     or scripts/ci/install-wasmtime.sh: `bash .agents/gate.sh full`; a change to release-gate.yml
     also links a green dispatched run of it on this branch. -->
```

Goldens: `git status --porcelain -- tools/golden-gen/goldens` → <!-- "prints nothing", or which goldens changed and why this PR exists to change them -->

## Checklist

- [ ] `CHANGELOG.md` entry under `## Unreleased`, cited `(GitHub #N)` — or this PR changes no behaviour, surface or published claim
- [ ] Docs that named what changed are updated in this PR
- [ ] `.github/workflows/ci.yml` untouched (frozen), and the strict-bit evidence tests (`crates/oce-conformance/tests/strict_bits/*`) untouched — or the issue says to and the PR says so
- [ ] No `--all-features`, no `unsafe`, no `todo!`, no database or async below `oce-api`, no absolute paths or UUID-shaped strings
- [ ] `CI OK` reported green (not merely nothing red)
