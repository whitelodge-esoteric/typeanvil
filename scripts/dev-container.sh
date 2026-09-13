#!/usr/bin/env bash
# Run a command inside the Typeanvil dev container with the current (or named)
# worktree mounted. Keeps Rust/Python builds off the host to avoid OOM.
#
# Usage: scripts/dev-container.sh [worktree-dir] <command> [args...]
#
# Examples:
#   scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml
#   scripts/dev-container.sh ~/workspace/typeanvil.worktrees/core-168 cargo test
set -euo pipefail

IMAGE=typeanvil-dev
MEM_CAP=6g          # host headroom: 16 GB total, Docker VM itself caps at 8 GB
CPU_QUOTA=4         # limit build parallelism; -j on the cargo command overrides

if [[ $# -eq 0 ]]; then
  echo "usage: $0 [worktree-dir] <command> [args...]" >&2
  exit 2
fi

WORKTREE="$PWD"
if [[ -d "$1" ]]; then
  WORKTREE="$(cd "$1" && pwd)"
  shift
fi
WORKTREE_NAME="$(basename "$WORKTREE")"

# Engine build artifacts live in a per-worktree named volume, NOT in the
# mounted tree: avoids per-file osxfs overhead and keeps host disk free.
VOL_TARGET="dev-target-${WORKTREE_NAME}"
docker volume create "$VOL_TARGET" >/dev/null

# Main's checkout holds the WPT fixtures (.wpt); mounted read-only.
MAIN_DIR="${TYPEANVIL_MAIN:-$HOME/workspace/typeanvil}"

# cargo caches shared across worktrees so deps download once.
docker volume create dev-cargo-home >/dev/null
docker volume create dev-cargo-git >/dev/null
docker volume create dev-cargo-registry >/dev/null

# Fresh named volumes are root-owned; fix ownership for the dev user (idempotent).
init_vol() {
  docker run --rm --user root -v "$1:/v" "$IMAGE" chown -R dev:dev /v >/dev/null 2>&1
}
for v in dev-cargo-home dev-cargo-git dev-cargo-registry "$VOL_TARGET"; do
  init_vol "$v"
done

# Interactive only when stdin is a TTY (background/CI runs have none).
TTY_FLAGS=""
if [[ -t 0 ]]; then TTY_FLAGS="-it"; fi

exec docker run --rm $TTY_FLAGS \
  --memory "$MEM_CAP" \
  --cpus "$CPU_QUOTA" \
  --pids-limit 512 \
  --env CARGO_HOME=/cargo-home \
  --volume dev-cargo-home:/cargo-home \
  --volume dev-cargo-git:/cargo-home/git \
  --volume dev-cargo-registry:/cargo-home/registry \
  --volume "$VOL_TARGET:/work/engine/target" \
  --volume "$MAIN_DIR:/main:ro" \
  --volume "$WORKTREE:/work" \
  --volume "$HOME/.typeanvil-docker-fonts:/System/Library/Fonts:ro" \
  --workdir /work \
  "$IMAGE" "$@"
