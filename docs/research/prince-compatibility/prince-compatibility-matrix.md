---
title: Prince Compatibility Research
type: research
status: approved
owner: maintainers
created: 2026-09-09
updated: 2026-10-11
sidebar_position: 1
tags: [prince, weasyprint, compatibility, migration, css, benchmark]
---

# Prince Compatibility Research

## Question

How should Typeanvil use PrinceXML as a comparison target without making
Prince-specific behavior a second rendering contract? And (CORE-318) what does
a second independent HTML→PDF engine — WeasyPrint — add to that comparison?

## Method

- Prince's public paged-media and CSS support documentation was checked on
  2026-09-27; WeasyPrint's was checked on 2026-10-11.
- The repository's CSS alignment convention and current feature specifications
  were checked on both dates.
- On 2026-10-11 the five adopted WeasyPrint sample fixtures
  (`demo/showcase/weasyprint-{invoice,letter,poster,report,ticket}.html`,
  CORE-317) were rendered through TypeAnvil, Prince 16.2, and WeasyPrint 70.0
  at their designed page sizes. Fixture-internal `@page` rules govern the
  geometry; CLI geometry flags are defaults only, exactly like
  `build-showcase.sh`. Per document we recorded page count, mean pixel diff at
  96 DPI (TypeAnvil vs each other engine, shared pages only), median wall-clock
  of 3 timed runs after 1 warmup, and max peak RSS (`/usr/bin/time -l`).
  Source revision `8fde3ab`; TypeAnvil was a fresh host release build of that
  revision. Historical local scoreboards and issue-specific mode proposals are
  archived and excluded from this summary.

## Findings

### Prince documents both standards support and deviations

Prince's public CSS support matrix lists CSS Paged Media Level 3 and CSS
Generated Content for Paged Media among its supported areas. The same matrix
also records deviations and extensions, including a different initial value for
`widows` and `orphans`, partial support for writing modes, and Prince-specific
interfaces for some paged-media features.

### WeasyPrint is a free, reproducible second reference

WeasyPrint is the leading open-source HTML→PDF engine (Python + Pango/GLib).
It needs no license, so the performance benchmark and the WPT triage run
without a Prince seat. Its support coverage is broad but not comprehensive,
and it is a comparison target, not an oracle — CSS specifications still govern
Typeanvil behavior (see `docs/conventions/css-standards-alignment.md`).

WeasyPrint installs on macOS via `uv tool install weasyprint` (plus
`brew install pango`). Its CLI has no page-size flags; geometry comes from CSS
`@page`, which is why `scripts/render-weasyprint.sh` translates the Typeanvil
CLI's per-side geometry flags into an `@page` stylesheet.

### Both engines are useful comparison targets, not contracts

Prince's paged-media documentation covers page size, page regions, named
pages, pagination controls, page numbering, and PDF-oriented features.
WeasyPrint's documentation is organized around the same CSS modules. Both are
useful for migration checks and fixture design. Neither establishes Typeanvil
behavior when CSS specifications define a different result.

### A global compatibility mode would duplicate the contract

A rendering switch would need a named rule for every meaningful difference,
separate tests, and a separate release gate. Prince's support matrix shows its
behavior spans both standardized features and vendor extensions; WeasyPrint
adds its own set. The lower-risk approach is to document each material
divergence and add a scoped option only when a concrete use case requires it
and the behavior is specified.

## Comparison results — WeasyPrint sample corpus (CORE-318)

Five adopted samples (CORE-317) rendered through TypeAnvil `8fde3ab`, Prince
16.2, and WeasyPrint 70.0 at their designed page sizes. Pixel diff: mean over
shared pages at 96 DPI. Timing: median of 3 runs after 1 warmup. Memory: max
peak RSS.

### Pages and mean pixel diff

| Fixture | TA pages | Prince pages | WeasyPrint pages | diff TA vs Prince | diff TA vs WeasyPrint |
|---|---|---|---|---|---|
| Invoice | 1 | 1 | 1 | 2.76% | 25.47% |
| Letter | 1 | 1 | 1 | 22.26% | 23.59% |
| Poster | 3 | 1 | 1 | 76.99% | 77.00% |
| Report | 13 | 8 | 8 | 35.85% | 33.00% |
| Ticket | 1 | 1 | 1 | 81.18% | 81.31% |

Prince and WeasyPrint agree with each other on every sample. TypeAnvil
over-paginates the poster (3 vs 1) and the report (13 vs 8) — the known
over-pagination family tracked in CORE-232/237/240 — which also explains the
large poster diff. The ticket's ~81% diff is geometry, not pagination: both
sides are one page, but this dense design exposes line-metric differences
between the engines.

### Timing (median s) and peak RSS (MiB)

| Fixture | TA | Prince | WeasyPrint |
|---|---|---|---|
| Invoice | 0.007 / 12.6 | 0.048 / 52.3 | 0.399 / 79.3 |
| Letter | 0.017 / 12.5 | 0.087 / 45.2 | 0.467 / 84.2 |
| Poster | 0.081 / 23.5 | 0.128 / 52.2 | 0.814 / 92.5 |
| Report | 0.018 / 19.4 | 0.194 / 58.4 | 0.933 / 153.5 |
| Ticket | 0.007 / 11.0 | 0.067 / 101.7 | 0.669 / 169.3 |

Reading the memory column: TypeAnvil is a small static Rust binary; Prince and
WeasyPrint ship their own runtimes (Python + Pango for WeasyPrint), so peak
RSS differences partly reflect that, not layout work alone.

## Implications

1. Keep CSS specifications as the governing source for Typeanvil behavior.
2. Use Prince AND WeasyPrint output for migration and visual comparison
   evidence. A result both independent engines agree on is a strong signal,
   and the open-source leg keeps the comparison runnable without a license.
3. Label each difference as spec-correct, engine gap, metric difference, or
   vendor-specific behavior after checking the relevant standard.
4. Pin the Prince AND WeasyPrint versions for any reproducible comparison and
   record them in generated artifacts. `benchmark.py` records both; the
   WeasyPrint wrapper prints its version to stderr.
5. In `harness triage`, WeasyPrint joins Chromium and Prince as an independent
   witness. When one print engine satisfies a reference the browser does not,
   check browser ground truth (wpt.fyi) before blaming layout.

## Sources

- [Prince paged media documentation](https://www.princexml.com/doc/paged/)
- [Prince supported CSS specifications](https://www.princexml.com/doc/css-refs/)
- [WeasyPrint documentation](https://doc.courtbouillon.org/weasyprint/stable/)
- [WeasyPrint installation](https://doc.courtbouillon.org/weasyprint/stable/first_steps.html#installation)
- [CSS Paged Media Level 3](https://www.w3.org/TR/css-page-3/)
- [CSS Generated Content for Paged Media Level 3](https://www.w3.org/TR/css-gcpm-3/)
- `docs/conventions/css-standards-alignment.md`
- `docs/specifications/visual-comparison-demo.spec.md`
- `docs/operations/benchmark-engines.md`
