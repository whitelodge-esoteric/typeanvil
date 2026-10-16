---
title: PDF Document Metadata
slug: /specifications/pdf-metadata
type: spec
status: draft
owner: maintainers
created: 2026-08-21
updated: 2026-10-16
sidebar_position: 13
tags: [engine, pdf, metadata, determinism]
spec_id: pdf-metadata
applies_to: engine 0.x
dependencies: [wpt-conformance-harness]
---

# PDF Document Metadata

## Overview

TypeAnvil deliberately emits no PDF metadata today (`pdf.rs`: "/ID from content
hash, no Metadata") so that identical input produces byte-identical output.
Buyers check Document Properties first; an empty Title/Author reads as
unfinished. This spec adds document-derived metadata through krilla's
`Metadata` API without breaking the determinism promise.

Verified krilla facts (krilla 0.8.2, checked 2026-08-21):

- `krilla::interchange::metadata::Metadata` is a builder: `title(String)`,
  `authors(Vec<String>)`, `description(String)` (→ PDF **Subject**),
  `keywords(Vec<String>)`, `creator`, `producer`, `creation_date(DateTime)`.
- `Document::set_metadata(Metadata)` installs it.
- Omitting `creation_date` writes **no** `CreationDate`/`ModDate` anywhere (Info
  dict or XMP) — determinism is preserved by never setting it.
- The XMP packet's `xmpMM:InstanceID` is `stable_hash_base64(pdf_bytes)`
  (chunk_container.rs:159) — content-derived, therefore deterministic.

## Goals

1. `<title>` → PDF Title; `<meta name="author">` → Author;
   `<meta name="description">` → Subject; `<meta name="keywords">` → Keywords.
2. Byte-identical output for identical input, WITH metadata present.
3. Optional CLI overrides `--title` / `--author` (additive flags only).

## Non-Goals

- No invented document metadata (title etc.) when the document declares none (see Decision).
- No creation/modification dates from any source — the clock is never read.
- No XMP custom schemas, no PDF/A or PDF/UA conformance (`tagged-pdf.spec.md` owns tagged
  PDF, which will revisit validation errors).
- No woff/woff2 concerns here (fonts are `font-resolution.spec.md`'s territory).

## Decision: absent document metadata stays absent; Producer is always set

When the document declares no title and no `--title` override, the engine sets
no document-derived field (no invented "Untitled"). Since CORE-389 the engine
always sets `Producer` = `TypeAnvil <CARGO_PKG_VERSION>` so every output
identifies its engine (Prince, WeasyPrint, Chromium and Adobe do the same).
DECIDED 2026-10-13 (user): the string includes the version. The cost is that
PDFs from different releases differ in those bytes. A constant string keeps
same-binary output byte-identical, so determinism holds. `Creator` names the
source authoring application, which is the user's, so the engine never sets it.

## Behavior

1. Metadata extraction SHALL read only the parsed DOM:
   - Title = concatenated text of the first `<title>` element, trimmed.
   - Author = `content` of the first `<meta name="author">`.
   - Subject = `content` of the first `<meta name="description">`.
   - Keywords = `content` of the first `<meta name="keywords">`, split on `,`
     and trimmed per entry; empty entries dropped.
2. `--title <s>` SHALL replace the document-derived Title; `--author <s>`
   SHALL replace the document-derived Author. Other fields are not
   CLI-overridable in this pass.
3. `render` SHALL always call `Document::set_metadata` with the non-empty
   document fields plus `Producer` = `TypeAnvil <CARGO_PKG_VERSION>`. The
   engine SHALL never set `creation_date` (no `CreationDate`, `ModDate` or
   `xmp:CreateDate`) and SHALL never set `creator`.
4. Rendering behavior (pages, text, fonts, `/ID`) SHALL be unchanged whether or
   not metadata is present.
5. The harness adapter contract stays additive: existing flags keep their exact
   meaning; `--title`/`--author` are new optional flags accepted after the
   positional input.

## Interfaces

```rust
// pdf.rs
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,     // single author → vec![author]
    pub subject: Option<String>,
    pub keywords: Vec<String>,
}

/// Existing signature unchanged (no metadata) — keeps all current callers.
pub fn render(layout: &Layout) -> Result<Vec<u8>>;

/// New: render with document metadata applied via Document::set_metadata.
pub fn render_with_metadata(layout: &Layout, meta: &DocumentMetadata) -> Result<Vec<u8>>;
```

CLI (main.rs, hand-parsed like every flag):

```text
typeanvil render in.html [flags] --title "Q3 Report" --author "A. Author" -o out.pdf
```

Extraction helper lives in `engine/src/metadata.rs`, shared by the CLI and
integration tests (registered as `pub mod metadata` in `lib.rs`):

```rust
pub fn extract_metadata(dom: &Dom, title_override: Option<String>, author_override: Option<String>) -> DocumentMetadata
```

## Acceptance Criteria

- AC-1 (determinism): rendering the same input twice with full metadata yields
  byte-identical files. Given two `render_with_metadata` calls on one layout,
  When both outputs are compared, Then they are equal.
  Test: `engine/tests/metadata.rs::metadata_render_is_deterministic`.
- AC-2 (title lands): a document with `<title>` produces a PDF whose Info dict
  carries that Title. Given an HTML with `<title>Quarterly Report</title>`,
  When rendered and read back (pypdfium2/pypdf or raw `/Title` inspection),
  Then Document Properties show `Quarterly Report`.
  Test: `engine/tests/metadata.rs::title_lands_in_document_properties`.
- AC-3 (absence): a document without title/meta and without CLI overrides
  produces an Info dict whose only entry is `Producer (TypeAnvil <version>)`.
  No Title/Author/Subject/Keywords/Creator key appears.
  Test: `engine/tests/metadata.rs::no_metadata_when_document_declares_none`.
- AC-6 (producer, CORE-389): every render carries
  `Producer (TypeAnvil <CARGO_PKG_VERSION>)`, alongside any document fields
  unchanged. Two renders are byte-identical, and the file has no
  `CreationDate`, `ModDate`, `xmp:CreateDate` or `Creator`.
  Test: `engine/tests/metadata.rs::producer_always_set_without_dates`.
- AC-4 (overrides): `--title`/`--author` replace doc-derived values.
  Test: `engine/tests/metadata.rs::cli_overrides_replace_document_values`
  (exercised through `parse_render_args` + extraction, since main.rs is a
  binary crate).
- AC-5 (no regression): full WPT suite shows 0 regressed (metadata cannot
  affect page rasters; gate confirms).

## Edge Cases

- Empty `<title></title>` → treated as absent (trim to empty = None).
- Multiple `<title>` elements → first wins (matches browser behavior of using
  the document head title).
- `<meta name="Author">` case variants → name matched case-insensitively? NO —
  match exactly `"author"` lowercase; HTML authors write lowercase in practice
  and exact matching is deterministic. Recorded as a known limitation.
- Non-ASCII titles → krilla writes TextStr (UTF-16 where needed); verified via
  read-back test with an em dash.
- Keywords with trailing commas / duplicate spaces → trimmed, empties dropped.

## References

- `engine/src/pdf.rs` — metadata application and deterministic PDF emission.
- krilla 0.8.2 `src/interchange/metadata.rs`, `src/chunk_container.rs`
  (instance-id hashing) — verified 2026-08-21.
- Determinism contract: module docs of `engine/src/pdf.rs`.
- `tagged-pdf.spec.md` — the separate tagged-PDF and validator contract.
