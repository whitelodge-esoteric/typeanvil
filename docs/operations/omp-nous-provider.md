---
title: omp + Nous Portal Setup
type: runbook
status: approved
owner: elijah
created: 2026-08-15
updated: 2026-08-16
sidebar_position: 1
tags: [tooling, omp, nous]
trigger: when setting up omp on a new machine
---

# omp + Nous Portal setup

omp had no API key on this machine (`ANTHROPIC_API_KEY` empty in `~/.hermes/.env`).
Solution: point omp at the Nous Portal inference API, which is OpenAI-compatible.

## Files

- `~/.omp/agent/models.yml` — custom provider `nous`:
  - `baseUrl: https://inference-api.nousresearch.com/v1`
  - `apiKey: "!/Users/elijah/.hermes/scripts/nous-token.sh"` — omp's bare-`!` command
    syntax (strips the `!`, execs the rest). **Not** `!command ...` — that made omp try
    to run a binary literally named `command` (401).
  - Models registered: deepseek/deepseek-v4-flash, anthropic/claude-opus-4.8,
    qwen/qwen3-coder-plus, openai/gpt-5.2-codex
- `~/.hermes/scripts/nous-token.sh` — prints the current Nous inference token
  from `~/.hermes/auth.json` (`agent_key` else `access_token`). The Hermes gateway
  refreshes it hourly, so reading fresh on every launch avoids expiry.
- `scripts/omp-nous.sh` — launcher: `./scripts/omp-nous.sh <cwd> [--model X] [args] [prompt]`,
  defaults to `nous/anthropic/claude-opus-4.8`.

## Verified 2026-08-15

- `omp -p --model nous/deepseek/deepseek-v4-flash` and `--model nous/anthropic/claude-opus-4.8`
  both answer correctly (trivial prompts).
- The direct endpoint rejects urllib (Cloudflare fingerprint 403, error 1010) but
  accepts curl and Bun's fetch — don't diagnose "blocked" with urllib.
- `omp models` shows the `nous` provider when models.yml is valid.

## Gotchas

- Token expires hourly — the `!` command refetch is what makes this viable.
- `OPENAI_BASE_URL`/`OPENAI_API_KEY` env vars were NOT enough for custom providers;
  the model still must be registered in models.yml (omp wouldn't find
  `deepseek/deepseek-v4-flash` otherwise).
- Never commit the static token; the script is the source of truth.
