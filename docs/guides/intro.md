---
title: Introduction
type: guide
status: approved
owner: maintainers
created: 2026-10-14
updated: 2026-10-17
sidebar_position: 1
tags: [guide, introduction, overview]
slug: /guides
---

# Introduction

TypeAnvil is an open-source HTML → PDF typesetting engine written in Rust. It
turns HTML documents that use paged-media CSS into PDFs for reports, invoices,
letters, papers, and books.

It is for developers who need print-quality pagination without a commercial
rendering engine or a headless browser.

## What TypeAnvil is

TypeAnvil is a batch document compiler. It reads one HTML file, collects its
CSS, applies paged-media layout rules, and writes a PDF. You run it as a
command-line tool, `typeanvil render`. The CLI is a thin driver over a Rust
library, and the engine's integration tests use that library directly.

The engine is a static renderer. It does not execute JavaScript, provide
browser APIs, or support interactive layout. It does not cover all of CSS
either. The [feature specifications](../specifications/paged-media-css.spec.md)
define what has shipped and list the known gaps. Expected behavior comes from
the CSS specifications. PrinceXML and Chromium are only comparison tools (see
[CSS standards alignment](../conventions/css-standards-alignment.md)).

### What the engine offers

- **Deterministic output.** The same input bytes, flags, engine version, and
  available fonts produce a byte-identical PDF, so you can diff-test documents
  in CI. System fonts are part of the input: a different font set can change
  the output.
- **Typography pipeline.** Text shaping, line breaking, and hyphenation are
  built into the engine (see the
  [typography specification](../specifications/typography-layer.spec.md)).
  Pagination handles widows and orphans (see the
  [fragmentation specification](../specifications/fragmentation-core.spec.md)).
- **Paged-media CSS.** Supported features include `@page` rules with margin
  boxes, named pages, page counters, running headers (`string-set`),
  footnotes, multi-column layout, floats, and table headers and footers that
  repeat on each page. Each feature has a specification that lists its limits.
- **Accessibility output.** Every PDF is tagged by default, with a logical
  structure tree; `--no-tagged` turns that off. `--ua` also runs a PDF/UA-1 validator and fails the render
  on any violation. See the
  [tagged PDF specification](../specifications/tagged-pdf.spec.md).
- **Diagnostics.** `--diagnostics json|text` reports CSS the engine does not
  recognize. It never changes the PDF bytes.

## What documents it targets

TypeAnvil targets paged documents, meaning content laid out on fixed pages
rather than in a scrolling viewport. Typical inputs are reports, invoices,
letters, papers, and books, including long documents with running headers,
page counters, and tables that span many pages.

Input is one local HTML file. CSS comes from `<style>` elements and from local
`<link rel="stylesheet">` files. `@page` rules set page size, margins,
counters, and margin boxes. The engine does not fetch anything over the
network.

## What the guides cover

| Page | What it covers |
|---|---|
| [Getting started](./getting-started.md) | Install a release binary or build from source, then render your first document. |
| [CLI reference](./cli.md) | The `render` subcommand: every flag, defaults, length units, diagnostics, and exit codes. |
| [Containers and packages](./container.md) | Rendering with the container image instead of a native binary. |
| [Paged media CSS](./paged-media-css.md) | `@page` rules, margin boxes, counters, and pagination. |
| [CSS support](./css-support.md) | Which CSS features the engine supports, and the known gaps. |

## Deeper references

- [Architecture overview](../architecture/overview.md): the pipeline from HTML
  parser to PDF emitter, plus module boundaries and the determinism boundary.
- Feature specifications, which are the behavior contracts. Start with
  [paged media CSS](../specifications/paged-media-css.spec.md),
  [fragmentation](../specifications/fragmentation-core.spec.md),
  [font resolution](../specifications/font-resolution.spec.md),
  [images](../specifications/images.spec.md), and
  [diagnostics](../specifications/diagnostics.spec.md).
- [CLI operations reference](../operations/cli.md) and the
  [container image runbook](../operations/container-image.md).

## License

TypeAnvil is free software under the
[AGPL-3.0-only](https://www.gnu.org/licenses/agpl-3.0.html) license. The engine
has no license check, watermark, activation flow, phone-home behavior, or
feature gate.
