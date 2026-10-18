---
title: Release cycle
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 11
tags: [release, ci, automation, calver]
trigger: When the release-cut workflow fails, or before you cut a release by hand
---

# Release cycle

`.github/workflows/release-cut.yml` cuts, verifies, and rotates releases. Spec: `docs/specifications/release-cycle.spec.md`.

## What runs

| When | Job | Effect |
|---|---|---|
| Monday 07:23 UTC | `cut` | If commits exist since the last tag, and `ci` and `nightly` are green on the tip, push annotated tag `vYYYY.M.PATCH` |
| after `cut` | `notes` | Set the release body from merged PR titles grouped by label |
| after `cut` | `verify` | Wait for `release.yml`, check `SHA256SUMS`, run `--version` and one render |
| 1st of month 05:41 UTC | `rotate` | Create `release/YYYY.M` from the old tip, fast-forward `main` to the old tip |

The tag starts `release.yml`, which builds the binaries, the npm packages, the image, and the Homebrew bump.

## One-time setup

Add the repository secret `RELEASE_BOT_TOKEN`: a token with `contents: write` that may push tags and may push to `main` (add it to the ruleset bypass list for the fast-forward). Without it the cut job fails on purpose.

## Run it by hand

Actions, `release-cut`, Run workflow. Pick `cut` or `rotate`. `dry_run` defaults to true: untick it to push.

## Skip and failure cases

- No commits since the last tag: the job ends green, no tag.
- `ci` or `nightly` not green on the tip: the job fails. Fix the tip, or wait for the next nightly, then re-run.
- `verify` fails: it opens an issue. The release stays published. Check the run before you re-point the tag (see the `typeanvil-release` skill).
- Promotion blocked (`main` has commits the release branch lacks): the job opens an issue. Land a PR into the release branch that merges `main`, then re-run `rotate`.

## Local helpers

`python3 scripts/release_cut.py next-version release/2026.10 $(git tag --list 'v*')` prints the next version. Tests: `python3 -m pytest tests/test_release_cut.py`.

The Hermes cron job for rotation (CORE-302) is replaced by the `rotate` job. Pause the cron job after the first green run.
