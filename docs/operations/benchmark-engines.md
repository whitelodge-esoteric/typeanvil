---
title: Engine Benchmark — TypeAnvil vs PrinceXML, WeasyPrint and Headless Chrome
type: runbook
status: approved
owner: maintainers
created: 2026-08-25
updated: 2026-10-17
sidebar_position: 2
tags: [benchmark, performance, prince, weasyprint, chrome]
---

# Engine benchmark — TypeAnvil vs PrinceXML, WeasyPrint and Headless Chrome

`scripts/benchmark.py` measures wall-clock render time and peak memory for all
four engines across the demo corpus. WeasyPrint joined the comparison in
CORE-318 as a free, open-source second reference point (Prince requires a
license). Headless Chrome joined in CORE-403 as a third reference point: it is
the PDF engine behind print-to-PDF in Chrome and behind Playwright's headless
mode, so its numbers bound what a browser-based converter can do. The benchmark
is an on-demand tool. It is not part of CI or pre-commit.

## Run it

```bash
cargo build --release --manifest-path engine/Cargo.toml
python3 scripts/benchmark.py --runs 5
```

Prerequisites: `prince` on PATH (see library/README.md), the `weasyprint`
CLI (`uv tool install weasyprint`, plus `brew install pango`; see
[Installing WeasyPrint](#installing-weasyprint)), and `chrome-headless-shell`
(see [Headless Chrome](#headless-chrome)).

Useful flags:

- `--runs N` — timed runs per fixture (default 5; median reported).
- `--warmup N` — untimed warmup runs (default 1) so font/file caches are hot.
- `--filter NAME` — substring match on a fixture name.
- `--engine LIST` — comma list or alias. Members are `ta`, `prince`, `weasy`,
  `chrome`; `all` = `ta,prince,weasy,chrome`; `both` keeps the legacy
  TypeAnvil + Prince pair (schema-1 baselines). Default is `all`.
- `--ta-binary PATH` — defaults to `engine/target/release/typeanvil`.
- `--chrome-binary PATH` — `chrome-headless-shell` binary; defaults to the
  newest one in the Puppeteer cache (see [Headless Chrome](#headless-chrome)).
  `$CHROME_BIN` is honored when the flag is absent.

## What it measures

Per fixture and engine:

1. **Median wall-clock seconds** across the timed runs. Process spawn cost
   included for all engines.
2. **Peak RSS** from `/usr/bin/time -l`, max and median across runs. This is
   the largest single process only.
3. **Process-tree memory**: summed resident memory and summed physical
   footprint over every process in the render's process group (see
   [Memory figures](#memory-figures)).
4. **Page count** of the output PDF as a sanity check (all engines must
   produce pages).

Any single run longer than 60 seconds is killed (whole process group) and
counted as a failure, so a hung render cannot hang the benchmark.

## Heavy documents, startup and scaling (CORE-493)

The `perf` role is the release-time measurement. Process start is about 10 ms
for TypeAnvil, so a 20 ms render of a small fixture mostly times the spawn.
The `perf` run therefore adds three things:

1. **Real geometry.** Each document renders at the page size its
   `library/manifest.json` `geometry` entry declares (Letter, A4, 6×9, the
   8×3.2in ticket). The forced 5×3in page applies only to documents that
   declare it. The `perf` role includes the `visual` documents (poster,
   ticket, flyer).
2. **Startup cost.** A one-paragraph document is timed on every engine and
   reported on its own. Every ms-per-page figure subtracts that engine's
   startup median.
3. **Scaling series.** A generated plain-flow book (deterministic text, a
   chapter heading every 10 pages, 6×9in, 0.75in margins) at about 10, 50, 150
   and 300 pages. Only length changes between rows. The report lists seconds,
   actual pages and ms per page for each, plus a growth exponent: the slope of
   log(net seconds) against log(pages). 1.00 is linear. A value clearly above
   1.00 means time per page grows with document length.

The per-run timeout is 300 s (`PERF_TIMEOUT_S`), up from 60 s, so a 300-page
render on WeasyPrint is not killed. `--no-scaling` skips the
startup and scaling runs; `--filter` also skips them. `--perf-json PATH`
writes a compact per-engine record (startup, ms per page per heavy document
and series member, growth exponent) for the `typeanvil-showcase` perf history.

Run it at release time only, on the same host, with a release-profile binary.
Do not compare a debug binary.

Geometry for the small corpus matches `scripts/build-demo.sh`: 5in × 3in pages
with 0.5in margins. TypeAnvil and Prince take CLI geometry flags directly;
WeasyPrint has no page-size flags, so the leg routes through
`scripts/render-weasyprint.sh`, which writes an `@page` stylesheet with the
same geometry (appended after the fixture's own CSS). Chrome's CLI has no
page-size flags either, so the leg writes a prepared copy of the fixture with
the same `@page` rule appended (see [Headless Chrome](#headless-chrome)).

## Headless Chrome

The `chrome` leg renders with **`chrome-headless-shell`**, the stripped
headless browser that ships with Chrome for Testing. It is also the default
headless browser in Playwright. Use the headless shell and not full Google
Chrome: full Chrome's `--headless` mode hangs in non-GUI shells on macOS (its
renderer thread blocks on CVDisplayLink and the process never exits), so it is
unusable from a script.

Install the default binary:

```bash
npx @puppeteer/browsers install chrome-headless-shell@stable
```

The benchmark resolves the default by globbing
`~/.cache/puppeteer/chrome-headless-shell/*/chrome-headless-shell-*/chrome-headless-shell`
and picking the highest version directory. Override it with
`--chrome-binary PATH` or `$CHROME_BIN`. When Chrome is selected and no binary
is found, the script prints an install hint and exits 2 (the same pattern as
the Prince and WeasyPrint checks).

Geometry and asset resolution. Chrome's CLI has no page-size flags, so the
leg writes a prepared copy of each fixture into `benchmarks/.work/` once,
outside the timed region:

- a `<base href="file://<corpus dir>/">` inserted right after `<head …>` so the
  fixture's relative images, fonts, and CSS resolve;
- an `@page { size: 5in 3in !important; margin: 0.5in !important; }` rule
  appended at the very end of the document, so it wins the cascade over the
  fixture's own `@page` rules. This mirrors the `@page` stylesheet that
  `scripts/render-weasyprint.sh` appends for WeasyPrint.

Each render is one cold Chrome process. The command uses a fresh
`--user-data-dir` (a temp directory, removed after the run) and
`--use-mock-keychain --password-store=basic`; the keychain flags are required,
because Chrome otherwise blocks on a macOS keychain prompt. The measured
command is roughly:

```bash
chrome-headless-shell --headless --disable-gpu --no-first-run \
  --no-default-browser-check --no-pdf-header-footer \
  --use-mock-keychain --password-store=basic \
  --disable-extensions --disable-background-networking \
  --user-data-dir=<fresh temp dir> --print-to-pdf=<abs out.pdf> \
  file://<abs prepared.html>
```

Non-goal: there is no warm or persistent-browser mode. Every render cold-starts
Chrome, which is how the other engines are measured too. A long-lived Puppeteer
browser would be faster per page and is a possible follow-up.

## Memory figures

The results report three memory numbers, because a single figure is misleading
for a multi-process engine:

- **Peak memory (MiB)** — the largest single process's peak RSS, from
  `/usr/bin/time -l`. This is the traditional figure, kept for continuity. It
  under-reports multi-process engines badly: Chrome runs around five processes
  (browser, renderer, GPU, utility), and only the biggest one is counted.
- **Peak memory, whole process tree — summed RSS (MiB)** — the sum of resident
  memory over every process the render spawned. This over-counts multi-process
  engines, because shared pages (each Chrome process maps the same framework)
  are counted once per process.
- **Peak memory, whole process tree — physical footprint (MiB)** — the sum of
  each process's private physical footprint, the figure Activity Monitor shows
  as "Memory". It excludes shared libraries, so it is the fairest single
  cross-engine comparison.

Quote the **physical footprint** for cross-engine comparison. Report the
single-process RSS when comparing against older baselines, and note that it
under-reports Chrome. The summed RSS and the footprint bracket the truth: the
real figure lies between them.

The process-tree figures are sampled on macOS with `libproc`
(`proc_listpgrppids` + `proc_pid_rusage`); a sampler thread takes one sample
before any wait, then polls the render's process group every 2 ms and reads the
kernel's `RUSAGE_INFO_V4` per-process lifetime peak physical footprint, so a
short render that ends between polls still reports its real peak. On non-macOS
platforms the sampler is unavailable and the tree figures are omitted, while
elapsed time and single-process RSS keep working. When the two tree tables are
present, a one-line summary lists the maximum process count seen per engine.

## Installing WeasyPrint

WeasyPrint must load Pango/GLib at import time (via cffi/dlopen). On macOS
with Homebrew those libraries live in `/opt/homebrew/lib`, which is not on the
default dyld search path, so the CLI fails with
`cannot load library 'libgobject-2.0-0'` unless the path is exported. Two
things handle this:

- `scripts/render-weasyprint.sh` (and therefore the benchmark and triage
  WeasyPrint legs) exports `DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib`
  itself when it is unset.
- `scripts/benchmark.py` exports the same fallback for its `weasyprint
  --version` probe, so `results.json` and `RESULTS.md` record the version
  string instead of a blank import-failure line.
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
  Peak RSS differences partly reflect that. Use the process-tree physical
  footprint (see [Memory figures](#memory-figures)) for the fairest
  cross-engine comparison.
- A failed render is reported loudly (`FAILURES: ...`) and sets a non-zero
  exit code. Do not commit results containing failures.
- **Known upstream crash (verified CORE-318):** WeasyPrint 70.0 crashes with an
  `AssertionError` in its `float_layout` on `library/book/book-chapter.html`
  (drop-cap `::first-letter` float interacting with `float: footnote`). Prince
  and TypeAnvil render it. The WeasyPrint leg therefore SKIPS that fixture
  automatically — `WEASY_KNOWN_CRASHES` in `scripts/benchmark.py` — and reports
  it as `Skipped:` on the console and in `RESULTS.md`. A skipped fixture is not
  a failure, so the run still exits 0 and its numbers can be committed. Re-test
  with a newer WeasyPrint and remove the entry once it renders.

## Baseline snapshot

The committed `benchmarks/results.json` + `RESULTS.md` pair is a baseline,
not a live dashboard. Re-run after major engine changes (fragmentation,
line-breaking, PDF emission) to check for regressions.
