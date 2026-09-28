---
title: Tagged PDF / PDF-UA Accessibility
slug: /specifications/tagged-pdf
type: spec
status: draft
owner: maintainers
created: 2026-08-24
updated: 2026-09-27
sidebar_position: 16
tags: [engine, pdf, accessibility, tagging, determinism]
spec_id: tagged-pdf
applies_to: engine 0.x
dependencies: [pdf-metadata]
---

# Tagged PDF / PDF-UA Accessibility

## Overview

TypeAnvil supports opt-in tagged PDFs through krilla 0.8.2's tagging API.
The `--tagged` path emits a logical structure tree for headings, paragraphs,
lists, tables, figures, links, and language. The default untagged output stays
byte-identical.

Verified krilla facts (krilla 0.8.2, checked 2026-08-24):

- `SerializeSettings.enable_tagging: bool` gates MCID tracking; when true,
  each page allocates a `PageTagIdentifier`.
- `Surface::start_tagged(ContentTag) -> Identifier` opens a marked-content
  sequence; `Surface::end_tagged()` closes it. `ContentTag::Artifact(_)`
  yields a dummy identifier (artifacts sit outside the tree);
  `ContentTag::Other` yields a real `(page, mcid)` identifier.
- `Document::set_tag_tree(TagTree)` installs the logical structure. The tree
  holds `Node::Group(TagGroup { tag: TagKind, children })` and
  `Node::Leaf(Identifier)` nodes.
- `TagKind` covers `P`, `Hn(level, title)`, `L/LI/Lbl/LBody`,
  `Table/THead/TBody/TFoot/TR/TH(scope)/TD`, `Figure(alt_text)`, `Link`,
  `Div`, `Span`, `Strong`, `Em`, `Code`, `BlockQuote`, `Caption`,
  `TOC/TOCI`, `NonStruct`, and more (generated.rs).
- `Configuration::builder().with_accessibility_validator(Accessibility)`
  enables krilla's built-in PDF/UA-1 validator: input-dependent violations are
  recorded during serialization and returned by `Document::finish()` as
  errors. This is the "VeraPDF or equivalent" gate — no external tool needed
  for the first pass.
- Marked-content sequences in the content stream do NOT need to nest like the
  logical tree; the structure tree references content by `(page, mcid)`. The
  emitter therefore keeps its existing paint order unchanged.

## Goals

1. `--tagged` produces a PDF whose structure tree maps HTML semantics to PDF
   roles: `h1–h6 → H1–H6`, `p → P`, `ul/ol → L` with `li → LI (Lbl, LBody)`,
   tables → `Table/THead|TBody|TFoot/TR/TH/TD`, `figure/img → Figure` with
   alt text, `a → Link`, `strong/b → Strong`, `em/i → Em`, `blockquote →
   BlockQuote`, `pre/code → Code`, everything else block-level → `Div`.
2. `<img alt>` and `<figcaption>` propagate as Figure alt text;
   `<html lang>` becomes the structure-tree language.
3. With `--tagged --ua`, krilla's PDF/UA-1 validator runs; violations fail
   the render with the recorded error list.
4. Without `--tagged`, output is byte-identical to today's.
5. Determinism preserved with tagging on (document-order tree, no hash
   iteration, no clock reads).

## Non-Goals

- No `Span`-level language switching or `ActualText` (hyphenation-aware copy)
  in this pass — spans exist in the API but the emitter tags whole draws.
- No list-item markers in the structure tree: the engine renders no visual
  markers yet, so `LI` carries an empty `Lbl` and all content in `LBody`.
- No form fields, notes, or math tagging.
- No PDF/A conformance (archival validator untouched).
- No full Acrobat/PAC certification — krilla's validator is the recorded gate;
  external-tool runs are follow-up evidence, not blockers.

## Decision: opt-in flag, artifacts for chrome

Tagging defaults OFF. Untagged rendering is the product default until the
structure tree is proven; the harness contract stays additive (`--tagged`,
`--ua` are new optional flags). Margin-box text, page backgrounds, and the
unlicensed watermark carry no DOM source, so they are tagged as `Artifact`
(Background / Header / Footer kinds) — excluded from the logical tree exactly
as PDF-400/UA-1 intend.

## Behavior

1. When `--tagged` is absent, `render` SHALL behave bit-for-bit as before:
   `enable_tagging: false`, no `start_tagged` calls, no tag tree.
2. When `--tagged` is present, the emitter SHALL set `enable_tagging: true`
   and wrap every draw call in a `start_tagged`/`end_tagged` pair:
   - draws whose owning fragment resolves (through `Fragment::source`,
     inheriting the nearest sourced ancestor) to a DOM element →
     `ContentTag::Other`;
   - draws with no sourced ancestor (margin boxes, page background,
     watermark) → `ContentTag::Artifact` with kind Background (fills) or
     Header/Footer (margin boxes) / Footer (watermark).
3. The structure tree SHALL be built from the parsed DOM in document order:
   a single pre-order walk assigns each element its `TagKind` (role table in
   Goals §1; `th` carries `scope` from the `scope` attribute, row/column
   inferred otherwise; `figure` alt = `<img alt>` else `figcaption` text);
   each element's group collects, in paint order, the leaf identifiers of the
   draws attributed to it (pages ascending, then MCID ascending).
4. `<html lang>` SHALL set the tag tree language via `TagTree::with_lang`;
   absent `lang` means no language entry.
5. Heading groups SHALL carry their text as the `Hn` title; heading text
   draws remain ordinary leaf content beneath the group.
6. Link annotations (`hyperlinks.spec.md`) rendered in tagged mode SHALL be registered
   with `add_tagged_annotation`, and the corresponding `Link` group SHALL
   contain the annotation identifier followed by the link text's identifiers.
7. With `--ua`, the document SHALL be finished under krilla's accessibility
   validator; any violation aborts the render with the full violation list on
   stderr. Without `--ua`, tagging proceeds without the validator (missing
   alt text degrades gracefully instead of failing).
8. Identical input with identical flags SHALL yield byte-identical output,
   tagged or not. All tree construction SHALL be index-ordered (arena Vec
   order), never hash-iterated.
9. The harness adapter contract stays additive: existing flags keep their
   exact meaning.

## Interfaces

New module `engine/src/tags.rs` (registered in `lib.rs`):

```rust
/// Role assignment for one DOM element (tags.rs).
pub fn role_for(el: &Element) -> TagRole;          // pure mapping, unit-tested
pub struct DrawRef {                                // one tagged draw
    pub page: usize,
    pub ident: krilla::interchange::tagging::Identifier,
    pub source: Option<NodeId>,                     // resolved owner
}
pub fn build_tag_tree(
    dom: &Dom,
    draws: &[DrawRef],
    lang: Option<String>,
) -> krilla::interchange::tagging::TagTree;
```

`pdf.rs`:

```rust
/// Existing signatures unchanged.
pub fn render(layout: &Layout) -> Result<Vec<u8>>;
pub fn render_with_metadata(layout: &Layout, meta: &DocumentMetadata, watermark: bool) -> Result<Vec<u8>>;

/// New: full control. `dom: Some(_)` + `tagged: true` enables tagging;
/// `ua: true` additionally runs the PDF/UA-1 validator at finish.
pub fn render_with_options(
    layout: &Layout,
    dom: Option<&Dom>,
    meta: &DocumentMetadata,
    watermark: bool,
    tagged: bool,
    ua: bool,
) -> Result<Vec<u8>>;
```

CLI (main.rs, hand-parsed like every flag):

```text
typeanvil render in.html [--tagged] [--ua] -o out.pdf
```

## Acceptance Criteria

- AC-1 (byte stability, off path): rendering any corpus fixture with and
  without the new code path selected (i.e. `render_with_options` with
  `tagged: false`) produces output identical to the pre-change binary's.
  Test: `engine/tests/tagged_pdf.rs::untagged_path_is_byte_stable` (compares
  against `render_with_metadata` output).
- AC-2 (roles land): a fixture with heading, paragraph, list, and table
  renders tagged; pypdf structure-tree read-back shows `H1`, `P`, `L`/`LI`,
  `Table`/`TR`/`TH`/`TD` elements in document order.
  Verification: `scripts/verify_tagged_pdf.py` (pypdf) run against the test
  output; recorded in this spec's validation log below.
- AC-3 (alt text): `<img src=… alt="Chart">` inside a figure yields a
  Figure element whose `/Alt` entry reads `Chart`; a broken image's
  placeholder alt behaves the same.
  Test: `engine/tests/tagged_pdf.rs::figure_alt_text_propagates` (Rust side:
  tagged render succeeds; Python side confirms `/Alt`).
- AC-4 (language): `<html lang="en">` sets the structure tree language.
  Test: `engine/tests/tagged_pdf.rs::html_lang_sets_tree_language` (render
  succeeds with validator on; absence of lang with validator on fails —
  asserting the validator is live).
- AC-5 (determinism): the same input rendered twice with `--tagged` yields
  byte-identical files.
  Test: `engine/tests/tagged_pdf.rs::tagged_render_is_deterministic`.
- AC-6 (UA gate): `--ua` on a fixture missing required properties (no
  title, no lang) fails with a recorded violation list; the same fixture
  completed (title + lang + alts) passes.
  Test: `engine/tests/tagged_pdf.rs::ua_validator_rejects_incomplete_document`.
- AC-7 (no regression): full WPT suite shows 0 regressed (untagged raster
  path cannot move; gate confirms). Engine `cargo test` green.

### Validation log

Recorded 2026-08-24 (worktree `core-111`, binary at 53854a0 + this change):

- **Structure tree read-back** (pdfium `FPDF_StructTree*`, script in
  `/tmp/core111/readback.py` against a fixture with h1/h2/p/table/ul/figure):
  `Document → Div → {H1 (T="Summary"), P, H2 (T="Observations"),
  Table → THead→TR→TH×2 + TBody→(TR→TD×2)×2, L→LI×2,
  Figure (Alt="Quarterly chart") → P}`. `FPDFCatalog_IsTagged = true`,
  `/Lang = en`. Figure alt propagates from `<img alt>`; figcaption text is
  the P child of the Figure group.
- **UA-1 gate**: the same fixture renders clean under
  `Accessibility::UA1`. Removing `<title>`/`<title>`-metadata/lang produces
  `Validation([MissingHeadingTitle, NoDocumentTitle])`; removing `<img alt>`
  produces `Validation([MissingAltText])` — the validator demonstrably gates.
- **Byte stability (untagged)**: fixture rendered through this branch and
  through main's pre-change binary — identical SHA-256.
- **WPT gate**: full run on the branch = 114 PASS / 169 FAIL (283 tests).
  The committed main baseline report predates the current harness bucket set
  (63-test intersection), so a naive diff flags 8 "regressions". Each of the
  8 was re-run on BOTH binaries in isolation: MAIN now also fails all 8
  (identical statuses and messages: page-count mismatches / fuzzy-budget
  pixel diffs). The stale baseline recorded PASS under an older harness
  revision; against today's harness, branch == main on every test. Zero
  true regressions.
- **Determinism (tagged)**: two tagged renders of one input — identical
  SHA-256; tagged output differs from untagged (structure objects present).
- **Residuals / follow-ups**:
  - krilla requires list numbering on `L`; the engine draws no markers yet,
    so `L` carries `ListNumbering::None` (legal, honest).
  - Margin boxes are Artifacts (Header/Footer kinds available later if we
    want finer artifact typing; currently Layout for fills).
  - VeraPDF external validation not yet run — krilla's UA-1 validator is the
    recorded gate for this iteration; an Acrobat/PAC spot-check remains as
    optional follow-up evidence.

## Edge Cases

- Fragment with `source: None` under a sourced ancestor (anonymous blocks) →
  inherits the nearest sourced ancestor; genuinely unsourced subtrees
  (margin boxes) → artifacts.
- A split element (paragraph across pages) → multiple draws across pages, all
  leaves under ONE `P` group; the tree is document-structured, not
  page-structured. Ordering key: (page index, MCID) ascending.
- `th` without `scope` attribute → `TableHeaderScope` inferred: first row of
  the table → Column, otherwise Row (HTML spec heuristic, deterministic).
- Nested lists → `L` groups nest inside the outer `LI`'s `LBody`.
- `<img>` without `alt` → Figure with NO alt text: legal unvalidated; the
  UA validator flags it when `--ua` (correct behavior — authors must fix).
- Empty elements (no draws) → group with zero leaf children, kept (roles
  still meaningful for navigation).
- Elements whose display computes to `none` → no fragments, no draws → no
  group (the walk covers DOM elements, but only ones with draws OR visible
  descendants produce groups; simplest correct rule: emit groups only along
  chains that own at least one draw).
- Watermark + tagged → watermark is an Artifact; excluded from the tree.

## References

- `engine/src/pdf.rs` and `engine/src/tags.rs` — tagging, artifact, and
  structure-tree emission.
- krilla 0.8.2 `src/interchange/tagging/mod.rs` (+ `generated.rs`),
  `src/surface.rs::start_tagged`, `src/document.rs::set_tag_tree`,
  `src/configure/mod.rs::with_accessibility_validator` — verified 2026-08-24.
- `pdf-metadata.spec.md` (title source for UA conformance).
- PDF/UA-1 (ISO 14289-1) basics; PDF-400 marked-content model.
