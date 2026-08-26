---
title: Typeanvil CLI Reference
type: runbook
status: approved
owner: elijah
created: 2026-08-26
updated: 2026-08-26
sidebar_position: 3
tags: [cli, usage, reference]
---

# Typeanvil CLI Reference

The engine ships one subcommand, `render`. It reads an HTML file and writes
a PDF.

```text
typeanvil render <input.html> [flags] -o <output.pdf>
```

## Flags

| Flag | Values | Default | Meaning |
|---|---|---|---|
| `-o`, `--output` | path | required | Output PDF path. |
| `--page-width` | `in` `pt` `px` `cm` `mm` | required | Page width. |
| `--page-height` | `in` `pt` `px` `cm` `mm` | required | Page height. |
| `--margin-top` | length | `0` | Top page margin. |
| `--margin-right` | length | `0` | Right page margin. |
| `--margin-bottom` | length | `0` | Bottom page margin. |
| `--margin-left` | length | `0` | Left page margin. |
| `--base-url` | URL/path | empty | Base for resolving document-relative resources. |
| `--title` | string | from `<title>` | PDF metadata title (overrides the document). |
| `--author` | string | from document | PDF metadata author. |
| `--tagged` | flag | off | Emit a tagged-PDF logical structure tree. |
| `--ua` | flag | off | Tagged output plus krilla's PDF/UA-1 validation. Implies `--tagged`. |
| `--diagnostics` | `json` \| `text` | none | Print CSS/layout diagnostics after the render. |

## Length units

Lengths accept CSS absolute units only: `in`, `pt`, `px`, `cm`, `mm`.
A bare number means points (`pt`). Relative units (`%`, `em`) are not
accepted.

Examples: `8.5in`, `595pt`, `210mm`.

## Diagnostics

`--diagnostics json` prints one schema-versioned JSON document to stdout
after the PDF is written. See
[the diagnostics spec](/specifications/diagnostics) for the schema.

`--diagnostics text` prints one line per event to stderr.

Either way, the PDF bytes are identical to a run without the flag.

## Examples

Render US Letter with half-inch margins:

```bash
typeanvil render report.html \
  --page-width 8.5in --page-height 11in \
  --margin-top 0.5in --margin-right 0.5in \
  --margin-bottom 0.5in --margin-left 0.5in \
  -o report.pdf
```

Render A4 portrait:

```bash
typeanvil render invoice.html \
  --page-width 210mm --page-height 297mm \
  -o invoice.pdf
```

Render with metadata and machine-readable diagnostics:

```bash
typeanvil render paper.html \
  --page-width 8.5in --page-height 11in \
  --title "Q3 Report" --author "A. Author" \
  --diagnostics json \
  -o paper.pdf
```

## Notes

- There is no `--help` flag yet. This page is the reference.
- Author CSS wins over CLI geometry: an `@page { size: … }` rule in the
  document overrides the command-line page size.
- Styles come from `<style>` elements in the document. External stylesheet
  files via `<link rel="stylesheet">` are not supported yet.
