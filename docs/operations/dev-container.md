---
title: Containerized Development Environment
type: runbook
status: approved
owner: maintainers
created: 2026-09-13
updated: 2026-09-27
sidebar_position: 6
tags: [docker, containers, memory, cargo, harness]
trigger: When building or testing the Rust engine and Python harness
---

# Containerized development environment

Use the repository's development container for Rust builds, engine tests, and harness runs when host resources are limited or a reproducible environment is required.

## Prerequisites

- Docker is running.
- The development image exists. Build it with `docker/Dockerfile.dev` when needed.
- Run commands from the repository root or pass a worktree directory to the wrapper.

## Steps

Build the image once when it is absent:

```bash
docker build -f docker/Dockerfile.dev -t typeanvil-dev .
```

Build and test inside the container:

```bash
scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml -j 2
scripts/dev-container.sh cargo test --manifest-path /work/engine/Cargo.toml -j 2
```

The wrapper also accepts a worktree path:

```bash
scripts/dev-container.sh /path/to/worktree cargo build --manifest-path /work/engine/Cargo.toml -j 2
```

Run the harness against the container-built binary:

```bash
scripts/dev-container.sh python3 -m harness --wpt /main/.wpt run \
  --engine cli \
  --cli-cmd '/work/engine/target/debug/typeanvil render' \
  --filter 'css-page/margin-boxes/content-003-print.html' \
  --workers 1 \
  --report /work/probe/report.json \
  --db /work/probe/history.sqlite
```

The wrapper mounts the current worktree at `/work`, the main checkout at `/main` read-only, and a per-worktree target volume at `/work/engine/target`. Keep reports under `/work` when they must persist.

## Resource and cache rules

- Use a lower Cargo job count when the linker reaches the container memory limit.
- Treat a report from an interrupted or failed container as incomplete.
- Build artifacts use a worktree-specific `dev-target-<worktree-name>` volume.
- Shared Cargo cache volumes and the `typeanvil-dev` image serve all worktrees. Keep them.
- A worktree-specific volume can be removed only after the work is closed, no contributor is using it, and required evidence is retained.

## Verification

A successful build or test command exits 0. A harness run must select the expected test IDs, write a report, and distinguish PASS, FAIL, ERROR, and SKIP results. A reproduction is evidence; it is not a release-gate result.

## Troubleshooting

- **Image absent:** rebuild it with the Dockerfile command above.
- **Linker killed or Docker reports out of memory:** reduce `-j` and stop competing builds.
- **Harness cannot find WPT:** check that `/main/.wpt` exists and pass `--wpt` before `run`.
- **Engine command fails:** include the `render` subcommand and use the container path `/work/engine/target/debug/typeanvil`.
- **Report disappears:** write it under `/work`, not container `/tmp`.
