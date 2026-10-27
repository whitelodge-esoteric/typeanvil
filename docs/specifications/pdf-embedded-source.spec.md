---
title: PDF Embedded Source (Machine-Readable Sidecar)
slug: /specifications/pdf-embedded-source
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 77
tags: [engine, pdf, ai-native, attachments, determinism]
spec_id: pdf-embedded-source
issue_id: CORE-577
applies_to: engine 0.x
dependencies: [pdf-metadata, tagged-pdf]
---

# PDF Embedded Source (Machine-Readable Sidecar)

## Overview

A downstream agent that reads a TypeAnvil PDF should not need text
extraction to recover the content. With `--embed-source`, `render` attaches
the exact input file (HTML or Markdown) to the PDF as an embedded file.
With `--embed-data <file>`, it attaches a structured data file (for example
the JSON an invoice was generated from). Tagged PDF (CORE-378) gives the
reading structure; the sidecar gives the exact source bytes.

## Goals

- Opt-in attachment of the source document and of data files.
- Discoverable relationship: `/AFRelationship /Source` or `/Data`, listed in
  the catalog `/AF` array.
- Byte-identical output across runs.
- No change to output when neither flag is given.

## Non-Goals

- Template data binding. The engine has no template input today, so
  `--embed-data` attaches a file the caller names; it does not read it.
- PDF/A-3 conformance and Factur-X / ZUGFeRD profiles. Those need an
  archival validator, XMP extension schemas and a fixed file name
  (`factur-x.xml`). The association keys written here are the same keys
  those standards use, so a later profile can build on this. Follow-up
  work belongs in a new issue.
- Embedding linked resources (stylesheets, images, iframes).
- MCP `render` tool parameters.

## Decision: krilla `EmbeddedFile`, PDF 2.0 when attaching

krilla 0.8.2 exposes `krilla::embed::EmbeddedFile` and
`Document::embed_file`. It writes the file stream, the `/EmbeddedFiles`
name tree and the file specification. It writes `/AFRelationship` and the
catalog `/AF` array only when the configuration "specifies associated
files": PDF 2.0, or a PDF/A-3 validator. TypeAnvil therefore switches the
file version to PDF 2.0 when at least one attachment is present. The
default version (1.7) is kept when nothing is attached, so every existing
render keeps its bytes.

PDF/UA-1 caps the version at 1.7. Under `--ua` the files are still embedded
(`/EmbeddedFiles`), but without `/AFRelationship` and `/AF`.

The modification date is never set (`modification_date: None`), so no
timestamp enters the file. krilla's file `/ID` derives from content.

Verified 2026-10-25 against the krilla 0.8.2 sources
(`src/interchange/embed.rs`, `src/chunk_container.rs`).

## Behavior

1. Without `--embed-source` and `--embed-data`, `render` SHALL produce the
   same bytes as before this feature.
2. With `--embed-source`, the PDF SHALL carry one embedded file whose bytes
   equal the input file's bytes exactly. Its name SHALL be the input file's
   base name. Its MIME type SHALL be `text/markdown` for Markdown input and
   `text/html` otherwise. Its relationship SHALL be `Source`.
3. Each `--embed-data <file>` (repeatable) SHALL add one embedded file with
   that file's exact bytes, its base name, relationship `Data`, and a MIME
   type from the extension: `.json` `application/json`, `.xml`
   `application/xml`, `.csv` `text/csv`, else `application/octet-stream`.
4. When any file is embedded and `--ua` is absent, the PDF SHALL be version
   2.0 and SHALL list every embedded file in the catalog `/AF` array, each
   with its `/AFRelationship`.
5. Embedded files SHALL carry no modification date. Two renders with the
   same inputs and flags SHALL be byte-identical.
6. Two embedded files with the same base name SHALL fail the render with a
   usage error naming the duplicate. An unreadable `--embed-data` file SHALL
   fail the render.
7. Tagging, metadata and page content SHALL be the same as without the
   flags, apart from the version header and the attachment objects.

## Interfaces

```text
typeanvil render in.md --embed-source [--embed-data data.json]... -o out.pdf
```

Rust (`engine/src/pdf.rs`):

```rust
pub enum AttachmentRelation { Source, Data }
pub struct Attachment {
    pub name: String, pub mime_type: String, pub description: String,
    pub relation: AttachmentRelation, pub data: Vec<u8>,
}
pub fn render_with_attachments(layout, dom, meta, tagged, ua,
    attachments: &[Attachment]) -> Result<Vec<u8>>;
```

`pipeline::RenderInput` gains `attachments: Vec<Attachment>`.

## Acceptance Criteria

Tests live in `engine/tests/it/core577_embedded_source.rs` (Rust,
`flate2`-decoded read-back) and `harness/tests/test_embedded_source.py`
(pypdfium2 read-back of the attachment).

- AC-1 (round trip): Given a Markdown file with non-ASCII text, when it
  renders with `--embed-source`, then pypdfium2 lists one attachment with the
  file's name and its data equals the source bytes.
- AC-2 (relationship): the PDF header is `%PDF-2.0`, the file spec carries
  `/AFRelationship /Source`, and the catalog has an `/AF` array.
- AC-3 (data file): `--embed-data invoice.json` adds an attachment with
  `/AFRelationship /Data`, subtype `application/json`, exact bytes.
- AC-4 (determinism): two renders with `--embed-source` are byte-identical,
  and the output contains no `/ModDate` or `/CreationDate`.
- AC-5 (no change by default): a render without the flags has no
  `/EmbeddedFiles` and keeps the `%PDF-1.7` header.
- AC-6 (`--ua`): with `--ua --embed-source`, the file is embedded and the
  render succeeds.
- AC-7 (duplicate): `--embed-source` plus `--embed-data` of a file with the
  input's base name fails with a non-zero exit.

## Edge Cases

- An empty source file embeds a zero-length stream.
- Input from a path with directories: only the base name is the attachment
  name.

## References

- CORE-577 (this feature), CORE-570 (AI-native epic), CORE-378 (tagged PDF).
- ISO 32000-2 §14.13 Associated files.
- krilla 0.8.2 `interchange/embed.rs`.
