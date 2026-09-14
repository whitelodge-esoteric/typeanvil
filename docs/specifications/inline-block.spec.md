---
title: Inline-block atomic boxes
slug: /specifications/inline-block
type: spec
status: approved
owner: Elijah Boston
created: 2026-08-25
updated: 2026-09-14
sidebar_position: 24
tags: [css-display-3, layout, core-120]
spec_id: SPEC-inline-block
issue_id: CORE-120
applies_to: engine/src/layout.rs, engine/src/css.rs
dependencies:
  - SPEC-fragmentation-core
---

# Inline-block atomic boxes (CORE-120)

## Overview

`display: inline-block` is an outer display that makes the box inline-level
while its children lay out with block flow (css-display-3 §2.3 / CSS 2.1
§9.2.4). The engine currently has no `InlineBlock` display variant: stylo
resolves `inline-block` and the element folds into the parent's text run,
so its border, padding, background, and box geometry never paint. Eight WPT
flexbox rows (081a-d / 082a-d) use inline-block references and fail only on
this residual.

## Goals

- Atomic inline-level boxes that paint their own border/padding/background.
- Consecutive inline-block siblings flow side by side on one line.
- Baseline alignment with surrounding inline text.

## Non-Goals

- Inline fragmentation of an over-tall inline-block across pages (atomic
  monolithic placement; defer whole box).
- `display: inline` blockification rules beyond what stylo already computes.
- Inline-table / inline-list-item variants.

## Behavior

1. `ComputedStyle` shall expose `Display::InlineBlock` when the author sets
   `display: inline-block` (stylo `DisplayOutside::Inline` +
   `DisplayInside::FlowRoot`).
2. `collect_items_rec` shall emit an inline-block child as its own item so
   it reaches the block layout path (never folded into the parent text run).
3. An inline-block shall be laid out through the block path at its resolved
   width (`width`, `width_percent`, or shrink-to-fit from min/max content),
   honoring border, padding, background, and margin.
4. Consecutive inline-block items with room remaining on the current line
   shall place horizontally next to each other; an item that does not fit
   wraps to a new line.
5. An inline-block whose height exceeds the remaining page space shall be
   deferred whole to the next fragmentainer (no mid-box split).
6. The inline-block's baseline shall be the baseline of its last line box
   (css-display-3 §2.3); a box with no line boxes aligns its bottom margin
   edge with the text baseline.
7. Surrounding text lines shall wrap around placed inline-blocks using the
   same intrusion mechanism as floats.
8. The line box shall grow to CONTAIN an atomic box that is taller than the
   current line (CORE-171). Aligning a tall box's bottom margin edge to a
   short line's baseline places the box above the block's content top: a
   `100px x 50px` empty inline-block as the first line painted only a 13.9pt
   slice at the page edge (Chromium paints it in full from the page top). The
   shift that performs the baseline alignment shall therefore be clamped so
   the box never starts above the line top; boxes that fit the line keep their
   baseline alignment unchanged.
9. An inline-block that follows bare text on the same line shall start at that
   text's advance width (CORE-172), not at the line origin. The bare-text path
   shall hand the end of each placed line to the atomic pen state, and the
   `fits_line`/wrap decision shall use that same pen position. This is what
   `css-page/margin-boxes/content-003`'s reference needs: `Hello` followed by a
   `100x50` inline-block paints the box at x 0..99 over the text instead of
   after it (Chromium: 40..139).

10. A line whose atomic box is TALLER than the strut shall move its baseline
   DOWN to the box's bottom margin edge (CORE-173), and the text fragments
   already placed on that line shall ride the shift. css2 §10.8.1 gives a
   replaced inline box with no in-flow line boxes its bottom margin edge as the
   baseline, so the line box grows to that ascent: the box spans
   `line_top..line_top + h` and the baseline sits at `line_top + h`. Without
   this the text stayed at the line top while the box filled the line
   (Chromium, `content-003`'s reference: text ink y 39..53 with the box at
   0..49; the engine drew the text at 3..18). A line whose atomics fit the
   strut keeps today's baseline alignment.

11. Collapsible white space before an inline-level atomic on the SAME line shall
   survive: the atomic shall start one space past the text's advance end
   (CORE-174). css-text-3 §4.1.1 removes white space at a LINE BREAK, and a
   space that the atomic continues past is not at a break. `build_items` leaves
   its word loop as soon as a white-space run reaches the end of the text, before
   pushing the inter-word glue, so that run reached no line's natural width and
   the atomic restarted at the text's INK end (`Hello` then a `100x50`
   inline-block put the box at x=36 where Chromium puts it at 40). The run's
   advance shall be recorded on the final line of the paragraph
   (`LineResult::trailing_space`, the width of ONE space — the run collapses to
   one) and added to the atomic pen, and it shall stay out of the line's own
   drawn width so justification, alignment and measurement do not see it. A
   white-space run holding a forced break is a break, not space, and a trailing
   space at a line END still collapses: an atomic that wraps to the next line
   starts at the line origin.

## Interfaces

- `css.rs`: add `Display::InlineBlock`; map stylo
  `(Inline, FlowRoot)` → `InlineBlock`.
- `layout.rs`: new `Item::Atomic(child)` (or reuse `Item::Block` with an
  inline-flow flag) handled in the bare-text/line placement loop;
  `place_atomic_box` helper returns the advance width and baseline offset.

## Acceptance Criteria

- Given a paragraph with two 50%-wide bordered inline-block divs, when
  rendered, then both paint their borders side by side on one line.
- Given an inline-block taller than the remaining page space, when rendered,
  then the box appears whole on the following page.
- Unit test: an inline-block child paints its border as a box (pixel scan or
  fragment-tree assertion), not folded into the parent text run.
- Given `Hello` followed by a 100x50 inline-block on one line, when rendered,
  then the text run's baseline is the box's bottom margin edge
  (`core173_tall_inline_block_baseline.rs::tall_inline_block_moves_the_line_baseline_down`).
- Given `Hello` followed by a `100x50` inline-block on the same line, when
  rendered, then the box's left edge is at or beyond the text's right edge and
  the box does not overlap the line origin
  (`core172_inline_block_after_text.rs::inline_block_after_text_starts_after_it`;
  confirmed to fail without the fix with `box x=0, text right edge=27.3`).
- Given a `100px x 50px` empty inline-block as the first line of a block, when
  rendered, then its background fragment is 75pt x 37.5pt at a page-absolute y
  of 0 or more — the box is fully visible, not a slice above the page top
  (`core171_inline_block_height.rs::inline_block_declared_height_paints_full_height`).
  The same box as `display:block` stays 75pt x 37.5pt
  (`core171_inline_block_height.rs::block_declared_height_paints_full_height`).
- Given `Hello` immediately followed by a `100x50` inline-block, when rendered,
  then the box starts at the text's advance end; and given the same markup with a
  collapsible run (`Hello`, newline, indentation) before the box, then the box
  starts exactly one inter-word space further right, while the text's own advance
  end is unchanged
  (`core174_space_before_atomic.rs::collapsible_space_before_box_shifts_it_by_one_space`;
  confirmed to fail without the fix with
  `got gap=0pt, tight x=27.345703125, spaced x=27.345703125`).
- Harness gate: css-break flexbox bucket fixed − regressed ≥ 0, targeting
  rows 081a-d / 082a-d.

## Edge Cases

- `width` exceeding the line: box takes full width alone.
- Nested inline-block inside inline-block: inner lays out via the same path.
- `break-before`/`break-after` on an inline-block propagate like other
  atomic boxes (already parsed by the breaks pass).
- Whitespace between inline-blocks collapses per normal inline whitespace
  processing.

## References

- css-display-3 §2.3; CSS 2.1 §9.2.4, §10.3.9 (shrink-to-fit width).
- Linear CORE-120; classification table in
  `flexbox-fragmentation.spec.md` (2026-08-24); source issue CORE-114.
