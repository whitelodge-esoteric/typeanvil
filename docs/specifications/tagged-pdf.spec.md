---
title: Tagged PDF / PDF-UA Accessibility
slug: /specifications/tagged-pdf
type: spec
status: approved
owner: maintainers
created: 2026-08-24
updated: 2026-10-16
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
lists, tables, figures, links, and language. Since CORE-378 tagging is the
default; `--no-tagged` yields the earlier untagged output, byte-identical.

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
4. With `--no-tagged`, output is byte-identical to the pre-CORE-378 default.
5. Determinism preserved with tagging on (document-order tree, no hash
   iteration, no clock reads).

## Non-Goals

- No `Span`-level language switching or `ActualText` (hyphenation-aware copy)
  in this pass — spans exist in the API but the emitter tags whole draws.
- No list-item markers in `Lbl`: since CORE-385 the engine paints markers
  (`list-markers.spec.md`), but the marker run is owned by the `li` fragment
  and rides in `LBody` with the item. Moving it into `Lbl` is CORE-374.
- No form fields, notes, or math tagging.
- No PDF/A conformance (archival validator untouched).
- No full Acrobat/PAC certification — krilla's validator is the recorded gate;
  external-tool runs are follow-up evidence, not blockers.

## Decision: tagged by default, `--no-tagged` opt-out, artifacts for chrome

DECIDED 2026-10-16 (agent, CORE-378): tagging defaults ON. The structure tree
is proven by the tagged gate (`tagged-gate.spec.md`, CORE-377) and Chrome has
tagged every Save-as-PDF since Chrome 85. Flipping the default changes the
bytes of every default render once; `--no-tagged` restores the previous
default output byte-for-byte. `--tagged` stays accepted as a no-op alias so
existing scripts keep working. `--ua` implies tagging and wins over
`--no-tagged`, in any argument order. History: CORE-111 shipped tagging
opt-in "until the structure tree is proven". Margin-box text, page backgrounds, and the
unlicensed watermark carry no DOM source, so they are tagged as `Artifact`
(Background / Header / Footer kinds) — excluded from the logical tree exactly
as PDF-400/UA-1 intend.

## Behavior

1. When `--no-tagged` is present (and `--ua` is not), `render` SHALL behave
   bit-for-bit as the pre-CORE-378 default did: `enable_tagging: false`, no
   `start_tagged` calls, no tag tree. Without `--no-tagged`, tagging is on.
2. When tagging is on (the default, `--tagged`, or `--ua`), the emitter SHALL set `enable_tagging: true`
   and wrap every draw call in a `start_tagged`/`end_tagged` pair:
   - draws whose owning fragment resolves (through `Fragment::source`,
     inheriting the nearest sourced ancestor) to a DOM element →
     `ContentTag::Other`;
   - draws with no sourced ancestor (margin boxes, page background,
     watermark) → `ContentTag::Artifact` with kind Background (fills) or
     Header/Footer (margin boxes) / Footer (watermark).
   - every page-chrome Background artifact (the `@page` fill, the propagated
     canvas fill, the `@page` outline and border rings, the root border
     ring) SHALL carry a BBox: the painted rect (a ring's outer rect),
     mapped through the page-orientation rotation when one is pushed.
     PDF 1.7 (ISO 32000-1 table 330) makes the BBox mandatory for
     Background artifacts, krilla panics without one, and krilla writes it
     in page space without applying the current transform (CORE-379).
   - every draw inside a page-margin box (text, fills, borders, images) SHALL
     be an Artifact **regardless of owner**: a margin box repeats page
     chrome, so even a copy derived from document content (`string()`,
     a future `element()`) must not enter the tree once per page. The kind
     follows the box's margin row: top row (`@top-*`, top corners) →
     Header; bottom row → Footer; left/right column → PaginationOther
     (CORE-380).
   - footnote bodies (`float: footnote`) are document content: each note's
     lines SHALL be owned by the footnote element, so the body enters the
     tree inside the group of the element that contains the call, at the
     call's position in reading order — not at the foot of the page. Its
     role (`Note`) is CORE-374's (CORE-380).
   - no draw SHALL be emitted outside a marked-content sequence: an
     unsourced text run anywhere else becomes a Layout artifact, the same
     as an unsourced fill (CORE-380).
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
   Each tagged link annotation SHALL carry alternate text (`/Contents`,
   PDF/UA-1 §7.18.5): the `<a>` element's text content with whitespace
   collapsed; if that is empty, its `title` attribute (collapsed); if that is
   empty, its `href`. The untagged path writes no `/Contents`, so untagged
   output stays byte-identical (CORE-381). An image-only link produces no
   annotation today (CORE-391); its alt fallback lands with that fix.

7. With `--ua`, the document SHALL be finished under krilla's accessibility
   validator; any violation aborts the render with the full violation list on
   stderr. Without `--ua`, tagging proceeds without the validator (missing
   alt text degrades gracefully instead of failing).
7a. A tagged render whose outline would be empty (no headings, or every
   heading suppressed by `bookmark-level: none`) SHALL emit a single outline
   entry titled with the document title (the resolved `DocumentMetadata`
   title), with an XYZ destination at the top of page 1. Without a title it
   emits none: the document is already non-conformant (`NoDocumentTitle`),
   and the engine doesn't invent text. Untagged renders keep
   `pdf-outlines.spec.md` §9 unchanged (DECIDED 2026-10-13, user: one entry
   from the title rather than heuristics or leaving letters unconformable;
   CORE-382).

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
typeanvil render in.html [--no-tagged] [--ua] -o out.pdf
```

`--tagged` is accepted as a no-op (tagging is the default). `--ua` implies
tagging and overrides `--no-tagged`.

## Acceptance Criteria

- AC-1 (byte stability, off path): rendering any corpus fixture with and
  without the new code path selected (i.e. `render_with_options` with
  `tagged: false`) produces output identical to the pre-change binary's.
  Test: `engine/tests/tagged_pdf.rs::untagged_path_is_byte_stable` (compares
  against `render_with_metadata` output).
- AC-2 (roles land): a fixture with heading, paragraph, list, and table
  renders tagged; pypdf structure-tree read-back shows `H1`, `P`, `L`/`LI`,
  `Table`/`TR`/`TH`/`TD` elements in document order.
  Verification: `scripts/verify_tagged_pdf.py` (pypdfium2; committed
  2026-10-13 with CORE-373) run against the test output; recorded in this
  spec's validation log below.
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
- AC-12 (default on, CORE-378): a render with no tagging flag carries a
  `/StructTreeRoot`; `--no-tagged` carries none; `--tagged` output equals the
  default output; `--ua` tags even with `--no-tagged`, in either order.
  Test: `engine/tests/tagged_default.rs`. Byte stability: on all 33 corpus
  documents, `--no-tagged` output from the CORE-378 binary is byte-identical
  to the default output of the binary at the branch point (60f8eff); see the
  cost table below.
- AC-8 (page chrome, CORE-379): each page-chrome kind (`@page`
  background, `body` canvas background, `@page` outline, `@page` border,
  `html` border) renders with `--tagged` and with `--tagged --ua`, and every
  Background artifact carries a BBox that lies on the page, including on a
  `page-orientation: rotate-left` page.
  Test: `engine/tests/tagged_page_chrome.rs`.
- AC-9 (nothing untagged, CORE-380): a document with `@top-center` text,
  `@bottom-center` `counter(page)`, a `string()` running header and
  footnotes over several pages renders tagged with zero page objects outside
  marked content; margin-box text is a Header/Footer artifact; each footnote
  body's text is reachable in the structure tree. Corpus check:
  `scripts/verify_tagged_pdf.py` reports `untagged: 0` on every corpus and
  showcase fixture. Test: `engine/tests/tagged_untagged_content.rs`.
- AC-10 (link alt text, CORE-381): an external link, an internal `#id`
  link, a link wrapped over two lines, and a link whose only text is
  whitespace (alt from `title`) each render with `--tagged --ua`, and the
  alt text equals the rule in Behavior 6.
  Test: `engine/tests/tagged_link_alt.rs`.
- AC-11 (outline fallback, CORE-382): a titled document with no headings
  renders with `--tagged --ua` and carries exactly one outline entry, the
  title, pointing at page 1; with headings, the outline is the heading tree
  as before; the untagged render has no `/Outlines`.
  Test: `engine/tests/tagged_outline_fallback.rs`.
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
    want finer artifact typing; currently Layout for fills). Superseded by
    CORE-380 (2026-10-13): margin-box text was in fact drawn untagged; all
    margin-box draws are now Header/Footer/PaginationOther artifacts.
  - VeraPDF external validation not yet run — krilla's UA-1 validator is the
    recorded gate for this iteration; an Acrobat/PAC spot-check remains as
    optional follow-up evidence.

Recorded 2026-10-13 (CORE-373 health check, release/2026.10 `f5241c3`,
host build). Every demo corpus + showcase fixture (32 + 10) rendered
untagged, `--tagged` (twice) and `--tagged --ua`; each tagged PDF read back
with `scripts/verify_tagged_pdf.py --html <source>`. Raw results:
`~/workspace/typeanvil-evidence/CORE-373/` (`health.json`, `scoreboard.md`).

- **Still holds:** untagged and tagged output deterministic (identical
  SHA-256 across two runs) for all 41 renderable fixtures; tagged page counts
  equal untagged; `engine/tests/tagged_pdf.rs` 4/4 green (container).
  `<html lang>`, heading titles, table structure and `<figure>` alt text read
  back correctly.
- **Crash:** `--tagged` aborts on any `@page`/canvas background
  (`Background artifact must have a bounding box in PDF 1.7`) —
  weasyprint-ticket. CORE-379.
- **Behavior §2 violated:** margin-box text and footnote bodies are painted
  outside any marked content (869 untagged text objects across 40
  fixtures). krilla's UA validator does not detect this — 34/42 fixtures
  pass `--ua` regardless. CORE-380.
- **`--ua` failures (7 fixtures):** `MissingAnnotationAltText` on links
  (CORE-381); `MissingDocumentOutline` on heading-less letters/posters
  (CORE-382); `ContainsNotDefGlyph` for `⁻`/`♥` (font coverage, not a
  tagging defect — the glyph is missing untagged too).
- **Role gaps (Goals §1 not met):** no `Strong`/`Em`/`Code` anywhere (text
  attributed to the block); `<img>` outside `<figure>` lands on a `Div` with
  its alt lost; `<caption>` → `Div`; `LI` mostly without `LBody`; 8 fixtures
  with `<a>` produce no `Link`. CORE-374.
- **Reading order:** 14/41 fixtures read back word-for-word in source order.
  Remaining drift: repeated table header/footer rows tagged as content on
  every page (CORE-375); hyphenated fragments (CORE-376); untagged
  footnotes (CORE-380).
- List markers are not painted at all (untagged renders too), so the
  `Lbl`-stays-empty Non-Goal remains accurate.

Recorded 2026-10-13 (CORE-377, release/2026.10 `e687fe3`): the tagged gate
(`scripts/tagged_gate.py`, spec `tagged-gate.spec.md`, runbook
`docs/operations/tagged-gate.md`) is now a required release-gate step.
Green run in the dev container and on the macOS host, identical: 42
fixtures, 0 unexpected failures, 6 known failures (3× `ua`
`ContainsNotDefGlyph` → CORE-386; 3× `dangling` empty marked-content
sequences → CORE-396). Red run: `h1` mapped to `P` fails `roles` on all 36
fixtures whose snapshot has an `H1`, exit 1. weasyprint-ticket's `<h1>`
already reads back without `H1`, a CORE-374 gap.

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

## Validation log: default flip (CORE-378, 2026-10-16)

Candidate bf73ff7 against baseline 60f8eff (release/2026.10 tip), container
debug builds:

- `--no-tagged` output byte-identical to the baseline default on 33 of 33
  corpus documents (AC2).
- Release gate: 283 WPT tests, PASS 183 / FAIL 100 on both sides, 0 changed
  documents, 13 of 13 direct checks pass, `GATE PASSED` (AC3, no raster
  change).
- Tagged gate: 42 fixtures, 6 expected failures (CORE-386, CORE-396),
  0 unexpected, `TAGGED GATE PASSED`.
- `cargo test -j 2 --no-fail-fast`: all test binaries green, including the
  4 new tests in `engine/tests/tagged_default.rs`. Pre-fix, all 4 failed
  (default untagged; `--no-tagged` an unknown flag).

## Cost of tagging (CORE-378, measured 2026-10-16)

Method: the 33 documents in `demo/corpus/` at the corpus geometry (5in x 3in,
0.5in margins), debug build in the dev container, one render each. Base is
the binary at 60f8eff with its default (untagged) output; tagged is the
CORE-378 default. Per-document table: `~/workspace/typeanvil-evidence/CORE-378/ac2-size-time.tsv`.

| Measure | Untagged | Tagged | Change |
|---|---|---|---|
| Total PDF bytes (33 docs) | 2,042,603 | 2,666,898 | +30.6% |
| Total render time, ms | 3,639 | 3,704 | +1.8% (inside run-to-run noise) |
| Largest size ratio | | table-stress 151,253 to 344,482 bytes | 2.28x |

Prose documents grow 8% to 15% (book-chapter +7.7%, paper +8.7%). Table-heavy
documents grow 30% to 130% (financial-statement +50.6%, table-stress +128%),
because every cell is a structure element. Tagging does not touch painting,
so rasters are unchanged (verified by the release gate, see the validation
log).

## References

- `engine/src/pdf.rs` and `engine/src/tags.rs` — tagging, artifact, and
  structure-tree emission.
- krilla 0.8.2 `src/interchange/tagging/mod.rs` (+ `generated.rs`),
  `src/surface.rs::start_tagged`, `src/document.rs::set_tag_tree`,
  `src/configure/mod.rs::with_accessibility_validator` — verified 2026-08-24.
- `pdf-metadata.spec.md` (title source for UA conformance).
- PDF/UA-1 (ISO 14289-1) basics; PDF-400 marked-content model.
