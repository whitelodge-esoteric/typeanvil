---
title: Regression gate (fingerprint baselines)
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 8
tags: [ci, regression, baselines, fingerprint, library]
trigger: When the `regression` CI job fails, or when an intended rendering change needs new baselines
---

# Regression gate

The `regression` job in `.github/workflows/ci.yml` renders the small-tier gate documents of the library (`--role gate --size small`, 25 documents) on every pull request. It compares each page to the committed fingerprint in `baselines/<doc>.json`. The job fails on an unexplained change.

## What a baseline holds

Per document: `file`, `engine_version` (from `engine/Cargo.toml`), `page_count`. Per page: `size` (pt), `text_sha256` (SHA-256 of the text layer), `dhash` (64-bit difference hash of the page at 72 DPI), `dark_pixels` (pixels below luminance 128 at 72 DPI).

## Tolerance

| Field | Rule |
|---|---|
| page count, page size, text hash | exact |
| `dhash` | Hamming distance at most 4 of 64 bits |
| `dark_pixels` | within 0.2 % of the baseline |

Measured noise floor: `scripts/regress.py noise --runs 5` renders the 25 documents five times on one commit. Result: max dhash distance 0, max dark-pixel drift 0, text-hash differences 0. The engine is deterministic, so the thresholds are not there to absorb noise. They let a sub-pixel change pass if it moves no text, and still catch any glyph or box that moves by a visible amount. Raise them only with a new measurement.

## Run it

```bash
# in CI's environment (Linux, bundled Liberation fonts):
./scripts/dev-container.sh python3 /work/scripts/regress.py check
```

Baselines shall come from the Linux environment. The Mac host resolves fonts differently, so its fingerprints differ on every document. Never run `update` on the host.

## When the job is red

1. Download the `regress-changed-pages` artifact. It holds, per changed document, the head PNGs of the changed pages (`head-page-NNN.png`), `baseline.json` and `head.json`.
2. Decide whether the change is intended.
3. If intended, regenerate in the container, review the `baselines/` diff, and commit it in the same PR:

```bash
./scripts/dev-container.sh python3 /work/scripts/regress.py update
git diff --stat baselines/
```

4. If not intended, fix the engine change.

Run `update` only after you know why the page changed. The baseline diff is the review record.

## Known limits

- The artifact has head PNGs only. A baseline stores fingerprints, not images, so base PNGs need a render of the base commit. CORE-487 owns the diff-image step and may add that render.
- The job covers the `small` tier. Heavy documents are not gated here.
- Unit tests for the comparison logic: `tests/test_regress.py`.

## Determinism check

Two runs on one commit shall produce identical baselines. Run `regress.py update` twice in the container and compare with `git diff --exit-code baselines/`. CI-side proof: the `regression` job passes on an unchanged PR.
