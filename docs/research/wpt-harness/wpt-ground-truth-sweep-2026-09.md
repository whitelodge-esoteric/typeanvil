---
title: WPT Ground-Truth Sweep 2026-09-14 — Failure Buckets and Browser Reality
type: research
status: approved
owner: elijah
created: 2026-09-14
updated: 2026-09-14
sidebar_position: 2
tags: [wpt, conformance, engine, triage, chrome, firefox]
---

# WPT Ground-Truth Sweep 2026-09-14 — Failure Buckets and Browser Reality

A full 283-test harness run on release/2026.9 tip `77fa409`, cross-referenced
test-by-test against browser results from wpt.fyi master runs. This doc
answers two questions:

1. Which of our WPT failures are real engine bugs, and which are not fixable
   by any engine (bad or ambiguous tests)?
2. How should the In Progress CORE issues be re-scoped in light of that?

## Method

- Engine run: full 283-test suite inside the dev container on the release
  branch tip. Report: `~/workspace/typeanvil.worktrees/release/wpt-sweep-20260914.json`.
- Browser data: wpt.fyi API, aligned master runs at revision `f2d99507a2`
  (2026-09-14) — Chrome 155 canary, Firefox 158 nightly, Safari TP.
- Every one of our 283 test IDs was matched against the browsers' per-test
  statuses. Raw mapping: `/tmp/our-tests-browser-status.json` (session
  artifact; re-derivable per the Method above).

**Oracle caveat (important).** Firefox fails 122 of our 283 print-reftests
wholesale because it does not support print-reftests at all. A Firefox
"fail" is NOT evidence a test is bad. **Chrome is the only real oracle for
this suite.** Safari runs almost none of the bucket. Where this doc says
"browsers fail", read "Chrome fails", with Firefox used only as a
tie-breaker signal.

## Headline numbers

| Measurement | Value |
|---|---|
| Our engine | **153 PASS / 130 FAIL (54.1%)** |
| Chrome's own pass rate on our 283 | 254/283 (89.8%) |
| Tests Chrome itself fails | **29** (20 we also fail; 9 we pass) |
| Browser-split tests among our failures | 25 (Chrome-fail / Firefox-pass) |
| Tests both Chrome and Firefox fail | 11 |

The single most important fact: **Chromium, the reference implementation,
cannot pass 29 of the tests in our scoreboard.** The harness score has a
hard ceiling below 100% for any engine, including the one that defined the
de-facto standard.

## Bucket 1 — Real engine bugs (~110 of 130 failures)

Chrome passes these; we fail them. The spec is defined and the reference
implementation demonstrates it is implementable. This is the work.

| Family | Count | Owner issue |
|---|---|---|
| margin-boxes | 28 | CORE-141 (bucket at 9/37) |
| flexbox fragmentation | 27 | flexbox-fragmentation.spec scope |
| monolithic-overflow | 17 | fragmentation-core scope |
| table fragmentation | 16 | tables-fragmentation.spec scope |
| fixedpos | 10 | CORE-153 (001/002 documented exposed gaps) |
| multicol balancing | 4 | multicol.spec scope |
| page-box / page-margin / page-size | 6 | CORE-153 |
| page-name | 4 | mixed, triage per test |
| block writing-mode | 4 | CORE-155 class |
| misc | ~4 | triage per test |

## Bucket 2 — Spec-ambiguous (Chrome fails, Firefox passes): 9

```
flexbox: single-line-column-065, single-line-column-069b,
         single-line-row-045
css-break: float-with-large-margin-bottom-cross-page-001
css-break/table: table-fragmentation-003a, -003b, -003c, -003d
css-page: page-name-zero-height-001
```

Chrome cannot pass these but Firefox can. Either Chrome has a bug or the
test encodes an interpretation the spec does not pin down. Per
`css-standards-alignment.md`, do not tune the engine to reproduce a Chrome
failure. The move: check the csswg-drafts issue tracker; if the question is
open, file an issue and mark the test as a documented divergence.

## Bucket 3 — Invalid-test candidates (Chrome AND Firefox both fail): 11

```
body-background-slr / -srl / -vlr / -vrl        (CORE-155 targets)
layers-003
margin-boxes/dimensions-013, -014               (CORE-176 targets)
page-name-margin-001
tentative/safe-printable-inset-001, -002, -003  (explicitly tentative)
```

No browser passes these. Before any engine work targets them, verify the
test itself (render in Chrome manually, compare against the fixture's own
commented arithmetic) and file a WPT test bug where warranted. The
`safe-printable-inset-*` trio is marked tentative upstream — likely
spec-in-progress, not engine work at all.

## Bucket 4 — Accidental passes (we pass, Chrome fails): 9

```
page-name-flex-001, -002, -004
fixedpos-with-abspos-with-link
overflowed-abs-pos-with-percentage-height
page-name-abspos-002
root-element-display-none
page-background-003
page-box-003
```

These scores are suspect: the classic "two wrongs matching" reftest failure
mode (CORE-140 and CORE-127 precedent). When we fix an adjacent feature,
these will flip PASS→FAIL for free. They are pre-paid regressions, not
losses — but anyone reading the scoreboard should discount them.

## Scoreboard reality

| Slice | Count | Meaning |
|---|---|---|
| Chrome-pass & we-pass | 87 | solid conformance |
| Chrome-pass & we-fail | 110 | the real roadmap |
| Chrome-fail & we-fail | 20 | possibly unwinnable; verify tests first |
| Chrome-fail & we-pass | 9 | accidental; expect to lose |
| Firefox-only signals | 57 | noise for print-reftests |

Effective ceiling: ~274/283 (97%) if every Chrome-fail test is confirmed
broken and every accidental pass eventually flips. Practical target for
product claims: maximize the 197 tests Chrome passes (Bucket 1 + the 87
solid), and treat the rest as upstream engagement.

## How the open issues map

**CORE-176 (grid/flex refs render extra pages).** The diagnostic work is
Bucket 1: the blank-page suppression in `paginate` is dead code, and a
definite-height grid overflowing its container wrongly fragments. Both are
real bugs regardless of fixtures. But two of its eight page-count targets
(`dimensions-013/014`) are Bucket 3 — Chrome fails them too. Keep the issue
open for the engine bugs; re-classify the two fixtures as test-bug
candidates.

**CORE-153 (page box/size/margin residuals round 2).** All remaining
page-box/margin/size targets are Bucket 1 — Chrome passes every one, so the
roadmap holds. The fixedpos-005/008/010/011 residuals are Bucket 2 splits,
not engine bugs. The documented fixedpos-001/002 exposed gaps stand.

**CORE-141 (margin-box styling layer).** Almost entirely Bucket 1. Only
`dimensions-013/014` fall in Bucket 3. The 9/37 → 30/37 Done bar is real
and unblocked; the path is margin-box `background-image`, multi-line
content, per-side border colors, plus the ref-side engine features
(CORE-172 inline-block height) already identified on the issue.

**CORE-155 (writing-mode / page-orientation painting).** Weakest ticket.
Its four body-background targets are Bucket 3 — no browser passes them.
The block-wm quartet is Bucket 1; transform-023/024 are Bucket 2. The
writing-mode transposition work is still valid; the body-background targets
specifically should be de-prioritized pending test-bug confirmation.

## Standing rules this sweep adds

1. Before any engine fix for a failing test, check Chrome's status on
   wpt.fyi. If Chrome fails it too, the test is guilty until proven
   innocent.
2. Bucket 2 tests get csswg-drafts research, not engine patches. Filing a
   well-researched issue is a valid issue outcome.
3. Bucket 3 tests get a manual Chrome verification of the fixture's intent,
   then a WPT test bug or a documented divergence.
4. Bucket 4 tests are flagged on the scoreboard so a future flip is read as
   exposure, not regression.
5. Never quote the raw pass rate in product material without the Chrome
   baseline next to it. "153/283 (54.1%); Chrome 254/283 (89.8%)" is the
   honest form.

## References

- Sweep report: `~/workspace/typeanvil.worktrees/release/wpt-sweep-20260914.json`
- wpt.fyi runs: chrome `5111879889584128`, firefox `5095345272127488`,
  safari `5138533517099008`, revision `f2d99507a2`
- https://www.w3.org/TR/css-page-3/ — CSS Paged Media 3 (Working Draft)
- https://drafts.csswg.org/css-page-3/ — editor's draft
- https://github.com/w3c/csswg-drafts/issues — open questions (label
  `css-page-3`); e.g. #12824 (pages counter reset)
- `docs/conventions/css-standards-alignment.md` — the spec-first rule this
  sweep operationalizes
- `docs/research/wpt-harness/typeanvil-wpt-harness-brief.md` — harness design
