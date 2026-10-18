---
title: PR visual diffs
slug: /specifications/pr-visual-diff
type: spec
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 40
tags: [ci, regression, visual-diff, pull-request]
spec_id: SPEC-PR-VISUAL-DIFF
issue_id: CORE-498
applies_to: [scripts/pr_visual_diff.py, .github/workflows/ci.yml]
dependencies: [regression-gate]
---

# PR visual diffs

## Overview

A pull request that changes rendering shows the changed pages as images in its description. Each image has three panels: base, head, and a highlighted diff. This spec belongs to epic CORE-487 and builds on the regression gate (CORE-489).

## Goals

- A reviewer sees what changed without downloading artifacts.
- The description section updates in place on every push.

## Non-Goals

- Fork PRs. Fork runs get no secrets. The repository has no external contributors, so the job skips them. A later ticket may add a fork path.
- Heavy-tier documents. The job covers the same documents as the `regression` job (`--role gate --size small`).

## Decisions

- DECIDED 2026-10-17 (user): host images on an orphan `pr-assets` branch of `whitelodge-esoteric/typeanvil-showcase`. The description embeds them by `raw.githubusercontent.com` URL. A GitHub artifact is not embeddable inline.
- DECIDED 2026-10-17 (agent): the job renders base and head itself (base = PR base commit built in `base/`), so it does not depend on stored fingerprints. A page counts as changed when the fingerprint diff flags it or the page counts differ, and at least one pixel differs. A fingerprint flag with identical pixels is dropped.
- DECIDED 2026-10-17 (agent): image path is `pr-<N>/<head sha8>/<doc>-pNNN.png`. A new path per push keeps the raw URLs immutable and avoids image caching.
- DECIDED 2026-10-17 (agent): the job writes the artifact `visual-diff` too, and runs after `regression` with `always()`, so a red regression job still gets images.

## Behavior

1. The job shall run on `pull_request` for same-repository PRs only, after the `regression` job.
2. It shall render every gate document with the base engine and the head engine and compare page by page.
3. For each changed page it shall write a PNG at 100 DPI with three panels: base, head, diff. In the diff panel, pixels that differ by more than 24 per channel are red on a dimmed head page. A page absent on one side shall show a blank panel labeled "page absent".
4. It shall write at most 20 PNGs, ordered by document path then page. It shall record all changed pages in `manifest.json`.
5. It shall push the PNGs to the `pr-assets` branch (created as an orphan branch when absent) and prune `pr-*` directories whose last commit is older than 30 days.
6. It shall rewrite the text between `<!-- typeanvil-visual-diff:begin -->` and `<!-- typeanvil-visual-diff:end -->` in the PR description. When the markers are absent it shall append the section. It shall not add comments.
7. With no changed page the section shall read `No visual change in N documents.`
8. With more than 20 changed pages the section shall end with `K more changed page(s) not shown` and a link to the run artifacts.

## Interfaces

`scripts/pr_visual_diff.py render|body|prune`. See the module docstring. The workflow needs the secret `SHOWCASE_DEPLOY_KEY` (already used by `showcase.yml`).

## Acceptance Criteria

- AC1. Given a PR that changes one fixture, when the job finishes, then the description contains that document's three-panel image inline. Evidence: manual check on the first such PR; logic covered by `tests/test_pr_visual_diff.py`.
- AC2. Given an engine-neutral PR, then the section reads `No visual change in N documents.` (`test_no_change_line`).
- AC3. Given a second push, then the section is replaced and not duplicated (`test_replace_in_place_not_appended`).
- AC4. Given 25 changed pages, then 20 images show and a "5 more" line appears (`test_cap_adds_more_line`).

## Edge Cases

- A document added by the PR has no base file. All its pages show with a blank base panel.
- Concurrent pushes: the `concurrency` group of the workflow cancels the older run.
- The first PR that adds this job cannot show it: `pull_request` runs the workflow from the merge commit, so it does run, but the secret may be missing until configured. A missing secret fails the publish step only.

## References

- `docs/operations/regression-gate.md`, `docs/operations/pr-visual-diff.md`
- CORE-487 (epic), CORE-489, CORE-494
