---
title: Review Issue Evidence
type: runbook
status: approved
owner: maintainers
created: 2026-09-15
updated: 2026-09-27
sidebar_position: 7
tags: [evidence, triage, workflow, testing]
trigger: Before implementation or creation of a residual issue
---

# Review issue evidence

## When to run

Run this checklist before implementing an issue or creating a follow-up for a remaining failure. The [issue evidence convention](../conventions/issue-evidence.md) defines the required fields and decisions. A documentation-only task checks its document and source premise and skips engine reproduction.

## Prerequisites

- Access to the complete project record, comments, and referenced evidence.
- An isolated worktree at the intended branch point. Check live work before creating or reusing one.
- The governing specification and an exact minimal input or WPT test ID.
- For engine work, the [dev container](dev-container.md), WPT checkout, and required fonts.

## Steps

### 1. Read the whole record

Read the complete issue or change record and all available comments. Follow supersession links. Read later related landings before copying a diagnosis. Record current status, relevant branch, and any unmerged candidate. A branch commit and reported result do not establish that code reached the target branch.

### 2. Check source and work ownership

Run these from the repository or intended worktree:

```bash
git status -sb
git worktree list
git log -12 --oneline
git rev-parse HEAD
git rev-parse origin/<target-branch>
```

Fetch the target ref first when it may have moved. Do not reset, rebase, or edit another contributor's tree. Compare historical commits with landed changes before reusing their evidence.

### 3. Capture a targeted engine reproduction

Use an exact test ID. The repository helper wraps build, reproduction, and evidence recording:

```bash
python3 scripts/pickup.py \
  --worktree "$PWD" \
  --issue ISSUE-ID \
  --wpt-id css-page/margin-boxes/content-003-print.html \
  --out probe/issue-evidence
```

Replace `ISSUE-ID` with the project's record identifier. This placeholder is required by the helper's CLI; it is not a public issue reference. The WPT ID is an example and must match the manifest.

For manual harness work, use the current interface:

```bash
COMMIT=$(git rev-parse HEAD)
mkdir -p probe/issue-evidence
scripts/dev-container.sh cargo build \
  --manifest-path /work/engine/Cargo.toml -j 2
scripts/dev-container.sh python3 -m harness --wpt /main/.wpt run \
  --engine cli \
  --cli-cmd '/work/engine/target/debug/typeanvil render' \
  --filter 'css-page/margin-boxes/content-003-print.html' \
  --workers 1 \
  --report /work/probe/issue-evidence/report.json \
  --db /work/probe/issue-evidence/history.sqlite
printf 'source=%s\n' "$COMMIT" > probe/issue-evidence/manifest.txt
```

Record the exact command. Check the report's IDs and count because a substring filter can select zero or multiple tests. Record the fixture revision, fonts, renderer and rasterizer versions, DPI, page geometry, and print preference flags. Verify PDF page sizes when comparing browsers.

A failing reproduction can return nonzero. Read the report and log to distinguish an expected failure from a crash, missing dependency, or empty selection. Do not report a result after a failed build.

### 4. Test the explanation

Inspect the fixture and both rendered documents. Check whether the suspected property is present and reaches the relevant code path. Create an isolating probe or regression test that distinguishes the cause from plausible alternatives.

Use direct PDF coordinates, page counts, line direction, or pixel regions when a reftest can change both sides together. CSS specifications determine expected behavior; browser output is comparison evidence.

Classify the cause as unknown, suspected, or confirmed. Record confirmed evidence and its limits. Correct the active record when the original premise is wrong.

### 5. Review a residual at the landed commit

After a change lands, rerun the reproduction at the landed source revision in a fresh evidence directory. Check new comments, other landings, and open ownership before proposing a separate record.

| Outcome | Action |
| --- | --- |
| Same capability, another boundary case | Keep the case on the canonical record. |
| Separate reproduced defect | Transfer scope with reciprocal links and evidence review. |
| Reproduction blocked | Record the blocker and mark the evidence unverified. |
| Failure no longer reproduces | Record the current result and correct or close the record. |
| Disputed expected result | Record the specification conflict and resolve it before implementation. |

### 6. Preserve the record

Keep the compact evidence fields, essential commands, minimal input, and durable results accessible to the next contributor. Copy artifacts out of a container before it exits. Do not include credentials or private document data. Apply [dev-container cleanup rules](dev-container.md) only after useful work and evidence are retained.

## Verification

Before implementation, confirm:

- Source and fixture revisions are recorded.
- The selected reproduction exists and its result matches the active record.
- Observations, expected behavior, and cause confidence are separate.
- Later work and live contributors were considered.
- Conflicts are corrected or explicitly awaiting a decision.
- The next bounded step and capability owner are clear.

Before creating a follow-up, add the landed-commit measurement and scope-review result. The documentation validator checks structure; it does not inspect project records or prove a root cause. The targeted reproduction does not replace the full change gate.

## Rollback

If a diagnosis is wrong, correct the active record and preserve the original in marked history. Do not revert source changes or delete another contributor's work as a side effect.

## Troubleshooting

- **Source advanced:** rebuild at the new branch point and record a new result.
- **Build or environment failure:** record the actual failure and stop the implementation gate until reproduction can run.
- **No tests selected:** check the exact manifest ID and filter.
- **Pair matches but output looks wrong:** run the [release gate](release-gate.md) for direct checks.
- **Cause unknown:** keep the record in investigation and use the next isolating experiment as the task.
