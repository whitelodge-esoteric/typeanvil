---
title: Rust Typesetting Architecture Brief — for Typeanvil build-vs-wrap decisions
type: research
status: approved
owner: elijah
created: 2026-08-14
updated: 2026-08-16
sidebar_position: 1
tags: [rust, ecosystem, typst, krilla, build-vs-wrap]
---

# Rust Typesetting Architecture Brief — for Typeanvil build-vs-wrap decisions

## 1. Typst's architecture

Typst (v0.15, Apache-2.0, ~55k★) is split into pipeline crates visible in its workspace: `typst-syntax` (parse) → `typst-eval` (evaluate markup/scripting) → `typst-realize` (realization: expand show rules into a uniform element tree) → `typst-layout` (measure & place) → `typst-pdf`/`typst-render`/`typst-svg` (export). Incremental compilation is powered by `comemo` (memoization of pure layout functions); layout was made fully pure and parallelized in 2024 (PR #4366).

**Layout model.** Per Laurenz Mädje's "TeX and Typst: Layout Models" (laurmaedje.github.io, 2024): TeX composes movable boxes+glue, with line breaking fully decoupled from page breaking. Typst instead layouts into *regions* (a sequence of shaped areas, currently rectangular and constant-width) producing *frames*. Constant-width regions preserve TeX's property that paragraphs can be line-broken independently of vertical position, while letting block elements (tables, blocks) react to where they land — key for cell-level page breaking. Trade-off he names explicitly: "regions require relayout" for high-quality vertical optimization (widows/orphans are handled but globally-optimized page breaking à la TeX is weaker; page breaking is essentially greedy over regions).

**Line breaking.** `typst-layout/src/inline/linebreak.rs` implements Knuth-Plass-style total-fit optimization with `f64` costs (hyphen cost 135, runt cost 100 — deliberately higher than K-P's 50 since Typst has no glue concept). Break opportunities come from **icu4x** (`icu_segmenter` LSTM `LineSegmenter`, plus a custom-data segmenter for CJ punctuation) and Unicode line-break properties from `icu_properties`. It falls back to a fast first-fit path when justification is off. Hyphenation via `hypher` (Typst's own zero-allocation Knuth-Liang pattern crate).

**Wraps vs builds.** Wraps: `rustybuzz` 0.20 (shaping), `ttf-parser` + `fontdb` (font parsing/discovery), `unicode-bidi`, icu4x crates (segmentation, properties, collation), `kurbo` (geometry), `usvg`/`image` for graphics, and `krilla` for PDF output (it migrated off its in-house `typst-pdf`-on-`pdf-writer` code to krilla, which itself sits on `pdf-writer`). Builds: realization, region/frame layout, inline layout + K-P, grid/table engine, math layout. Since 0.14 Typst emits tagged, accessible PDFs by default and supports PDF/UA-1 and all PDF/A profiles A-1 through A-4 (typst.app/docs/reference/pdf, 0.14 changelog, "accessible PDFs" blog post).

## 2. Servo: what's reusable

- **stylo** (github.com/servo/stylo): the CSS engine shared by Firefox & Servo, now published as versioned crates (`stylo` 0.20, `selectors` 0.40, `stylo_traits`, etc.), MPL-2.0, actively released (Aug 2026 commits). It gives you a full, web-grade cascade: parsing (via `cssparser`), selector matching, specificity, inheritance, computed values, media queries, custom properties, parallel restyling. Cost: it's big, generic over your DOM (you implement `TElement`/`TNode` traits), and geared to continuous restyle rather than one-shot batch — Blitz (DioxusLabs) proves it's embeddable outside Servo.
- **html5ever / xml5ever**: spec-compliant WHATWG HTML parsing, mature, widely used (also by Blitz, kuchiki, scraper).
- **Servo layout itself** (`layout` crate, formerly layout_2020) is *not* published for reuse and is deeply tied to Servo's script/DOM; not practically extractable.
- **taffy** (DioxusLabs): standalone block/flexbox/grid layout, high quality and used by Bevy/Zed/Blitz — but it's a UI layout tree solver with **no inline/text layout and no fragmentation/pagination**, so for a paged engine it can at most inspire the flex/grid algorithms (or be used for unfragmentable flex/grid subtrees).

## 3. Rust text stack maturity (2026)

- **Shaping:** the ecosystem consolidated on **HarfRust** — the official Rust port of HarfBuzz (harfbuzz org), successor to `rustybuzz` (which ports an older HarfBuzz snapshot). Parley and cosmic-text both now shape with HarfRust; Typst still ships rustybuzz 0.20. Verdict: pure-Rust shaping is production-grade; HarfBuzz C FFI is no longer necessary and costs you deterministic-build simplicity and cross-compilation ease.
- **i18n:** **icu4x** 2.x (Unicode Consortium project) is production-grade — Typst uses `icu_segmenter`/`icu_properties`/`icu_collator` in anger, Parley depends on ICU4X. Data slicing (blob providers) keeps binaries small. ICU4C FFI is unjustified for a new engine.
- **Fonts:** **skrifa/read-fonts** (Google Fonts "oxidize" project) is Google's production path for font parsing/scaling and memory-safe by design; `ttf-parser`+`fontdb` remain solid (Typst). **fontique** (Linebender) does font enumeration/fallback across platforms.
- **Layout libraries:** **Parley** (Linebender) = Fontique + HarfRust + Skrifa + ICU4X; rich-text layout, bidi, line breaking; released and improving fast but aimed at UI text boxes (greedy breaking, no K-P, no pagination). **cosmic-text** (System76) similar scope, editor-oriented. **swash** now mostly relevant for rasterization. None does paragraph-level total-fit optimization or microtypography — that layer you must build regardless.

## 4. PDF generation crates

- **pdf-writer** (typst org): low-level, allocation-light typed PDF serializer. Full control, zero abstraction — you'd hand-write tagging, subsetting, validation.
- **krilla** (LaurenzV, builds on pdf-writer; actively maintained, MSRV 1.92, Aug 2026 commits): high-level painting/text/image API **plus font subsetting (CFF & TTF), tagged PDF, and validated export for PDF/A-1/-2/-3/-4 and PDF/UA-1**, PDF 1.4–2.0. Its README states its target group verbatim: "libraries that have some kind of intermediate representation of layouted content… and want to easily translate this representation into a PDF file." This is Typst's production backend — meaning tagged/UA output is regression-tested by a large real deployment.
- **printpdf** (fschutt): lopdf-based, print-oriented, decent HTML-ish helpers, but no validated PDF/A/UA conformance story comparable to krilla.

**Recommendation: krilla**, dropping to pdf-writer only for exotic needs.

## 5. WeasyPrint

Python HTML/CSS→PDF engine (Kozea/CourtBouillon). Architecture: `tinycss2` + `cssselect2` (CSS), html5 parsing, **Pango via cffi for shaping/fonts**, custom pure-Python box layout, `pydyf` for PDF writing. Weaknesses (documented): maintainers describe it as "quite slow by choice and by design" (issue #578); official docs warn "WeasyPrint is often slower than other web engines" and that **tables spanning multiple pages are notoriously slow**; long documents show super-linear time/memory because pagination re-lays-out content and Python object overhead dominates; no tagged-PDF/UA maturity comparable to Prince. Lesson for Typeanvil: fragmentation retry loops must be designed for O(n) behavior (carry-over state, not relayout-from-scratch), and layout must be in a compiled language with flat data structures.

## 6. Wrap-vs-build table

| Concern | Decision | Crate(s) | Rationale |
|---|---|---|---|
| HTML parsing | **Wrap** | `html5ever` | Spec-compliant, battle-tested (Servo, Blitz) |
| CSS parse + cascade | **Wrap** | `stylo` + `cssparser` + `selectors` | Web-grade cascade for free; implement DOM traits; fallback: hand-rolled cascade on `cssparser` if stylo's weight/MPL bites |
| Text shaping | **Wrap** | `harfrust` (or rustybuzz as fallback) | HarfBuzz-class, pure Rust, deterministic builds |
| Bidi / i18n | **Wrap** | `unicode-bidi` + `icu4x` (segmenter, properties) | Typst-proven; blob data providers keep size bounded |
| Hyphenation | **Wrap** | `hypher` (or `hyphenation`) | Knuth-Liang patterns, zero-alloc, done |
| Font loading | **Wrap** | `skrifa`/`read-fonts` + `fontique` (or `ttf-parser`+`fontdb`) | Google/Linebender production stacks |
| Line breaking (K-P total-fit + microtypography) | **Build** | — (breakpoints from icu_segmenter) | The differentiator; nobody ships K-P + protrusion/expansion as a crate; Typst's linebreak.rs is the reference implementation to study |
| Box layout (block/inline/flex/table) | **Build** | — (taffy as algorithm reference) | No reusable paged CSS layout exists; taffy lacks inline + fragmentation |
| Fragmentation (page/column breaking) | **Build** | — | Core IP; adopt Typst's regions model but design for relayout-free carry-over (WeasyPrint's failure mode) |
| PDF output | **Wrap** | `krilla` (on `pdf-writer`) | Tagged PDF, PDF/UA-1, PDF/A-1..4, subsetting — Typst-production-tested |

**Determinism note:** every wrapped component above is pure Rust — no C FFI anywhere — so bit-identical output across platforms reduces to controlling your own arithmetic (Typst uses f64 wrapped in `Scalar`; consider fixed-point in layout if float reproducibility across targets worries you) and pinning icu4x data blobs.

### Sources
- laurmaedje.github.io/posts/layout-models/ (Typst vs TeX layout model)
- github.com/typst/typst — Cargo.toml (deps: rustybuzz, icu_*, hypher, krilla, fontdb, unicode-bidi); crates/typst-layout/src/inline/linebreak.rs (K-P costs, icu_segmenter)
- typst.app/docs/reference/pdf/, typst.app/blog/2025/accessible-pdf/, 0.14 changelog (PDF/UA, PDF/A, tagged PDF)
- github.com/LaurenzV/krilla README (scope, PDF/A-1..4 + PDF/UA-1, subsetting; on pdf-writer)
- github.com/servo/stylo README (published crates, release process)
- github.com/linebender/parley README (Fontique + HarfRust + Skrifa + ICU4X stack); crates.io/crates/cosmic-text (HarfRust shaping)
- github.com/dioxusLabs/taffy, docs.rs/taffy (block/flex/grid, no inline/fragmentation)
- github.com/Kozea/WeasyPrint issue #578 ("slow by choice and by design"); doc.courtbouillon.org/weasyprint (perf guidance: slower than other engines, multi-page tables slow)
