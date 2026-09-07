#!/bin/bash
# Launch omp against the Nous Portal inference API.
# Provider "nous" is defined in ~/.omp/agent/models.yml; its apiKey is a
# !command that reads the live token from Hermes' auth store (auto-refreshed).
# Usage: omp-nous.sh <cwd> [omp flags...] [prompt]
set -euo pipefail
export PATH="$HOME/.bun/bin:$PATH"

MODEL="nous/z-ai/glm-5.3-flash"
if [[ "${1:-}" == "--model" ]]; then
  MODEL="$2"
  shift 2
fi

exec omp -p --cwd "$1" --auto-approve --model "$MODEL" "${@:2}"
