# Third-party sources and licenses

Everything under `library/` that is not authored in this repository is listed
here. Each manifest entry names its `source` and `license`
(`library/manifest.json`). A new third-party source (Project Gutenberg, arXiv,
Federal Register, ...) adds a section below before its documents enter the
manifest.

| Source | Where | License |
|---|---|---|
| Authored in-repo | most `library/<category>/*.html`, chart PNGs under `assets/` | AGPL-3.0-only (repo license) |
| WeasyPrint samples | `library/{financial,letters,posters,reports,tickets}/weasyprint-*.html` + `assets/weasyprint-samples/` | public-domain samples; fonts per the table below |
| Liberation Serif | `library/letters/assets/liberation-serif.ttf` | SIL OFL 1.1; Red Hat copyright notice in `library/letters/assets/liberation-serif-LICENSE.txt` |

## WeasyPrint sample fonts — provenance and license

These fonts are vendored from the
[WeasyPrint samples repository](https://github.com/CourtBouillon/weasyprint-samples)
(CourtBouillon, public-domain samples; fonts carry their own licenses). They are
used by the adopted WeasyPrint sample documents in the showcase
(`library/*/weasyprint-*.html`).

Every font below is **open source** and redistributable. Each is available on
Google Fonts under the stated license. The Madcats "All Cats Are Beautiful"
book example is **excluded** from this collection — its text is Madcats'
property, not public domain — so its fonts (OpenDyslexic, Source Serif Pro)
are not vendored here.

| Sample | Font file(s) | Family | License | Google Fonts |
|---|---|---|---|---|
| invoice | `SourceSans3-Regular.ttf`, `SourceSans3-Bold.ttf` | Source Sans 3 | SIL OFL 1.1 | [Source Sans 3](https://fonts.google.com/specimen/Source+Sans+3) |
| invoice | `pacifico.ttf` | Pacifico | SIL OFL 1.1 | [Pacifico](https://fonts.google.com/specimen/Pacifico) |
| letter | `ComingSoon-Regular.ttf` | Coming Soon | Apache 2.0 | [Coming Soon](https://fonts.google.com/specimen/Coming+Soon) |
| letter | `Pacifico-Regular.ttf` | Pacifico | SIL OFL 1.1 | [Pacifico](https://fonts.google.com/specimen/Pacifico) |
| poster | `archivoblack-regular.ttf` | Archivo Black | SIL OFL 1.1 | [Archivo Black](https://fonts.google.com/specimen/Archivo+Black) |
| report | `FiraSans-{Regular,Italic,Light,LightItalic,Bold}.ttf` | Fira Sans | SIL OFL 1.1 | [Fira Sans](https://fonts.google.com/specimen/Fira+Sans) |
| ticket | `barlowcondensed-{regular,light,bold}.otf` | Barlow Condensed | SIL OFL 1.1 | [Barlow Condensed](https://fonts.google.com/specimen/Barlow+Condensed) |
| ticket | `librebarcode128-regular.ttf` | Libre Barcode 128 | SIL OFL 1.1 | [Libre Barcode 128](https://fonts.google.com/specimen/Libre+Barcode+128) |

## License texts

- **SIL Open Font License 1.1** — applies to Source Sans 3, Pacifico, Archivo
  Black, Fira Sans, Barlow Condensed, Libre Barcode 128. Full text:
  https://openfontlicense.org/ (OFL-1.1).
- **Apache License 2.0** — applies to Coming Soon. Full text:
  https://www.apache.org/licenses/LICENSE-2.0.

## Notes

- Font files are copied verbatim from the WeasyPrint samples repo (commit
  `main` as of 2026-10-11). They are the exact files the sample documents
  reference via `@font-face`.
- The book sample's fonts (OpenDyslexic, Source Serif Pro) are also open
  source (OFL) but are not vendored because the book example itself is
  excluded.
- License determination was made against Google Fonts (2026-10-11): every
  family listed above is hosted there under the stated license.

## Fixture adaptations

The five showcase fixtures (`weasyprint-invoice.html`, `weasyprint-letter.html`,
`weasyprint-poster.html`, `weasyprint-report.html`, `weasyprint-ticket.html`)
are adapted from the public-domain samples so the TypeAnvil engine can render
them. The HTML body and document structure are unchanged; the CSS is adapted:

1. **CSS nesting flattened** — the samples use modern CSS nesting (`&`,
   nested rules), which the engine does not parse. Nesting is flattened to
   plain selectors via `postcss-nested`.
2. **`var()` custom properties inlined** — the samples define colors in
   `:root { --x: ... }` and reference them as `var(--x)`. The engine has no
   custom-property support, so each `var(--x)` is replaced with its literal
   value and the `:root` block is dropped.
3. **Explicit `@page { size }` added** — the samples relied on WeasyPrint's
   A4 default page size. The engine's CLI default is US Letter, so
   `@page { size: 210mm 297mm }` is prepended to invoice/letter/report and
   `@page { size: 297mm 210mm }` (A4 landscape) to ticket. The poster keeps
   its declared `size: 278mm 388mm`.
4. **`@page :blank` rules dropped** — the engine has no `:blank` page
   pseudo-class; the rules are removed (behavior-preserving: they only
   suppressed margin boxes on blank pages).
5. **Asset URLs rewritten** — `url(...)` and `<img src>` refs point at
   `assets/weasyprint-samples/<sample>/...` so each fixture resolves from its
   own category directory as CWD (CORE-488: assets moved next to the fixtures).

Known rendering limitations (recorded in `library/manifest.json`):
- **Element-level `background-image` is not rendered** (CORE-319) — the
  letter's header skyline and the report's cover image do not paint. The
  report's `@page :first` background is a page-level image; the page canvas
  paints color only, so the cover image is also absent.
- **SVG figure icons** in the report wait for SVG rasterization (CORE-131).
