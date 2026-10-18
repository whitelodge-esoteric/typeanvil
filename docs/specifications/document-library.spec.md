---
title: Document Library
slug: /specifications/document-library
type: spec
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 7
tags: [demo, library, manifest, corpus, showcase, benchmark]
spec_id: document-library
issue_id: CORE-488
applies_to: library/, repros/, scripts/build-demo.sh, scripts/build-showcase.sh, scripts/visual-qa.sh, scripts/benchmark.py, scripts/tagged_gate.py
dependencies: [visual-comparison-demo, showcase-render, tagged-gate]
---

# Document Library

## Overview

Two fixture sets used to live side by side: the comparison corpus
(`demo/corpus/`, 5in × 3in, Prince diff) and the showcase (`demo/showcase/`,
US Letter, 300 DPI). They overlapped (journal, invoice, report and book each
existed twice) and diverged over time. One library now holds every document.
One manifest tags each document, and the tags decide which pipeline uses it.

Epic: CORE-486. Issue: CORE-488.

## Goals

- One directory tree, `library/<category>/<doc>.html`, and one manifest,
  `library/manifest.json`.
- Per-document `geometry` replaces the fixed small-versus-Letter split.
- Every script reads the manifest and filters by tag.
- Engine-gap repros live apart from the library in `repros/`.
- Moving a document does not change its page count.

## Non-Goals

- New documents beyond the four heavy documents (CORE-491). Other sources
  arrive in later issues and add their entries to `LICENSES.md` first.
- Changing the scoreboard schema, the triage buckets or the showcase DPI.
- Renaming scoreboard keys beyond the new `<category>/<doc>.html` path.

## Behavior

1. Fixtures SHALL live at `library/<category>/<doc>.html`. A fixture SHALL
   resolve its relative `assets/...` URLs from its own category directory, so
   each category directory carries the assets its fixtures reference.
2. `library/manifest.json` SHALL be a JSON array. Each entry SHALL carry:
   - `name`, `file` (`<category>/<doc>.html`, relative to `library/`),
     `category` (equal to the directory name);
   - `size`: `small` (renders at 5in × 3in and fewer than 10 pages), `heavy`
     (renders at 5in × 3in, 10 pages or more) or `visual` (print-resolution
     showcase document);
   - `oracle`: the engine whose output the document is compared against
     (`prince`, `weasyprint` or `chromium`);
   - `roles`: a non-empty subset of `gate` (Prince comparison and scoreboard
     guard), `perf` (benchmark), `showcase` (print-resolution gallery);
   - `geometry`: `{width, height, margin}`, passed to the engine as
     `--page-width`, `--page-height` and four equal `--margin-*` flags;
   - `source` and `license`;
   - `baseline_pages`: the page count at that geometry on commit `570d3a5`;
   - `wedge_features`, `known_limitations`, `expected_deltas`, and the optional
     `expected_change` note defined by visual-comparison-demo.
3. Every script that renders library documents SHALL accept the tag filters
   `--role`, `--size`, `--oracle` and `--category`. A filter that is not set
   matches every entry. A filter that is set matches only entries that carry
   that value (`--role` matches any member of `roles`).
   - `scripts/build-demo.sh`: default `--role gate --oracle prince`.
   - `scripts/build-showcase.sh`: default `--role showcase`.
   - `scripts/visual-qa.sh`: `--track corpus` defaults to `--role gate
     --oracle prince`; `--track showcase` defaults to `--role showcase`.
   - `scripts/benchmark.py`: default `--role perf`.
   - `scripts/tagged_gate.py`: renders every manifest entry; no filter.
   - Heavy documents (CORE-491): `size: heavy` also covers print-geometry
     documents (Letter, A4, 6 × 9 in) that run for 10 pages or more. They carry
     the roles `gate`, `perf` and `showcase`, render at their own manifest
     geometry, and are produced by `scripts/build_heavy_library.py` from the
     sources named in `source` and `library/LICENSES.md`. A run that must stay
     short filters with `--size small`. `scripts/benchmark.py` renders every
     document at 5 in × 3 in; use `--category` to skip the heavy documents
     until the benchmark reads manifest geometry.
4. Fixture-internal `@page { size }` and `@page { margin }` rules override the
   CLI geometry, as before.
5. `repros/manifest.json` SHALL list the engine-gap fixtures (one per open or
   regression-evidence issue). Files SHALL be named `CORE-<n>-<slug>.html` and
   render at the 5in × 3in geometry from `repros/` as the working directory.
6. `library/LICENSES.md` SHALL list every third-party source, font and
   license. A manifest `license` value that is not the repository license
   SHALL have a matching entry there.
7. Generated output SHALL live under `library/out/` (`compare/` for
   build-demo.sh, `showcase/` for build-showcase.sh). The comparison gallery
   is written to `library/out/compare/gallery.md` and the showcase gallery to
   `library/out/showcase/gallery.md` (CORE-495). Generated output is
   ignored and never committed.
8. `demo/` SHALL hold no fixtures. The reusable helper scripts moved to
   `scripts/demo/`.

## Interfaces

```
python3 scripts/demo_compare.py list-manifest --manifest library/manifest.json \
    [--role R] [--size S] [--oracle O] [--category C]
# → file<TAB>name<TAB>geometry flags   (sorted by file)

scripts/build-demo.sh     [--role R] [--size S] [--oracle O] [--category C] [--guard ...]
scripts/build-showcase.sh [--role R] [--size S] [--oracle O] [--category C] [--determinism]
scripts/visual-qa.sh --track corpus|showcase --report R [--role ...] [--category ...]
python3 scripts/benchmark.py [--role perf] [--size small] [--oracle prince]
```

## Acceptance Criteria

1. **Pure move.** Given the engine at `570d3a5`-equivalent behavior, when
   every library document and every repro renders at its manifest geometry,
   then the page count equals `baseline_pages`. Evidence for CORE-488: 50
   renders, 0 mismatches, 4 deliberate drops.
   Test: `tests/test_library_manifest.py::test_baseline_pages_recorded`.
2. **No fixtures under `demo/`.** Test:
   `tests/test_library_manifest.py::test_demo_has_no_fixtures`.
3. **Manifest is complete and consistent.** Every `file` exists, `category`
   matches its directory, tags use the allowed values, `geometry` is
   well-formed, and every `.html` under `library/` is listed. Test:
   `tests/test_library_manifest.py` (several cases).
4. **Tag filters.** `list-manifest --role gate --size small` returns only
   entries with those tags. Test: `tests/test_library_manifest.py::test_filters`.
5. **Scripts run from the manifest.** `build-demo.sh --dry-run`,
   `build-showcase.sh --dry-run` and `visual-qa.sh --dry-run` print one render
   command per matching entry, with that entry's geometry.
6. **Dedupe record.** The dropped fixtures and the kept twin of each are listed
   in `library/README.md`.

## Edge Cases

- A filter that matches nothing makes the script exit 2 with a message that
  names the filters.
- Two documents with the same file stem in different categories would collide
  in `out/images/<stem>/`. The manifest test rejects duplicate stems.
- The repro layer ignores tag filters (its entries carry no tags).

## References

- visual-comparison-demo.spec.md (scoreboard, triage, determinism)
- showcase-render.spec.md (300 DPI Letter target, Prince overlay)
- tagged-gate.spec.md (tagged PDF gate reads this manifest)
- Epic CORE-486; issue CORE-488
