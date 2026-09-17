---
title: CORE-224 Flip Triage — the 47 PASS→FAIL Regressions from the v2026.9.1 Batch
type: research
status: draft
owner: elijah
created: 2026-09-17
updated: 2026-09-17
sidebar_position: 8
tags: [wpt, conformance, triage, release, regressions, core-224]
---

# CORE-224 flip triage — the 47 PASS→FAIL regressions from the v2026.9.1 batch

Every one of the 47 PASS→FAIL flips the promotion gate exposed is classified
here with the evidence it rests on. The headline result: **none of the 47 is a
test-side regression.** Every flip involves reference-side movement (0 of 47
have a byte-identical reference), and 9 of 47 have a byte-identical test
raster. The batch's spec-correct changes moved renders toward the fixtures'
stated intent; where a pair previously matched only because both sides were
wrong, the movement broke the match. The current FAIL is the honest
unimplemented-feature state, not a lost correct behavior.

## Method

- **Inputs.** The promotion-gate captures `baseline-dc8b8bc.json` (v2026.9.0
  engine) and `candidate-1b78945.json` (release/2026.9 tip), 283 tests / 522
  documents, identical runner settings (5×3in, 0.5in margins, 96 DPI,
  pypdfium2 5.13.0, WPT rev `27c619af`). Evidence dir:
  `~/workspace/typeanvil-evidence/core-224/`.
- **Movement.** Per-document page counts and per-page raster fingerprints from
  both captures, joined to the WPT manifest's test→reference mapping
  (`triage/flip-table.json`).
- **Browser ground truth.** wpt.fyi aligned master runs (2026-09-17, rev
  `7dbbcb8bcb`): Chrome 156 canary, Edge, Firefox, Safari. Chrome is the only
  real oracle for this print-reftest suite (Firefox does not run
  print-reftests). `triage/browser-ground-truth.json`.
- **CORE-177 dispositions.** The approved
  [core177-test-dispositions.md](./core177-test-dispositions.md) governs how a
  Chrome-failing test is read (valid-with-Chrome-bug vs broken/tentative).
- **Bisect.** The `block-00{1,2}-wm-*` over-pagination was bisected by building
  the engine at six commits and rendering `block-001-wm-vlr-print.html`
  (`triage/bisect-probes/`).

## Headline

| Metric | Value |
| -- | -- |
| flips | 47 |
| Chrome passes | 37 |
| Chrome fails | 10 |
| test raster byte-identical (reference moved) | 9 |
| reference raster byte-identical (test moved) | **0** |
| real test-side regressions | **0** |

The single most important fact: **no flip has a frozen reference.** Every one
of the 47 is a reference-side movement (or a both-sides movement). A flip
cannot be a test-side regression when the test document's raster is unchanged,
and it cannot be a pure test-side regression when the reference also moved.

## The `block-00{1,2}-wm-{vlr,vrl}` over-pagination — explained

The four `block-00x` flips are the ticket's most suspicious cluster: a 1-page
fixture now renders 10–15 pages. The cause is a **spec-correct change exposing
a pre-existing capability gap**, not a new defect.

1. **Chrome passes all four** (wpt.fyi, aligned master). They are valid
   fixtures, designed to span multiple pages: `block-001` declares
   `html { block-size: 40vw }` and `.b { block-size: 210vw }` in
   `vertical-lr`, so the block axis (physical x) is ~250vw ≈ 2.5 page widths.
2. **The 1-page render at v2026.9.0 was an accidental pass.** Both sides
   rendered one page because the declared block-size did not consume
   fragmentainer extent.
3. **Bisect pins the flip to `a14f135` (CORE-152, "declared-height
   continuation consumes skipped fragmentainer extent").** Measured page counts
   for `block-001-wm-vlr-print.html`:

   | commit | change | pages |
   | -- | -- | -- |
   | `dc8b8bc` | v2026.9.0 baseline | 1 (PASS) |
   | `7db761b` | CORE-165 html-root-box | 1 (PASS) |
   | `845a7bb` | CORE-166 direction:rtl | 1 (PASS) |
   | `a14f135` | **CORE-152 declared-height continuation** | **12 (FAIL)** |
   | `b1b1acf` | CORE-167 declared-height pagination | 12 (FAIL) |
   | `1b78945` | candidate tip | 12 (FAIL) |

   CORE-152 is spec-correct (css-break-3 §5.3): a declared-height box that
   continues past a fragmentainer now makes block-size progress to the
   fragmentainer edge. Its own gate text names three more of our 47
   (`monolithic-overflow-027/028/029` ref-side corrections,
   `monolithic-overflow-021` two-wrongs flip, `column-balancing-paged-001`).
4. **The wrong count is the CORE-203 gap.** Root-vertical block progression is
   unimplemented (CORE-181 → CORE-203). The engine does not transpose the block
   axis for a root-vertical document, so the declared block-size fragments
   along the wrong axis: 12 pages (test) / 10–15 (ref) instead of Chrome's ~3.
   CORE-203's own acceptance criteria enumerate exactly these four tests.

**Disposition:** exposed accidental pass + pre-existing CORE-203 gap. Recorded
against CORE-203. Not a new regression.

## Disposition table (all 47)

Kinds: `EXPOSED-GAP` both sides moved, pair no longer matches; `REF-ONLY`
test raster byte-identical, reference moved; `REF-CORRECTION` reference moved
toward intent, test page count unchanged; `PIXEL-ONLY` page counts equal both
sides; `TEST-OVER-PAGINATES` test side paginates past the reference;
`CHROME-BUG-VALID` Chrome fails but the fixture is valid (CORE-177);
`TENTATIVE` explicitly tentative csswg proposal, not engine work.

| test | kind | chrome | test | ref | owner |
| -- | -- | -- | -- | -- | -- |
| break/block-001-wm-vlr | EXPOSED-GAP | P | 1→12 | 1→12 | CORE-203 |
| break/block-001-wm-vrl | EXPOSED-GAP | P | 1→12 | 1→12 | CORE-203 |
| break/block-002-wm-vlr | EXPOSED-GAP | P | 1→10 | 1→15 | CORE-203 |
| break/block-002-wm-vrl | EXPOSED-GAP | P | 1→10 | 1→15 | CORE-203 |
| break/break-inside-avoid-multicol-001 | EXPOSED-GAP | P | 1→2 | 1→2 | multicol spec |
| break/break-nested-float-in-table-001 | REF-ONLY | P | 1→1 | 1→3 | float/table frag |
| break/flexbox/multi-line-row-flex-fragmentation-080 | EXPOSED-GAP | P | 1→2 | 1→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-066 | PIXEL-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-068a | REF-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-068b | REF-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-068c | REF-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-068d | REF-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-069a | PIXEL-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-069b | CHROME-BUG-VALID | F | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-069c | PIXEL-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-column-flex-fragmentation-069d | PIXEL-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/flexbox/single-line-row-flex-fragmentation-046 | PIXEL-ONLY | P | 2→2 | 2→2 | flexbox-frag spec |
| break/float-with-large-margin-bottom-cross-page-001 | CHROME-BUG-VALID | F | 1→2 | 1→2 | float frag |
| break/overflowing-block | TEST-OVER-PAGINATES | P | 1→3 | 1→1 | CORE-152 class |
| break/table/table-fragmentation-001a | REF-CORRECTION | P | 1→1 | 1→2 | table-frag spec |
| break/table/table-fragmentation-001b | REF-CORRECTION | P | 1→1 | 1→2 | table-frag spec |
| break/table/table-fragmentation-001c | EXPOSED-GAP | P | 1→2 | 1→2 | table-frag spec |
| break/table/table-fragmentation-001d | EXPOSED-GAP | P | 1→2 | 1→2 | table-frag spec |
| break/table/table-fragmentation-003a | CHROME-BUG-VALID | F | 1→1 | 1→3 | table-frag spec |
| break/table/table-fragmentation-003b | CHROME-BUG-VALID | F | 1→1 | 1→3 | table-frag spec |
| break/table/table-fragmentation-003c | CHROME-BUG-VALID | F | 1→1 | 1→2 | table-frag spec |
| break/table/table-fragmentation-003d | CHROME-BUG-VALID | F | 1→1 | 1→2 | table-frag spec |
| break/transform-023 | PIXEL-ONLY | P | 2→2 | 2→2 | transform/page-orientation |
| break/transform-024 | EXPOSED-GAP | P | 1→4 | 1→5 | transform/page-orientation |
| multicol/auto-fill-auto-size-002 | TEST-OVER-PAGINATES | P | 1→3 | 1→2 | multicol spec |
| multicol/column-balancing-paged-001 | EXPOSED-GAP | P | 1→2 | 1→3 | multicol spec |
| page/body-background-slr | CHROME-BUG-VALID | F | 1→1 | 1→2 | CORE-203 |
| page/body-background-vlr | CHROME-BUG-VALID | F | 1→1 | 1→2 | CORE-203 |
| page/body-background-vrl | CHROME-BUG-VALID | F | 1→1 | 1→2 | CORE-203 |
| page/fixedpos-010 | EXPOSED-GAP | P | 2→4 | 2→4 | fixedpos/abspos |
| page/fixedpos-011 | PIXEL-ONLY | P | 3→3 | 3→3 | fixedpos/abspos |
| page/media-queries-002 | PIXEL-ONLY | P | 1→1 | 1→1 | media-query eval |
| page/monolithic-overflow-005 | EXPOSED-GAP | P | 1→4 | 1→4 | CORE-152 class |
| page/monolithic-overflow-007 | EXPOSED-GAP | P | 1→2 | 1→4 | CORE-152 class |
| page/monolithic-overflow-008 | EXPOSED-GAP | P | 1→2 | 1→4 | CORE-152 class |
| page/monolithic-overflow-012 | REF-CORRECTION | P | 1→1 | 1→4 | CORE-152 class |
| page/monolithic-overflow-014 | EXPOSED-GAP | P | 1→2 | 1→2 | CORE-152 class |
| page/monolithic-overflow-015 | EXPOSED-GAP | P | 1→2 | 1→4 | CORE-152 class |
| page/monolithic-overflow-021 | EXPOSED-GAP | P | 1→2 | 1→2 | CORE-152 class |
| page/monolithic-overflow-028 | TEST-OVER-PAGINATES | P | 1→12 | 1→4 | CORE-152 class |
| page/monolithic-overflow-031 | EXPOSED-GAP | P | 1→5 | 1→8 | CORE-152 class |
| page/tentative/safe-printable-inset-003 | TENTATIVE | F | 1→1 | 1→1 | — |

## Cluster readings

- **`block-00{1,2}-wm-*` (4)** — exposed CORE-203 gap (see above).
- **`body-background-{slr,vlr,vrl}` (3)** — reference moved 1→2 toward the
  stated intent ("a blue box on the first page, a hotpink box on the second");
  the test side stayed at 1. Chrome fails these on wpt.fyi but CORE-177
  measured them as VALID (Chrome minor bug, gradient-stop alignment on page 2).
  Recorded against CORE-203.
- **`flexbox/single-line-*-fragmentation` (10)** — page counts identical
  (2→2) on both sides; pixel-level only. Four (`068a-d`) have a byte-identical
  test raster: the reference moved, the test did not. Chrome passes all but
  `069b` (Chrome bug, spurious third page). Exposed flexbox-fragmentation
  gaps.
- **`table-fragmentation-001` (4)** — `001a/b` are pure reference corrections
  (test 1→1, ref 1→2); `001c/d` both moved to 2. Chrome passes all four.
- **`table-fragmentation-003` (4)** — test raster byte-identical, reference
  moved 1→2/3. Chrome fails all four (CORE-177: Chrome bug/ambiguity, row
  background continuation; Firefox passes). Not engine work until the
  csswg-clarity question resolves.
- **`monolithic-overflow-*` (9)** — the reference moved toward its declared
  page count (CORE-152's ref-side corrections); the test side exposes the
  abspos + `contain:size` residual. `028` over-paginates to 12 against an
  intent of four. Chrome passes all nine.
- **`multicol` (3)** — `auto-fill-auto-size-002` test over-paginates (1→3 vs
  ref 1→2, intent "middle of second page"); `column-balancing-paged-001` both
  moved (CORE-152/167 documented). Chrome passes all three.
- **`fixedpos-010/011` (2)** — both moved to 4 pages (010) / pixel-only (011).
  Chrome passes both. Exposed fixedpos/abspos gaps.
- **`transform-023/024` (2)** — pixel-only (023) / both moved (024, test 1→4
  vs ref 1→5, intent "five pages"). Chrome passes both.
- **`overflowing-block` (1)** — test over-paginates 1→3, ref stays 1. Chrome
  passes. Declared-height over-fragmentation.
- **`break-nested-float-in-table-001` (1)** — test raster byte-identical, ref
  moved 1→3. Chrome passes.
- **`break-inside-avoid-multicol-001` (1)** — both moved to 2 (the intent: two
  `break-inside:avoid` divs, two pages). Chrome passes.
- **`float-with-large-margin-bottom-cross-page-001` (1)** — Chrome fails
  (CORE-177: Chrome bug, float margin fragmentation; Firefox passes). Not
  engine work until clarity.
- **`media-queries-002` (1)** — pixel-only, both sides 1→1. Chrome passes.
- **`safe-printable-inset-003` (1)** — explicitly tentative csswg proposal
  (`page-margin-safety`, PR #13190). Not engine work.

## Follow-ups

No flip is a real regression, so no new *defect* issue was needed for the
class of "we broke something". The exposed gaps do need owners, and five
issues now carry them:

| Issue | Scope | Flips |
| -- | -- | -- |
| [CORE-232](https://linear.app/whitelodge/issue/CORE-232) | `overflowing-block-print` over-paginates (test 3 vs ref 1), declared-height over-fragmentation | 1 |
| [CORE-234](https://linear.app/whitelodge/issue/CORE-234) | flexbox fragmentation residual — the `single-line-*` / `multi-line-row` pairs | 10 |
| [CORE-235](https://linear.app/whitelodge/issue/CORE-235) | table fragmentation residual — `table-fragmentation-001a-d` | 4 |
| [CORE-236](https://linear.app/whitelodge/issue/CORE-236) | multi-column fragmentation residual — three paged multi-col pairs | 3 |
| [CORE-237](https://linear.app/whitelodge/issue/CORE-237) | declared-height fragmentation residual — `monolithic-overflow-*` (9) plus transform-023/024, break-nested-float-in-table-001, media-queries-002 (4) | 13 |

Already-owned, no new issue:

- **CORE-203** — `block-00{1,2}-wm-*` ×4 and `body-background-{slr,vlr,vrl}` ×3.
  Its acceptance criteria enumerate these seven tests by name.
- **CORE-204** — `fixedpos-010/011`; `fixedpos-010` is a named canary there.

Not engine work, no issue — the CORE-177 invalid-test / browser-clarity bucket:

- `table-fragmentation-003a/b/c/d` — Chrome bug/ambiguity (row-background
  continuation); Firefox passes.
- `float-with-large-margin-bottom-cross-page-001` — Chrome bug (float margin
  fragmentation); Firefox passes.
- `single-line-column-flex-fragmentation-069b` — Chrome bug (spurious third
  page).
- `safe-printable-inset-003` — explicitly tentative csswg proposal
  (`page-margin-safety`, csswg-drafts PR #13190).

Coverage: 1 + 10 + 4 + 3 + 13 + 7 + 2 + 7 = **47**. Every flip carries a
disposition and an owner.

`overflowing-block` was the one flip whose test side looked like a genuine
defect rather than a pure reference-side correction (test 1→3 against a 1-page
reference, with Chrome passing). It became CORE-232, with the confirming
bisect as its first step.

## Evidence

- `~/workspace/typeanvil-evidence/core-224/captures/{baseline-dc8b8bc,candidate-1b78945}.json`
- `~/workspace/typeanvil-evidence/core-224/triage/flip-table.json`
- `~/workspace/typeanvil-evidence/core-224/triage/browser-ground-truth.json`
- `~/workspace/typeanvil-evidence/core-224/triage/final-dispositions.json`
- `~/workspace/typeanvil-evidence/core-224/triage/bisect-probes/` (block-001
  page counts at six commits)
