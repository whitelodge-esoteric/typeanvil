---
title: Border Styles (dashed and dotted)
slug: /specifications/border-styles
type: spec
status: draft
owner: maintainers
created: 2026-10-20
updated: 2026-10-23
sidebar_position: 17
tags: [layout, borders, css-backgrounds, engine]
spec_id: border-styles
issue_id: CORE-531
applies_to: engine 0.x
dependencies: [table-backgrounds]
---

# Border Styles (dashed and dotted)

## Overview

css-backgrounds-3 §4.2 defines how each `border-style` value paints. Before
CORE-531 the engine painted every non-`none` style as a solid band. This spec
records which styles paint as drawn and which still fall back to solid.

## Behavior

1. Each border side shall carry its own line style (`top`, `right`, `bottom`,
   `left`) from the cascade into the border fragment.
2. A `dashed` side shall paint as a series of rectangles with gaps between
   them. Each dash is three times the side's thickness long. The gap is
   adjusted so the first dash starts at the start of the edge and the last
   dash ends at the end of the edge.
3. A `dotted` side shall paint as a series of square dots. Each dot is as long
   as the side's thickness. The gap is adjusted the same way.
4. A side shorter than one dash shall paint as one solid rectangle.
5. The rules apply to all four sides and to zero-width boxes (a box whose
   only visible border is one side).
6. `double`, `groove`, `ridge`, `inset` and `outset` shall paint as `solid`.
   They are a known limitation, listed below.

## Supported styles

| Style | Painted as |
|---|---|
| `none`, `hidden` | no border, zero width |
| `solid` | solid band |
| `dashed` | dashes |
| `dotted` | square dots |
| `double`, `groove`, `ridge`, `inset`, `outset` | solid band (limitation) |

## Non-Goals and known limitations

- Round dots. Dots are squares, as the engine paints each border side with
  filled rectangles.
- Dash phase at corners. Each side is divided on its own, so dashes on
  adjacent sides do not join at the corner.
- The `@page` border ring and the root-element border ring paint solid for
  every style.

## Acceptance Criteria

1. Given a box with `border: 4px dashed`, when it paints, then each side has
   more than three dashes separated by gaps. Test:
   `engine/tests/it/core531_border_dashed.rs`.
2. Given a zero-width box with `border-left: 4px dashed`, then the left side
   carries the dashed style. Test: `zero_width_box_keeps_dashed_left_border`.
3. Given a dotted side, then each dot is as long as the side is thick. Test:
   `dotted_side_has_square_dots`.
4. The concert ticket tear line (`corpus/tickets/concert-ticket.html`)
   paints dashed.

## References

- CORE-531
- css-backgrounds-3 §4.2
