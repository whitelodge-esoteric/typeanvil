---
title: Nightly full run
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-27
sidebar_position: 9
tags: [ci, nightly, regression, visual-qa, release]
trigger: When the `nightly` workflow fails, or when you read the release precondition
---

# Nightly full run

`.github/workflows/nightly.yml` runs every corpus document and the visual QA checks on the newest `release/*` tip. It starts on a daily cron (06:17 UTC) and on `workflow_dispatch`.

## What it runs

| Step | Command | Scope |
|---|---|---|
| Regression | `regress.py check --role any --size all --allow-missing` | small, heavy and visual documents |
| Visual QA | `visual-qa.sh --track showcase --role showcase` | TypeAnvil output only |

Documents without a committed baseline (heavy and visual tiers) are still rendered. A crash or render error fails the run. Their fingerprints are not compared.

The comparison against the third-party engine stays at release time. The workflow does not install or call it (licensing).

## Skip rule

The `resolve` job picks the newest release branch by numeric order. If that SHA already has a green `nightly` commit status, the run ends. Use `workflow_dispatch` with `force` to run anyway.

## Release precondition (CORE-487)

The `report` job sets a commit status with context `nightly` on the tip SHA. The release automation shall read it:

```bash
gh api repos/OWNER/REPO/commits/<sha>/statuses --jq '[.[] | select(.context=="nightly")][0].state'
```

`success` means the nightly passed for exactly that commit.

## When it is red

1. Open the issue labelled `nightly-failure`, on GitHub or in Linear. One issue stays open on each side. Each red run adds a comment with the run link. A green run closes the GitHub issue and only comments on the Linear issue.
2. Read the job summary: changed documents, visual QA failures, slowest documents.
3. Download the `nightly-evidence` artifact for logs and changed-page PNGs.
4. For an intended rendering change, follow `docs/operations/regression-gate.md`.

## Linear failure issue (CORE-592)

The `report` job also mirrors a red run to Linear with `scripts/nightly_linear.py`. The GitHub issue steps do not change.

- **What opens.** A red run creates one Linear issue in team Core, project TypeAnvil Core, titled `Nightly full run failing on <branch>`. It has priority Urgent and the labels `nightly-failure` and `Bug`. The body has the branch, SHA, run link, the regression change table from `regress.log`, a one-line visual QA summary from `qa-report.json`, and a pointer to the `nightly-evidence` artifact.
- **Dedupe rule.** We look for open Core issues (any state except completed or canceled) with the `nightly-failure` label. If one exists, a red run comments on the oldest one instead of creating a new issue.
- **Green runs.** A green run comments on the open issue: "Nightly green again ... Confirm the fix, then mark this Done." It never changes the state or closes the issue. We confirm the fix and close it by hand.
- **Secret.** The repository secret `LINEAR_API_KEY` holds a Linear personal API key. If the secret is missing, or the Linear API fails, the step prints a `::warning::` and exits 0. The Linear step never changes the nightly result.

Preview without sending anything: `python3 scripts/nightly_linear.py fail --dry-run --run-url URL --branch B --sha SHA --regress regress.log`. With `LINEAR_API_KEY` set, the dry run still sends the read-only lookup.

## Prove the failure path

Run `workflow_dispatch` from a branch whose `regress.py` or a baseline is altered on purpose. The run shall go red, set the `nightly` status to `failure`, and open the issue.

Unit tests: `tests/test_nightly.py`, `tests/test_nightly_linear.py`.
