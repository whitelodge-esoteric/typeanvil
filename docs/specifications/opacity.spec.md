---
title: Opacity (group opacity)
slug: /specifications/opacity
type: spec
status: approved
owner: maintainers
created: 2026-10-18
updated: 2026-10-18
sidebar_position: 41
tags: [paint, css-color, engine]
spec_id: opacity
issue_id: CORE-518
applies_to: engine 0.x
dependencies: [paged-media-css]
---

# Opacity (group opacity)

## Overview

Before CORE-518 the engine ignored `opacity`: `opacity: .5` painted fully
opaque. This spec covers css-color-4 §3.2 group opacity on block-level boxes.

## Goals / Non-Goals

Goals: `opacity` on any laid-out box, applied to the box and its whole
subtree as one transparency group.

Non-Goals: `opacity` on inline (non-atomic) boxes; `mix-blend-mode`;
`filter`; `will-change`; opacity animations.

## Behavior

1. The computed `opacity` shall be clamped to 0..=1 and stored on the box's
   fragment. The initial value is 1.
2. A fragment with `opacity < 1` shall be a stacking-context root: its own
   paint and its whole subtree paint as one unit (background, border, text,
   images and descendants), composited once at that alpha.
3. Nested opacities shall multiply, because each group composites inside its
   parent group.
4. A fragment with `opacity: 1` shall paint exactly as before. PDF output for
   documents that do not use `opacity` shall not change.
5. Opacity shall not change layout.

## Acceptance Criteria

1. Given `.c { background:#00f; opacity:.5; border:2px solid #000 }`, when
   laid out, then exactly one fragment has `opacity == 0.5`
   (`core518_opacity::opacity_is_carried_on_the_box_fragment`).
2. Given the same box, when rendered, then the PDF contains a `/ca 0.5`
   graphics state; without `opacity` it does not
   (`core518_opacity::pdf_paints_semitransparent_group_only_when_opacity_set`).
3. Given `opacity: -3`, then the stored value is 0
   (`core518_opacity::out_of_range_opacity_is_clamped`).

## Known limitations

- Abspos descendants of an opacity box attach at the page root, not under
  the box, so they do not receive the box's group opacity.

## References

- CORE-518; css-color-4 §3.2; CORE-481 (transform, same stacking-context
  path); CORE-153 (color alpha).
