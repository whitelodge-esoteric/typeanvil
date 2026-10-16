#!/usr/bin/env bash
# Host-runnable engine adapter (CORE-212). Forwards a `typeanvil` invocation
# into the dev container, so the demo pipelines can use the container-built
# Linux binary without a host cargo build (banned by CORE-168).
#
# Use it as the pipelines' engine:
#   ENGINE_BIN=scripts/engine-in-container.sh scripts/build-demo.sh
#
# Paths under the worktree map onto the /work mount; the caller's CWD is kept
# (relative to the worktree). Requires a container build to exist:
#   scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml -j 2
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
CWD="$(pwd -P)"

case "$CWD" in
  "$REPO_ROOT") REL="" ;;
  "$REPO_ROOT"/*) REL="${CWD#"$REPO_ROOT"/}" ;;
  *) echo "error: engine-in-container.sh must run inside $REPO_ROOT (cwd: $CWD)" >&2; exit 2 ;;
esac

args=()
for a in "$@"; do
  case "$a" in
    "$REPO_ROOT") a="/work" ;;
    "$REPO_ROOT"/*) a="/work/${a#"$REPO_ROOT"/}" ;;
  esac
  args+=("$a")
done

exec "$REPO_ROOT/scripts/dev-container.sh" "$REPO_ROOT" \
  sh -c 'cd "/work/$0" && exec /work/engine/target/debug/typeanvil "$@"' "$REL" "${args[@]}"
