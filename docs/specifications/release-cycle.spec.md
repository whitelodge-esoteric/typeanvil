---
title: Automated release cycle — scheduled cut, tag, verify, rotate
type: spec
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
slug: /specifications/release-cycle
sidebar_position: 46
tags: [release, ci, automation, calver]
spec_id: release-cycle
issue_id: CORE-499
applies_to:
  - .github/workflows/release-cut.yml
  - scripts/release_cut.py
dependencies:
  - release-ci
---

# Automated release cycle

## Overview

`release.yml` turns a tag into binaries (see `release-ci`). This spec covers what creates the tag, checks the result, and rotates the monthly branch. All of it runs in `.github/workflows/release-cut.yml`.

## Decisions

- DECIDED 2026-10-17 (user): cadence is weekly. A week with no new commits since the last tag produces no release.
- DECIDED 2026-10-17 (user): the tag goes on `release/YYYY.M`. Promotion to `main` happens at the monthly rotation.
- DECIDED 2026-10-17 (agent): the preconditions are a green `ci` workflow run and a green `nightly` commit status on the exact tip. A missing nightly status counts as not green. Why: the nightly is the only run that renders every library document.
- DECIDED 2026-10-17 (agent): the tag is pushed with a `RELEASE_BOT_TOKEN` secret. Why: a tag pushed with `GITHUB_TOKEN` does not start other workflows.
- DECIDED 2026-10-17 (agent): promotion is a fast-forward only. If `main` has commits the release branch lacks, the job opens an issue and stops. Why: the branch rules forbid force pushes.

## Behavior

1. The workflow shall run on a weekly schedule (Monday), on a monthly schedule (the 1st), and on `workflow_dispatch`. A manual run shall default to dry-run.
2. The `cut` job shall pick the newest `release/YYYY.M` branch by numeric order.
3. The `cut` job shall end without a tag when no commit exists since the newest tag merged into that branch.
4. The `cut` job shall fail, and push no tag, when `ci` has no successful run on the tip or the `nightly` status on the tip is not `success`.
5. The version shall be `vYYYY.M.PATCH`. YYYY.M comes from the branch name (month not padded). PATCH is the highest existing patch for that month plus one, or 0.
6. The tag shall be annotated, created by the bot identity, and pushed without force.
7. The `notes` job shall write the release body from merged PR titles grouped by label (`engine`, `harness`, `docs`, `tooling`, `demo`, then Other). The body shall link the showcase and the compare view.
8. The `verify` job shall wait for `release.yml` on the tag, download one tarball and `SHA256SUMS`, check the sum, run `--version` (it shall equal `typeanvil <version>`), and render one corpus document to a PDF. A failure shall open an issue.
9. The `rotate` job shall create `release/YYYY.M` at the previous branch tip, never overwrite an existing branch at another tip, and fast-forward `main` to the previous branch tip when `main` is an ancestor of it.
10. The workflow shall not force-push any ref.

## Interfaces

`scripts/release_cut.py`: `next-version BRANCH TAG...`, `rotation-names [YYYY-M]`, `notes --prs FILE`, `verify-sums SUMS FILE...`.

## Acceptance Criteria

1. Given tags `v2026.9.5` and branch `release/2026.10`, `next-version` prints `v2026.10.0` (`tests/test_release_cut.py::test_next_version_first_of_month_is_zero`).
2. Given `v2026.9.2` and `v2026.9.10`, the next patch is 11, not 3 (`test_next_version_increments_numerically`).
3. Rotation in January names the previous December branch (`test_rotation_names_plain_month_and_year_wrap`).
4. Notes group by label with Other last (`test_notes_grouped_by_label_with_other_last`).
5. A wrong checksum is reported (`test_verify_sums`).
6. The workflow has the schedule, dispatch, and the three jobs, and no force push (`test_workflow_*`).
7. One scheduled run cuts a release with no hand step and `verify` passes. This needs the `RELEASE_BOT_TOKEN` secret and is checked by the first live run.

## Non-Goals

- Gating promotion on the WPT engine-change tier. Promotion rides on release-branch commits that already passed CI.
- Re-pointing a tag. That stays a manual step in the `typeanvil-release` skill.

## References

- CORE-499, epic CORE-487, `release-ci.spec.md`, `docs/operations/release-cycle.md`.
