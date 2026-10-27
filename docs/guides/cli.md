---
title: CLI reference
type: guide
status: approved
owner: maintainers
created: 2026-10-14
updated: 2026-10-25
sidebar_position: 3
tags: [guide, cli, reference]
slug: /guides/cli
---

# CLI reference

TypeAnvil ships one command-line program, `typeanvil`. The `render`
subcommand reads an HTML or Markdown file and writes a PDF. The
`capabilities --json` subcommand prints a manifest of supported CSS (see
[Capabilities](#capabilities)). The `mcp` and `call` subcommands serve the
agent tools (see [MCP and JSON requests](#mcp-and-json-requests)). Two more
flags print the version and the help text.

```text
typeanvil render <input.html|input.md> [flags] -o <output.pdf>
typeanvil capabilities --json
typeanvil mcp [--root <dir>]...
typeanvil call <tool> [--root <dir>]... < request.json
typeanvil --version
typeanvil --help
```

To install the binary, see [Getting started](./getting-started.md). The
behavior on this page matches the argument parser in `engine/src/main.rs`.

## Top-level flags

| Flag | Output | Exit |
|---|---|---|
| `--version`, `-V` | `typeanvil <version>` on stdout | `0` |
| `--help`, `-h` | Usage text for subcommands and flags on stdout | `0` |

These flags work only as the **first** argument. In any other position they
are rejected like any other unknown flag:

```text
$ typeanvil render doc.html --help
typeanvil: error: unknown flag `--help`
```

`render` has no help of its own, so run `typeanvil --help` instead of
`typeanvil render --help`.

The version is compiled into the binary from `engine/Cargo.toml`. Release
builds stamp it from the git tag, so a release binary prints the release
version. A source build prints whatever version `engine/Cargo.toml` has in your
checkout.

If you run `typeanvil` with no arguments, it prints
``typeanvil: error: missing subcommand (expected `render`)`` to stderr and
exits `1`. Any other first argument is an unknown subcommand and also exits
`1`.

## `render`

```text
typeanvil render <input.html|input.md> [flags] -o <output.pdf>
```

`render` takes exactly one positional argument, the input file path. The
extensions `.md`, `.markdown` and `.mdown` (any letter case) select Markdown.
Every other extension is HTML. See [Markdown to PDF](./markdown.md).
Flags and the input path can come in any order after `render`.

### Flags

| Flag | Value | Default | Meaning |
|---|---|---|---|
| `-o`, `--output` | path | required | Output PDF path. The parent directory must exist; an existing file is overwritten. |
| `--page-width` | length | required | Page width. |
| `--page-height` | length | required | Page height. |
| `--margin-top` | length | `0` | Top page margin. |
| `--margin-right` | length | `0` | Right page margin. |
| `--margin-bottom` | length | `0` | Bottom page margin. |
| `--margin-left` | length | `0` | Left page margin. |
| `--base-url` | directory path | none | Directory used to resolve relative image paths. |
| `--title` | text | document `<title>` | PDF metadata title. Replaces the document title. |
| `--author` | text | `<meta name="author">` | PDF metadata author. Replaces the document author. |
| `--no-tagged` | none (switch) | off | Skip the logical structure tree. Tagged PDF is the default. |
| `--tagged` | none (switch) | on | Write a tagged PDF with a logical structure tree. This is the default, so the flag changes nothing. |
| `--ua` | none (switch) | off | Tagged PDF plus PDF/UA-1 validation. Overrides `--no-tagged`. |
| `--diagnostics` | `json` or `text` | off | Report stylesheet diagnostics after the PDF is written. |
| `--from` | `html` or `markdown` | by extension | Input format. Overrides the file extension. Any other value is a usage error. |
| `--css` | file path | none | Markdown only. Append an author stylesheet after the default one. Repeat the flag for more files. HTML input rejects it. |
| `--no-default-css` | none (switch) | off | Markdown only. Omit the built-in stylesheet. HTML input rejects it. |
| `--assert` | JSON file path | none | Evaluate layout assertions after the render. Verdicts join the `--diagnostics` JSON. Implies `--diagnostics json`. |
| `--fail-on-assert` | none (switch) | off | Exit `3` when any assertion fails. Needs `--assert`. |
| `--data` | JSON file path | none | Treat the input as a data template and render it with this data. See [Data templates](#data-templates). |
| `--schema` | JSON file path | sibling `<name>.schema.json` | Validate `--data` against this JSON Schema. Needs `--data`. |
| `--embed-source` | none (switch) | off | Attach the exact input file to the PDF (`/AFRelationship /Source`). See [PDF embedded source](../specifications/pdf-embedded-source.spec.md). |
| `--embed-data` | file path (repeatable) | none | Attach a data file such as invoice JSON (`/AFRelationship /Data`). An unreadable file is a usage error. |

Parsing rules:

- Separate a flag from its value with a space: `--page-width 8.5in`. The
  `--flag=value` form is rejected as an unknown flag.
- There is no `--margin` shorthand. Set each side on its own.
- If a flag is repeated, the last value wins.
- `-o` takes a real file path. `-o -` does not write to stdout; it creates a
  file named `-`.
- Any argument that starts with `-` and is not listed above is an error, and
  so is a second positional argument.

### Page geometry and `@page`

The geometry flags are defaults. An `@page` rule's `size` and margins override
them on every page the rule matches. Pages that no `@page` rule matches use the
CLI values. For example, a document with `@page { size: Letter; }` renders on
8.5in × 11in pages even if you pass `--page-width 5in --page-height 3in`. The
[paged media CSS specification](../specifications/paged-media-css.spec.md)
defines the matching rules, and [Paged media CSS](./paged-media-css.md) covers
the details.

### `--base-url`

Despite its name, `--base-url` takes a local directory path, not a web URL.
The engine joins relative `<img src>` paths onto it. Absolute paths and `data:`
URIs ignore it. Without `--base-url`, relative image paths resolve against the
current working directory.

`--base-url` does not affect stylesheets or fonts. Local
`<link rel="stylesheet">` files and `@font-face` `url()` sources always resolve
against the input file's directory. The engine never fetches `http://` or
`https://` resources. See [Getting started](./getting-started.md#how-input-files-are-resolved)
for the full set of rules.

### Metadata: `--title` and `--author`

Without these flags, the PDF title comes from the first `<title>` element and
the author from `<meta name="author">`. Subject and keywords come from
`<meta name="description">` and `<meta name="keywords">`, and no flag sets
those two. An empty `--title ""` or `--author ""` is ignored, so the document
value is used. See the
[PDF metadata specification](../specifications/pdf-metadata.spec.md).

### Tagged PDF: `--no-tagged` and `--ua`

Tagging is on by default. The engine builds a structure tree from the DOM in
document order and tags every draw. Pass `--no-tagged` to leave the tree out
(smaller file, same painted pages). `--tagged` is still accepted and changes
nothing. The language comes from `<html lang>`. `--ua` also runs the
PDF/UA-1 validator in [krilla](https://github.com/LaurenzV/krilla), the PDF
library the engine uses. If the validator finds any violation, the render
fails, prints the violation list to stderr, exits `1`, and writes no output
file. For example, a document without a `<title>` fails with
`NoDocumentTitle`. Without `--ua`, missing alt text does not fail the render.

The validator is the only conformance check. The engine does not claim
certification by external PDF/UA tools, and it does not produce PDF/A. See the
[tagged PDF specification](../specifications/tagged-pdf.spec.md) for the
structure mapping and known gaps.

## Length units

Every length flag takes a number followed by an optional absolute unit:

| Unit | Meaning |
|---|---|
| `in` | inches (1in = 72pt) |
| `pt` | points |
| `px` | CSS pixels (1px = 0.75pt) |
| `cm` | centimeters |
| `mm` | millimeters |
| none | points, so `595` means `595pt` |

Units are case-insensitive. Relative units such as `em`, `rem`, `%`, and `vh`
are rejected:

```text
$ typeanvil render doc.html --page-width 8.5em --page-height 11in -o doc.pdf
typeanvil: error: unsupported length unit `em` in `8.5em`
```

Common page sizes: US Letter is `8.5in` × `11in`, and A4 is `210mm` × `297mm`.

## Diagnostics

`--diagnostics` scans the collected stylesheet text and reports what the
engine does not recognize. The codes are `unsupported-property`,
`unknown-at-rule`, `display-fallback`, and `malformed-declaration`. Broken
bookmark anchors found during layout are reported as
`bookmark-anchor-unresolved`.

- `--diagnostics json` prints one JSON document to stdout after the PDF is
  written:

  ```json
  {
    "schema": 1,
    "diagnostics": [
      {"code": "unsupported-property", "severity": "warning", "message": "property `text-indent` is not supported and was ignored", "line": 3, "column": 5}
    ],
    "counts": {"warnings": 1}
  }
  ```

- `--diagnostics text` prints one line per event to stderr, in the form
  `LINE:COL warning [code]: message`.

Things to know about diagnostics:

- The PDF bytes are identical with and without `--diagnostics`.
- Every event is a warning. Diagnostics never change the exit code.
- `line` and `column` point into the collected stylesheet. That is the text of
  every `<style>` element and linked stylesheet, joined in document order. They
  do not point into the HTML file. Anchor events use `0:0`.
- The scanner compares properties against a fixed list in
  `engine/src/diagnostics.rs`. That list can flag a property the layout engine
  partly honors, so treat an `unsupported-property` warning as a prompt to
  check [CSS support](./css-support.md), not as a final verdict.

The [diagnostics specification](../specifications/diagnostics.spec.md) defines
the schema.

## Capabilities

`typeanvil capabilities --json` prints a JSON manifest of CSS properties,
`@page` descriptors, `display` values, at-rules, selector features, and input
formats. Each entry has a `status` (`supported`, `partial`, or
`unsupported`), a `note`, and sometimes an `alternative`. A property that is
not listed is unsupported. The manifest comes from the same tables as
`--diagnostics`, so a warning always agrees with it. Spec:
`docs/specifications/capability-manifest.spec.md`.

## MCP and JSON requests

`typeanvil mcp` runs an MCP server over stdio with the tools `render`,
`validate`, `inspect_page`, `capabilities` and `explain`. `typeanvil call
<tool>` runs one of those tools: it reads the request object from stdin and
prints the result object as one JSON line. `call` exits `0` for a result, `1`
for a tool error and `2` for a usage error. `--root <dir>` limits file access
to that directory. Client setup and request examples:
[MCP server and JSON requests](./mcp.md).

## Assertions

`--assert goals.json` states layout goals and gets a verdict per goal. The
file is a JSON array. Each object has a `kind` and an optional `id`:

```json
[
  {"id": "fit", "kind": "max-pages", "value": 2},
  {"kind": "keep-together", "selector": "table.totals"},
  {"kind": "on-page", "selector": "#signature", "page": 2},
  {"kind": "outline-covers", "selector": "h1, h2"},
  {"kind": "no-overflow"}
]
```

The kinds are `max-pages`, `exact-pages`, `no-overflow`, `no-empty-pages`,
`keep-together`, `on-page`, `outline-covers`, and `fonts-resolved`. The
diagnostics JSON gains an `assertions` object with `passed`, `failed`, and
one result per goal. A failing result lists evidence: page, rect, source
line and column, and element selector. The exit status does not change
unless you also pass `--fail-on-assert`. A bad assertions file is a usage
error (exit `2`). Spec: `docs/specifications/intent-assertions.spec.md`.

## Data templates

`--data invoice.json` treats the input as a MiniJinja (Jinja2 syntax)
template and renders it with the JSON data. Values are HTML-escaped by
default; `| safe` opts out. Templates cannot read files or the network.
When a JSON Schema is present (`--schema`, or `<name>.schema.json` next to
`<name>.tmpl.html`), the data is validated first. Each violation prints
`typeanvil: error: [TEMPLATE_DATA_SCHEMA] <json-pointer>: <message>`, no PDF
is written, and the exit code is `5`. `--diagnostics json` also prints the
errors as `{"schema": 1, "template_errors": [...]}` on stdout.
`typeanvil template-schema <template>` prints the schema an agent must fill.
Examples live in `templates/`. Spec:
`docs/specifications/data-templates.spec.md`.

## Exit codes and errors

| Exit | Meaning |
|---|---|
| `0` | Success: the PDF was written, or `--version` / `--help` was printed. |
| `1` | Any error. |
| `2` | Usage error for `--from`, `--css`, `--no-default-css`, `--assert`, `--fail-on-assert`, or `--embed-data`, and duplicate embedded file names. |
| `3` | `--fail-on-assert`: at least one assertion failed. The PDF and diagnostics were written. |
| `4` | A `--max-*` or `--timeout` limit stopped the render. No PDF. |
| `5` | `--data`: template, data or schema error (`TEMPLATE_*` codes). No PDF. |

On error, the CLI prints a single line to stderr in the form
`typeanvil: error: <reason>`. The reasons include:

- missing subcommand
- unknown subcommand
- unknown flag
- flag missing its value
- missing input path, `--page-width`, `--page-height`, or `-o`
- invalid length or unit
- invalid `--diagnostics` mode
- unreadable input
- unparseable HTML
- PDF/UA validation failure
- unwritable output

Errors in argument parsing, input reading, and rendering happen before the
output file is written. If one of them occurs, no file is written and an
existing file at the output path is left unchanged.

## Examples

US Letter with half-inch margins:

```bash
typeanvil render report.html \
  --page-width 8.5in --page-height 11in \
  --margin-top 0.5in --margin-right 0.5in \
  --margin-bottom 0.5in --margin-left 0.5in \
  -o report.pdf
```

A4 portrait:

```bash
typeanvil render invoice.html \
  --page-width 210mm --page-height 297mm \
  -o invoice.pdf
```

Metadata plus machine-readable diagnostics, with JSON saved to a file:

```bash
typeanvil render paper.html \
  --page-width 8.5in --page-height 11in \
  --title "Q3 Report" --author "A. Author" \
  --diagnostics json \
  -o paper.pdf > paper.diagnostics.json
```

Accessible output validated against PDF/UA-1, with images resolved from
`./assets`:

```bash
typeanvil render letter.html \
  --page-width 8.5in --page-height 11in \
  --base-url ./assets \
  --ua \
  -o letter.pdf
```

## Related references

- [CLI operations reference](../operations/cli.md)
- [CLI surface specification](../specifications/cli-surface.spec.md):
  `--version`, `--help`, and the exit-code contract.
- [Containers and packages](./container.md): the same CLI, run from the container
  image.
