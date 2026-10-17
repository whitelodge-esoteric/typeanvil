#!/usr/bin/env bash
# Run the same steps as .github/workflows/ci.yml, locally (CORE-474).
# Rust runs in the dev container (host cargo builds are banned: OOM guard).
# Harness tests run on the host with the main checkout venv.
# Stops at the first failing step and names it.
#
# Usage: scripts/ci-local.sh
# Runbook: docs/operations/ci.md
set -uo pipefail

cd "$(dirname "$0")/.."

MAIN_DIR="${TYPEANVIL_MAIN:-$HOME/workspace/typeanvil}"
PY="${PY:-$MAIN_DIR/.venv/bin/python}"
if [[ ! -x "$PY" ]]; then
  PY=".venv/bin/python"
fi
if [[ ! -x "$PY" ]]; then
  echo "ci-local: no harness venv python found (set PY=...)" >&2
  exit 2
fi

step() {
  local name="$1"
  shift
  echo
  echo "=== ci-local: $name ==="
  echo "+ $*"
  if ! "$@"; then
    echo >&2
    echo "ci-local: FAILED at step '$name'" >&2
    exit 1
  fi
  PASSED+=("$name")
}

PASSED=()

# engine job (flags match CI; -j 2 only caps container build parallelism)
step "engine: clippy" \
  scripts/dev-container.sh cargo clippy --manifest-path /work/engine/Cargo.toml \
  -j 2 --all-targets -- -D warnings
step "engine: test" \
  scripts/dev-container.sh cargo test --manifest-path /work/engine/Cargo.toml \
  -j 2 --no-fail-fast

# harness job
step "harness: pytest" \
  env -u PYTHONPATH "$PY" -m pytest tests -q

echo
echo "=== ci-local: summary ==="
for name in "${PASSED[@]}"; do
  echo "PASS  $name"
done
echo "ci-local: all ${#PASSED[@]} steps passed"
