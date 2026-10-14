---
title: Run the Tagged PDF Gate
slug: /operations/tagged-gate
type: runbook
status: approved
owner: maintainers
created: 2026-10-13
updated: 2026-10-13
sidebar_position: 9
tags: [harness, tagging, accessibility, gate, testing]
trigger: Before landing an engine change, and before promoting the release branch
---

# Run the tagged PDF gate

The tagged gate renders every demo corpus and showcase fixture with
`--tagged` and `--tagged --ua`, reads each structure tree back, and fails on
tagging regressions that krilla's own validator can't see. The full contract
is in the [tagged gate spec](../specifications/tagged-gate.spec.md).

## When to run

Run it as part of the [release gate](release-gate.md), in the same two
situations:

1. **Before landing any engine change.** Painting, layout, fonts and the DOM
   all feed the structure tree, so this isn't limited to tagging changes.
2. **Before promoting the release branch to `main`.**

It takes a few minutes for 42 fixtures × 3 renders.

## Prerequisites

- A built engine binary from the branch under test, built in the dev container
  ([dev container](dev-container.md)). The container image already has
  `pypdfium2`; the host venv works too, and results are identical
  (checked 2026-10-13, CORE-377).

## Steps

### 1. Run the gate

```bash
scripts/dev-container.sh python3 /work/scripts/tagged_gate.py \
  --bin /work/engine/target/debug/typeanvil --report /work/probe/tagged-gate.json
```

It must print `TAGGED GATE PASSED` and exit 0.

### 2. Read a failure

Each failure line names a fixture, a check, and the detail:

| Check | Means |
|---|---|
| `render` | the tagged render crashed or exited non-zero |
| `deterministic` | two tagged renders of the same input differ |
| `untagged` | page objects drawn outside any marked content (PDF/UA-1 §7.1) |
| `duplicated` | one marked-content id referenced twice in the tree |
| `dangling` | the tree references a marked-content id no page object carries |
| `ua` | krilla's PDF/UA-1 validator rejected the render |
| `roles` | the structure-type counts changed versus the snapshot |
| `coverage` | the tags cover noticeably less of the source text than the snapshot (drop > 0.005) |
| `stale-known-failure` | a listed known failure now passes: remove its entry |
| `snapshot-missing` / `snapshot-stale` | a fixture was added or removed: run `--update` |

A `roles` or `coverage` failure is a regression unless your change was meant
to alter roles or reading order. The detail shows the before → after counts
per role.

### 3. Accept an intended role change

When the change is meant to alter the tree (for example, a CORE-374 role
fix), refresh the snapshot and commit it with the change:

```bash
scripts/dev-container.sh python3 /work/scripts/tagged_gate.py \
  --bin /work/engine/target/debug/typeanvil --update
git diff harness/tagged-gate/snapshot.json   # review: only the intended roles moved
```

Name the moved fixtures and roles in the commit message. `--update` never
edits the known-failures file.

### 4. Retire a known failure

`harness/tagged-gate/known-failures.json` lists each expected failure as
`{doc, check, ticket, reason}`. When a fix makes one pass, the gate fails
with `stale-known-failure` until you delete that entry in the same commit as
the fix. Add an entry only with a ticket that owns the fix.

## Verification

- `TAGGED GATE PASSED`, exit 0, on the branch tip.
- `python3 -m pytest tests/test_tagged_gate.py -q` is green (host venv).

## Troubleshooting

- `error: no engine binary` (exit 2): build first; inside the container the
  binary is `/work/engine/target/debug/typeanvil`.
- Every fixture fails `roles` after a rebase: the snapshot predates a landed
  role change. Rebuild at the new tip, re-run, and `--update` only if the
  release tip itself fails (that means a landing skipped the gate).
