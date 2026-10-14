---
title: Tagged PDF Gate
slug: /specifications/tagged-gate
type: spec
status: draft
owner: maintainers
created: 2026-10-13
updated: 2026-10-13
sidebar_position: 48
tags: [harness, tagging, accessibility, gate, testing]
spec_id: tagged-gate
applies_to: scripts/tagged_gate.py
dependencies: [tagged-pdf, harness-release-gate]
---

# Tagged PDF gate

## Overview

Tagged output went about 190 commits without coverage. Nothing outside the
`engine/tests/tagged_pdf*.rs` unit tests rendered real documents tagged, so a
crash on page backgrounds (CORE-379) and 869 untagged text objects (CORE-380)
landed unnoticed. krilla's own PDF/UA-1 validator missed the second entirely.

The tagged gate renders every demo corpus and showcase fixture in tagged mode,
reads each structure tree back with `scripts/verify_tagged_pdf.py`, and fails
on any regression in a fixed set of checks. It runs in the dev container as a
required step of the [release gate](../operations/release-gate.md). The
engine has no hosted CI (CORE-377 decision, 2026-10-13); a GitHub Actions
lane is possible later and would run the same script.

## Goals

1. Catch tagging regressions the krilla validator cannot see: untagged
   content, empty or duplicated marked-content leaves, nondeterminism, and
   role changes.
2. Keep the gate honest about known failures: each expected failure names its
   ticket, and an expected failure that stops failing fails the gate until its
   entry is removed.
3. Make intentional role changes cheap to accept: one `--update` command
   refreshes the snapshot, and the diff is reviewable.

## Non-Goals

- Full structure-tree snapshots. Trees change with every role-mapping or
  reading-order fix (CORE-374, CORE-375); per-document role counts plus
  reading-order coverage catch the same regressions with a readable diff.
- Pixel comparison. The demo guard and the WPT release gate own rendering.
- Hosted CI. The script is CI-ready, but wiring a workflow is separate work.

## Behavior

1. The gate SHALL render each fixture listed in `demo/corpus/manifest.json`
   (at the corpus geometry, 5in×3in with 0.5in margins) and
   `demo/showcase/manifest.json` (Letter with 0.75in margins), in that order,
   with the given engine binary, three times: `--tagged` twice and
   `--tagged --ua` once. Fixture ids are `<track>/<file>`.
2. For each fixture it SHALL evaluate these checks; each check name is
   stable:
   - `render`: the first `--tagged` render exits 0;
   - `deterministic`: the two `--tagged` renders are byte-identical;
   - `untagged`: zero page objects outside marked content
     (`totals.untagged`);
   - `duplicated`: zero MCIDs referenced twice (`totals.mcids_duplicated`);
   - `dangling`: zero MCIDs the tree references that no page object
     carries (`totals.mcids_dangling`);
   - `ua`: the `--tagged --ua` render exits 0;
   - `roles`: the structure-type counts equal the snapshot's exactly;
   - `coverage`: the reading-order source coverage is no more than 0.005
     below the snapshot's.

   When `render` fails, the other checks for that fixture are not evaluated.
3. The snapshot (`harness/tagged-gate/snapshot.json`) SHALL map each fixture
   id to `{roles, coverage}`. A fixture with no snapshot entry fails with
   `snapshot-missing`; a snapshot entry for a fixture no manifest lists fails
   with `snapshot-stale`.
4. Known failures (`harness/tagged-gate/known-failures.json`) SHALL be a list
   of `{doc, check, ticket, reason}`. A failing check with a matching entry is
   reported as `expected` and does not fail the gate. An entry whose check
   passes fails the gate as `stale-known-failure`, so a fix must also remove
   its entry. An entry naming an unknown check fails as
   `invalid-known-failure`.
5. `--update` SHALL rewrite the snapshot from the current run (fixtures whose
   `render` passed) and then evaluate against it. It never edits the
   known-failures file.
6. Exit status: 0 when no check fails unexpectedly (`TAGGED GATE PASSED`), 1
   on any unexpected failure (`TAGGED GATE FAILED`), 2 on usage or
   environment errors (missing binary or manifest, unreadable data file).
   `--report <path>` writes the full result as JSON.

## Interfaces

```text
python3 scripts/tagged_gate.py --bin <typeanvil> [--update] [--report out.json]
                               [--repo <root>] [--keep <dir>]
```

```python
# scripts/tagged_gate.py (pure, unit-tested)
def evaluate(results: dict[str, dict], snapshot: dict, known: list[dict]) -> GateResult
```

`results` maps fixture id to the measured values
(`render_exit, deterministic, ua_exit, totals, roles, coverage`).
`GateResult.failures` lists `(doc, check, detail)` unexpected failures;
`.expected` lists the matched known failures; `.passed` is
`not failures`.

## Acceptance Criteria

- AC-1: on the release tip, the gate passes in the dev container with the
  committed snapshot and known failures.
  Verification: the run is recorded in `tagged-pdf.spec.md`'s validation log.
- AC-2 (red run): with `h1` deliberately mapped to `P` in
  `engine/src/tags.rs`, the gate fails with `roles` on every fixture whose
  snapshot has an `H1`, and exits 1. Verification: recorded in the CORE-377
  resolution and in `tagged-pdf.spec.md`'s validation log.
- AC-3: `evaluate` contract: each failure kind, expected vs unexpected,
  `stale-known-failure`, `snapshot-missing`/`snapshot-stale`, the coverage
  tolerance, and `render` short-circuiting.
  Test: `tests/test_tagged_gate.py`.

## Edge Cases

- A fixture whose tagged render crashes reports only `render` (the PDF cannot
  be read back).
- Coverage can rise when a fix improves reading order. That passes, and
  `--update` records the new floor.
- Known failures are keyed by `(doc, check)`. One entry covers one check on
  one fixture; a ticket that fixes several fixtures has one entry per fixture.

## References

- `docs/specifications/tagged-pdf.spec.md`, the behavior these checks guard.
- `scripts/verify_tagged_pdf.py`, the read-back measurement (CORE-373).
- CORE-377 (this gate), CORE-386 (`.notdef` glyphs), CORE-396 (empty
  marked-content sequences).
