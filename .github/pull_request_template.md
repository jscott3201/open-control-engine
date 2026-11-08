<!-- Title: type(scope): what it does — imperative. One PR per issue; fix rounds push to this branch. -->

Closes #

## What and why

<!-- Two paragraphs at most. Link the design note if one exists. -->

## Gate

```text
<!-- Paste the TAIL of `bash .agents/gate.sh full`, run first-hand. Not a summary. -->
```

Goldens: `git status --porcelain -- tools/golden-gen/goldens` → <!-- "prints nothing", or which goldens changed and why this PR exists to change them -->

## Checklist

- [ ] `CHANGELOG.md` entry under `## Unreleased`, cited `(GitHub #N)` — or this PR changes no behaviour, surface or published claim
- [ ] Docs that named what changed are updated in this PR
- [ ] `.github/workflows/ci.yml` untouched (frozen), and the strict-bit evidence tests (`crates/oce-conformance/tests/strict_bits/*`) untouched — or the issue says to and the PR says so
- [ ] No `--all-features`, no `unsafe`, no `todo!`, no database or async below `oce-api`, no absolute paths or UUID-shaped strings
- [ ] `CI OK` reported green (not merely nothing red)
