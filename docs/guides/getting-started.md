---
title: Getting started
type: guide
status: approved
owner: maintainers
created: 2026-10-14
updated: 2026-10-17
sidebar_position: 2
tags: [guide, installation, quickstart]
slug: /guides/getting-started
---

# Getting started

This page gets you from nothing to a rendered PDF. You install the `typeanvil`
binary, either from a release archive or by building it from source, and then
render a document with the CLI.

TypeAnvil reads one local HTML file and its CSS, lays the content out on pages,
and writes a PDF. It does not execute JavaScript.

## Option 1: download a release binary

Tagged releases on
[GitHub Releases](https://github.com/whitelodge-esoteric/typeanvil/releases)
include one archive per platform, named `typeanvil-<version>-<target>.tar.gz`,
plus a `SHA256SUMS` file:

| Target | Platform |
|---|---|
| `aarch64-apple-darwin` | macOS on Apple Silicon |
| `x86_64-apple-darwin` | macOS on Intel |
| `x86_64-unknown-linux-gnu` | Linux x86-64 (glibc) |
| `x86_64-unknown-linux-musl` | Linux x86-64, static binary |
| `aarch64-unknown-linux-gnu` | Linux ARM64 (glibc) |

Each archive unpacks to a `typeanvil-<version>-<target>/` directory that holds
the `typeanvil` binary and a short `README.md`. Check the archive against the
checksum file, then unpack it:

```bash
shasum -a 256 -c SHA256SUMS --ignore-missing
tar -xzf typeanvil-<version>-<target>.tar.gz
./typeanvil-<version>-<target>/typeanvil --version
```

A release binary reports the release version, so the `v2026.9.5` binary prints
`typeanvil 2026.9.5`. The
[release CI specification](../specifications/release-ci.spec.md) describes how
the binaries are built.

## Option 2: build from source

You need a current stable [Rust toolchain](https://www.rust-lang.org/tools/install).
Clone the repository and build the release binary:

```bash
git clone https://github.com/whitelodge-esoteric/typeanvil
cd typeanvil
cargo build --release --manifest-path engine/Cargo.toml
```

The binary is written to `engine/target/release/typeanvil`. A source build
reports the version in `engine/Cargo.toml`, not a release tag:

```bash
./engine/target/release/typeanvil --version
```

## Your first render

The CLI has one subcommand, `render`:

```text
typeanvil render <input.html> [flags] -o <output.pdf>
```

`--page-width`, `--page-height`, and `-o` / `--output` are required. Lengths
take the absolute units `in`, `pt`, `px`, `cm`, or `mm`, and a bare number means
points. Margins default to `0`. The output directory must already exist.

Save this file as `input.html`:

```html
<!DOCTYPE html>
<html>
<head>
<title>Quarterly Report</title>
<style>
@page {
  size: Letter;
  margin: 1in;
  @top-left { content: string(chapter); font-size: 9pt; color: #666; }
  @bottom-center { content: "Page " counter(page) " of " counter(pages); }
}
h1 { string-set: chapter content(); }
table { width: 100%; }
thead { display: table-header-group; }
</style>
</head>
<body>
<h1>Quarterly Report</h1>
<p>Long content flows across pages with running headers and table pagination.</p>
<table>
  <thead><tr><th>Item</th><th>Value</th></tr></thead>
  <tbody><tr><td>Revenue</td><td>1,000</td></tr></tbody>
</table>
</body>
</html>
```

Render it:

```bash
./engine/target/release/typeanvil render input.html \
  --page-width 8.5in --page-height 11in \
  -o output.pdf
```

If you installed a release binary, run that binary instead. A successful run
prints nothing and exits `0`. On failure, the CLI prints
`typeanvil: error: <reason>` to stderr and exits `1`.

The result is a US Letter page with one-inch margins, the chapter title in the
top-left margin box, and a page count in the footer. The PDF title comes from
`<title>`. The page geometry comes from the `@page` rule, which overrides the
CLI geometry for every page it matches. The CLI values are the fallback for
pages that no `@page` rule matches (see the
[paged media CSS specification](../specifications/paged-media-css.spec.md)).

The [CLI reference](./cli.md) covers every flag, including PDF metadata, tagged
PDF, PDF/UA validation, and diagnostics.

## How input files are resolved

- **Stylesheets.** CSS comes from `<style>` elements and from local
  `<link rel="stylesheet" href="…">` files, in document order. A relative
  `href` resolves against the input file's directory. A root-absolute `href`
  (`/…`) is looked up in the input file's directory first, then in each parent
  directory in turn.
- **Fonts.** `@font-face` `url()` sources follow the same rules. Other
  families resolve to installed system fonts. If none of them resolves, the
  engine uses its fallback sans-serif face (see the
  [font resolution specification](../specifications/font-resolution.spec.md)).
  The binary does not embed that face; it reads it from disk. It looks for
  Arial in `/System/Library/Fonts/Supplemental/` (macOS) and then Liberation
  Sans in `/usr/share/fonts/typeanvil/`. The container image installs
  Liberation Sans there. On a Linux host without either font, copy the
  Liberation files from the repository's `docker/fonts/typeanvil/` directory
  to that path, or use the container image.
- **Images.** A relative image path resolves against `--base-url` when you
  pass it. Without `--base-url`, it resolves against the current working
  directory, not the input file's directory. Supported formats are PNG, JPEG,
  GIF (first frame), and SVG, plus `data:` URIs (see the
  [images specification](../specifications/images.spec.md)).
- **No network access.** `http://` and `https://` stylesheets, fonts, and
  images are never fetched.

## Other ways to run TypeAnvil

### Container image

The repository's [container specification](../specifications/container-distribution.spec.md)
defines an image that wraps the Linux release binary. It runs as a non-root
user, bundles the Liberation fonts as a fallback, and reads extra fonts from
`/usr/share/fonts/user`. The CLI contract is the same as the native binary:

```bash
docker run --rm -v "$PWD:/work" \
  REGISTRY/OWNER/typeanvil:TAG \
  render /work/in.html --page-width 8.5in --page-height 11in -o /work/out.pdf
```

Replace `REGISTRY/OWNER/typeanvil:TAG` with the image location given in the
release notes. The [Containers and packages](./container.md) guide and the
[container image runbook](../operations/container-image.md) cover fonts,
permissions, and troubleshooting.

### npm package (not yet published)

The repository contains an npm wrapper package, `typeanvil` in `npm/`. It
targets Node ≥ 18 and Bun and installs the native binary for your platform as
an optional dependency. As of 2026-10-14 it is not on the npm registry: the
release workflow skips npm publishing until a publish token is configured (see
the [npm distribution specification](../specifications/npm-distribution.spec.md)).
Until it is published, use a release binary or a source build.

## Next steps

- [CLI reference](./cli.md): every flag, length units, diagnostics, and exit
  codes.
- [Paged media CSS](./paged-media-css.md): `@page` rules, margin boxes, and
  counters.
- [CSS support](./css-support.md): what the engine supports today.
- [Architecture overview](../architecture/overview.md): how the pipeline
  works.
