---
title: PR visual diffs runbook
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 9
tags: [ci, visual-diff, pull-request]
trigger: When the `visual-diff` CI job fails or the PR description shows broken images
---

# PR visual diffs

Spec: `docs/specifications/pr-visual-diff.spec.md`. The `visual-diff` job in `.github/workflows/ci.yml` renders base and head, builds three-panel PNGs for changed pages, pushes them to the `pr-assets` branch of `whitelodge-esoteric/typeanvil-showcase`, and rewrites the marked section of the PR description.

## Run it locally

Build a base binary from the base commit and a head binary, then:

```bash
python3 scripts/pr_visual_diff.py render --base-bin /path/base --head-bin /path/head \
  --base-root /path/to/base-checkout --out probe/visual-diff --pr 0 --sha deadbeef
```

Fonts differ on the Mac host. Use CI's environment (container) when you compare against CI output.

## Failure modes

| Symptom | Cause | Action |
|---|---|---|
| Publish step fails with `Permission denied (publickey)` | `SHOWCASE_DEPLOY_KEY` missing or read-only | Add a write deploy key to the showcase repo and the secret to this repo. |
| Images show as broken links | push failed, or pruned after 30 days | Re-run the job; it republishes the head sha. |
| Section absent | markers deleted, job skipped (fork PR) | The job appends the section on the next run. Fork PRs are not supported. |

## Cleanup

Each run prunes `pr-*` directories whose last commit is older than 30 days. The branch history grows with each push; squash it by recreating the orphan branch if it passes a few hundred MB.
