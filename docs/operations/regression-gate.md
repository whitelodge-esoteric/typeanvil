---
title: Regression gate (fingerprint baselines)
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-23
sidebar_position: 8
tags: [ci, regression, baselines, fingerprint, corpus]
trigger: When the `regression` CI job fails, or when an intended rendering change needs new baselines
---

# Regression gate

The `regression` job in `.github/workflows/ci.yml` renders the small-tier gate documents of the corpus (`--role gate --size small`, 25 documents) on every pull request. It compares each page to the committed fingerprint in `baselines/<doc>.json`. The job fails on an unexplained change.

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

Baselines shall come from the GitHub runner. The `regress-baselines` workflow is the only approved way to write them. The Mac host resolves fonts differently, so its fingerprints differ on every document. Never run `update` on the host.

### Which environment is authoritative

CI is authoritative. At CORE-527 the dev container and the runner disagreed on the same commit: the container showed 43 changes against the committed baselines and the runner showed 199. The two Linux environments do not give the same fingerprints (font set and rasterizer details differ). A baseline from the container can therefore fail in CI. The container check is a quick local hint only. A green container run does not prove a green CI run.

## Regenerate baselines in CI

Use the `regress-baselines` workflow (`.github/workflows/regress-baselines.yml`). It runs `regress.py update` on the runner, runs it a second time and fails if the files differ, runs `check`, then commits `baselines/` to the branch.

1. Add the label `update-baselines` to a same-repo pull request. Alternatively, once the workflow is on the default branch, run `gh workflow run regress-baselines.yml --ref <branch>`.
2. Review the `baselines/` diff in the new commit.
3. A push made with the workflow token starts no new CI run. Push an empty commit to re-run the checks.

The workflow refuses to run on `main` and `release/**`.

## When the job is red

1. Download the `regress-changed-pages` artifact. It holds, per changed document, the head PNGs of the changed pages (`head-page-NNN.png`), `baseline.json` and `head.json`.
2. Decide whether the change is intended.
3. If intended, regenerate in CI (see "Regenerate baselines in CI"), review the `baselines/` diff, and land it in a dedicated PR (see "Baselines change only in a dedicated PR").

4. If not intended, fix the engine change.

## Baselines change only in a dedicated PR

Baselines and `corpus/visual-qa-known-failures.json` shall change only in a dedicated, reviewed PR. A feature commit shall not carry them. CORE-519 re-baselined the tagged gate inside the feature commit and hid a letter-spacing regression (CORE-539). The baseline diff is the review record, so it needs its own review.

The `regress-baselines` workflow regenerates every document the nightly checks (`--role any --size all`), the same selection as `nightly.yml`.

Run `update` only after you know why the page changed. The baseline diff is the review record.

## Known limits

- The artifact has head PNGs only. A baseline stores fingerprints, not images, so base PNGs need a render of the base commit. CORE-487 owns the diff-image step and may add that render.
- The PR job covers the `small` tier. The nightly covers all sizes.
- Unit tests for the comparison logic: `tests/test_regress.py`.

## Determinism check

Two runs on one commit shall produce identical baselines. Run `regress.py update` twice in the container and compare with `git diff --exit-code baselines/`. CI-side proof: the `regression` job passes on an unchanged PR.
