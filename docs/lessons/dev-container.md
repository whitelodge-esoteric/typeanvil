---
title: Dev Container Resource Discipline
type: lesson
status: approved
owner: maintainers
created: 2026-09-16
updated: 2026-09-27
sidebar_position: 5
tags: [docker, dev-container, volume, memory, disk]
---

# Dev container resource discipline

## Context

Rust builds and rendering tests can consume substantial memory and disk space. The repository wrapper keeps build artifacts in Docker volumes and runs commands in a bounded container.

## Durable practices

- **Host disk full wedges the VM, and the reclaim lever is inside the VM** (verified): with ~116 MiB free on the host, small commands still worked while the full suite died with `ld: Input/output error` plus a containerd `meta.db` I/O error. `Docker.raw` is sparse (245 GB logical, 104 GB allocated) and cannot grow when the host is full, so new-block writes fail; freeing blocks inside the VM lets its filesystem recycle them without host growth. The bulk is the `dev-target-*` volumes (~8–15 GB each, one per worktree). Safe lever: `docker volume rm` the volumes whose issue is landed and whose worktree is dormant, meaning zero modified tracked files and no fresh commit. Check for an active sibling first: one worktree produced a commit between two status checks and was mid-flight. Deleting a build volume costs a rebuild, never source. Keep the release volume (needed to land) and `ms-playwright` (the Chromium oracle).
- **Build with `-j 2` when another session may be building** (verified): `cargo test -j 4` twice died with `collect2: fatal error: ld terminated with signal 9 [Killed]`, in the linker, not a test. The host has 16 GB and the Docker VM gets 8.3 GB, so four parallel link jobs plus another session's container exhaust it. `-j 2` completes. The signature is a build failing in `collect2 ... signal 9` with no test output: lower the job count, do not hunt for a code bug. Serialise the suite and the gate too; running both at once is what triggered it.
- **Host disk full silently kills Docker mid-gate** (verified 2026-09-13): the 48 GB Docker VM plus stale host `engine/target` directories can hit 100% (116 MB free); containers die with `unexpected EOF` and the daemon does not restart until space is freed. Recovery: delete stale host cargo caches (about 15 GB, with user approval), `kill -9` stuck `com.docker.backend` processes, then `open -a Docker`. Container builds use the `dev-target-*` volumes, never the host path. Background gate runs that exited 0/1 with truncated output during a disk-full window are not valid results; re-run them after recovery. Note `measure_block` now sums declared extent and content, so a declared-height page-float band must be sized from `resolved_height` directly, and the page-start re-absorb special case must skip `deferred_once` tokens or a deferred float re-defers forever.
- **`git worktree remove` does not delete the worktree's Docker build volume** (user rule, 2026-09-15): `dev-target-<worktree-name>` survives its worktree (8–15 GB each), so volumes from closed issues accumulate until a gate dies on a full disk. Delete it as part of closing the issue: `docker volume rm dev-target-core-<N>`. Never remove the shared `dev-cargo-*` volumes or the `typeanvil-dev` image, and never remove a volume whose worktree is live; another session may be mid-build in it.
- **`scripts/dev-container.sh` lives only on the release branch, not main** (verified 2026-09-14): the `./scripts/dev-container.sh` snippet fails on a main checkout. Run it from the release worktree, or copy the invocation. Inside the container, `bash -lc` (login shell) drops cargo from PATH (`cargo: command not found`); use plain `bash -c` plus `export PATH="$PATH:/usr/local/cargo/bin"`.
- **`dev-target-*` volumes can be wiped between sessions while the shared cargo volumes survive** (verified): `docker images` empty, then `scripts/dev-container.sh` exits 125 with no output; `set -e` kills it in `init_vol` and that stderr goes to /dev/null. That signature means rebuild the image (`docker build -f docker/Dockerfile.dev -t typeanvil-dev .`, about 3 minutes), not a config error; the build after it is fast (about 36s) because the cargo caches lived.
- **The dev image's Chromium lives under `/root`, unreadable by the `dev` user** (verified): install once into a mounted path (`PLAYWRIGHT_BROWSERS_PATH=/work/.pw-browsers python3 -m playwright install chromium`), then the oracle runs in-container: `cd /work/harness && PYTHONPATH=/work PLAYWRIGHT_BROWSERS_PATH=/work/.pw-browsers python3 <script>`. Browsers are present, under root's home. Keep the rule: one Chromium render per process.

## Verification

The current wrapper accepts either `<command> [args...]` or `<worktree-dir> <command> [args...]`. Its engine target is mounted at `/work/engine/target`, the main checkout is mounted read-only at `/main`, and the worktree is mounted at `/work`. Harness runs therefore use `--wpt /main/.wpt` and an engine command under `/work`.

```bash
scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml -j 2
scripts/dev-container.sh python3 -m harness --wpt /main/.wpt run \
  --engine cli --cli-cmd '/work/engine/target/debug/typeanvil render' \
  --filter 'css-page/margin-boxes/content-003-print.html' \
  --workers 1 --report /work/probe/report.json \
  --db /work/probe/history.sqlite
```

The image must exist before the wrapper can run. Build it with the Dockerfile named by the current development setup.

## References

- [Containerized development environment](../operations/dev-container.md)
- [Issue evidence and diagnosis](../conventions/issue-evidence.md)
