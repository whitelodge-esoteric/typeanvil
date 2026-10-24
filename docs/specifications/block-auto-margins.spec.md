---
title: "Block Auto Margins — horizontal auto margins on definite-width blocks"
slug: /specifications/block-auto-margins
type: spec
status: draft
owner: maintainers
created: 2026-10-24
updated: 2026-10-24
sidebar_position: 41
tags: [engine, layout, margins, css2]
spec_id: block-auto-margins
issue_id: CORE-564
applies_to: engine 0.x
dependencies: [min-max-sizing, inline-base-direction]
---

# Block Auto Margins

## Overview

An in-flow block with a definite width and an `auto` left margin is
positioned by the free space in its containing block (CSS 2.2 §10.3.3).
Stylo computes `auto` margins to 0, so layout reads the
`margin_left_auto` / `margin_right_auto` flags on `ComputedStyle`.

## Goals

1. Right-align a definite-width block with a lone `margin-left: auto`.
2. Center a definite-width block with `margin-left: auto; margin-right: auto`.

## Non-Goals

1. Vertical (`margin-top/bottom: auto`) margins in normal flow (they compute
   to 0 per CSS 2.2 §10.6.3).
2. rtl and the vertical writing-mode logical frame keep their existing
   anchoring (see the inline base direction spec).
3. Auto-width blocks: the width absorbs the free space, so auto margins are 0.
4. Parent/child margin collapse where the child's top margin exceeds the
   parent's (`body-background-slr-print` ref; separate defect).

## Behavior

1. When an in-flow block in the physical horizontal frame (ltr, not the
   vertical logical frame) has a definite width and `margin-left: auto`,
   its border-box x SHALL be `origin + margin-left + free`, where
   `free = avail - margin-left - margin-right - border-box width`.
2. When `margin-right` is also `auto`, the border-box x SHALL be
   `origin + margin-left + free / 2`.
3. When `free <= 0` (over-constrained), the box SHALL keep its start-edge
   anchoring.

## Acceptance Criteria

1. Given `margin-left:auto; width:200px` in a 288pt content box, when laid
   out, then the background x is 174pt
   (`core564_auto_inline_margins::margin_left_auto_pushes_box_to_end_edge`).
2. Given the same box inside a definite-height parent, then x is 174pt
   (`margin_left_auto_inside_definite_height_parent`).
3. Given `margin: 0 auto`, then x is 105pt (`both_auto_margins_center_box`).
4. Given a 500px box with `margin-left:auto`, then x stays at 36pt
   (`over_constrained_auto_margin_keeps_start_edge`).
5. WPT `css/css-page/body-background-vrl-print.html` and
   `monolithic-overflow-030-print.html` pass.

## References

- CSS 2.2 §10.3.3 Block-level, non-replaced elements in normal flow.
- CORE-564, CORE-556.
