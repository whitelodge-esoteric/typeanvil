---
title: Hyphenation density + line-break parity
slug: /specifications/hyphenation-line-break-parity
type: spec
status: draft
owner: elijah
created: 2026-08-20
updated: 2026-08-20
sidebar_position: 15
tags: [engine, typography, hyphenation, line-breaking, parity]
spec_id: hyphenation-line-break-parity
issue_id: CORE-97
applies_to: engine 0.x
dependencies: [typography-layer, line-height, visual-comparison-demo]
---

# Hyphenation density + line-break parity

## Overview

Every corpus fixture still diverges from Prince's line breaks. The 2026-08-20
demo triage (at `5afe57d`) measured the gap: on a packing probe Prince takes 9
hyphen breaks where TypeAnvil takes 2, and Prince fits 12–14 words/line vs
TypeAnvil's 10–13 (PR 49 body lines vs TA 56 on the margin-zeroed probe).
[CORE-90] and [CORE-94] closed the baseline-placement and justification-glue
halves; the density parity itself was never closed. This issue tunes the
hyphenation objective and verifies the line-box-height slope, so the corpus
page counts and per-page diffs converge on Prince.

**CORE-97 probe (2026-08-20, this branch):** a 5-page probe (10pt Arial,
justified, `hyphens: auto`, identical text at `line-height` 1.2/1.4/1.5/1.6,
288pt content width) rendered through both engines. Findings:

1. **Line-box height is NOT a divergence at declared values.** Line pitch
   (baseline-to-baseline) is exactly `font-size × factor` in BOTH engines:
   12.0/14.0/15.0/16.0pt at 1.2/1.4/1.5/1.6. TypeAnvil's `y += lh` matches
   Prince's line-box height exactly. The residual prose 10v11 gap at 1.6 is
   therefore break-DENSITY driven (more hyphen breaks → more lines → more
   pages), not line-box slope. The `line-height: normal` path is untouched
   (out of scope; corpus pins explicit values).
2. **Break positions still diverge.** On one probe page both engines produce
   9 lines and 5 hyphen breaks, but at different places. TypeAnvil packed
   line 1 to 9 words and hyphenated `demon|strating`; Prince broke line 1
   after 7 words and did not hyphenate `demonstrating`. Syllable choices
   differ (`hyphen|ation` vs `hy|phenation`, `jus|tification` vs a space
   break). Every observed break in both engines is a valid Knuth-Liang
   boundary (verified against `hypher 0.1.7` English patterns for all probe
   words — e.g. `hyphenation` → `hy|phen|ation`, so TypeAnvil's
   `hyphen|ation` is the `phen|` boundary, not an arbitrary cut).
3. **The hyphen glyph extracts as a placeholder (`￾`) at line ends in BOTH
   engines' text layer** (pypdfium2). Equal behavior — not a TypeAnvil
   defect; hyphen counting must count line-end placeholders, not `-`.

The two fixture directions are opposite (prose needs MORE lines to reach
11=11; float-showcase needs FEWER pages to reach 8=8), so calibration must
find one parameter set that satisfies all three Done targets — a single knob
sweep per fixture will not do.

**Path chosen: engine-internal tuning, no stylo dependency.** The `hyphens`
property is already a manual author-CSS pass (`Hyphens::Auto`, `css.rs`),
not stylo; `line-height` is stylo (verified CORE-74, 2026-08-18). This issue
changes only the Knuth-Plass objective constants in `typography.rs` — no new
CSS properties, no stylo surface.

## Goals / Non-Goals

**Goals**

- Close the hyphenation-density gap: TypeAnvil's hyphen break rate on the
  calibration probe within ±1 of Prince's (packing probe: PR 9, TA 2 today).
- Preserve line-box-height parity at declared line-heights (verified equal by
  the probe; add a regression probe so it stays equal).
- Enforce a left-hyphenation minimum of 2 characters (TeX default
  `\lefthyphenmin=2`; Prince's smallest observed hyphenation prefix on the
  probe is 2 — `hy|phenation`, `un|comfortable`).
- Keep the existing ≥5-char minimum word length for hyphenation (matches
  TeX's effective minimum given left=2/right=3).
- Meet the Done criteria via a fresh `build-demo.sh` run: prose 11=11 pages,
  float-showcase 8=8 pages, paper per-page diff < 15%.
- Determinism unchanged: identical input → byte-identical PDF.

**Non-Goals** (deferred; scope stays honest)

- Exact Prince dictionary parity. Prince ships a proprietary hyphenation
  dictionary; the engine uses `hypher`'s public Knuth-Liang English patterns.
  We match observable break DENSITY and corpus page counts, not per-word
  dictionary equality.
- The existing-hyphen double-hyphen bug (`page--` vs `page-`, letterhead
  p1) — separate ticket [CORE-98].
- `line-height: normal` used-value parity (CORE-74 fixed `normal → 1.2`;
  corpus fixtures pin explicit values; Prince's `normal` is font-metric-
  derived and differs from 1.2 — out of scope).
- Non-English hyphenation, non-Latin scripts, `hyphenate-character`,
  hyphenation-quality longhands.
- A right-hyphenation minimum (`\righthyphenmin`): no probe evidence that
  Prince enforces one beyond the Liang patterns; adding it would remove
  break options and fights the prose 10→11 direction.

## Behavior

The engine shall:

1. Keep the Knuth-Plass item-stream hyphenation mechanism (`hypher` English
   patterns → `Item::Penalty` between syllable boxes, `push_word`).
2. Offer no hyphenation break that leaves fewer than 2 characters before the
   break (left-hyphenation minimum 2). A syllable boundary with a 1-character
   left syllable is not emitted as a penalty.
3. Offer no hyphenation for words shorter than 5 characters (unchanged).
4. Set `HYPHEN_PENALTY` (typography.rs) to the calibrated value, chosen by
   the calibration procedure in §Calibration, verified against the corpus
   Done targets, and recorded in §References. The value is a single named
   constant; the demerit formula `(1+b)² + penalty²` is unchanged.
5. Not change the line-box-height computation (`y += lh`, `line_height` per
   element): declared line-heights scale as `font-size × factor`, which the
   probe verified matches Prince exactly.
6. Not change the justification glue model (stretch 1/2, shrink 1/3 of the
   space advance; badness `100·|r|³` clamped; CORE-94's breakpoint-glue and
   measure-fill behavior).
7. Remain deterministic: all new constants and guards are pure functions of
   (text, font, width); no new nondeterminism sources.

## Calibration

The penalty is tuned by probe, not by guess:

1. **Calibration probe** (`demo/probes/`): the packing paragraph from the
   triage (float-showcase text; the probe that measured PR 9 vs TA 2) plus
   the CORE-97 probe page above, rendered through both engines at demo
   geometry (5in × 3in, 0.5in margins → 288pt content).
2. **Measurement**: pypdfium2 text-layer per-page line counts and line-end
   hyphen counts (count the `￾` placeholder, not `-`).
3. **Sweep**: render TypeAnvil at candidate penalty values; record
   (penalty, hyphen count, line count, per-page diff vs Prince) for each
   corpus fixture.
4. **Accept**: the penalty that meets all three Done targets with margin
   (prose 11=11, float 8=8, paper per-page < 15%). If no single value
   satisfies all three, relax in the order the issue defines (per-page diff
   is the tie-breaker) and record the residual honestly in the spec —
   never silently accept a fixture.

## Interfaces

### `engine/src/typography.rs`

```rust
/// The K-P hyphen penalty. Calibrated against Prince (CORE-97): the 135
/// (Typst/TeX) default made the DP prefer clean space breaks, under-
/// hyphenating vs Prince (~4× on the triage packing probe).
const HYPHEN_PENALTY: f64 = /* calibrated value */;

/// Minimum characters that must precede a hyphenation break
/// (TeX `\lefthyphenmin`; Prince's smallest observed prefix is 2).
const LEFT_HYPHEN_MIN: usize = 2;
```

`push_word` gains the `LEFT_HYPHEN_MIN` guard: a syllable whose byte length
(plus the accumulated left-syllable lengths) would leave fewer than
`LEFT_HYPHEN_MIN` characters before the break is not a break opportunity —
the word's leading 1-character syllables merge into the first box.

No changes to `ComputedStyle`, `layout.rs` line-box math, or the glue model.

## Acceptance Criteria

1. **Penalty behavior (engine test, `typography.rs` unit)** — Given a
   justified paragraph containing a long hyphenatable word and `hyphens:
   auto`, when the word cannot fit on the line without hyphenation, then the
   word breaks at a Liang boundary with a trailing hyphen glyph, and the
   break's demerit uses the calibrated `HYPHEN_PENALTY` (assert the chosen
   break count equals the calibrated expectation).
2. **Left minimum (engine test)** — Given a word whose first Liang syllable
   is 1 character (e.g. `documentation` → `doc|u|men|…`), when `hyphens:
   auto`, then no break opportunity exists at `u|`; the word's first box
   spans `docu|` at the earliest.
3. **Line-box regression (probe script)** — Given the CORE-97 probe page,
   when rendered by both engines, then line pitch is `font-size × factor`
   for line-heights 1.2/1.4/1.5/1.6 in both (12/14/15/16pt), recorded as a
   checked probe output.
4. **Density parity (probe script)** — Given the calibration probe, when
   rendered by both engines, then TypeAnvil's line-end hyphen count is
   within ±1 of Prince's.
5. **Done targets (demo pipeline)** — Given a fresh `build-demo.sh` run
   (`PY=/Users/elijah/workspace/typeanvil/.venv/bin/python`), then prose
   11=11 pages, float-showcase 8=8 pages, and paper per-page diff < 15%,
   with the regenerated scoreboard committed (`git add -f demo/out/`).
6. **WPT gate** — Given the harness run (css-page 367, css-break 640), then
   0 regressions vs the pre-change baseline (fixed − regressed ≥ 0; ship
   only 0-regression states).

## Edge Cases

- **1-character left syllable** (`documentation`, `administration`): merged
  into the first box; the word may still hyphenate at later boundaries.
- **Word with no breakable boundary after the guard** (e.g. `ugly`):
  single box, no hyphenation — unchanged fallback.
- **The paragraph-final line** never hyphenates for fit (K-P final-line
  exemption, unchanged).
- **Line-end hyphen glyph** extracts as `￾` (U+FFFE placeholder) in both
  engines' text layer — hyphen counting uses the placeholder, never `-`.
- **A word already containing a hyphen** (`page-margin`): out of scope
  (CORE-98 double-hyphen bug tracked separately); this spec does not change
  existing-hyphen handling.

## References

- Triage evidence: `demo/TRIAGE.md` §Driver 2 (packing probe PR 9 vs TA 2,
  PR 49 vs TA 56 body lines).
- CORE-94 (justified line breaking — breakpoint glue, measure fill):
  `docs/specifications/typography-layer.spec.md` §Behavior 9/11.
- CORE-90 (line-box height divergence — baseline placement, slope cross):
  `docs/specifications/line-height.spec.md` §Behavior 10, criteria 8–9.
- CORE-98 (double hyphen at existing hyphen breaks): related ticket,
  blocked by this spec's fixture evidence.
- Probe artifacts (2026-08-20): `/tmp/core97/probe.html` (line-box +
  density probe), `/tmp/core97/measure.py` (pypdfium2 line/pitch/hyphen
  extraction), `/tmp/hyphertest` (syllable-pattern verification).
- TeX hyphenation minima: Knuth, *The TeXbook* — `\lefthyphenmin=2`,
  `\righthyphenmin=3` (left only adopted here; see Non-Goals).

[CORE-90]: https://linear.app/whitelodge/issue/CORE-90
[CORE-94]: https://linear.app/whitelodge/issue/CORE-94
[CORE-98]: https://linear.app/whitelodge/issue/CORE-98
