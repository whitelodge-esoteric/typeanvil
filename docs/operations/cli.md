---
title: TypeAnvil CLI Reference
type: runbook
status: approved
owner: maintainers
created: 2026-08-26
updated: 2026-10-18
sidebar_position: 3
tags: [cli, usage, reference]
---

# TypeAnvil CLI Reference

The engine ships one subcommand, `render`. It reads an HTML or Markdown file
and writes a PDF. The extensions `.md`, `.markdown` and `.mdown` select
Markdown; any other extension is HTML. `--from` overrides the extension.

```text
typeanvil render <input.html|input.md> [flags] -o <output.pdf>
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
| `--base-url` | directory path | empty | Filesystem directory against which relative `<img>` paths resolve. Affects nothing else. |
| `--title` | string | from `<title>` | PDF metadata title (overrides the document). |
| `--author` | string | from `<meta name="author">` | PDF metadata author. |
| `--no-tagged` | flag | off | Omit the tagged-PDF logical structure tree. Tagging is on by default (CORE-378). |
| `--tagged` | flag | on | Emit a tagged-PDF logical structure tree. Accepted as a no-op; this is the default. |
| `--ua` | flag | off | Tagged output plus krilla's PDF/UA-1 validation. Overrides `--no-tagged`. |
| `--diagnostics` | `json` \| `text` | none | Print CSS/layout diagnostics after the render. |
| `--from` | `html` \| `markdown` | by extension | Input format. Overrides the file extension. |
| `--css` | file | none | Markdown only. Append an author stylesheet after the default one. Repeatable. Rejected for HTML input. |
| `--no-default-css` | flag | off | Markdown only. Omit the built-in stylesheet. Rejected for HTML input. |

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

Render Markdown with your own stylesheet:

```bash
typeanvil render notes.md --css print.css \
  --page-width 8.5in --page-height 11in \
  -o notes.pdf
```

See [Markdown to PDF](/guides/markdown) for the dialect and the frontmatter keys.

## Notes

- Use `typeanvil --help` (or `-h`) to inspect the command surface. `--help`
  and `--version` work only as the FIRST argument. `typeanvil render --help`
  is an error (exit 1).
- Author CSS wins over CLI geometry: an `@page { size: … }` rule in the
  document overrides the command-line page size, but only on the pages
  that rule matches. Other pages keep the command-line size.
- Styles come from `<style>` elements and from local files named by
  `<link rel="stylesheet" href="…">`. Remote stylesheets are not fetched.
- The full user-facing reference is the [CLI guide](/guides/cli). If this
  page and the guide disagree, the guide wins.

## Version, help, and exit codes

- `typeanvil --version` (or `-V`) prints `typeanvil <version>` and exits 0.
- `typeanvil --help` (or `-h`) prints usage and exits 0.
- Exit code 0 means success. Exit code 1 means any error.
- On error, the engine prints `typeanvil: error: <message>` to stderr.
  With no arguments the message is
  ``missing subcommand (expected `render`)``.
