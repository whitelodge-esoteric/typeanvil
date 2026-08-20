---
title: Hyphenation density + line-break parity
slug: /specifications/hyphenation-line-break-parity
type: spec
status: approved
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
halves; the density parity itself was never closed.

**CORE-97 probe results (2026-08-20, this branch) — the ticket's hypotheses
did NOT hold; the real drivers were found by direct measurement:**

1. **Line-box height is NOT a divergence at declared values.** A probe page
   (10pt Arial, justified, `hyphens: auto`, identical text at `line-height`
   1.2/1.4/1.5/1.6, 288pt content width) renders with line pitch EXACTLY
   `font-size × factor` in BOTH engines (12.0/14.0/15.0/16.0pt). TypeAnvil's
   `y += lh` matches Prince's line-box height exactly. The residual prose
   10v11 gap at 1.6 is NOT a line-box slope problem.
2. **Hyphen density barely moves page counts.** A full `HYPHEN_PENALTY`
   sweep (5, 10, 15, 20, 30, 50, 100, 135) changed ZERO page counts on
   prose/float-showcase/paper (all flat at 10/10/11). The remaining gap is
   breakpoint CHOICE (the K-P glue model), not hyphen count. `HYPHEN_PENALTY`
   is kept at the Typst/TeX default 135.
3. **All observed TypeAnvil hyphen breaks are valid Liang boundaries.**
   `hyphen|ation` is the `phen|` boundary (hypher English: `hy|phen|ation`),
   `jus|tification` the `fi|` boundary — verified for every probe word. The
   line-end hyphen extracts as U+FFFE (`￾`) in BOTH engines' text layer —
   equal behavior, not a TypeAnvil defect; hyphen counting must count the
   placeholder, never `-`.
4. **The real prose driver was a SILENT TEXT-LOSS BUG (found this branch).**
   `apply_orphans_widows` can return `split > li` (lines placed) when the
   orphans bump would fix an impossible violation — e.g. a paragraph starting
   with 0 lines fitting at the page bottom gets `split = first+orphans` even
   though nothing was placed. The outgoing break token then claims those
   lines consumed and the next fragmentainer resumes PAST them. Prose's
   `.no-hyph` paragraph silently dropped its first two lines (page 9 started
   at paragraph line 3). Fix: `let split = split.min(li)` in `layout.rs` —
   never consume more lines than were placed (css-break-3 §4.4 drops the
   constraint when it cannot be honored). **Prose 10 → 11 pages.**
5. **The real float driver is the whole-box deferral.** Prince fragments the
   floated figure across pages (title + body on p2, caption on p3);
   TypeAnvil defers the ENTIRE box when `y + fh > bottom_limit`, wasting
   ~123pt of p2. Probed: with the float placed-and-fragmented instead, the
   p2 structure matches Prince but the cascade runs +1 page (10 → 11), so
   the change was REVERTED — float fragmentation needs its own ticket.
   Filed as CORE-101.
6. **Ragged-right tie-break fixed.** Non-justified lines are all "free" when
   underfull, so in the K-P DP every break tied and the SHORTEST fit won —
   paper's h1 wrapped as `"On the"` + `"Texture…Study"` instead of Prince's
   `"On the Texture of Justified Text: A Field"` + `"Study"`. `<=` in the DP
   keeps the longest fit on exact ties (forward loop, last tie = longest
   line). Paper p1 diff 13.6% → 9.8%.
7. **Widows default is 2 in CSS but 1 in Prince.** Direct probe: a paragraph
   splitting 10+1 lines stays 10+1 in Prince (widows effectively 1);
   TypeAnvil's CSS-initial 2 pulled the break back to 9+2. The engine default
   is now `widows: 1` (explicit author CSS still overrides; WPT widows tests
   set explicit values, so the change only affects the unset case).

**Path chosen: engine-internal tuning, no stylo dependency.** The `hyphens`
property is already a manual author-CSS pass (`Hyphens::Auto`, `css.rs`),
not stylo; `line-height` is stylo (verified CORE-74, 2026-08-18). This issue
changes only the Knuth-Plass objective, the line-resume bookkeeping, and the
widows default in `typography.rs` / `layout.rs` / `css.rs` — no new CSS
properties, no stylo surface.

## Goals / Non-Goals

**Goals**

- Fix the orphans/widows resume text-loss bug (silent dropped lines at page
  breaks) — the real prose 10v11 driver.
- Match Prince's widows behavior for unset paragraphs (allow a 1-line widow).
- Fix the ragged-right tie-break so non-justified lines pack to the longest
  fit (headings wrap like Prince).
- Enforce a left-hyphenation minimum of 2 characters (TeX `\lefthyphenmin`;
  Prince's smallest observed hyphenation prefix on the probe is 2 —
  `hy|phenation`, `un|comfortable`).
- Meet the Done criteria via a fresh `build-demo.sh` run: prose 11=11 pages
  (MET), float-showcase 8=8 (NOT met — 10 pages; see Non-Goals), paper
  per-page diff < 15% (NOT met — ~22%; break positions still diverge).
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
- **Float whole-box deferral → fragmentation parity (CORE-101, filed from
  this branch).** Prince fragments an over-tall float; TypeAnvil defers the
  whole box. The placed-and-fragmented experiment matched Prince's p2 but
  regressed total pages 10 → 11 (the fragmented continuation + pullquote
  cascade), so it was reverted; float fragmentation is its own ticket. Until
  CORE-101 lands, float-showcase stays 10 vs Prince 8.
- Non-English hyphenation, non-Latin scripts, `hyphenate-character`,
  hyphenation-quality longhands.
- A right-hyphenation minimum (`\righthyphenmin`): no probe evidence that
  Prince enforces one beyond the Liang patterns; adding it would remove
  break options.

## Behavior

The engine shall:

1. Keep the Knuth-Plass item-stream hyphenation mechanism (`hypher` English
   patterns → `Item::Penalty` between syllable boxes, `push_word`).
2. Offer no hyphenation break that leaves fewer than 2 characters before the
   break (left-hyphenation minimum 2). A syllable boundary with a 1-character
   left syllable is not emitted as a penalty.
3. Offer no hyphenation for words shorter than 5 characters (unchanged).
4. Keep `HYPHEN_PENALTY` at 135 (Typst/TeX default). The 2026-08-20 sweep
   (5..=135) showed the corpus page counts and per-page diffs are INSENSITIVE
   to this value; the demerit formula `(1+b)² + penalty²` is unchanged.
5. Not change the line-box-height computation (`y += lh`, `line_height` per
   element): declared line-heights scale as `font-size × factor`, which the
   probe verified matches Prince exactly.
6. Not change the justification glue model (stretch 1/2, shrink 1/3 of the
   space advance; badness `100·|r|³` clamped; CORE-94's breakpoint-glue and
   measure-fill behavior).
7. Never consume more lines at a fragmentainer break than were actually
   placed: `apply_orphans_widows`'s result is clamped with `split.min(li)`
   (CORE-97 orphans/widows text-loss fix).
8. Default `widows` to 1 for unset paragraphs (Prince parity, probed); any
   explicit author `widows` declaration overrides.
9. Break non-justified lines at the LONGEST equal-cost fit: the K-P DP uses
   `<=` on the tie compare so the last (longest) equal candidate wins
   (CORE-97 ragged tie-break fix).
10. Remain deterministic: all changes are pure functions of (text, font,
    width); no new nondeterminism sources.

## Calibration

The penalty was tuned by probe, not by guess — and the probe answered
"don't touch it":

1. **Calibration sweep** (2026-08-20): `HYPHEN_PENALTY` ∈ {5, 10, 15, 20,
   30, 50, 100, 135}, prose / float-showcase / paper rendered at demo
   geometry (5in × 3in, 0.5in margins → 288pt content). Result: **all page
   counts flat** (prose 10, float 10, paper 11) across the entire sweep —
   the K-P DP prefers space breaks regardless of penalty because the
   space-break lines score within the glue tolerance. The knob was reverted
   to 135.
2. **Line-box probe** (`/tmp/core97/probe.html`, 2026-08-20): line pitch is
   `font-size × factor` in both engines at 1.2/1.4/1.5/1.6 — no slope work
   needed; recorded as a regression probe (AC 3).
3. **Widows probe** (2026-08-20): Prince allows a 1-line widow
   (`[10,10,10,10,1]`); the engine's default moved 2 → 1 (AC 5).
4. **Corpus acceptance** (fresh `build-demo.sh`): prose 11=11 (MET), float
   10 vs 8 (NOT met — CORE-101), paper per-page ~22% (NOT met — break
   positions; the structural h1/abstract splits now align).

## Interfaces

### `engine/src/typography.rs`

```rust
/// The K-P hyphen penalty (Typst's default, from Knuth-Plass §hyphenation).
/// CORE-97 swept 5..=135 and found the corpus page counts and per-page diffs
/// are INSENSITIVE to this value; kept at the documented default.
const HYPHEN_PENALTY: f64 = 135.0;

/// Minimum characters that must precede a hyphenation break
/// (TeX `\lefthyphenmin`; Prince's smallest observed prefix is 2).
const LEFT_HYPHEN_MIN: usize = 2;

/// Test-facing break-offset list: Liang syllable boundaries with at least
/// LEFT_HYPHEN_MIN chars on the left, for words >= 5 chars.
pub fn allowed_hyphenation_breaks(word: &str) -> Vec<usize>;
```

`push_word` uses `hyphenation_break_indices(&syllables)` (the internal
variant of `allowed_hyphenation_breaks`) to decide which syllable boundaries
become `Item::Penalty` candidates.

### `engine/src/layout.rs`

```rust
// In the run-break branch, after apply_orphans_widows:
let split = split.min(li); // NEVER consume more lines than were placed (CORE-97)
```

### `engine/src/css.rs`

```rust
// Defaults (both the initial-style and stylo-converted constructors):
widows: 1, // Prince parity (probed); CSS initial is 2. Author CSS overrides.
```

No changes to `ComputedStyle`'s public surface beyond the existing
`orphans`/`widows` fields.

## Acceptance Criteria

1. **Orphans/widows text preservation (engine test)** — Given a paragraph
   that starts at the very bottom of a page (0 lines fit), when laid out,
   then NO source text is lost: the paragraph's first lines appear on the
   NEXT page (regression: prose page 9 previously started at line 3).
2. **Left minimum (engine test)** — Given a word whose Liang syllables
   include a 1-character syllable, when `hyphens: auto`, then no break
   opportunity exists with fewer than 2 characters on the left; a
   2-character-left boundary IS still a break (`allowed_hyphenation_breaks`
   invariant).
3. **Line-box regression (probe script)** — Given the CORE-97 probe page,
   when rendered by both engines, then line pitch is `font-size × factor`
   for line-heights 1.2/1.4/1.5/1.6 in both (12/14/15/16pt).
4. **Widows parity (probe script)** — Given a paragraph whose natural split
   leaves a 1-line tail, when rendered by both engines with no explicit
   widows declaration, then both keep the 1-line tail (10+1, not 9+2).
5. **Done targets (demo pipeline)** — Given a fresh `build-demo.sh` run
   (`PY=/Users/elijah/workspace/typeanvil/.venv/bin/python`), then: prose
   11=11 pages (MET); float-showcase 8=8 (NOT met — 10, blocked by CORE-101);
   paper per-page diff < 15% (NOT met — ~22%). The scoreboard regenerated
   with `git add -f demo/out/`.
6. **WPT gate** — Given the harness run (full print-reftest suite), then 0
   regressions vs the pre-change baseline (fixed − regressed ≥ 0; ship only
   0-regression states). The widows default and ragged tie-break change
   unset-case behavior only; explicit-value tests must be unaffected.

## Edge Cases

- **1-character left syllable** (`documentation`, `administration`): merged
  into the first box; the word may still hyphenate at later boundaries.
- **Paragraph starting at the page bottom (0 lines fit)**: the orphans
  constraint is dropped (cannot be honored); the whole paragraph moves to
  the next page — text is never lost (AC 1).
- **1-line widow without an explicit declaration**: allowed (widows=1
  default); an explicit `widows: 2` still pulls the break back.
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
- CORE-101 (float fragmentation parity — the float-showcase 8=8 blocker):
  filed from this branch; Prince fragments an over-tall float, TypeAnvil
  defers the whole box.
- Probe artifacts (2026-08-20): `/tmp/core97/probe.html` (line-box +
  density probe), `/tmp/core97/measure.py` (pypdfium2 line/pitch/hyphen
  extraction), `/tmp/hyphertest` (syllable-pattern verification),
  `/tmp/core97/widows.html` (widows behavior probe).
- TeX hyphenation minima: Knuth, *The TeXbook* — `\lefthyphenmin=2`,
  `\righthyphenmin=3` (left only adopted here; see Non-Goals).

[CORE-90]: https://linear.app/whitelodge/issue/CORE-90
[CORE-94]: https://linear.app/whitelodge/issue/CORE-94
[CORE-98]: https://linear.app/whitelodge/issue/CORE-98
[CORE-101]: https://linear.app/whitelodge/issue/CORE-101
