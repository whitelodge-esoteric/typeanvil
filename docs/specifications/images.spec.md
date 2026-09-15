---
title: Images — <img> Decode, Layout, PDF Embedding
slug: /specifications/images
type: spec
status: draft
owner: elijah
created: 2026-08-21
updated: 2026-09-15
sidebar_position: 23
tags: [engine, pdf, images, layout, determinism]
spec_id: images
issue_id: CORE-106
applies_to: engine 0.x
dependencies: [fragmentation-core]
---

# Images — `<img>` Decode, Layout, PDF Embedding

## Overview

The engine renders zero `<img>` elements: no decode, no layout box, no draw
path. Any real report, invoice, or letterhead needs logos and figures. This
is the single largest functional gap vs Prince (the letterhead demo
divergence is partly structural: "Prince renders a logo image + left-aligned
heading").

krilla 0.8 provides native image embedding backed by the `image` crate
(PNG + JPEG decode/embed without extra dependencies). The engine's
coordinate system (top-left origin, y down, points) matches krilla's surface
space directly.

## Goals

1. `<img src="…">` decodes, lays out as a replaced-element box, and embeds
   in the PDF at the correct size.
2. Sources: file paths resolved against `--base-url`, and `data:` URIs
   (base64, percent-encoded).
3. Formats: PNG and JPEG (krilla's native path), inline SVG (rasterised to
   PNG at intern time, CORE-131), and GIF (first frame, normalised to PNG at
   intern time, CORE-186).
4. Sizing follows CSS2.1 replaced-element rules: intrinsic size at 96 dpi,
   `width`/`height` attributes and CSS `width`/`height` scale it, and a
   single specified dimension preserves aspect ratio.
5. An image never fragments across pages — it moves whole.
6. Deterministic output: identical input → byte-identical PDF.

## Non-Goals

1. **object-fit / object-position**: deferred unless trivial during
   implementation; the decision (and reason) is recorded in the issue, not
   silently accepted.
2. **WebP/other formats**: deferred. A document using them gets the
   broken-image placeholder (Behavior 7). Recorded decision, follow-up if
   needed. (GIF was originally on this list; CORE-186 landed support — see
   Behavior 4.)
3. **Float interaction**: an image participates in float wrapping like any
   other box; float-specific image tuning is out of scope.
4. **SVG**: deferred entirely (separate ticket if filed).

## Behavior

1. **Parsing.** html5ever already parses `<img>` as a void element; the DOM
   stores all attributes in source order. Layout SHALL treat an `<img>`
   element as a replaced element and MUST NOT descend into it for text
   collection (it is void; it has no children).

2. **Inline treatment (v1 simplification).** The current inline model folds
   inline content into styled text runs and has no atomic-inline slot.
   TypeAnvil v1 SHALL lay out each `<img>` as a monolithic block-level box
   in the block flow. Consequence: an `<img>` in the middle of a paragraph
   splits the paragraph at that point (text after the image resumes in a new
   anonymous block). This diverges from CSS inline-block behavior and from
   Prince; it is RECORDED as a known limitation, and a later inline-box
   feature supersedes it. Rationale: every corpus use case (logo in header,
   figure in report) places images as sole children of their container, so
   the block model covers them while keeping the change surgical.

3. **Source resolution.**
   - `src="data:image/png;base64,…"` (or `image/jpeg`, `image/gif`) SHALL
     decode to raw bytes directly. The MIME gate is load-bearing: rejecting a
     type here means the bytes never load and the reference is hashed as its
     URL TEXT instead (CORE-186 found `image/gif` rejected this way).
   - Any other `src` SHALL resolve as a file path relative to the
     `--base-url` CLI argument (same resolution rules as stylesheet paths);
     absolute paths pass through unchanged.
   - A missing/unreadable file, unknown extension, or undecodable byte
     stream is a broken image (Behavior 7).

4. **Decoding + caching.** The engine SHALL decode exactly once per unique
   image byte stream per process. The cache key SHALL be the SHA-256 of the
   decoded source bytes (NOT the path — two paths with identical bytes are
   one entry). The cache SHALL be a `BTreeMap` (deterministic iteration;
   never `HashMap`). Decoding happens lazily at first use during layout.

   **GIF (CORE-186)** SHALL be normalised to PNG at intern time: the FIRST frame
   is decoded to RGBA at the GIF's LOGICAL SCREEN size — a frame covering only
   part of the canvas is composed at its own offset — and re-encoded as PNG. The
   cache key therefore stays the SHA-256 of the GIF source bytes while the
   embedded payload is a PNG, and `ImageKind` stays `Png`. An ANIMATED GIF prints
   its FIRST frame only, matching Chromium's behaviour for a static medium.
   Alpha from a transparent palette index is preserved by the PNG. The GIF
   logical screen size is the intrinsic size (Behavior 5).

5. **Intrinsic size.** The intrinsic size SHALL be the decoded pixel
   dimensions converted at the CSS reference pixel rate: 1 image px =
   1/96 in = 0.75 pt. A 192×64 PNG is intrinsically 144 pt × 48 pt.

6. **Sizing algorithm** (CSS2.1 §10.3.2 simplified, no min/max yet):
   - CSS `width`/`height` (from `ComputedStyle`) wins over the HTML
     attributes.
   - HTML `width`/`height` attributes are lengths in CSS px (pt = value ×
     0.75); percentages resolve against the containing block's content
     width (height percentages resolve to auto in v1).
   - Both dimensions specified → used size is exactly that (aspect ratio
     NOT preserved — CSS2.1 behavior).
   - One dimension specified → the other is computed from the intrinsic
     aspect ratio.
   - Neither specified → intrinsic size.
   - A broken image uses the attribute sizes if present, else the CSS2.1
     default suggested size 300 px × 150 px (225 pt × 112.5 pt).

7. **Broken image placeholder.** A broken image SHALL produce a
   rectangular placeholder box of the size from Behavior 6, and MUST NOT
   abort the render. v1 gate finding (WPT A/B, 2026-08-22): without explicit
   dimensions the box COLLAPSES to zero size and no alt text is drawn — the
   CSS2.1 300×150 default suggestion would inject a large block into
   documents that never expected the image to render (notably WPT refs that
   lean on `position:absolute` images, which the engine's inline-style pass
   does not honor), reflowing them and regressing the css-page bucket. With
   an explicit `width` (attr or CSS) the box takes that width and the alt
   text draws inside it; height stays collapsed unless explicitly given.

8. **Fragmentation.** An image fragment SHALL be monolithic: it never
   splits. When it does not fit in the remaining fragmentainer space, it
   defers whole to the next page (same rule as `break-inside: avoid`).
   Implementation note: this falls out naturally if the image rides a
   `Line`-kind fragment (lines are already monolithic) or a dedicated
   fragment kind marked unsplittable.

9. **PDF embedding.** The emitter SHALL paint the image through
   `krilla::image::Image` + `surface.draw_image()` covering exactly the
   fragment's rect. One krilla `Image` object per unique cached decode, so
   repeated logos share one embedded stream (krilla deduplicates by object).

10. **Determinism.** Embedded image streams SHALL be the canonical encoded
    bytes (PNG/JPEG as decoded — no re-encode, no metadata rewrite). No
    timestamps anywhere in the image path. Two renders of the same
    document SHALL be byte-identical (covered by an acceptance test).

## Interfaces

```rust
// frag.rs — new payload variant:
pub enum FragmentContent {
    // … existing variants …
    /// An embedded raster image painted over the fragment's rect.
    Image(ImageRun),
}

#[derive(Clone, Debug)]
pub struct ImageRun {
    /// Cache key == SHA-256 of source bytes; indexes the engine's
    /// image store.
    pub key: [u8; 32],
    /// Alt text, if any (drawn only for broken-image placeholders).
    pub alt: Option<String>,
    /// True when the source failed to load (placeholder mode).
    pub broken: bool,
}
```

```rust
// new module src/images.rs:
pub struct ImageStore { /* BTreeMap<[u8;32], StoredImage> */ }
impl ImageStore {
    pub fn new() -> Self;
    /// Resolve + decode; Ok(entry) even for broken sources (broken flag),
    /// Err only on internal failure.
    pub fn intern(&mut self, src: &str, base_url: Option<&Path>) -> Result<[u8; 32]>;
    pub fn get(&self, key: &[u8; 32]) -> Option<&StoredImage>;
}
pub struct StoredImage {
    pub width_px: u32,
    pub height_px: u32,
    pub kind: ImageKind,          // Png | Jpeg | Broken
    pub original: Vec<u8>,        // canonical bytes for embedding
}
```

Layout threads `&mut ImageStore` through the same context that carries the
font/shaping state; pdf.rs reads the store at emit time. `ComputedStyle`
needs NO new fields — `width`/`height`/`width_percent`/`height_percent`
already exist (stylo accessors verified in CORE-81/CORE-66 work).

The CLI contract is unchanged (`render <in.html> … --base-url -o out.pdf`);
`--base-url` already exists.

## Acceptance Criteria

Tests live in `engine/tests/images.rs` unless noted. Fixtures generate PNGs
at test time (small solid-color images written to a temp dir) — no binary
fixtures committed.

| # | Criterion | Test |
|---|-----------|------|
| 1 | Intrinsic sizing: 192×64 PNG with no dimensions → fragment 144×48 pt (±0.5) | `intrinsic_size_at_96dpi` |
| 2 | Attribute sizing: `width="96"` → 72 pt wide, height from ratio | `attribute_width_preserves_ratio` |
| 3 | CSS sizing beats attributes: CSS `width: 50pt` overrides `width="960"` | `css_width_beats_attribute` |
| 4 | Cross-page monolithic move: page break before an image that doesn't fit moves it whole; both pages' fragments carry full-size rects | `image_moves_whole_across_pages` |
| 5 | Data URI: base64 PNG renders identically (same fragment geometry) to the same bytes from a file | `data_uri_matches_file_source` |
| 6 | Broken image: missing file → placeholder box at attribute/default size, alt text drawn, render succeeds | `broken_image_placeholder_box` |
| 7 | Determinism: rendering the same doc twice → byte-identical PDFs | `image_render_is_deterministic` |
| 8 | Single embed: two `<img>` tags with the same file → one XObject image stream in the PDF | `duplicate_images_embed_once` |
| 9 | Full suite green: `cargo test` in `engine/` passes | CI |

WPT gate: run the fitness buckets before merge; ship only 0 fixed /
0 regressed (images appear in no WPT subset today, so this should be a
pure no-op gate).

## Edge Cases

- **Zero-byte file / truncated PNG** → broken placeholder, no panic.
- **Huge image** (larger than the page): still monolithic; if taller than
  the content box it paints clipped to the page (krilla clips at the page
  boundary). Prince probes pending; recorded if divergent.
- **`<img>` inside margin boxes / generated content**: NOT supported in v1
  (margin-box content pipeline is text-only) — EXCEPT `content: url(<path>)`
  inside a `@page` margin box, which paints the image at its intrinsic size
  (CORE-141). An `<img>` ELEMENT inside a margin box is still unsupported:
  margin boxes take generated content, not element subtrees.
- **Same image, different display sizes**: one cache entry, multiple
  fragments scaling the shared XObject.
- **Percent width inside a float**: resolves against the float's containing
  block like any percentage width.

## References

- Issue: CORE-106 (model: codex `nous/openai/gpt-5.2-codex` — engine work).
- krilla 0.8 image API: `krilla::image::{Image, ImageKind}` (to be verified
  against the vendored source during implementation, same verification
  discipline as the hyperlinks spec).
- CSS2.1 §10.3.2 (replaced element widths), §10.6.2 (heights), §14.2 alt
  rendering conventions.
- Fragment tree model: `docs/specifications/fragmentation-core.spec.md`.
