<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="typeanvil-lockup-white.svg">
    <img src="typeanvil-lockup.svg" alt="Typeanvil" width="340" />
  </picture>
</p>

Typeanvil is an open-source HTML → PDF typesetting engine written in Rust. It
turns HTML documents with paged-media CSS into production PDFs for reports,
invoices, letters, papers, and books.

It is built for developers who need print-quality pagination without a
commercial rendering engine or a headless browser farm.

## Why Typeanvil

- **Deterministic output.** Identical input produces byte-identical PDFs.
  Diff-test your documents in CI.
- **Typography by default.** Measured text shaping, line breaking, hyphenation,
  and widow/orphan handling are part of the engine's typography pipeline.
- **Paged-media CSS support.** The engine supports `@page` rules with margin
  boxes, named pages, page counters, running headers, footnotes, leader fills,
  multi-column layout, floats, and repeating table headers and footers. See the
  feature specifications for exact behavior and known gaps.
- **Accessibility features.** `--tagged` emits a tagged structure tree;
  `--ua` additionally validates against PDF/UA-1.

PrinceXML and Chromium are comparison tools. CSS specifications define
Typeanvil's expected behavior.

## Getting started

Build from source with a current stable Rust toolchain:

```bash
git clone https://github.com/whitelodge-esoteric/typeanvil
cd typeanvil
cargo build --release --manifest-path engine/Cargo.toml
```

Render a document:

```bash
./engine/target/release/typeanvil render input.html \
  --page-width 8.5in --page-height 11in \
  --margin-top 1in --margin-right 1in \
  --margin-bottom 1in --margin-left 1in \
  -o output.pdf
```

The full flag reference is in [docs/operations/cli.md](docs/operations/cli.md).
The [architecture overview](docs/architecture/overview.md) explains the
pipeline and module boundaries.

## Usage example

A report with chapter headers in the page margin and page numbers in the footer:

```html
<!DOCTYPE html>
<html>
<head>
<style>
@page {
  size: Letter;
  margin: 1in;
  @top-left { content: string(chapter); font-size: 9pt; color: #666; }
  @bottom-center { content: "Page " counter(page) " of " counter(pages); }
}
h1 { string-set: chapter content(); }
table { border-collapse: collapse; width: 100%; }
th { background: #eee; }
thead { display: table-header-group; }
</style>
</head>
<body>
<h1>Quarterly Report</h1>
<p>Long content flows across pages with running headers and table pagination.</p>
<table><!-- rows spanning many pages --></table>
</body>
</html>
```

The CLI currently accepts standalone HTML files. Inline `<style>` elements and
local linked stylesheets are supported. JavaScript is not executed.

## Feature status

See [the specifications](docs/specifications/) for exact contracts and known
gaps. The [WPT harness](docs/specifications/wpt-conformance-harness.spec.md)
and [release gate](docs/operations/release-gate.md) describe verification.

## Documentation

- [Architecture overview](docs/architecture/overview.md)
- [CLI reference](docs/operations/cli.md)
- [Feature specifications](docs/specifications/)
- [Contributor and development guidelines](docs/conventions/development-guidelines.md)
- [Documentation site](docs/operations/docs-site.md)

## License

Typeanvil is free software under the
[AGPL-3.0-only](https://www.gnu.org/licenses/agpl-3.0.html) license. The engine
has no license check, watermark, activation flow, phone-home behavior, or
feature gate. Read [CLA.md](CLA.md) for contribution terms.

This repository documents the engine and its tooling. Public documentation for
a hosted service will be linked separately when available.