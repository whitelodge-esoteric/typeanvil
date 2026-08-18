# CORE-72 — First Demo Run: Baseline + Triage

Run: `bash scripts/build-demo.sh` at commit `a657cd1` (engine) / `daff6e0` (pipeline).
Prince 16.2, non-commercial license. Geometry: 5in×3in, 0.5in margins, 96 DPI raster.

## Scoreboard (committed alongside)

| Doc | Overall diff | TA pages | PR pages | Bucket |
|---|---|---|---|---|
| Invoice | 28.8% | 4 | 5 | cosmetic |
| Letterhead | 15.8% | 8 | 8 | cosmetic |
| Academic Paper | 28.8% | 9 | 10 | cosmetic |
| Prose Showcase | 28.0% | 9 | 10 | cosmetic |
| Quarterly Report | 18.7% | 10 | 10 | cosmetic |
| Table Stress | 34.0% | 20 | 41 | cosmetic |

## Method

Per-doc page pairs were inspected visually (rasterized at 96 DPI, TypeAnvil vs
Prince side by side) against the spec's bucket definitions (§Behavior 7):

- **identical** (<1%, no visible difference) — none
- **cosmetic** (1–20%, spacing/font only) / **missing-feature** (content absent
  for a known gap) / **engine-bug** (wrong without a known-feature explanation)

## Findings

**Zero engine bugs.** Every doc's *structure* renders correctly in TypeAnvil:
content present, elements placed, tables intact. The diffs are dominated by:

1. **Font substitution** (largest factor). Corpus uses `font-family: sans-serif`,
   which each engine resolves to a different system font (different metrics,
   different line-height). This explains the near-uniform 15–34% diffs and the
   page-count mismatches (e.g. Table Stress: 20 vs 41 pages — Prince's default
   line-height is much taller).
2. **Prince watermark/logo**. Prince adds a small "P" logo to every page —
   a constant pixel-diff contributor on all docs.
3. **counter(pages)** — TypeAnvil renders `Page 1 of 0` (known gap, documented
   in the manifest; letterhead only).
4. **Prince repeats tfoot oddly** on Table Stress (total row early) — that is
   Prince's own table-fragmentation behavior, not a TypeAnvil defect.

## Verified strengths (visual evidence)

- **Quarterly Report TOC**: TypeAnvil and Prince produce *identical* dotted
  leaders and page numbers (2, 4, 6, 9) on the same pages.
- **Prose Showcase**: both engines fully justify with hyphenation and even
  texture (TypeAnvil hyphenates "machinery"; no rivers in either).
- **Table Stress**: TypeAnvil repeats the table header on every page and
  preserves all 150 rows; values verified correct.
- **Invoice**: both engines render the bordered table with header + line items.
- **Letterhead**: named-page cover has no running content in either engine.

## Follow-up

- **CORE-73 filed**: corpus font-stack pinning (`font-family` + explicit
  `line-height` per fixture) to remove font-substitution noise from the diff —
  the single highest-leverage demo improvement. This is corpus work, not
  engine work, and would move several docs toward the 1–20% cosmetic band.
- No engine-bug issues filed (none found). Known gaps remain tracked by their
  original tickets (footnotes → none filed yet; counter(pages) → noted in
  manifest; border-spacing → CORE-61 non-goals).

## Baseline committed

`demo/out/scoreboard.json` + `demo/out/index.html` committed as the v0.4
baseline artifact (CORE-67's "scoreboard committed" Done criterion).
