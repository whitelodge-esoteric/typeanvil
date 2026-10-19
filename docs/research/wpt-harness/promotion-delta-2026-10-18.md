---
title: Promotion delta — release/2026.10 to main
type: research
status: approved
owner: maintainers
created: 2026-10-19
updated: 2026-10-19
sidebar_position: 20
tags: [harness, wpt, release, promotion, gate]
---

# Promotion delta: `release/2026.10` to `main` (CORE-525)

This record follows [Run the release gate](../../operations/release-gate.md#promotion).
It lists what changes in the WPT results and rendered output when `main` moves to the release tip.

## Identities

| Side | Ref | Commit | Binary sha256 |
|---|---|---|---|
| Baseline | `origin/main` | `3ee931bcf7f5a6714de816dfaa1c8a20d381816f` | `60203b7b…c025e` |
| Candidate | `origin/release/2026.10` | `599a14386606889355080ed06cf73a515c9d383f` | `e6516462…49690` |

Both sides were built in the dev container (debug profile) and captured at the same WPT revision
`27c619af234cb834defa10edfc24e92d495adc7f`, 5in x 3in pages, 0.5in margins, 96 DPI, pypdfium2 5.14.0.
`main` is 119 commits behind the release tip and has 3 commits of its own (see [Merge resolution](#merge-resolution)).

## Engine-change tier

Direct checks (22) plus `--filter css-page/margin-boxes` (37 tests, 70 documents).

- WPT results: baseline FAIL 25 / PASS 12, candidate FAIL 23 / PASS 14. No PASS to FAIL flip.
- The gate with an empty `--reviews` directory reported 65 `unreviewed_change` conditions (84 changed properties).
- All 22 direct checks passed.
- With the family-level record `gate/reviews/candidate-599a143.json`, the gate printed `GATE PASSED`.
- Tagged gate: 47 fixtures, 0 unexpected, `TAGGED GATE PASSED`.

The 65 dispositions are family-level. Nobody examined each document. Two families:

- 22 documents with a `page_size` change (360x216 pt to 384x324 / 384x336 pt), `dimensions-003` to `dimensions-014`.
  Cause: CORE-258 sizes the page box from the declared `@page` width and height (css-page-3 section 3).
- 43 documents with a `rendered_image` change only. Causes: generic font families (CORE-361), margin-box layout,
  running elements (CORE-390).

## Full selection delta

Full selection: 283 tests on `main`, 309 on the release tip (522 and 564 documents).

| Movement | Count |
|---|---|
| PASS to FAIL (regressions) | 0 |
| FAIL to PASS | 24 |
| New tests, PASS | 19 |
| New tests, FAIL | 7 |
| Overall | FAIL 102 / PASS 181 to FAIL 85 / PASS 224 |

The 26 new tests are all `mathml/presentation-markup/*` (the MathML subset in `harness/mathml_subset.py`).

Changed documents: 470 of 522 common documents render differently. This is expected after 119 commits that
changed fonts, margins, tables, and fragmentation. The count is a measure of drift, not a defect count.

### FAIL to PASS (24)

css-break:
- `break-nested-float-in-table-001-print`
- `flexbox/multi-line-row-flex-fragmentation-080-print`
- `flexbox/single-line-column-flex-fragmentation-066-print`, `-068a`, `-068b`, `-068c`, `-068d`
- `flexbox/single-line-row-flex-fragmentation-046-print`
- `table/table-fragmentation-003a-print`, `-003b`, `-003c`, `-003d`
- `transform-023-print`

css-multicol:
- `column-balancing-paged-001-print`

css-page:
- `margin-boxes/content-006-print`
- `margin-boxes/dimensions-009-print`
- `media-queries-002-print`
- `monolithic-overflow-005`, `-007`, `-014`, `-015`, `-021`, `-028`, `-031` (all `-print`)

### New MathML tests (26)

PASS (19): `frac-invalid`, `frac-invalid-2`, `mo-invisibleoperators`, `mo-lspace-rspace-4`,
`munder-mover-align-accent-false`, `munder-mover-align-accent-true`, `stretchy-mover-1a`, `-1b`, `-3`,
`stretchy-msup-1a` to `-1d`, `tables/columnspan-rowspan-001` to `-006`.

FAIL (7): `frac-invalid-3`, `mo-lspace-rspace`, `mo-lspace-rspace-2`, `munderover-align-accent-false`,
`munderover-align-accent-true`, `stretchy-mover-2a`, `stretchy-mover-2b`.
These are not regressions: the tests did not exist in the baseline selection. They are existing MathML gaps.

### Page-count changes (16 documents)

| Document | Main | Release |
|---|---|---|
| `table/table-fragmentation-001c-print` / `-ref`, `-001d-print` | 2 | 3 |
| `table/table-fragmentation-002c-print` / `-ref`, `-002d-print` | 3 | 4 |
| `table/table-fragmentation-003c-print` / `-ref`, `-003d-print` | 2 | 3 |
| `transform-024-print` | 4 | 5 |
| `column-balancing-paged-001-print` | 2 | 3 |
| `monolithic-overflow-016-print` | 4 | 5 |
| `monolithic-overflow-028-print` | 12 | 4 |
| `monolithic-overflow-031-print-ref` | 8 | 5 |
| `flexbox/multi-line-row-flex-fragmentation-082-print-ref` | 4 | 2 |
| `moz-multicol3-column-balancing-break-inside-avoid-1` | 3 | 2 |

None of these moves a test from PASS to FAIL. The table fragmentation tests that pass in 003a-d now fragment as
their references do. `transform-024-print` (4 to 5) and `monolithic-overflow-016-print` (4 to 5) are
failing on both sides. The release history has CORE-242 for `transform-024`; the owner for `monolithic-overflow-016` was not checked.

## Regressions and follow-up issues

There are no PASS to FAIL regressions in the engine tier or the full selection, so this promotion needs no
regression follow-up issues. The 85 remaining FAIL tests were not triaged here.

## Merge resolution

`main` has 3 commits that the release branch does not contain: `97e827d` (CORE-305 HTML-only wording),
`3f5f7e8` and `3ee931b` (CORE-313 branding). A trial merge (`git merge-tree`) conflicts in 7 files:
`README.md`, `docsite/docusaurus.config.ts`, `docsite/static/img/favicon.ico`, `engine/src/main.rs`,
`npm/typeanvil/README.md`, `npm/typeanvil/package.json`, `scripts/release-README.md`.

They were not carried onto the release line. The release line replaced them with newer work:

- **HTML-only wording (CORE-305)**: stale. Markdown input is in the release line (`engine/src/main.rs:81` reads
  "HTML and Markdown to PDF"). Keep the release text. Taking main's text would make the CLI help wrong.
- **README lockup and favicon (CORE-313, 2026-10-06)**: superseded by CORE-442 (2026-10-17), which points the README
  at `docs/assets/brand/typeanvil-lockup-{dark,light}@2x.png` and replaced the favicon. Keep the release version.
  `typeanvil-lockup-dark.png` at the repository root exists only on main and is no longer referenced by the release README.

Decision: resolve every conflict with the release side (`git merge -X theirs` from the release branch, or
`git merge -s ours` into a release-side merge commit). After the merge, the README lockup is the CORE-442 `<picture>`
element and no "HTML-only" string remains.

## Promotion status

The promotion merge and its PR are owner actions. This record does not promote. A fast-forward is impossible
(main has 3 commits of its own), and a GitHub merge button fails on the 7 conflicts. See the card comment for the
exact steps.
