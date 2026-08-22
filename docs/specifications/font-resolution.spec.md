---
title: Font Resolution — @font-face, System Discovery, Family Stacks
slug: /specifications/font-resolution
type: spec
status: in-review
owner: elijah
created: 2026-08-22
updated: 2026-08-22
sidebar_position: 13
tags: [engine, css-fonts, typography, pdf, determinism]
spec_id: font-resolution
issue_id: CORE-103
applies_to: engine 0.x
dependencies: [font-weight-style, typography-layer, wpt-conformance-harness]
---

# Font Resolution — @font-face, System Discovery, Family Stacks

## Overview

The engine renders every document with a hardcoded 4-face Arial bundle
(`src/fonts.rs`: `FontFace { Regular, Bold, Italic, BoldItalic }`,
`face_for()` bold threshold ≥600). `font-family` is computed by stylo but only
its FIRST family name survives into `ComputedStyle`, and nothing reads it.
Any non-Arial stack silently renders Arial. This blocks branded documents and
gates OpenType feature work (CORE-113).

This issue replaces the closed 4-face enum with a **runtime face registry**:
system font discovery (macOS first), generic family mapping, full-stack
`font-family` resolution with css-fonts-4 weight/style matching, and
`@font-face` with local-file sources. Determinism (identical input →
byte-identical PDF) is preserved: face selection is a pure function of the
resolved styles; the registry assigns stable ids.

**Path chosen for discovery: fontdb 0.23.** Verified on this machine
(2026-08-22, probe `/tmp/fontdb-probe`): loads 1285 system faces in ~433 ms,
resolves `Georgia` bold+italic to `Georgia-BoldItalic` (postScript name),
matches nearest weight correctly (Georgia w500 → 400; Helvetica w900 → 700,
its darkest cut), and maps generics sensibly (serif → Times New Roman,
sans-serif → Arial, monospace → Courier New, cursive → Comic Sans MS,
fantasy → Papyrus). fontdb's query already implements the css-fonts-4
matching order we need; wrapping it beats hand-rolling directory walks.
fontdb itself uses ttf-parser/skrifa-family parsing and adds no C FFI.

## Goals

1. Resolve `font-family` lists (all families, in order) against installed
   system fonts via fontdb.
2. Map generic families (`serif | sans-serif | monospace | cursive |
   fantasy`) to concrete faces through fontdb's generic slots (set from its
   system scan; defaults recorded below).
3. Weight/style selection within a family per css-fonts-4 §5.2 (nearest
   weight; desired < 400 → lighter-first below target, ≥ 400 → darker-first;
   italic preferred over oblique).
4. `@font-face` at-rules: `src: local(...)` + relative/absolute file `url()`
   sources tried in order; descriptors `font-weight` (single value or range),
   `font-style`, `font-family` (the name it defines). No network fetch.
5. Per-character fallback: when the primary resolved face lacks a glyph,
   shaping retries the next family in the stack before falling back to
   `.notdef`.
6. Deterministic output: same input bytes → byte-identical PDF.

## Non-Goals

- woff2 decompression (recorded decision: not cheap — needs a Brotli decoder
  plus table reconstruction; plain TTF/OTF/TTC cover the corpus). Ticketed
  follow-up if a fixture demands it.
- Network font loading (`https://` sources fail resolution deterministically
  and fall through to the next source).
- Variable-font named-instance selection beyond fontdb's default instance.
- fontique-based discovery (revisit if fontdb proves insufficient).
- Changing `@page` margin-box content resolution (margin boxes inherit the
  page context's resolved style and flow through the same registry).

## Behavior

1. The engine SHALL build one fontdb `Database` per process (LazyLock),
   loading system fonts once (~430 ms measured) before or at first use.
2. The cascade SHALL resolve each element's computed `font-family` list to an
   ordered candidate list of concrete faces at convert time; the FIRST face
   that exists wins as the element's primary face.
3. A family name defined by an `@font-face` rule SHALL resolve to that
   rule's loaded face data ahead of system fonts with the same name.
4. Weight/style matching within the winning family SHALL follow css-fonts-4
   §5.2: if target weight < 400, check weights ≤ target descending first;
   otherwise check weights ≥ target ascending first; then the remaining
   direction. Italic desired → italic/oblique faces first; normal desired →
   normal faces first.
5. Generic families SHALL resolve through fontdb's generic mapping
   (`Family::Serif` etc.). On this machine that yields Times New Roman /
   Arial / Courier New / Comic Sans MS / Papyrus; the spec records these as
   observed defaults, not promises.
6. When no family in the stack resolves, the engine SHALL fall back to the
   existing Arial bundle path (current behavior), never panic.
7. `@font-face src: url(...)` SHALL resolve relative paths against the input
   document's base URL/directory, then absolute paths; missing files fall
   through to the next `src` entry, then to system fonts.
8. Shaping SHALL use the element's primary face; a codepoint missing from it
   (`.notdef` glyph id returned for that cluster) SHALL be re-shaped against
   the next candidate face that contains the glyph, splitting the run so
   mixed-script text draws correctly. If no candidate has the glyph, the
   primary face's `.notdef` stands.
9. Every embedded font subset SHALL be derived deterministically: the
   registry keys faces by content-independent id (source path or @font-face
   rule order + index), and krilla's content-hash dedup does the rest.

## Interfaces

```rust
// fonts.rs (rewritten)
pub struct FaceId(pub u32);                 // registry handle, Copy

/// One resolvable face: either a system path (+ttc index) or inline bytes.
pub enum FaceSource { Path(String, u32), Bytes(std::sync::Arc<[u8]>) }

pub struct ResolvedFace {
    pub source: FaceSource,
    pub postscript_name: String,           // verification + /BaseFont greps
    pub weight: f32,
    pub italic: bool,
}

/// Resolve a computed family list + weight/style to a primary FaceId and
/// the fallback chain (Behavior 2/4/8). Pure function of (families, db).
pub fn resolve_font(families: &[SingleFontFamily], weight: f32,
    style: FontStyle) -> ResolvedFace;

// ComputedStyle (css.rs)
pub font_face: FaceId,                      // replaces font_family: String
pub font_fallbacks: Vec<FaceId>,            // Behavior 8 chain (may be empty)
```

`TextRun.font_face: FaceId`; `typography.rs` replaces the `[LazyLock; 4]`
arrays with a registry-backed shaper cache (`Mutex<HashMap<FaceId, …>>` or a
leaked 'static slot map — implementation detail; HarfRust keeps borrowing
'static bytes, which `FaceSource::Bytes` provides via leak-on-register).
`pdf.rs::font_for(FaceId)` builds krilla `Font`s from the same registry.

stylo accessors (verified 2026-08-19, see `references/stylo-font-api-notes.md`
and generated `properties.rs`): `get_font().clone_font_family()` returns
`FontFamily` whose `.families.list: Vec<SingleFontFamily>` carries BOTH
`FamilyName(FamilyName)` and `Generic(GenericFontFamily)` variants — the
full stack, not just the first name. `clone_font_weight().value(): f32`,
`clone_font_style() == StyloFontStyle::ITALIC`.

## Acceptance Criteria

- Given a document with `body { font-family: Georgia, serif }`, when rendered,
  then `/BaseFont` greps show `Georgia` subsets and no `ArialMT`.
- Given `font-weight: 500` against Georgia (400/700 cuts only), when
  rendered, then the regular cut is selected (nearest, lighter side first
  below 400 boundary — verified probe behavior above).
- Given an `@font-face` rule pointing at a repo-local .ttf via relative url,
  when the doc renders from a different cwd, then the face embeds (path
  resolved against the document location, Behavior 7).
- Given a paragraph mixing Latin and a glyph absent from the primary face but
  present in a later stack member, when shaped, then the missing cluster draws
  from the later face (run split) rather than `.notdef`.
- Given any document rendered twice, outputs stay byte-identical
  (existing determinism tests extended to a multi-face doc).
- Regression gates: `cargo test` green; WPT harness 0 fixed / 0 regressed
  (css-page, css-break buckets); demo scoreboard re-run records deltas.

## Edge Cases

- `.ttc` collections: fontdb exposes per-index faces; registry stores index.
- Case-insensitive family matching: fontdb matches names case-insensitively
  (verified: "courier new" resolves); quoted vs unquoted CSS strings are
  normalized by stylo before us.
- A family name that shadows a generic ("Serif" as a real family): stylo
  parses bare `serif` as Generic; quoted `"serif"` stays a FamilyName —
  fontdb lookup decides; miss falls through the stack.
- Empty/whitespace-only family lists cannot occur (stylo always yields ≥1).
- Fonts unreadable at render time (permissions changed mid-session): shape/
  draw error surfaces as anyhow error, same as today's missing-Arial panic
  path, but scoped to the affected run.

## References

- Probe evidence: `/tmp/fontdb-probe` runs recorded in this spec's Overview
  (2026-08-22); issue CORE-103.
- `references/stylo-font-api-notes.md` — verified stylo font accessors.
- css-fonts-4 §5.2 (font matching algorithm); §4.3 (@font-face descriptors).
- fontdb 0.23 docs (query semantics, generic family slots).
- Related: CORE-80 (weight/style faces), CORE-113 (OpenType features,
  blockedBy this), CORE-86 (corpus font pinning rationale).
