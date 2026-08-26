<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="typeanvil-wordmark-dark.svg">
    <img src="typeanvil-wordmark.svg" alt="Typeanvil" width="340" />
  </picture>
</p>

Typeanvil is an open-source HTML → PDF typesetting engine written in Rust.
It turns HTML documents with paged-media CSS into production PDFs — reports,
invoices, letters, papers, books.

It is built for developers who need print-quality pagination without
a commercial rendering engine or a headless browser farm.

```html
<style>
@page {
  size: A4;
  margin: 25mm;
  @top-center { content: string(chapter); }
  @bottom-center { content: counter(page) " / " counter(pages); }
}
h2 { string-set: chapter content(); }
</style>
<h2>Introduction</h2>
<p>Your document here. Typeanvil paginates it.</p>
```

## Why Typeanvil

- **Deterministic output.** Identical input produces byte-identical PDFs.
  Diff-test your documents in CI. No wall clock, no randomness, no
  iteration-order effects.
- **Typography by default.** Knuth-Plass total-fit line breaking,
  automatic hyphenation, orphans and widows handling. These are defaults,
  not opt-ins.
- **Paged-media CSS support.** `@page` rules with margin boxes, named
  pages, `counter(pages)` and `target-counter()`, running headers through
  `string-set`, footnotes, leader fills (`content: leader(dotted)`),
  multi-column layout, floats that fragment across pages, repeating table
  headers and footers.
- **Fast and light.** On most of our benchmark corpus, Typeanvil renders
  about 1.5–2× faster than PrinceXML using roughly 3.5× less memory.
- **Accessibility-ready.** `--tagged` emits a tagged structure tree;
  `--ua` additionally validates against PDF/UA-1.

## How it compares

| | Typeanvil | PrinceXML | WeasyPrint |
|---|---|---|---|
| Language | Rust | C++ | Python |
| License | AGPL-3.0, commercial available | Commercial | BSD-3 |
| Cost | Free | Paid | Free |
| Deterministic byte-identical output | Yes | No | No |
| Knuth-Plass line breaking | Yes | No | No |
| Footnotes | Yes | Yes | No |
| Tagged PDF / PDF-UA | Yes | Yes | No |

PrinceXML remains the most complete HTML-to-PDF engine available and is an
excellent product. Typeanvil's goal is conformance on real-world paged-media
CSS with better speed, memory use, determinism, and price. Our test corpus
currently matches PrinceXML page-for-page across all seven fixtures; see
[the comparison demo](demo/out/index.html) for side-by-side renders.

Compared with WeasyPrint, Typeanvil offers stronger default typography
(Knuth-Plass versus greedy line breaking), deterministic output, and lower
latency at scale, since there is no Python runtime cost per render.

## Performance

Measured on our public benchmark corpus (seven document types, identical
geometry for both engines):

- ~1.5–2× faster than PrinceXML on six of seven fixtures
- ~3.5× less peak memory on all seven
- Slower today on one fixture (very wide tables) — tracked work

Full methodology, per-fixture tables, and reproduction steps:
[benchmarks/RESULTS.md](benchmarks/RESULTS.md) ·
[benchmark runbook](docs/operations/benchmark-engines.md)

## Getting started

Build from source (current stable Rust):

```bash
git clone https://github.com/elijahboston/typeanvil
cd typeanvil/engine
cargo build --release
```

Render a document:

```bash
./target/release/typeanvil render doc.html \
  --page-width 8.5in --page-height 11in \
  --margin-top 0.8in --margin-right 0.8in \
  --margin-bottom 0.8in --margin-left 0.8in \
  -o doc.pdf
```

The full flag reference, including metadata, tagged-PDF, and diagnostics
options, lives in [docs/operations/cli.md](docs/operations/cli.md).

## Usage example

A report with chapter headers in the page margin and "Page N" footers:

```html
<!DOCTYPE html>
<html>
<head>
<style>
@page {
  size: Letter;
  margin: 1in;
  @top-left { content: string(chapter); font-size: 9pt; color: #666; }
  @bottom-center { content: "Page " counter(page) " of " counter(pages);
                   font-size: 9pt; }
}
h1 { string-set: chapter content(); }
table { border-collapse: collapse; width: 100%; }
th { background: #eee; }
thead { display: table-header-group; } /* repeats on every page */
</style>
</head>
<body>
<h1>Quarterly Report</h1>
<p>Long content flows across pages automatically, with running
headers, footnotes via <code>float: footnote</code>, and a table of
contents driven by <code>target-counter(attr(href), page)</code>.</p>
<table><!-- rows spanning many pages --></table>
</body>
</html>
```

Supported input today: standalone HTML files with `<style>` elements.
Command-line options can override PDF title and author metadata.

## Feature status

| Feature | Status |
|---|---|
| `@page` sizing, margins, named pages | Supported |
| Margin boxes (`@top-*`, `@bottom-*`) | Supported |
| `counter(page)` / `counter(pages)` / `target-counter()` | Supported |
| Running headers (`string-set` / `string()`) | Supported |
| Footnotes (`float: footnote`) | Supported |
| Leader fills (`content: leader(...)`) | Supported |
| Floats, incl. cross-page fragmentation | Supported |
| Multi-column layout | Supported |
| Flexbox | Supported |
| Tables (auto layout, column freezing, repeating headers/footers) | Supported |
| Hyphenation · orphans/widows | Supported |
| Raster images (PNG, JPEG, GIF, WebP) | Supported |
| Custom fonts via `@font-face` (local files) | Supported |
| Internal and external hyperlinks | Supported |
| PDF bookmarks and metadata | Supported |
| Tagged PDF and PDF/UA validation | Supported |
| Machine-readable diagnostics (`--diagnostics json`) | Supported |
| SVG images | Unsupported |
| WOFF2 fonts | Unsupported |
| OpenType feature settings | Unsupported |
| External stylesheets via `<link>` | Unsupported |
| Markdown input | Unsupported |
| JavaScript | Unsupported by design — static renderer |

See the [specifications](docs/specifications/) directory for planned work,
and file an issue if a gap blocks you.

## Documentation

- [CLI reference](docs/operations/cli.md)
- [Feature specifications](docs/specifications/)
- [Architecture overview](docs/README.md)
- [Benchmark methodology](docs/operations/benchmark-engines.md)

## License

Typeanvil is free software under the [AGPL-3.0-only](LICENSE) license.
You may use, modify, and host it freely; modifications offered as a network
service must be published under the same license.

If the AGPL does not fit your use case, commercial licenses are available —
contact us through the repository.

Contributions are welcome. By opening a pull request you agree to the terms
in [CLA.md](CLA.md).
