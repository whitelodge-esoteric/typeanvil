---
title: Development Guidelines
type: convention
status: approved
owner: maintainers
created: 2026-08-26
updated: 2026-09-27
sidebar_position: 1
tags: [conventions, workflow, process, testing]
---

# Development guidelines

These rules apply to every change that lands on the default branch. Pull requests are checked against them.

## Spec-driven development

Every feature starts as a specification; code follows the specification; tests prove the specification.

1. Write a specification in `docs/specifications/<feature>.spec.md` before or with the implementation.
2. Treat approved Behavior statements and Acceptance Criteria as the contract. Raise specification and code conflicts in review.
3. Update the specification in the same change as behavior changes.
4. For a bug fix, name the restored behavior and add a regression test that fails without the fix.

## Documentation

- Read the relevant document before coding and update it with the change.
- Follow the [documentation conventions](doc-conventions.md) and bump `updated` on every edit.
- Put behavior changes in the governing specification, architectural changes in `docs/architecture/`, and operational knowledge in runbooks or lessons.
- Run `python3 scripts/validate_docs.py` after touching `docs/`. Install PyYAML when the validator reports that it is missing.

## Verification before landing

Run the checks that map to the change:

1. **Tests.** Run the engine test suite and new regression tests.
2. **Conformance.** For layout, pagination, or typography changes, run the WPT harness against a fresh baseline and candidate binary with identical settings.
3. **Release evidence.** Run the baseline/candidate evidence path. Compare each document with its own earlier output and assert direct PDF properties when pair equality is insufficient. See the [release gate specification](../specifications/harness-release-gate.spec.md) and [runbook](../operations/release-gate.md).
4. **Benchmarks and demos.** For engine changes, run the benchmark and demo scripts when their inputs are affected. Record meaningful movement.
5. **Documentation.** Run `pre-commit run --all-files` when configured, and build the docs site with `cd docsite && npm run build`.

Do not merge a failing or skipped check without recording the reason in the pull request.

## Evidence review

Before implementation or a follow-up, apply [Issue evidence and diagnosis](issue-evidence.md). Read the complete record, inspect later work, reproduce at the current source revision, and keep observations separate from suspected causes. Unknown causes can proceed as investigation. Remeasure residuals at the landed revision before creating a follow-up.

## Cleanup

A worktree's Docker build volume survives worktree removal. Remove a worktree-specific volume only after the work is closed, no contributor is using it, and useful evidence is retained. Keep shared cache volumes and the shared development image. See [containerized development](../operations/dev-container.md).
