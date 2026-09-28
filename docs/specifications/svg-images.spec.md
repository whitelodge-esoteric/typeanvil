---
title: SVG Images — resvg Rasterizer Bridge (<img> + inline <svg>)
slug: /specifications/svg-images
type: spec
status: in-review
owner: maintainers
created: 2026-09-03
updated: 2026-09-27
sidebar_position: 24
tags: [engine, pdf, images, svg, determinism]
spec_id: svg-images
applies_to: engine 0.x
dependencies: [images]
---

# SVG Images — resvg Rasterizer Bridge (`<img>` + inline `<svg>`)

## Overview

Real documents embed diagrams and icons as SVG: linked files
(`<img src="chart.svg">`) and inline `<svg>` elements. The image pipeline now
sniffs SVG content and routes it through the deterministic rasterizer bridge.
Prince renders SVG natively; this spec closes the gap with a rasterizer
bridge: decode SVG with `resvg` (usvg parse + tiny-skia raster, pure Rust,
no system deps), rasterize once at a fixed deterministic scale, and feed the
raster through the existing image store so layout, PDF embedding, and
deduplication keep their `images.spec.md` contracts unchanged.

## Goals

1. `<img src="*.svg">` and inline `<svg>` (shapes + text) render correctly at
   the 96 DPI baseline with correct intrinsic sizing.
2. SVG output is byte-stable across runs of identical input (the determinism
   contract in `docs/specifications/pdf-metadata.spec.md`).
3. Reuse the `images.spec.md` image store: one intern key per unique SVG, broken
   fallback for unparseable sources, monolithic replaced-element layout.

## Non-Goals

1. Native vector PDF embedding (raster-only bridge; a vector follow-up can
   replace `original` bytes without changing the store contract).
2. SMIL animation (static SVG only — same as resvg).
3. External resource loading inside SVG (`resources_dir` stays `None`;
   `<image href="...">` inside an SVG does not resolve external files).
4. CSS-sized `<svg>` attributes other than `width`/`height` (no
   `style="width:..."` handling; the stylo inline-style seam is unchanged).

## Behavior

1. **Sniffing.** The image store sniffs SVG by content prefix: a trimmed
   `<?xml` prolog or a trimmed `<svg` element tag, checked after PNG/JPEG
   magic. File extension plays no role (content decides).
2. **Rasterization.** A sniffed SVG decodes via `usvg::Tree::from_str` and
   rasterizes through `resvg::render` to a transparent-background PNG at a
   fixed 1× scale (1 SVG user unit = 1 CSS px, 96 DPI). Raster size is the
   tree's intrinsic size rounded to whole CSS px, capped at 16384 px per
   side; an undecodable, empty, or oversized SVG interns as Broken.
3. **Determinism.** Rasterization uses a fixed font family default (Arial),
   a process-wide font database (system fonts + the four bundled Arial
   faces), no timestamps, and no environment reads. Identical SVG bytes
   produce identical raster bytes.
4. **Intrinsic sizing.** The interned entry carries the intrinsic CSS px
   size as `width_px`/`height_px`, so the replaced-element sizing
   (CSS width/height → attribute → intrinsic ratio → shrink) applies
   unchanged. A `width`/`height`-less SVG sizes from its `viewBox`.
5. **Inline `<svg>`.** The layout collector serializes an inline `<svg>`
   DOM subtree back to SVG text (elements, attributes in source order,
   escaped text) and interns those bytes. The serialization is a pure
   function of the DOM; attribute-name casing survives (html5ever keeps
   camelCase SVG attributes such as `viewBox` on foreign content).
6. **Layout.** An inline `<svg>` is a replaced element: monolithic
   block-level box (`images.spec.md` Behavior 2), sized by the same
   `image_used_size` path, item-collected as its own item so it never folds
   into a parent text run.
7. **Embedding.** The PDF emitter embeds the interned bytes as a PNG image
   (`Image::from_png`); alpha rides the SMask. Krilla dedupes repeated
   references of the same raster.
8. **CLI `--base-url`.** The CLI threads `--base-url` into
   `layout_with_images`, so linked relative SVG (and any image) resolves
   against it. (Found during this ticket: the flag was parsed but never
   consumed — regression in the CLI surface introduced with the image store.)

## Interfaces

- `engine/src/images.rs`: `ImageKind::Svg` variant;
  `ImageStore::intern_bytes(bytes, alt)` (inline path);
  `rasterize_svg(bytes) -> Option<(u32, u32, Vec<u8>)>`; private SVG_FONTDB.
- `engine/src/layout.rs`: `is_replaced_image(id)` (img OR svg);
  `serialize_svg_subtree(dom, id)`; svg branch in
  `collect_image_sources_rec`; `Ctx::measure_block` /
  `measure_float` / item collection use `is_replaced_image`.
- `engine/src/main.rs`: render threads `--base-url` via
  `layout::layout_with_images`.
- `engine/src/pdf.rs`: emit arm `ImageKind::Svg => Image::from_png`.

## Acceptance Criteria

- Given an `<img src="x.svg">` whose SVG declares `width="120" height="80"`,
  when laid out, then one image fragment exists sized 90×60 pt and the store
  holds a Loaded `Svg` entry with `(120, 80)` px and PNG bytes.
  (`images.rs::svg_img_intrinsic_size_from_attributes`)
- Given an inline `<svg width="100" height="50">` with a rect and text, when
  laid out, then one image fragment exists sized 75×37.5 pt.
  (`images.rs::inline_svg_element_renders`)
- Given the same SVG document rendered twice, the PDF bytes are identical.
  (`images.rs::svg_render_is_deterministic`)
- Given a viewBox-only SVG `viewBox="0 0 200 100"`, the fragment sizes
  150×75 pt. (`images.rs::svg_viewbox_only_uses_viewbox_size`)
- Given three references to identical inline SVG, the store holds one entry
  and the PDF embeds one image + one SMask.
  (`images.rs::duplicate_inline_svgs_embed_once`)
- Given bytes that are neither valid UTF-8 nor a parseable SVG, the entry is
  Broken and no raster is embedded.
  (`images.rs::broken_svg_falls_back_to_broken_entry`)

## Edge Cases

- HTML comment nodes inside an inline `<svg>` are dropped by the serializer
  (comments never reach our DOM).
- Nested `<svg>` serializes recursively and stays a single interned raster.
- An SVG whose raster is 0×0 (e.g. empty content) interns as Broken.
- Attribute escaping covers `& < > " '`; text escaping covers `& < >`.

## References

- Images spec (`docs/specifications/images.spec.md`) — the store,
  sizing, and embedding contracts this builds on.
- resvg 0.45 / usvg 0.45 (vendored API surface verified in-source).
- `engine/src/images.rs` — SVG sniffing, rasterization, and image interning.
