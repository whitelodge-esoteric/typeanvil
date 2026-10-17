#!/usr/bin/env bash
# Run the same steps as .github/workflows/ci.yml, locally (CORE-474), and write
# a verifiable receipt to .gate/result.json (CORE-479). Thin wrapper: the logic
# lives in scripts/gate.py.
# Rust runs in the dev container (host cargo builds are banned: OOM guard).
# Harness tests run on the host with the main checkout venv.
#
# Usage: scripts/ci-local.sh            (same as: python3 scripts/gate.py)
#        scripts/ci-local.sh --verify   (check the receipt against HEAD's tree)
# Runbook: docs/operations/ci.md
exec python3 "$(dirname "$0")/gate.py" "$@"
