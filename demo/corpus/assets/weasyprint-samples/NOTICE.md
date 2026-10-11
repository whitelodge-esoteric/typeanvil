# WeasyPrint sample fonts — provenance and license

These fonts are vendored from the
[WeasyPrint samples repository](https://github.com/CourtBouillon/weasyprint-samples)
(CourtBouillon, public-domain samples; fonts carry their own licenses). They are
used by the adopted WeasyPrint sample documents in this corpus.

Every font below is **open source** and redistributable. Each is available on
Google Fonts under the stated license. The Madcats "All Cats Are Beautiful"
book example is **excluded** from this corpus — its text is Madcats' property,
not public domain — so its fonts (OpenDyslexic, Source Serif Pro) are not
vendored here.

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
