---
title: Dev Container and Docker Volume Discipline
type: lesson
status: approved
owner: elijah
created: 2026-09-16
updated: 2026-09-16
sidebar_position: 5
tags: [docker, dev-container, volume, memory, disk]
---

# Dev Container and Docker Volume Discipline

Docker build volume, memory, and disk-full lessons. The dev-container runbook (docs/operations/dev-container.md) has the operational setup; this file records what went wrong in practice.

## Lessons

- **Host disk full wedges the VM, and the reclaim lever is INSIDE the VM** (verified CORE-185): with ~116 MiB free on the host, small commands still worked while the full suite died with `ld: Input/output error` plus a containerd `meta.db` I/O error. `Docker.raw` is SPARSE (245 GB logical, 104 GB allocated) and cannot grow when the host is full, so new-block writes fail; freeing blocks INSIDE the VM lets its filesystem recycle them without any host growth. The bulk is `dev-target-*` volumes (~8–15 GB each, one per worktree). Safe lever: `docker volume rm` the volumes whose issue is LANDED and whose worktree is dormant — zero modified tracked files AND no fresh commit. Check for an ACTIVE sibling first: one worktree produced a commit between two status checks and was mid-flight. Deleting a build volume costs a rebuild, never source. Keep the release volume (needed to land) and `ms-playwright` (the Chromium oracle).
- **Build with `-j 2` when another session may be building** (verified CORE-174): `cargo test -j 4` twice died with `collect2: fatal error: ld terminated with signal 9 [Killed]` — the LINKER, not a test. The host has 16 GB and the Docker VM gets 8.3 GB, so four parallel link jobs plus another session's container exhaust it. `-j 2` completes. The signature is a build failing in `collect2 ... signal 9` with no test output: lower the job count, do not go hunting for a code bug. Serialise the suite and the gate too — running both at once is what triggered it.
- **Host disk full silently kills Docker mid-gate (verified CORE-130, 2026-09-13):** the 48 GB Docker VM plus stale host `engine/target` dirs can hit 100% (116 MB free); containers die with `unexpected EOF` and the daemon won't restart until space is freed. Recovery: delete stale host cargo caches (`~/workspace/typeanvil/engine/target` ~15 GB — container builds use the `dev-target-*` volumes, never this path; old worktrees' targets too) with user approval, `kill -9` stuck `com.docker.backend` processes, `open -a Docker`. Background gate runs that exited 0/1 with truncated output during a disk-full window are NOT valid results — re-run them after recovery. Note `measure_block` post-CORE-167 SUMS declared extent + content, so a declared-height page-float band must be sized from `resolved_height` directly, and the page-start re-absorb special case must skip `deferred_once` tokens or a deferred float re-defers forever.
- **`git worktree remove` does NOT delete the worktree's Docker build volume** (user rule, 2026-09-15): `dev-target-<worktree-name>` survives its worktree (8–15 GB each), so volumes from closed issues accumulate until a gate dies on a full disk. Delete it as part of closing the issue: `docker volume rm dev-target-core-<N>`. Never remove the shared `dev-cargo-*` volumes or the `typeanvil-dev` image, and never remove a volume whose worktree is live — another session may be mid-build in it. Full rule: docs/operations/dev-container.md.
- **dev-container.sh lives only on release/2026.9, not main** (verified 2026-09-14): the `./scripts/dev-container.sh` snippet fails on a main checkout. Run it from the release worktree, or copy the invocation. Inside the container, `bash -lc` (login shell) DROPS cargo from PATH (`cargo: command not found`) — use plain `bash -c` plus `export PATH="$PATH:/usr/local/cargo/bin"`.
- **`dev-target-*` volumes can be wiped between sessions while the shared cargo volumes survive** (verified CORE-187): `docker images` empty, then `scripts/dev-container.sh` exits **125 with NO output** — `set -e` kills it in `init_vol` and that stderr goes to /dev/null. That signature means REBUILD THE IMAGE (`docker build -f docker/Dockerfile.dev -t typeanvil-dev .`, ~3 min), not a config error; the build after it is fast (~36s) because the cargo caches lived.
- **The dev image's Chromium lives under `/root`, unreadable by the `dev` user** (verified CORE-187): install once into a mounted path (`PLAYWRIGHT_BROWSERS_PATH=/work/.pw-browsers python3 -m playwright install chromium`), then the oracle runs IN-CONTAINER — `cd /work/harness && PYTHONPATH=/work PLAYWRIGHT_BROWSERS_PATH=/work/.pw-browsers python3 <script>`. This supersedes the older "no browsers in the dev image" note (they ARE there, under root's HOME) and keeps CORE-183's rule: one Chromium render per process.
