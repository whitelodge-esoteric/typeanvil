---
title: Engine Benchmark — Typeanvil vs PrinceXML vs WeasyPrint
type: runbook
status: approved
owner: maintainers
created: 2026-08-25
updated: 2026-10-11
sidebar_position: 2
tags: [benchmark, performance, prince, weasyprint]
---

# Engine benchmark — TypeAnvil vs PrinceXML vs WeasyPrint

`scripts/benchmark.py` measures wall-clock render time and peak memory for all
three engines across the demo corpus. WeasyPrint joined the comparison in
CORE-318 as a free, open-source second reference point (Prince requires a
license). It is an on-demand tool. It is not part of CI or pre-commit.

## Run it

```bash
cargo build --release --manifest-path engine/Cargo.toml
python3 scripts/benchmark.py --runs 5
```

Prerequisites: `prince` on PATH (see demo/corpus/README.md) and the
`weasyprint` CLI (`uv tool install weasyprint`, plus `brew install pango`;
see [Installing WeasyPrint](#installing-weasyprint)).

Useful flags:

- `--runs N` — timed runs per fixture (default 5; median reported).
- `--warmup N` — untimed warmup runs (default 1) so font/file caches are hot.
- `--filter NAME` — substring match on a fixture name.
- `--engine ta|prince|weasy|all|both` — restrict engines. Default is `all`;
  `both` keeps the legacy TypeAnvil + Prince pair (schema-1 baselines).
- `--ta-binary PATH` — defaults to `engine/target/release/typeanvil`.

## What it measures

Per fixture and engine:

1. **Median wall-clock seconds** across the timed runs. Process spawn cost
   included for all engines.
2. **Peak RSS** from `/usr/bin/time -l`, max and median across runs.
3. **Page count** of the output PDF as a sanity check (all engines must
   produce pages).

Geometry matches `scripts/build-demo.sh` exactly: 5in × 3in pages with
0.5in margins. TypeAnvil and Prince take CLI geometry flags directly;
WeasyPrint has no page-size flags, so the leg routes through
`scripts/render-weasyprint.sh`, which writes an `@page` stylesheet with the
same geometry (appended after the fixture's own CSS).

## Installing WeasyPrint

WeasyPrint must load Pango/GLib at import time (via cffi/dlopen). On macOS
with Homebrew those libraries live in `/opt/homebrew/lib`, which is not on the
default dyld search path, so the CLI fails with
`cannot load library 'libgobject-2.0-0'` unless the path is exported. Two
things handle this:

- `scripts/render-weasyprint.sh` (and therefore the benchmark and triage
  WeasyPrint legs) exports `DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib`
  itself when it is unset.
- For interactive use, add the export to your shell profile:
  `export DYLD_FALLBACK_LIBRARY_PATH="$(brew --prefix)/lib"`.

Note: a Python interpreter spawned from inside the app may strip DYLD_* vars
from its children (verified CORE-318), which is why the wrapper re-exports the
variable rather than relying on inheritance.

## Outputs

- `benchmarks/results.json` — full machine-readable data (environment,
  versions, per-run stats).
- `benchmarks/RESULTS.md` — human-readable tables. Commit both when the
  numbers matter.

## Reading the results

- TypeAnvil numbers are only meaningful for a **release** build. Debug
  builds are several times slower; the script records the profile in
  `results.json`.
- Memory comparison caveat: Prince and WeasyPrint link their own runtimes
  (Python + Pango for WeasyPrint); TypeAnvil is a small static Rust binary.
  Peak RSS differences partly reflect that.
- A failed render is reported loudly (`FAILURES: ...`) and sets a non-zero
  exit code. Do not commit results containing failures.
- **Known upstream crash (verified CORE-318):** WeasyPrint 70.0 crashes with an
  `AssertionError` in its `float_layout` on `demo/corpus/book-chapter.html`
  (drop-cap `::first-letter` float interacting with `float: footnote`). Prince
  and TypeAnvil render it. Re-test with a newer WeasyPrint before adding that
  fixture to a committed baseline.

## Baseline snapshot

The committed `benchmarks/results.json` + `RESULTS.md` pair is a baseline,
not a live dashboard. Re-run after major engine changes (fragmentation,
line-breaking, PDF emission) to check for regressions.
