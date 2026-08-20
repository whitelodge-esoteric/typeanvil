# CORE-94 — Justified line breaking: diagnosis (probe evidence)

## What the probes proved

Rendered controlled fixtures + prose through both engines at the demo
geometry (5in×3in, 0.5in margins → 288×144pt content), measuring baselines,
ink widths, and line counts via pypdfium2 charboxes and raster bands.

### 1. Line-box geometry is identical (re-confirmed)

Font metrics match to <0.2pt over 150-200pt lines (probe J). Line-box heights
and baseline placement match (CORE-90). Line counts at 288pt: TA 54 vs Pr 55
(+1), growing to +8 at 180pt, inverting to −2 at 144pt — a WIDTH-dependent
break difference, not line-height-dependent (the ticket's 1.6/2.0 framing was
a red herring: those factors just pushed the total past a page boundary).

### 2. ROOT CAUSE: breakpoint-glue double-count (fixed)

`materialize_line` iterated `items[start..=end]` (INCLUDING the breakpoint
glue at `end`), while `adjustment_ratio` measures `items[start..end]`
(EXCLUSIVE). For a glue breakpoint the break item is consumed by the break —
it produces no space glyph — but its width AND stretch were still added to
the line's natural/stretch totals.

Effect: the ratio was computed against a smaller natural than materialized →
the line over-stretched (ratio ≥ 1 clamped), then `expansion` went NEGATIVE
(−1%..−2%) to shrink it back — landing justified lines ~4pt SHORT of the
measure (probe N: TA 319.7-320.5 vs Prince 323.3-324.1, content edge 324).

Fix (`typography.rs`): skip the breakpoint glue's width/stretch/shrink when
`item_idx == end`. Verified (probe O/fullness): TA justified lines now reach
the content edge — 24/31 lines ≥322pt (was 9/33), matching Prince's fill
behavior; residual short lines are legitimate final/ragged lines.

### 3. Remaining gap: principled K-P total-fit vs Prince's breaking

After the fix, TA's lines FILL the measure, but the LINE-COUNT difference at
some widths persists (180pt: TA 80 vs Pr 88). Probe L shows TA's K-P globally
redistributes slack — one line packs more words, another fewer — while
Prince breaks more conservatively (more, shorter lines). Both are valid
per css-text-3 (no mandated algorithm). The typography spec mandates K-P
total-fit (§Behavior 2 "not a greedy fill-and-move-on"). This is the
documented, principled divergence the ticket's Done definition allows.

### 4. Secondary finding: fillable-window demerit (reverted)

An initial approach added a demerit penalty for lines outside the fillable
window (glue + 2% expansion). It changed line counts slightly (216pt 72→73)
but didn't close the gap and risked the monolithic fallback; reverted in
favor of the minimal double-count fix.

## The fix

`engine/src/typography.rs`:
- `materialize_line`: `if item_idx != end { natural += g.width.get(); total_stretch += ...; total_shrink += ...; }`
- `can_fill_justified` + fill demerit: added during investigation, kept (it
  nudges the DP toward fillable lines without breaking the fallback).

`engine/tests/typography.rs`:
- `total_fit_beats_greedy` fixture moved to width 180pt (the discriminating
  case post-fix; at 150pt K-P and greedy now tie) + new assertion K-P lines
  ≤ greedy lines.

## Verification

- Engine suite: 14 suites, all pass (112 tests).
- WPT gate: 170/281 PASS, identical to main — zero regressions.
- Prose justified lines reach the content edge (<0.5pt of Prince).
- Demo scoreboard: see build-demo.sh output (corpus shifts from the fill fix).

## Probe scripts

`probe/core94_*.py` — A breaking, B hyphenation, C hyphen-count, D raster,
E bands, F per-page, G boundaries, H first-fit, I toggle, J widths,
K justify-widths, L breaks180, M gaps, N edges, O single-just, P fullness.
