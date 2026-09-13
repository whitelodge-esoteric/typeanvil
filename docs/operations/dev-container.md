---
title: Containerized Dev Environment — Rust & Python in Docker
type: runbook
status: approved
owner: elijah
created: 2026-09-13
updated: 2026-09-13
sidebar_position: 6
tags: [docker, containers, memory, cargo, harness]
issue_id: CORE-168
---

# Containerized dev environment — Rust engine + Python harness

Host builds of `cargo build` / `cargo test` have exhausted system memory on the
16 GB Mac. All Rust builds/tests and Python tooling runs SHOULD run inside
Docker. The container caps memory so a runaway build cannot take the host
down.

## Layout

- `docker/Dockerfile.dev` — image with Rust 1.97, Python 3.11, and the harness
  Python deps (Pillow, pypdfium2, playwright).
- `scripts/dev-container.sh` — wrapper. Runs any command in a container with
  the current (or named) worktree mounted at `/work`.
- `docker-compose.dev.yml` — the same setup as a compose service.

## Usage

```bash
# one-off build of the image (first time, ~10 min)
docker build -f docker/Dockerfile.dev -t typeanvil-dev .

# build + test the engine inside the container (from any worktree root)
scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml
scripts/dev-container.sh cargo test  --manifest-path /work/engine/Cargo.toml

# run the harness against the container-built engine binary
scripts/dev-container.sh bash -c \
  '/work/engine/target/debug/typeanvil render --help'

# from main's checkout, mount a different worktree:
scripts/dev-container.sh ~/workspace/typeanvil.worktrees/core-168 cargo build
```

## Memory guardrails

- The container is capped at 6 GB (`--memory 6g`), leaving ~10 GB of host
  headroom. Docker Desktop's own VM cap (Settings → Resources) is a second
  ceiling — this machine's is set to 8 GB.
- If a build hits the cap it fails with an OOM message; do NOT raise it on the
  host — reduce parallelism instead (`cargo build -j 4`).
- `cargo` parallelism defaults to the container's CPU count; override with
  `-j` when memory is tight.

## Caching

Build artifacts live in a **named Docker volume per worktree** (`dev-target-<name>`),
mounted at `/work/engine/target`. Deleting a worktree does not delete its
volume — clean up with:

```bash
docker volume rm dev-target-<name>
```

The cargo registry/git caches are shared via the `dev-cargo-home` volume, so
dependencies download once, ever.

## System fonts

The engine's bundled Arial faces are macOS paths
(`/System/Library/Fonts/Supplemental/...`), and the system-face tests use
macOS fonts (Georgia, Arial Black). The wrapper mounts `/System/Library/Fonts`
read-only so containers behave like the host. If Docker rejects the mount
("path is not shared"), add it once in Docker Desktop → Settings → Resources →
File Sharing. Unit tests that assert macOS-only system faces are
`#[cfg(target_os = "macos")]`-gated; everything else runs identically on Linux.

## Harness runs

The WPT fixtures live in main's checkout (`~/workspace/typeanvil/.wpt`), which
is mounted read-only at `/main`. Harness invocations inside the container use
`--wpt /main/.wpt`:

```bash
scripts/dev-container.sh bash -c \
  'python3 -m harness --wpt /main/.wpt run --engine cli \
   --cli-cmd "/work/engine/target/debug/typeanvil render" \
   --filter css-page --report /tmp/report.json'
```

Reports write to `/tmp` inside the container (ephemeral) or to a mounted
worktree path (persistent).

## Do not run builds on the host

If Docker is not running: `open -a Docker`, wait for the daemon, then use the
wrapper. Host `cargo build` is the failure mode this setup exists to prevent.
