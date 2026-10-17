---
title: "CSS Transforms — 2D transform and transform-origin"
slug: /specifications/css-transforms
type: spec
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 41
tags: [engine, css, transform, pdf]
spec_id: css-transforms
issue_id: CORE-481
applies_to: engine 0.x
dependencies: [out-of-flow-positioning]
---

# CSS Transforms

## Overview

Before CORE-481 the engine ignored `transform`. WPT `css-break/transform-023-print`
shifts an abspos subtree with `transform: translateX(49px)` and failed on
page 2. This spec adds 2D transforms (css-transforms-1) for boxes.

## Goals

1. Parse `transform` and `transform-origin` from stylesheets and inline `style`.
2. Support `translate`, `translateX`, `translateY`, `scale`, `scaleX`, `scaleY`,
   `rotate`, `rotateZ`, `skew`, `skewX`, `skewY` and `matrix`.
3. Paint a transformed box and its whole subtree under the matrix.

## Non-Goals

1. 3D functions (`translate3d`, `rotateX`, `perspective`, `matrix3d`) are invalid
   here; the declaration is ignored.
2. The individual properties `translate`, `rotate`, `scale` are not parsed.
3. Hit testing, `transform-box`, `transform-style` and `will-change` are out of
   scope.
4. A transform does not fragment: each fragment of a box fragmented across pages
   receives the matrix resolved against that fragment's own border box.
5. Percentages in `translate` and `transform-origin` resolve against the
   fragment's border box.

## Behavior

1. A `transform` shall be parsed by `engine/src/transform.rs`. The first
   function in the list is the outermost (css-transforms-1 §13).
2. `ComputedStyle.transform` shall hold the function list; an empty list is `none`.
3. The matrix shall be taken about `transform-origin`. The initial origin is
   `50% 50%`.
4. Layout shall attach the resolved matrix to the box's `Fragment.transform`, in
   the fragment's local space (origin = its top-left).
5. A transform shall not change layout: siblings and the parent cursor do not
   move.
6. A transformed box shall be a containing block for its abspos descendants
   (css-transforms-1 §3). Abspos fragments of such descendants are attached at
   the fragmentainer root, so layout shall compose the box's matrix onto each of
   them.
7. The PDF emitter shall paint a transformed fragment and its subtree as one
   paint group under a `cm` matrix, in the fragment's stacking position.
8. An abspos box with a declared `width` shall keep that width even when it is
   wider than its containing block (css-position-3 §5.3).
9. An abspos box with both horizontal insets `auto` shall sit at its static
   position (the parent's content-box left edge).

## Acceptance Criteria

1. Given `transform:translateX(49px)` on a 0×0 abspos box containing a 51px abspos
   child, when laid out, then the child fragment carries a 36.75pt x translation
   (`core481_transform::translate_on_zero_size_abspos_box_moves_its_abspos_child`).
2. Given an abspos child with `width:51px` in a 0-wide abspos parent, then the
   child is 38.25pt wide (`abspos_child_with_declared_width_is_not_clamped_to_zero_width_parent`).
3. Given `rotate(90deg)` and `scale(2)`, then the matrices keep the box centre
   fixed (`rotate_and_scale_attach_a_matrix_about_the_center`).
4. Given a transformed box, then following siblings do not move
   (`transform_does_not_move_siblings`).
5. WPT `css-break/transform-023-print.html` passes.

## Edge Cases

1. A transform that evaluates to the identity attaches no matrix.
2. An invalid function list leaves the previous value in force.

## References

- CORE-481, CORE-470, CORE-242 (transform-024 page count, separate).
- css-transforms-1 §2, §3, §6, §13.
