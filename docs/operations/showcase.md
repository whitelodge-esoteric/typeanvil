---
title: Showcase Publishing
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 8
tags: [showcase, release, github-pages]
trigger: When you set up showcase publishing, or when a showcase publish fails
---

# Showcase publishing

Each release publishes rendered documents to
[whitelodge-esoteric/typeanvil-showcase](https://github.com/whitelodge-esoteric/typeanvil-showcase).
Site: https://whitelodge-esoteric.github.io/typeanvil-showcase/. Contract:
`docs/specifications/showcase-publish.spec.md`.

## One-time setup (owner)

1. Create an SSH key pair: `ssh-keygen -t ed25519 -f showcase_key -N ""`.
2. In the showcase repo, Settings, Deploy keys: add `showcase_key.pub` with write access.
3. In the engine repo, Settings, Secrets, Actions: add `SHOWCASE_DEPLOY_KEY` with the private key.
4. After the first publish creates `main`, enable Pages: Settings, Pages, deploy from branch `main`, folder `/ (root)`.

## Run

- Automatic: when the `release` workflow succeeds on a `v*` tag, `showcase.yml` starts through `workflow_run`. It does not use a `release:` trigger, because `release.yml` creates the release with `GITHUB_TOKEN` and such events start no workflows. A failed release run publishes nothing.
- Manual: Actions, showcase, Run workflow, enter the tag.
- Local dry run of the tree builder:
  `python3 scripts/showcase_publish.py --site /tmp/site --tag v2026.10.0` after `scripts/build-showcase.sh`.

## Failures

- Job skipped with a notice: the secret is missing.
- Prince install warning: the site then shows TypeAnvil pages only.
- Push rejected: the deploy key lacks write access.
