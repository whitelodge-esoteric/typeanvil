---
title: Development Guidelines
type: convention
status: approved
owner: elijah
created: 2026-08-26
updated: 2026-08-26
sidebar_position: 1
tags: [conventions, workflow, process, testing]
---

# Development guidelines

These rules apply to every change that lands on `main`. Pull requests are
checked against them; see the PR template checklist.

## Spec-driven development

Every feature starts as a spec; code follows the spec; tests prove the spec.

1. **Spec first.** New feature → write its spec in
   `docs/specifications/<feature>.spec.md` before (or in the same PR as) the
   code. No spec, no feature.
2. **Implement to the spec.** The spec's Behavior ("shall" statements) and
   Acceptance Criteria are the contract. If code and spec disagree, one of
   them is wrong — raise it in the PR, never silently drift from an approved
   spec.
3. **Same-PR sync.** A PR that changes behavior updates the spec in the same
   PR. Docs are part of "done", not an afterthought.

Bug fixes are exempt from a new spec, but must name the behavior they
restore and add a regression test that fails without the fix.

## Documentation

- Read the relevant doc before coding; write or update docs as you go,
  not after the fact.
- Every doc follows the frontmatter contract in
  [doc-conventions.md](doc-conventions.md): bump `updated` on every edit,
  in the same commit.
- Behavior changes update the spec; architectural changes update
  `docs/architecture/`; operational learnings become runbooks or lessons.
- Run `python3 scripts/validate_docs.py` after touching anything under
  `docs/`. It runs in pre-commit and CI, so a non-conforming doc cannot be
  committed — but catch it locally first.

## Model attribution

AI-assisted code generation is expected here; it must be attributed so
reviewers know what generated the code and where to look for failure modes.

- Note the model used on the Linear issue (`Model: <model>` line) when work
  is delegated to an AI agent.
- Record the model tier used for each significant code generation
  (`Model: opus-class`, `deepseek`, etc.) in the PR description when it may
  matter for review depth.

## Verification before landing

All of these run against your branch before merge:

1. **Tests.** `cargo test` in `engine/` — green, including any new
   regression tests mapped from the spec's acceptance criteria.
2. **Conformance gate.** For engine changes affecting layout, pagination, or
   typography: run the WPT harness A/B against main's binary and ship only a
   zero-regression state ("fixed − regressed" net positive). See
   [the WPT conformance harness spec](../specifications/wpt-conformance-harness.spec.md).
3. **Benchmarks and demo suite.** For engine changes: rebuild release, run
   `scripts/benchmark.py` and `scripts/build-demo.sh`, and commit the updated
   `benchmarks/results.json` + `benchmarks/RESULTS.md` pair and regenerated
   `demo/out/scoreboard.json` (use `git add -f` for scoreboard files) with
   the PR. Parity or performance movement belongs in the PR description.
4. **Docs validation.** `python3 scripts/validate_docs.py`.

Never merge with a failing or skipped step without recording why on the PR.

## Workflow conventions

- Work is tracked in Linear (team Core, project Typeanvil, issue ids like
  CORE-57). Reference the issue id in commit messages:
  `feat: ... (CORE-57)`.
- End-to-end issue execution (worktree setup → reproduce → fix → gate →
  land) follows the loop documented in the team's issue-loop process;
  step ordering there is proven practice, not suggestion.
