# Document corpus

The document corpus (CORE-488): one tagged set of fixtures that feeds the
Prince comparison, the print-resolution showcase, the performance
benchmark, and the tagged-PDF gate. Tags in `manifest.json` decide which
document serves which use.

Rendered pages are not committed here (CORE-495). Browse them in the public
[typeanvil-showcase](https://github.com/whitelodge-esoteric/typeanvil-showcase)
repo or on its [Pages site](https://whitelodge-esoteric.github.io/typeanvil-showcase/).
See `docs/operations/showcase.md` for how renders are published and
`docs/operations/demo-refresh.md` for the refresh and triage flow.

| Path | What it is |
|---|---|
| `<category>/*.html` | Fixtures (self-contained HTML + CSS), grouped by category |
| `<category>/assets/` | Assets that fixture set references with a relative `assets/...` URL |
| `manifest.json` | Per-document tags and metadata (see below) |
| `LICENSES.md` | Third-party sources, fonts and licenses |
| `out/compare/` | Generated, untracked: per-page PNGs, `scoreboard.json`, `gallery.md` (build-demo.sh) |
| `out/showcase/` | Generated, untracked: Letter @ 300 DPI PNGs, `inspect.md`, `gallery.md` (build-showcase.sh) |
| `SHOWCASE.md` | Pointer to the published showcase |
| `TRIAGE.md` | Frozen triage archive (see the triage policy below) |
| `README.md` | This file |

Engine-gap repros live outside the corpus in `repros/` (`repros/manifest.json`,
files named `CORE-<n>-<slug>.html`).

Manifest fields per document: `name`, `file` (`<category>/<doc>.html`),
`category`, `size` (`small` | `heavy` | `visual`), `oracle` (`prince` |
`weasyprint` | `chromium`), `roles` (`gate`, `perf`, `showcase`), `geometry`
(`width`, `height`, `margin`), `source`, `license`, `baseline_pages` (page
count at that geometry on 570d3a5), plus `wedge_features`, `known_limitations`,
`expected_deltas` and optional `expected_change`.

Every script filters by tag, for example
`scripts/build-demo.sh --role gate --size small`,
`scripts/build-showcase.sh --category posters`,
`scripts/benchmark.py --role perf --size small`,
`scripts/visual-qa.sh --track corpus --oracle prince`.

### Dedupe record (CORE-488)

Four showcase fixtures duplicated corpus fixtures and were dropped; the corpus
copy is kept and now carries the `gate` and `perf` roles.

| Dropped (`demo/showcase/`) | Kept | Why |
|---|---|---|
| `invoice.html` | `financial/invoice.html` | same document family; corpus copy is in the Prince gate |
| `report.html` | `reports/report.html` | same; the corpus copy has the Prince baseline |
| `journal.html` | `academic/journal-paper.html` | same |
| `book.html` | `book/book-chapter.html` | same |

Also dropped: `demo/corpus/_smoke.html` (removed) (unreferenced scratch) and the four
generated showcase chart assets those fixtures used. All stay in git history.

The contracts are `docs/specifications/document-corpus.spec.md` (layout,
manifest, tag filters) and `docs/specifications/visual-comparison-demo.spec.md`
(scoreboard schema, triage buckets, determinism rules).

## Prince install (macOS)

Prince is a commercial product. **This track uses Prince's free non-commercial
license** — comparison-only, never for commercial purposes. By running the
build you accept the license terms shown at install time (`LICENSE.txt` in the
cask directory).

Installed 2026-08-17:

```sh
brew install --cask prince            # downloads the 16.2 cask
cd /opt/homebrew/Caskroom/prince/16.2/prince-16.2-macos
./install.sh /opt/homebrew            # pass the prefix as $1 (the interactive
                                      # prompt defaults to /usr/local, which
                                      # fails on Apple Silicon without sudo)
```

- Binary: `/opt/homebrew/bin/prince`
- Version pinned: **Prince 16.2** (`prince --version` → "Prince 16.2 … Non-commercial License")
- License file: `/opt/homebrew/Caskroom/prince/16.2/prince-16.2-macos/LICENSE.txt`

### Why `$1`?

The installer's interactive prompt (`Press Enter to accept the default
directory`) defaults to `/usr/local`, which is not writable on a Homebrew Apple
Silicon setup. Passing `/opt/homebrew` as the first argument installs there
without sudo. Re-running the installer upgrades in place.

## Wrapper

`scripts/render-prince.sh` translates the `typeanvil render` CLI contract into
Prince flags:

| Engine flag            | Prince flag             |
|------------------------|-------------------------|
| `--page-width` `--page-height` | `--page-size="W H"` (space-separated; `5inx3in` is rejected) |
| `--margin-top/right/bottom/left` | `--page-margin="T R B L"` (same order) |
| `--base-url`           | `--baseurl=`            |
| `-o`                   | `-o`                    |

Verified: `5in × 3in, 0.5in margins` → Prince MediaBox `0 0 360 216`, identical
geometry to `typeanvil render`.

## Pipeline

```sh
scripts/build-demo.sh      # corpus → both engines → corpus/out/compare/ (gallery.md, images, scoreboard.json)
scripts/build-showcase.sh  # the Letter @ 300 DPI showcase → corpus/out/showcase/
```

Everything lands under the ignored `corpus/out/` tree. Nothing there is
committed. Deterministic: re-running produces byte-identical output except
the scoreboard's `generated` timestamp.

## Triage policy (2026-09-15, updated CORE-495)

Triage findings are tracked as **Linear issues**, not as prose in this
directory. `TRIAGE.md` is a frozen archive of the 2026-08/09 passes (CORE-79
through CORE-209): read it for method and attribution evidence, but do not
append to it.

When a refresh moves a doc's diff versus the previous baseline:

1. File or update a Linear issue for anything unexplained. Only a genuine
   engine bug becomes an engine ticket.
2. Record the per-doc delta in the Linear comment. The fingerprint baselines
   (`docs/operations/regression-gate.md`) carry the machine-checked state.
3. Keep per-fixture expectations in `manifest.json` — machine-readable, checked
   in, and the place a reader looks for "why is this number what it is".

Attribute a movement before calling it an improvement or a regression, and
measure both sides at the baseline commit and the candidate commit.
