<!-- Title: type(scope): what it does — imperative. One PR per issue; fix rounds push to this branch. -->

Closes #

## What and why

<!-- Two paragraphs at most. Link the design note if one exists. -->

## Gate

```text
<!-- Paste the TAIL of the local proof for this PR's row, run first-hand on the final head. Not a
     summary. docs only: `bash .agents/gate.sh docs`. crates/leverage/**, docs, root README.md:
     `bash .agents/gate.sh leverage`. Anything else: `bash .agents/gate.sh`. A pin advance or
     .agents/gate.sh: `bash .agents/gate.sh full`. The tail names the row the checkout needs. A change to release-gate.yml also links a
     green dispatched run of it on this branch. -->
```

Goldens: `git status --porcelain -- tools/golden-gen/goldens` → <!-- "prints nothing", or which goldens changed and why this PR exists to change them -->

## Checklist

- [ ] `CHANGELOG.md` entry under `## Unreleased`, cited `(GitHub #N)` — or this PR changes no behaviour, surface or published claim
- [ ] Docs that named what changed are updated in this PR
- [ ] `.github/workflows/ci.yml` untouched (frozen), and the strict-bit evidence tests (`crates/oce-conformance/tests/strict_bits/*`) untouched — or the issue says to and the PR says so
- [ ] No `--all-features`, no `unsafe`, no `todo!`, no database or async below `oce-api`, no absolute paths or UUID-shaped strings
- [ ] `CI OK` reported green (not merely nothing red)
