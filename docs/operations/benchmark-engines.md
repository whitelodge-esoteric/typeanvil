---
title: Engine Benchmark — TypeAnvil vs PrinceXML
type: runbook
status: approved
owner: elijah
created: 2026-08-25
updated: 2026-08-25
sidebar_position: 2
tags: [benchmark, performance, prince]
issue_id: CORE-123
---

# Engine benchmark — TypeAnvil vs PrinceXML

`scripts/benchmark.py` measures wall-clock render time and peak memory for
both engines across the demo corpus. It is an on-demand tool. It is not part
of CI or pre-commit.

## Run it

```bash
cargo build --release --manifest-path engine/Cargo.toml
.venv/bin/python scripts/benchmark.py --runs 5
```

Useful flags:

- `--runs N` — timed runs per fixture (default 5; median reported).
- `--warmup N` — untimed warmup runs (default 1) so font/file caches are hot.
- `--filter NAME` — substring match on a fixture name.
- `--engine ta|prince|both` — restrict engines.
- `--ta-binary PATH` — defaults to `engine/target/release/typeanvil`.

## What it measures

Per fixture and engine:

1. **Median wall-clock seconds** across the timed runs. Process spawn cost
   included for both engines.
2. **Peak RSS** from `/usr/bin/time -l`, max and median across runs.
3. **Page count** of the output PDF as a sanity check (both engines must
   produce pages).

Geometry matches `scripts/build-demo.sh` exactly: 5in × 3in pages with
0.5in margins (`360pt 216pt` / `36pt` for Prince). Numbers therefore map
onto the demo corpus page counts.

## Outputs

- `benchmarks/results.json` — full machine-readable data (environment,
  versions, per-run stats).
- `benchmarks/RESULTS.md` — human-readable tables. Commit both when the
  numbers matter.

## Reading the results

- TypeAnvil numbers are only meaningful for a **release** build. Debug
  builds are several times slower; the script records the profile in
  `results.json`.
- Memory comparison caveat: Prince links its own runtime stack; TypeAnvil
  is a small static Rust binary. Peak RSS differences partly reflect that.
- A failed render is reported loudly (`FAILURES: ...`) and sets a non-zero
  exit code. Do not commit results containing failures.

## Baseline snapshot

The committed `benchmarks/results.json` + `RESULTS.md` pair is a baseline,
not a live dashboard. Re-run after major engine changes (fragmentation,
line-breaking, PDF emission) to check for regressions.
