---
title: Promotion Delta 2026-09-16 — release/2026.9 → main
type: research
status: draft
owner: elijah
created: 2026-09-16
updated: 2026-09-16
sidebar_position: 7
tags: [release, gate, promotion, wpt, regressions]
---

# Promotion delta 2026-09-16 — `release/2026.9` → `main`

Measured evidence for the first promotion gate run on the v2026.9.1 batch.

| | |
|---|---|
| baseline | `dc8b8bc` — the branch point. Its engine source IS `main`'s engine (`main` = `dc8b8bc` + one non-engine commit), so this is the last shipped behavior, v2026.9.0 |
| candidate | `1b78945` — `release/2026.9` tip, 97 commits later |
| binaries | freshly built debug binaries, one per side; identities COMPATIBLE |
| selection | 283 tests, 522 documents; 5×3 in page, 0.5 in margins, 96 DPI, pypdfium2 5.13.0 |

## Headline

The pair score improved sharply and the scoreboard hides a large amount of churn.

| Metric | Value |
|---|---|
| WPT score | **117 → 166 PASS** (net +49) |
| FAIL → PASS | 96 |
| PASS → FAIL | **47** |
| status unchanged | 140 |
| changed documents (full selection) | **490 of 522** |
| changed TEST docs, status unchanged (hidden) | 131 |
| changed REFERENCE docs | 228 |
| direct checks | **13 / 13 PASS** |
| gate verdict | FAILED — 490 unreviewed changes |

The gate exists because the pair comparison is self-consistent. Here 131 changed
test documents moved without moving the score, and 228 references changed
underneath their tests.

## The 47 PASS → FAIL flips

None of the 47 appear in the 2026-09-14 ground-truth sweep, so every one is new
relative to v2026.9.0. Chrome ground truth from wpt.fyi (`master`, aligned run)
says **37 PASS and 10 FAIL** of the 47 — so most are valid tests, not invalid
fixtures.

The dominant pattern is the one this project has hit repeatedly (CORE-140,
CORE-152, CORE-153): the **reference side moved toward its stated intent while
the test side did not follow**, so a pair that previously matched only because
both sides were wrong now disagrees. Examples, with the fixture's own words:

- `monolithic-overflow-012`: intent "This text should be at the middle of the
  fourth page"; ref went 1 → 4 pages, test stayed at 1.
- `monolithic-overflow-028`: intent "There should be four green pages"; ref went
  1 → 4, the test went 1 → 12.
- `body-background-{slr,vlr,vrl}`: intent "a blue box on the first page, and a
  hotpink box on the second page"; each ref went 1 → 2, each test stayed at 1.
- `transform-024`: intent "There should be five pages"; ref went 1 → 5, the test
  went 1 → 4.

Four clusters carry most of the flips, and each needs a decision rather than a
blanket disposition:

| Cluster | Flips | Reading |
|---|---|---|
| `css-break/table/table-fragmentation-001/003` | 8 | `001a/b/d` test stayed 1 page while the ref moved to 2; `003*` are Chrome-failing fixtures |
| `css-break/flexbox/single-line-*-fragmentation` | 10 | page counts identical (2 → 2) — pixel-level differences, nothing structural |
| `css-page/monolithic-overflow-*` | 9 | ref-side corrections exposing test-side gaps, per the intents above |
| `block-00{1,2}-wm-v{lr,rl}` | 4 | test 1 → 10-12 pages; the largest page-count movements in the batch |

### Suggested follow-ups (nothing filed yet)

1. `block-001-wm-vlr/vrl` and `block-002-wm-vlr/vrl` — a 1-page fixture rendering
   10-12 pages deserves its own investigation before this batch is called good.
2. `monolithic-overflow-028` — test at 12 pages against an intent of four.
3. The `css-break/flexbox/single-line-column-flex-fragmentation-068*/069*` family —
   same page counts, different pixels. Confirm which side is right.
4. The 10 Chrome-failing flips are invalid-test candidates in the CORE-177
   disposition sense and belong in that bucket, not in an engine ticket.

## The bounded (engine-change) tier is not green either

A bounded tier was captured on `css-page/margin-boxes` (37 tests, 70 documents)
to test whether a subset clears the bar:

- 0 PASS → FAIL in the family; 12 tests FAIL → PASS.
- **69 changed documents**, and only 23 of them sit on a test whose status moved.
- The changes are large, not uniform drift: 65 of 69 move ink by more than 10%,
  and 46 gain ink. A reference that previously painted nothing now paints 1482
  dark pixels (`content-001-print-ref`), and `background-001` goes from 235 to
  2662 — consistent with the margin-box background, content and paint-order work
  (CORE-141/143/179/184/202) landing, but not proof of it per document.

So a green bounded-tier gate still needs 69 recorded dispositions, and approving
them wholesale would violate the gate's own rule against blanket approval.

## Outcome

The promotion bar is the **engine-change tier** (spec Behavior 26, updated
2026-09-16): the bounded tier must pass, and the full-selection delta above is the
recorded residual rather than 490 per-document approvals.

- Engine-change tier — `--filter css-page/margin-boxes`, 37 tests / 70 documents:
  **`GATE PASSED`**, 0 conditions, 13/13 direct checks, environment COMPATIBLE.
- Review record: `gate/reviews/eng-candidate-1b78945.json`, 69 dispositions.
  34 are corrections resting on a FAIL→PASS status flip or a measured ink gain;
  35 are variations, mostly reference-side movement. Every reason states its
  evidence, and each says whether the document was diffed visually (none were —
  the approvals are family-level).
- Full-selection delta: the 490 changed documents in this record. Not a pass.

The 47 flips and the clusters above still need follow-up issues; this record is
the evidence for them.
