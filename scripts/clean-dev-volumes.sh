#!/usr/bin/env bash
# Remove per-worktree Docker build volumes (dev-target-<worktree-name>) that
# are no longer needed (CORE-458).
#
# Usage:
#   scripts/clean-dev-volumes.sh --worktree <name>   remove one volume (run by the landing flow)
#   scripts/clean-dev-volumes.sh --sweep [--release-ref <ref>]
#                                                    remove every orphaned/merged volume
#   add --dry-run to print what would be removed.
#
# A sweep removes dev-target-<name> only when its worktree directory no longer
# exists under the worktrees root. A volume whose worktree still exists is
# kept. --release-ref is accepted for compatibility and only printed.
#
# Never removed, by construction: dev-cargo-*, dev-target-release, the
# typeanvil-dev image. Only volumes named dev-target-* are ever passed to
# `docker volume rm`.
set -euo pipefail

MAIN_DIR="${TYPEANVIL_MAIN:-$HOME/workspace/typeanvil}"
WT_ROOT="${TYPEANVIL_WORKTREES:-$HOME/workspace/typeanvil.worktrees}"
DOCKER="${DOCKER:-docker}"
DRY=0
MODE=""
NAME=""
RELEASE_REF=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --worktree) MODE=one; NAME="${2:?--worktree needs a name}"; shift 2 ;;
    --sweep) MODE=sweep; shift ;;
    --release-ref) RELEASE_REF="${2:?--release-ref needs a ref}"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$MODE" ]] || { echo "usage: $0 --worktree <name> | --sweep [--dry-run]" >&2; exit 2; }

# Protected names. Checked before any removal.
protected() {
  case "$1" in
    dev-cargo-*|dev-target-release|dev-target-) return 0 ;;
    dev-target-*) return 1 ;;
    *) return 0 ;;
  esac
}

remove_volume() {
  local vol="$1"
  if protected "$vol"; then
    echo "skip (protected): $vol"
    return 0
  fi
  if ! "$DOCKER" volume inspect "$vol" >/dev/null 2>&1; then
    echo "absent: $vol"
    return 0
  fi
  if [[ $DRY -eq 1 ]]; then
    echo "would remove: $vol"
  else
    "$DOCKER" volume rm "$vol" >/dev/null && echo "removed: $vol"
  fi
}

if [[ "$MODE" == one ]]; then
  NAME="${NAME#dev-target-}"
  remove_volume "dev-target-$NAME"
  exit 0
fi

# Sweep.
if [[ -z "$RELEASE_REF" ]]; then
  git -C "$MAIN_DIR" fetch -q origin || true
  RELEASE_REF="$(git -C "$MAIN_DIR" branch -r --list 'origin/release/*' \
    | sed 's/^ *//' | sort -V | tail -n 1)"
fi
[[ -n "$RELEASE_REF" ]] || { echo "no release ref found" >&2; exit 1; }
echo "release ref: $RELEASE_REF"

"$DOCKER" volume ls -q | grep '^dev-target-' | while read -r vol; do
  if protected "$vol"; then continue; fi
  name="${vol#dev-target-}"
  wt="$WT_ROOT/$name"
  if [[ ! -d "$wt" ]]; then
    echo "orphan (no worktree): $vol"
    remove_volume "$vol"
    continue
  fi
  # A live worktree is never swept. An "is the branch an ancestor of the
  # release ref" test is unsafe: a fresh branch still at the base commit is an
  # ancestor, and squash-landed branches never are. The landing flow removes
  # the landed worktree's volume with --worktree instead.
  echo "keep (live worktree): $vol"
done
