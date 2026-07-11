---
name: codex-handoff
description: >-
  Emit a paste-ready Codex fix-brief from the active lead review on the open-control
  repo. Use this whenever you (the project lead) finish an adversarial PR review and
  need to hand review fixes back to Codex — i.e. any time you would otherwise hand-write
  a "fixes for Codex", "brief for Codex", "send this to Codex", "round-trip Codex on the
  review", or end-of-review handoff. Trigger it for review-fix handoffs, re-review fix
  rounds, and follow-up dispatch on Codex-authored PRs, even when the user just says
  "give me something to paste into Codex" without naming this skill. Produces a single
  self-contained brief block (Codex starts fresh each time) plus optional PR-comment
  posting and a memory capture.
---

# Codex handoff brief

You are the **open-control project lead**. Codex is the implementor: it works from a
fresh context each round, pushes to its own `codex/*` branch, and opens/updates non-draft
PRs into `development`; you review adversarially and merge on green. This skill turns the
**active review in this conversation** into a brief Codex can act on without you present.

The brief must be **self-contained** — Codex has none of your conversation context. Every
fact it needs (PR, branch, head SHA, what passed, the exact fixes, what's out of scope,
where deferred work is tracked, and how to verify) goes in the block.

## When to use

Use right after you've produced an adversarial review verdict and decided to send fixes
back rather than merge as-is (typical when the owner's "robust / zero-tech-debt / no
shortcuts" bar applies). Also use for **re-review rounds** (second pass of fixes) and for
dispatching a fresh lane brief to Codex — adapt the template's framing accordingly.

Do **not** use it to merge, to write the review itself, or to capture review findings —
those are separate steps. This only assembles the outbound brief.

## Step 1 — Gather the brief inputs from the active review

Pull these from the current conversation / review artifacts. If any is genuinely missing
and you cannot derive it, ask the user one focused question rather than guessing — a wrong
SHA or branch sends Codex to the wrong place.

- **PR number + branch + head SHA** — `gh pr view <n> --json number,headRefName,headRefOid`.
- **Verdict** — APPROVE-WITH-FIXES | REQUEST-CHANGES | (fresh-lane) NEW BRIEF, and the
  one-line "why" so Codex knows whether this is polish or rework.
- **What you verified** — baseline test result + any mutation/empirical proof, so Codex
  trusts the gate is sound and treats the list as hardening, not firefighting.
- **In-PR fix list** — the confirmed findings you want fixed in *this* PR. Number them;
  give concrete file paths; put any guardrail **inline on the risky item** (e.g. "if X
  surfaces, stop and surface to lead — do not silently narrow").
- **Out-of-scope items** — deferred findings, each with the **work-item ID** that tracks
  it, so Codex doesn't scope-creep.
- **Codex identity** — so it loads the right memory namespace (see constants below).

## Step 2 — Emit the brief

Output the brief inside a single fenced ```text block so the user can copy it in one go.
Fill every placeholder; delete sections that don't apply (e.g. "OUT OF SCOPE" if none).
Keep the imperative voice — it's an instruction to Codex, not prose about Codex.

```text
TASK: <one line — address lead review fixes on PR #<n> (<lane name>) | implement <lane>>.
Push to the SAME branch (<branch>). Do NOT open a new PR; do NOT merge.   <-- for fix rounds
[fresh lane: open a non-draft PR into development from branch <branch>.]

IDENTITY/MEMORY: Load your own Aionforge identity from codex-id.json in the repo root
(git-ignored, machine-local — it carries your agent id and team). Recall
work <work-id> + lead review capture <capture-id> before starting; capture a handoff when
done. Never use the env steward identity, and never inline the raw agent UUID in
committed files.

REVIEW OUTCOME (the bar): <verdict>. <one-line why>. Lead verified: <baseline + mutation/
empirical proof>. The items below are <robustness/zero-tech-debt polish | required changes>,
not <rework | optional>.

FIX LIST:
1. <concrete fix, file path>.
2. <concrete fix, file path>.
   >> GUARDRAIL: <inline constraint on any risky item — what to do instead of silently
      working around it; when to stop and surface to lead>.
...

OUT OF SCOPE (already filed, do NOT do here):
- <deferred item> — work <work-id>.

BEFORE PUSHING, all must be green locally:
  cargo nextest run
  cargo test --doc
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo machete
  bash .github/scripts/<the no-db / file-size / no-secret gate(s) relevant to the change>

ON COMPLETION: push to <branch>, comment on PR #<n> summarizing what changed + local gate
results + flag anything a guardrail told you to surface, and capture a handoff to memory.
The lead will re-run the full empirical + review pass and merge on green under standing
authority. If anything is ambiguous or a fix balloons, PAUSE and ask the lead rather than
guess.
```

## Step 3 — Optional follow-through (offer, don't assume)

After printing the brief, offer to:

- **Post it to the PR** so Codex/the owner see it on GitHub:
  `gh pr comment <n> --body-file <tmpfile>` (write the brief body to a temp file first to
  avoid shell-escaping pain).
- **Capture the handoff** to `team:open-control-engine-team` if you haven't already folded
  it into the review capture, so the brief is durable across contexts.

Only do these when the user confirms (or has already asked for them) — the primary
deliverable is the paste-ready block.

## Project constants (open-control)

- **Base branch:** `development`. Codex feature branches: `codex/<lane-slug>`.
- **Codex identity:** Codex loads it from `codex-id.json` (git-ignored, machine-local;
  it carries the agent id and the `open-control-engine-team` membership). Your own
  lead identity is the *separate* `claude-id.json`; never conflate the two — `codex-id.json`
  is Codex's, `claude-id.json` is the lead's. Never inline either raw UUID in committed
  files — the no-secrets CI gate rejects UUID-shaped identities in tracked content.
- **CI is dev-light:** the per-PR gate into `development` runs fmt/clippy/build/rustdoc/
  file-size/no-secret/no-db/cargo-deny but **NOT the test suite** (tests run only on the
  `development -> main` release gate). So a green PR does **not** prove tests pass — that's
  why the brief always lists the full local gate commands, and why you run `nextest`
  independently during review. Keep this fact in the brief's framing so Codex doesn't
  treat green CI as "tests passed."
- **Local gate commands** are the block above; include the specific `.github/scripts/*`
  gate(s) touched by the change (no-db canary, file-size cap, no-secret) when relevant.
- **Collaboration model:** Codex implements/commits/pushes/opens non-draft PRs; the lead
  reviews adversarially (mutation-probe + independent verification) then merges on green
  under standing authority. Fix rounds push to the **same** branch — never a new PR.

## Why the brief is shaped this way

Codex re-derives nothing from your head, so the brief carries identity, exact locations,
and verification commands up front. Guardrails live **inline on the specific finding**
because a generic "be careful" gets ignored, while "if broadening the gate surfaces an
allocation, stop and surface it" is actionable at the moment of risk. Out-of-scope items
carry their tracking work-item IDs so deferred work is visibly owned, not silently dropped
or silently pulled forward. And the "pause and ask if ambiguous" close matters because a
safety-critical control engine punishes confident guessing — a paused question is cheaper
than a wrong fix that looks plausible.
