---
title: Element Background Image (position / size / repeat)
slug: /specifications/element-background-image
type: spec
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 40
tags: [layout, css-backgrounds, images, engine]
spec_id: element-background-image
issue_id: CORE-319
applies_to: engine 0.x
dependencies: [images, paged-media-css]
---

# Element Background Image (position / size / repeat)

## Overview

Before CORE-319 an element box painted only `background-color`. A
`background-image: url(...)` on an element was parsed by stylo and dropped.
This spec covers element-level `background-image` and the longhands that place
it. Margin boxes already tiled images at natural size (CORE-141) and share the
painter.

## Goals / Non-Goals

Goals: first `url()` layer, `background-position`, `background-size`,
`background-repeat`, and the `background` shorthand.

Non-Goals:

- Multiple layers. Only the first layer paints.
- Gradients other than the solid collapse of CORE-153.
- `background-origin`, `background-clip`, `background-attachment`. The
  positioning area is the padding box and the clip is the border box (the CSS
  initial values).
- `repeat: space` and `repeat: round`. Both paint as `repeat`.
- `@page` and margin-box position, size and repeat. They still tile at
  natural size from the top-left.
- `html` / `body` canvas propagation of the image.
- `url()` paths with leading `../`. Stylo resolves the reference against
  `http://localhost/`, so the root swallows `..` and the loader sees a path
  without it. Same-directory and sub-directory paths and `data:` URIs work.
  Changing the shared stylesheet base would also affect `@font-face`.
- Image fragments split across pages. Each page fragment of a box paints its
  own image from the fragment's top-left.

## Behavior

1. A box with a loadable `background-image: url(...)` shall paint the image
   above `background-color` and below borders, text and child content.
2. The `src` shall resolve like `<img src>`: against `--base-url`, or a
   `data:` URI. An image that fails to load shall paint nothing.
3. With no size, the tile shall be the image's natural size (96 DPI px to pt).
4. `background-size` shall accept `cover`, `contain`, and one or two of
   `auto`, a length, or a percentage of the padding box. One `auto` axis keeps
   the aspect ratio.
5. `background-position` shall place the first tile at
   `fraction * (area - tile) + offset` per axis. The area is the padding box.
6. `background-repeat` shall tile along each axis that repeats. A repeating
   axis tiles in both directions from the first tile.
7. The painter shall clip to the border box.
8. The `background` shorthand shall yield both the colour and the image.

## Interfaces

- `css::BackgroundImageStyle` on `ComputedStyle.background_image`.
- `frag::BackgroundImageRun` carries tile size, first-tile offset and repeat
  flags. `tile_origins(w, h)` lists the tiles the painter draws.
- `layout::element_background_image` resolves size and position.

## Acceptance Criteria

Tests in `engine/tests/it/core319_background_image.rs`:

- Given `background-image: url(t.png)`, when laid out, then a run with the
  natural tile size and 25 tiles in a 150x75pt box exists.
- Given `no-repeat 100% 100%`, then one tile sits at the bottom-right.
- Given `repeat-x`, then tiles repeat along x only.
- Given `no-repeat top left / 50%`, then the tile is 50% wide at image ratio.
- Given `center / cover` and `center / contain`, then tile size and offset
  follow css-backgrounds-3 §3.9.
- Given `#0f0 url(t.png) no-repeat`, then a colour fill and an image both
  exist.
- Given a missing file, then no run exists.

## References

- css-backgrounds-3 §3.3-3.9
- CORE-141 (margin-box background image), CORE-153, CORE-317
