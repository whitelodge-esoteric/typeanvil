---
title: Markdown Input
slug: /specifications/markdown-input
type: spec
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 71
tags: [cli, markdown, input, metadata]
spec_id: markdown-input
issue_id: CORE-305
applies_to:
  - engine/src/main.rs
  - engine/src/markdown.rs
  - engine/tests/it/markdown.rs
dependencies:
  - cli-surface
  - pdf-metadata
  - diagnostics
  - math-rendering
  - footnotes
  - images
---

# Markdown Input

## Overview

Typeanvil renders HTML to PDF. This spec adds Markdown as a second input
format. The engine keeps one canonical layout input: HTML. A Markdown file is
converted to an HTML string, and that string enters the existing `Dom::parse`
pipeline. No layout, fragmentation, or PDF code changes.

This spec is a definition. It ships no engine code. Implementation is
follow-on work (see Follow-on issues).

## Goals

1. `typeanvil render doc.md -o out.pdf` produces a PDF with no extra flags.
2. YAML frontmatter feeds the existing PDF metadata pipeline.
3. The output looks like a finished document with no author CSS.
4. Author CSS (paged media, `@page`, running headers) still applies.
5. Identical input gives byte-identical PDF (determinism promise).

## Non-Goals

- No second layout path. Markdown never produces a DOM directly.
- No JavaScript, no TeX engine. Math is converted to MathML before layout
  (decision in `math-rendering.spec.md` Non-Goals).
- No Markdown extensions beyond the dialect below (no wikilinks, no
  definition lists, no admonitions, no custom containers).
- No new CLI subcommand.
- No stdin input in this slice.

## Decisions

### D1. Converter: comrak, Markdown → HTML string → html5ever DOM

DECIDED 2026-10-17 (agent; approved by owner 2026-10-17). Use the `comrak` crate to render HTML, then call
the existing `Dom::parse`.

Rationale:

- HTML stays the only canonical input. Every HTML-input spec and test applies
  to Markdown output by construction.
- Raw HTML in Markdown must pass through html5ever anyway. A direct
  Markdown → DOM builder would need a second DOM path for that HTML.
- `comrak` implements CommonMark and the GFM extensions in one crate, with
  footnotes, tables, task lists, strikethrough, autolinks, and math options.
  It is pure Rust, so the zero-C-FFI rule holds.
- `pulldown-cmark` was the alternative. It is an event stream, so the engine
  would own the HTML writer, the footnote placement, and the table
  alignment mapping. That is effort with no layout benefit.
- Cost: one string round-trip per render. This is small against layout time.

The crate version and feature flags are fixed by the implementation PR. The
implementation shall disable comrak default features that pull a syntax
highlighter or CLI dependencies.

### D2. Input contract: `render` accepts `.md` directly

DECIDED 2026-10-17 (agent; approved by owner 2026-10-17). No new subcommand.

- `typeanvil render input.md` selects Markdown when the extension is `.md`,
  `.markdown`, or `.mdown` (ASCII case-insensitive). Every other extension is
  HTML (unchanged).
- `--from <html|markdown>` overrides the extension. It is the only new flag
  of this slice. `--from` is the contract for stdin in a later slice.
- All existing `render` flags keep their meaning, including page geometry,
  `--base-url`, `--title`, `--author`, `--tagged`, `--ua`, and
  `--diagnostics`. The harness adapter contract in `harness/engine.py` does
  not change.

### D3. Dialect: CommonMark 0.31 plus a named GFM subset

DECIDED 2026-10-17 (agent; approved by owner 2026-10-17). The baseline is CommonMark. The enabled
extensions are:

| Extension | Maps to |
|---|---|
| Tables (pipe, with alignment) | `<table>` with `<thead>`/`<tbody>`; alignment as `style="text-align:…"` |
| Strikethrough (`~~x~~`) | `<del>` |
| Task lists (`- [ ]`, `- [x]`) | `<input type="checkbox" disabled>` inside `<li>` (see Gaps) |
| Autolinks (bare URLs) | `<a href>` |
| Footnotes (`[^id]`) | footnote model of `footnotes.spec.md` (see Behavior 9) |
| Heading ids | `id` attribute on each heading, GitHub-style slug, for internal links and bookmarks |
| Smart punctuation | off (a typographic choice stays with the author) |
| Math (`$…$`, `$$…$$`) | MathML (see Behavior 10) |
| Frontmatter (`---` YAML) | metadata (see Behavior 3) |

Fenced code blocks (``` and ~~~) render as `<pre><code class="language-X">`.
The engine applies no syntax highlighting in this slice.

Raw HTML passes through unchanged (CommonMark behavior). The engine is not a
sanitizer: Typeanvil renders local trusted files and runs no scripts, so
stripping HTML gives no safety gain and would break documents that embed
`<div class="…">`, `<img>`, or page-break helpers.

### D4. Default stylesheet: shipped, small, replaceable

DECIDED 2026-10-17 (agent; approved by owner 2026-10-17). Markdown output has no author CSS, and the UA
print defaults (`ua-print-defaults.spec.md`) are plain. The engine shall
insert one embedded stylesheet as the FIRST `<style>` element in the
generated `<head>`. It supplies body font and size, heading scale and
`break-after: avoid`, `pre`/`code` monospace and wrapping, table borders and
header-row repeat, blockquote rule, and `img { max-width: 100% }`. It sets no
`@page` size or margins: page geometry stays with the CLI flags.

An author overrides it two ways:

- `--css <file>` (repeatable) appends `<link rel="stylesheet">`-equivalent
  author sheets after the default sheet. Later rules win by the normal cascade.
- Raw `<style>` elements in the Markdown body come after the default sheet and
  win the same way.
- `--no-default-css` omits the embedded sheet.

`--css` and `--no-default-css` are rejected for HTML input with a non-zero
exit (they have no HTML meaning).

## Behavior

1. The engine shall select the input format per D2 before reading the file.
2. For Markdown input the engine shall build one HTML document of the form
   `<!doctype html><html lang=…><head><meta charset="utf-8">[title/meta]
   [default sheet][--css sheets]</head><body>…converted…</body></html>` and
   pass it to `Dom::parse`.
3. A leading YAML frontmatter block (first line exactly `---`, closed by `---`
   or `...`) shall be removed from the body and mapped as follows:
   `title` → `<title>`; `author` (string or list) → `<meta name="author">`
   (list joined as the existing metadata pipeline expects); `description` →
   `<meta name="description">` (PDF Subject); `keywords` (string or list) →
   `<meta name="keywords">`; `lang` → `<html lang>`. Unknown keys are ignored
   and reported as a diagnostic. `--title` / `--author` flags override
   frontmatter (same precedence as they override HTML `<title>`).
4. Frontmatter that is not valid YAML, or whose top level is not a mapping,
   shall exit 1 with a message naming the line. No partial render.
5. With no frontmatter `title`, the engine shall NOT invent a title from the
   first heading (`pdf-metadata.spec.md`: absent metadata stays absent).
6. Relative image and link paths shall resolve exactly as for HTML input:
   against `--base-url` when given, otherwise the process working directory
   (`images.spec.md`). Markdown does not change this rule.
7. Invalid UTF-8 input shall exit 1 with the same message as HTML input.
   A UTF-8 BOM shall be stripped.
8. Conversion shall be deterministic: no clock, no randomness, no map
   iteration order in output (heading id de-duplication uses `-1`, `-2`
   suffixes in document order).
9. Footnotes: each `[^id]` reference with a definition shall become an inline
   `<span style="float: footnote">` holding the definition content at the
   reference point, the form `footnotes.spec.md` already renders. Definitions
   shall not also render as a trailing list. A reference with no definition
   renders as literal text. Numbering stays with the engine.
10. Math: `$…$` becomes inline `<math>` and `$$…$$` (own paragraph) becomes
    `<math display="block">`, using the MathML subset in
    `math-rendering.spec.md`. A `$` followed by whitespace or preceded by a
    digit-currency pattern is not math (so `$5 and $6` stays text). Escaped
    `\$` is a literal dollar. Math that the converter cannot translate renders
    as its source in a `<code>` and emits a diagnostic; it never aborts.
11. `--diagnostics` shall work unchanged. Markdown conversion adds events
    with `source: "markdown"` for: unknown frontmatter key, untranslatable
    math, and unresolved footnote reference. Stylesheet diagnostics apply to
    the combined default + author CSS.
12. `--tagged` / `--ua` shall work unchanged: the generated HTML is ordinary
    semantic HTML (`h1`–`h6`, `p`, `ul`, `table`, `figure` not used), so the
    tag tree follows `tagged-pdf.spec.md`.
13. `typeanvil --help` shall list `--from`, `--css`, and `--no-default-css`,
    and say that `render` accepts HTML or Markdown input.
14. Exit codes: 0 success; 1 for a read, frontmatter, conversion, or render
    error; 2 for a usage error (unknown `--from` value, `--css` with HTML
    input). Same convention as `cli-surface.spec.md`.

## Interfaces

```text
typeanvil render doc.md -o out.pdf [page flags] [--title T] [--author A]
                        [--from html|markdown] [--css file]... [--no-default-css]
                        [--base-url dir] [--tagged] [--ua] [--diagnostics json|text]
```

```rust
// engine/src/markdown.rs
pub struct MarkdownDoc { pub html: String, pub diagnostics: Vec<Diagnostic> }
pub fn convert(source: &str, css: &[String], default_css: bool) -> Result<MarkdownDoc>;
```

`main.rs` calls `convert`, then `Dom::parse(&doc.html)`, then appends
`doc.diagnostics` to the diagnostics list.

## Inherited specs

The Markdown path inherits all behavior of the specs below, because it enters
after HTML parsing:

| Spec | What the Markdown path gets |
|---|---|
| `paged-media-css.spec.md`, `named-pages.spec.md`, `string-set-running-headers.spec.md`, `page-floats.spec.md` | author CSS in `--css` or `<style>` drives pages, margin boxes, running headers |
| `ua-print-defaults.spec.md` | the base UA sheet under the default Markdown sheet |
| `tables-fragmentation.spec.md`, `auto-table-layout.spec.md`, `table-backgrounds.spec.md`, `table-cell-vertical-align.spec.md` | GFM tables |
| `images.spec.md`, `svg-images.spec.md` | `![alt](src)` images, SVG |
| `footnotes.spec.md` | footnotes (Behavior 9) |
| `hyperlinks.spec.md`, `cross-references.spec.md`, `pdf-outlines.spec.md` | links, `#heading-id` links, bookmarks from headings |
| `list-markers.spec.md`, `white-space-pre.spec.md` | lists, fenced code |
| `math-rendering.spec.md` | math (Behavior 10) |
| `pdf-metadata.spec.md`, `diagnostics.spec.md`, `tagged-pdf.spec.md`, `font-resolution.spec.md`, `typography-layer.spec.md` | metadata, diagnostics, tags, fonts, hyphenation |

## Gaps to close

Each gap becomes a follow-on issue (see below). Probe at the implementation
branch point before treating any of these as confirmed; they come from this
definition, not from a measurement.

1. **Task-list checkbox.** `<input type="checkbox">` may have no layout box.
   If so, emit a `☐` / `☑` character in a `<span class="task">` instead.
2. **Long code lines.** `pre` with long lines must wrap (`white-space:
   pre-wrap`) rather than overflow the page; verify `white-space-pre.spec.md`
   covers `overflow-wrap`.
3. **Code highlighting.** Not in this slice. Fenced code carries a `language-X`
   class so a later issue or author CSS can hook it.
4. **Frontmatter YAML parser.** The engine has no YAML dependency. The
   implementation shall use a small crate (for example `serde_yaml`-class) or
   a restricted scalar/list parser; the choice is the implementation PR's,
   and Behavior 4 fixes the observable result.
5. **Math translation.** `math-rendering.spec.md` names this step as part of
   CORE-305. Converting TeX to MathML needs a TeX subset translator. It is
   large enough to ship as its own issue after the base slice. Until then,
   Behavior 10 falls back to `<code>` source with a diagnostic.
6. **Footnote float inline form.** Verify `float: footnote` on a `<span>`
   inside a table cell and list item paginates (footnotes spec limits).

## Acceptance Criteria

Fixtures live in `harness/direct_fixtures/markdown/` and are listed in
`harness/direct_manifest.json` with `input_source: "fixture"`. Unit tests live
in `engine/tests/it/markdown.rs`. CLI tests use the `cli_surface.rs` style.

- **AC1** (D2, B1): Given `basic.md` (headings, paragraphs, lists, emphasis),
  when run through `render`, then the PDF has ≥ 1 page and the text layer
  contains each heading text. `basic.html` with the same body renders the same
  page count when the default sheet is disabled via `--no-default-css`.
- **AC2** (D2): `render doc.txt --from markdown` renders as Markdown;
  `render doc.md --from html` renders as HTML; `--from rst` exits 2.
- **AC3** (B3): `frontmatter.md` with `title`, `author` (list), `description`,
  `keywords`, `lang` → PDF Info has Title, Author, Subject, Keywords. With
  `--title X` the Title is `X`.
- **AC4** (B4): `bad-frontmatter.md` exits 1; stderr names the line; no PDF
  written.
- **AC5** (B5): `no-title.md` has no PDF Title.
- **AC6** (D3): `gfm-table.md` renders a table whose header row text and
  right-aligned column position are asserted by `direct_manifest` text
  checks. Strikethrough text produces a `<del>` in the generated HTML (unit
  test on `convert`).
- **AC7** (D3): `code-fence.md` embeds a monospace face (`/BaseFont` check)
  and keeps every source line's text in the text layer.
- **AC8** (D3): `raw-html.md` containing `<div class="x">` and a `<style>`
  rule styled by that class renders with the rule applied (page ink check).
- **AC9** (B6): `image.md` with a relative image and `--base-url` embeds one
  image; a missing image follows the `images.spec.md` broken-image rule.
- **AC10** (B9): `footnote.md` with two references yields two footnote
  areas at the page bottom with numbers 1 and 2; an undefined reference stays
  literal text and a diagnostic event is emitted.
- **AC11** (D4): `basic.md` rendered with `--css extra.css` where the sheet
  sets `h1 { color: red }` shows red ink; `--no-default-css` changes the
  body font size relative to the default (unit test on generated HTML:
  default sheet present/absent).
- **AC12** (D4): `@page { size: 5in 3in }` in a `--css` sheet changes the
  page size; the default sheet itself contains no `@page` rule (unit test).
- **AC13** (B8): rendering `basic.md` twice gives byte-identical PDFs.
- **AC14** (B11): `--diagnostics json` on a doc with an unknown frontmatter key
  reports one event with `source: "markdown"`.
- **AC15** (B13, B14): `--help` text lists `--from`, `--css`,
  `--no-default-css`; `render x.html --css a.css` exits 2.
- **AC16** (B10, after the math follow-on): `math.md` produces `<math>`
  elements; before it lands, the fallback `<code>` source and a diagnostic.
- **AC17** (no regression): the full `cargo test` suite and the release gate
  show zero changes for HTML input (the Markdown code is not on the HTML path).

## Edge Cases

- Empty file: one empty page, exit 0 (same as an empty HTML body).
- Frontmatter-only file: empty body, metadata applied.
- A `---` line later in the file is a thematic break, not frontmatter.
- Heading id collisions get `-1`, `-2` suffixes in document order.
- CRLF line endings are normalized before conversion.
- Windows-style `\` paths in image sources are not rewritten.

## Branding updates that land with the feature

After the feature ships (not before), the implementation PR shall update:

- npm package description and keywords, and the npm README;
- `--help` text (Behavior 13) and `docs/` CLI reference;
- `README.md` line "The CLI currently accepts standalone HTML files." to name
  Markdown;
- the docsite tagline and the release README.

Until the feature ships, all public text stays HTML-only (the correction
already made under CORE-305's docs-alignment cleanup).

## Follow-on issues

Create these after this spec is approved. Each is one defect or one slice:

1. Slice 1: `markdown.rs` converter, D2 CLI routing, default sheet, fixtures
   AC1–AC9, AC11–AC15, AC17.
2. Slice 2: footnotes mapping (AC10) and the inline-float gap (Gap 6).
3. Slice 3: TeX → MathML translator (AC16, Gap 5). Depends on the
   `math-rendering` slice that renders MathML.
4. Slice 4: branding updates.
5. Later: stdin input, syntax highlighting.

## References

- Issue: CORE-305.
- `cli-surface.spec.md`, `pdf-metadata.spec.md`, `diagnostics.spec.md`,
  `math-rendering.spec.md` (Non-Goals: TeX handled by Markdown preprocessing).
- CommonMark spec 0.31; GFM spec (tables, strikethrough, task lists,
  autolinks).
