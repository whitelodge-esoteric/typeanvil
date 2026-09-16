---
title: "PrinceXML compatibility matrix"
type: research
status: approved
owner: elijah
created: 2026-09-09
updated: 2026-09-15
sidebar_position: 1
tags: [prince, compatibility, migration, measurements]
---

# PrinceXML compatibility matrix

## What this is

A measured record of where Typeanvil and PrinceXML render differently, plus the
same comparison against Chromium. Use it during a migration to decide whether a
difference matters and who is right.

Every number here was produced by running the engines, not by reading
documentation.

## What it is not

This matrix does not describe a rendering mode. Typeanvil renders one way, and
that way follows the CSS specification. The entries below record where Prince
disagrees with the specification, where Typeanvil still has a gap, and where the
difference is too small to matter. A separate analysis covers why a user-facing
switch is the wrong shape for this:
`prince-compatibility-mode.md`.

## The four verdicts

Each entry carries one label.

| Label | Meaning | Action for a migrating user |
|---|---|---|
| **Spec-correct** | Typeanvil follows the CSS specification and Prince deviates. | Expect the difference. It is intentional and will not be "fixed". |
| **Engine gap** | Typeanvil is wrong, confirmed against a browser. | Expect parity after the tracked issue lands. |
| **Metric noise** | Sub-pixel or font-metric difference with no behavioral consequence. | None. |
| **Prince quirk** | Prince-specific behavior that no specification requires. | Do not expect parity; the engine will not chase it. |

## Corpus comparison (7 documents)

Renderings from `scripts/build-demo.sh`, geometry 5in x 3in at 96 DPI, Prince
16.2 (`demo/corpus/out/scoreboard.json`).

| Document | Typeanvil pages | Prince pages | Page count | Mean pixel diff |
|---|---|---|---|---|
| Letterhead | 8 | 8 | match | 7.77% |
| Quarterly Report | 10 | 10 | match | 7.87% |
| Prose Showcase | 11 | 11 | match | 10.52% |
| Academic Paper | 11 | 11 | match | 13.19% |
| Invoice | 5 | 5 | match | 19.17% |
| Float Showcase | 8 | 8 | match | 20.88% |
| Inventory Ledger (table stress) | 43 | 45 | **mismatch** | 20.94% |

Six of seven documents paginate identically. The residual is pixel-level.

The one page-count mismatch is a recorded Prince quirk. Prince renders that
table 7.7pt wider than its own content box (right edge 331.8pt against a 324pt
content edge, MediaBox 360x216), so it fits fewer rows per page. Typeanvil
fits the table to its content box, which is the spec-correct behavior.

Pixel residuals of this size come from many independent decisions rather than
one defect: font adoption, UA stylesheet values, hyphenation points, letter
spacing, and float placement. Each document's `expected_deltas` entry in
`demo/corpus/manifest.json` names the drivers it is known to have.

## Full WPT suite, all three engines

The same 283 print-reftests through each engine (each engine renders the test
and its reference, then the harness compares them).

| Engine | Passing |
|---|---|
| Chromium | 214 / 283 (75.6%) |
| Typeanvil | 128 / 283 (45.2%) |
| PrinceXML | 112 / 283 (39.6%) |

Chromium scores highest because WPT references encode browser behavior. That
makes Chromium the oracle and Prince a comparison target, as
`docs/conventions/css-standards-alignment.md` records.

## Agreement between the three engines

| Tests | Reading |
|---|---|
| 76 | All three pass. |
| 73 | Only Chromium passes. |
| 50 | No engine passes. |
| 43 | Typeanvil and Chromium pass; Prince does not. |
| **22** | **Only Typeanvil fails.** Both comparison engines pass. Treat as our bug. |
| 10 | Only Prince passes. |
| 5 | Only Typeanvil passes. Suspect a test pair that agrees for the wrong reason. |
| 4 | Typeanvil and Prince pass; Chromium does not. |

Two readings carry the most weight for a migration.

**Typeanvil fails and both comparison engines pass (22 tests).** Two independent
engines satisfy those references, so the gap is ours. The list is actionable —
see the next section.

**Typeanvil and Prince pass while Chromium fails (4 tests).** On these the
engine agrees with the commercial target against the browser. These are worth
reviewing before a fix assumes Chromium is always right.

## Actionable engine gaps (22 tests)

Both Chromium and Prince pass these; Typeanvil fails. Each is a real defect.

| Area | Tests | Tracked as |
|---|---|---|
| `contain:size` monolithic overflow | `monolithic-overflow-005/007/008/012` (4) | CORE-152 |
| Margin-box content | `margin-boxes/content-002/004/005/007` (4) | CORE-141, CORE-140 |
| Break behavior | `abspos-overflow-hidden-001`, `break-inside-avoid-multicol-001`, `break-nested-float-in-table-001`, `firefox-bug-2026295`, `overflowed-abs-pos-with-percentage-height`, `transform-023` (6) | CORE-127 tail |
| Page box, size, margin | `page-box-011`, `page-margin-001`, `page-margin-006` (3) | CORE-153 |
| Flex fragmentation | `flexbox .../080`, `.../060` (2) | CORE-114 |
| Bare-text page boundaries | `page-name-002` (1) | CORE-158 |
| Orthogonal writing mode | `page-name-orthogonal-writing-004` (1) | CORE-155 |
| Multicol | `auto-fill-auto-size-002` (1) | CORE-63 tail |

One name matches a browser bug number (`firefox-bug-2026295`) because the WPT
suite carries regression tests filed against browser defects. Chromium and
Prince both pass it, so it describes real behavior.

Note what the list does **not** contain: the `fixedpos-005/006/011` and
`fixedpos-with-abspos-with-link` tests. Those fail for the engine and for Prince
alike, so they sit in the Chromium-only bucket instead. Their divergence traces
to `vh` resolution and to fixture geometry, both covered by CORE-140.

## Where Chromium and Prince disagree

Eighty-three tests in the suite have exactly one comparison engine passing. They
are the places where "follow Chromium" and "follow Prince" give different
answers, and they divide sharply.

**Only Chromium passes (73 tests).** The reference encodes browser behavior, and
the engine follows Chromium here. These cluster in css-break (flex
fragmentation, table fragmentation, grid, transforms) and in the css-page
monolithic-overflow and margin families.

**Only Prince passes (10 tests).** On these the commercial target disagrees with
the browser. Every one of them is already explained by a tracked issue:

| Test | Why Prince passes |
|---|---|
| `margin-boxes/content-001`, `content-008`, `content-009` | Margin-box features still missing (CORE-141, CORE-140). |
| `margin-boxes/dimensions-010`, `inapplicable-properties` | Same margin-box layer. |
| `page-box-007` | `vh` resolution against the page box (CORE-140). |
| `page-left-right-001` | `:left`/`:right` margin handling (CORE-153). |
| `basic-pagination-003` | Declares its own `@page size`, which the harness overrides; Prince honors the fixture. |
| `flexbox .../045`, `.../065` | Flex fragmentation (CORE-114). |

Seven of the ten wait on features already ticketed. The engine will land on the
Chromium side for most of them once those features exist, because the
references encode browser behavior.

**Typeanvil and Prince pass while Chromium fails (4 tests).**
`page-name-abspos-002`, `page-name-flex-001`, `page-name-flex-002`,
`page-name-margin-001`. The engine sides with the commercial target against the
browser here. `page-name-abspos-002` is the pair partner of `page-name-003`
described in `docs/research/css-page/named-page-boundary-model.md`; Prince takes
the same side the engine does on that contradictory pair.

## Known divergences by behavior

| Behavior | Typeanvil | Prince | Verdict |
|---|---|---|---|
| `box-decoration-break` default | `slice` (CSS initial value) | `slice` since Prince 16; `clone` before | Spec-correct. Prince 16 moved to the standard. |
| Default `widows` | 1 | 1 (probed) | Spec-correct for migration: CSS's initial value is 2, and Prince uses 1. The engine matches Prince. |
| Print UA stylesheet | Prince-like fixed-point heading sizes and margins; body margin zeroed | Its own `html.css` | Spec-correct. Adopted from Prince in CORE-92 and CORE-95. |
| Footnote separator | None by default | None (probed 16.2) | Spec-correct. Prince has no default rule. |
| Table frozen column widths | Fit to content box | Freezes from first-page content and overflows its content box by ~7.7pt | Prince quirk. Reproducing the overflow would break the spec. |
| Hyphenation points | Liang patterns from the bundled dictionary | Proprietary dictionary | Metric noise. Break points differ; page counts still match on six of seven documents. |
| `target-text` / `target-counter` | Resolved through named counters | Supported | Spec-correct; unified with the bookmark machinery in CORE-129. |
| Orthogonal flow pagination | Suppressed inside orthogonal subtrees | Paginates | Spec-correct with a recorded deviation — see the named-pages spec Non-Goals. |

## Migration guidance

1. **Compare page counts first.** On the corpus, six of seven documents already
   match Prince. Page count is what breaks a print pipeline.
2. **Treat pixel residuals as expected.** A mean diff of 8-21% on
   text-heavy pages comes from font and hyphenation metrics, not from broken
   layout.
3. **Check this matrix before filing a parity issue.** An entry labelled
   spec-correct or Prince quirk will not change.
4. **For a divergence not in this list, run three-way triage:**
   `python -m harness triage <filter>`. It reports which engine is the odd one
   out and applies the reading rules above.

## Reproducing these numbers

```bash
# Corpus scores (needs Prince installed)
bash scripts/build-demo.sh

# Full three-way WPT comparison
python -m harness triage <filter>          # one filter
python -m harness --wpt .wpt run --engine cli --cli-cmd "scripts/render-prince.sh" \
  --report /tmp/prince.json --db /tmp/h.sqlite --artifacts /tmp/art-p   # full Prince leg
```

## References

- `docs/conventions/css-standards-alignment.md` — the governing rule.
- `prince-compatibility-mode.md` — why there is no user-facing switch.
- `docs/research/css-page/named-page-boundary-model.md` — the boundary study
  that produced the three-way method.
- `demo/corpus/out/scoreboard.json` and `demo/corpus/manifest.json` — corpus evidence.
- Prince 16 release notes — https://www.princexml.com/releases/16/
