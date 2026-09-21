---
title: Review Issue Evidence
type: runbook
status: approved
owner: elijah
created: 2026-09-15
updated: 2026-09-21
sidebar_position: 7
tags: [linear, evidence, triage, workflow, testing]
trigger: Before implementation or creation of a residual issue
issue_id: CORE-205
---

# Review issue evidence

## When to run

Run this checklist before implementing an issue or creating a follow-up for a
remaining failure. The [issue evidence convention](../conventions/issue-evidence.md)
defines the required fields and decisions. A documentation-only task checks
its document/source premise and skips engine reproduction.

## Prerequisites

- Access to the canonical Linear issue, all comments, and referenced evidence.
- An isolated worktree at the intended branch point. Check live work before
  creating or reusing one.
- The governing specification and a minimal input or actual WPT test ID.
- For engine work, the [dev container](dev-container.md), shared WPT checkout,
  and required fonts. Use its setup procedure if the image is absent.

## Steps

### 1. Read the whole record

Fetch the issue through Linear. Fetch all comment pages until pagination is
complete. Follow a supersession link to its replacement. Read later related
landings and comments before copying a diagnosis into a prompt.

Record current status, owner, relevant branch, and any unmerged candidate. A
branch commit and a reported gate are useful evidence; they do not establish
that the code reached the release branch.

### 2. Check source and work ownership

Run these from the repository or intended worktree:

```bash
git fetch origin
git status -sb
git worktree list
git log -12 --oneline
git rev-parse HEAD
git rev-parse origin/release/2026.9
```

Use the actual active release ref if it differs. Inspect `git status -sb` and
recent commits inside relevant worktrees. Do not reset, rebase, or edit another
session's tree. Compare historical branch commits with release landings before
reusing them; squash landings can have different commit IDs.

### 3. Capture a targeted engine reproduction

From your isolated worktree root, run the following Bash commands. Set the real
issue identifier and a filter matching the actual test ID. An input path in
`/work` refers to this worktree; `/main` holds the shared WPT checkout.

The [pickup command](../specifications/issue-pickup.spec.md) (`scripts/pickup.py`)
wraps the build + reproduction + evidence recording below into one call. It
resolves the release ref, checks worktree ownership and dirty state, builds in
the container, runs the smallest reproduction, and writes a non-overwriting
evidence directory with the manifest, logs, statuses, and PDFs. Use it when the
issue names an exact WPT test ID or a concrete HTML input:

```bash
python3 scripts/pickup.py \
  --worktree "$PWD" --issue CORE-180 \
  --wpt-id css-page/margin-boxes/content-003-print.html \
  --out probe/issue-evidence
```

The manual sequence below records the same evidence when you need finer control
over the build or the harness invocation.

```bash
ISSUE=CORE-180
FILTER=css-page/margin-boxes/content-003-print.html
COMMIT=$(git rev-parse HEAD)
mkdir -p "probe/issue-evidence/$ISSUE"
EVIDENCE=$(mktemp -d "probe/issue-evidence/$ISSUE/$COMMIT.XXXXXX")

# Fresh build for this worktree revision. Keep the unfiltered build log.
BUILD_STATUS=0
if scripts/dev-container.sh cargo build \
  --manifest-path /work/engine/Cargo.toml -j 2 \
  > "$EVIDENCE/build.log" 2>&1; then
  printf '%s\n' "$BUILD_STATUS" > "$EVIDENCE/build.exit"
  RUN_STATUS=0
  scripts/dev-container.sh python3 -m harness --wpt /main/.wpt run \
    --engine cli --cli-cmd '/work/engine/target/debug/typeanvil render' \
    --filter "$FILTER" --workers 1 \
    --report "/work/$EVIDENCE/before.json" \
    --db "/work/$EVIDENCE/history.sqlite" \
    --artifacts "/work/$EVIDENCE/artifacts" \
    > "$EVIDENCE/run.log" 2>&1 || RUN_STATUS=$?
  printf '%s\n' "$RUN_STATUS" > "$EVIDENCE/run.exit"
else
  BUILD_STATUS=$?
  printf '%s\n' "$BUILD_STATUS" > "$EVIDENCE/build.exit"
  printf 'Build failed; inspect %s/build.log before continuing.\n' "$EVIDENCE"
fi
```

The issue ID and filter are an example, not a universal test. Record the exact
command you ran. Check the report's actual IDs and result count: a substring
filter can select no tests or extra tests. Resolve references through the WPT
manifest; some tests share a reference with another numbered test.

Record the WPT revision with `git -C .wpt rev-parse HEAD` when that checkout is
available from the worktree. Otherwise run the command at the shared WPT path.
Record fonts, rasterizer/renderer versions, DPI, page geometry and page-size
preference flags. Verify the actual PDF page sizes when comparing browsers.
The source commit alone does not identify those inputs.

A failing reproduction can return a nonzero exit code. Read the report and log
to distinguish an expected failing test from a crash, missing dependency, or
empty selection. Do not report a test result after a failed build.

### 4. Test the explanation

Inspect the fixture and both rendered documents. Check whether the suspected
property is present and whether its value reaches the relevant code path.
Create an isolating probe or failing regression test that distinguishes the
suspected cause from plausible alternatives.

Use direct PDF coordinates, page counts, line direction, or pixel regions when
a reftest can change both sides together. Match browser version and page
settings before interpreting disagreement. CSS specifications determine the
expected behavior; a browser result is comparison evidence.

Classify the cause as unknown, suspected, or confirmed in the active issue.
Record a confirmed cause's isolating evidence and its limits. If the old
premise is wrong, correct the description before implementation.

### 5. Review a residual at the landed commit

After a change lands, rerun its remaining reproduction at the landed source
revision. Use a fresh evidence directory for that revision. Check new comments,
other landings, and open owners before proposing a separate issue.

The lead reviews the measured residual and records one outcome:

| Outcome | Action |
| --- | --- |
| Same capability, another boundary case | Keep the test/checklist item on the canonical ticket. |
| Separate reproduced defect | Review the evidence fields and transfer scope with reciprocal links. |
| Report exists, reproduction blocked | Record unverified evidence and the exact blocker; schedule investigation. |
| Failure no longer reproduces | Record the current result and correct or close with an accurate disposition. |
| Wrong expected result or disputed standard | Record the conflict and obtain the required decision before implementation. |

Use the Linear MCP write tools. Set project/team and intended state explicitly.
Read back the exact target after writing. Do not use Done for administrative
consolidation or create duplicate work from an old parent summary.

### 6. Preserve the record

Copy the compact evidence template into the current description. Keep essential
commands/results and the minimal reproduction accessible to the next session.
Link durable raw artifacts and label results reported by another session.

| Material | Keep until |
| --- | --- |
| Current description, acceptance criteria, specification citation | The capability is completed or explicitly superseded. |
| Minimal input and regression test | Retain as the behavior's test evidence. |
| Raw reports, PDF probes, logs | Reviewed and transferred to a durable location before deleting their worktree. |
| Unmerged candidate and live worktree | Work owner approves reuse, landing, or disposal. |
| Superseded diagnosis | Retain in marked history or comments with the correction reason. |

For evidence only stored inside a container's `/tmp`, copy it to the mounted
worktree before the container exits. Do not include credentials or private user
documents in public evidence. Use the [dev-container cleanup rules](dev-container.md)
only after useful work and evidence have been retained. Superseding a ticket
does not by itself make its artifacts disposable.

## Verification

Before moving from investigation to implementation, the lead checks:

- Current source and fixture revisions are recorded.
- The selected reproduction exists and its result matches the active issue.
- Observations, expected behavior, and cause confidence are separate.
- Later comments and unmerged work were considered.
- Contradictory instructions are corrected or explicitly awaiting a decision.
- The next step and capability owner are explicit.

Before creating a follow-up, add the landed-commit residual measurement and
lead scope-review result. Then verify project, state, and links by reading the
written Linear issue back.

This checklist is a manual review requirement. `python3 scripts/validate_docs.py`
checks documentation structure. It does not inspect Linear evidence or prove a
root cause. The targeted reproduction does not replace the full change gate.

## Rollback

If a published diagnosis is wrong, correct the active description and preserve
the original in marked history. If a duplicate follow-up was created, first
transfer any unique evidence and obligations to its canonical owner, then use
an accurate duplicate or superseded disposition. Read back the result.

Do not revert source changes or delete another session's work as a side effect
of correcting a ticket.

## Troubleshooting

- **Source advanced:** rebuild at the new branch point and record a new result;
  retain the previous result with its original commit.
- **Build or environment failure:** record the actual failure and stop the
  implementation gate until the reproduction can run.
- **No tests selected:** check the actual manifest ID and filter; an empty run
  does not confirm a fix.
- **Pair matches but output looks wrong:** run the
  [release gate](release-gate.md). It compares each document with its own
  earlier output and adds direct page, geometry, text, orientation, paint, and
  link checks independent of test-versus-reference equality.
- **Comments contradict the description:** identify the supporting evidence,
  update active instructions, and preserve the correction reason.
- **Cause unknown:** keep the issue in investigation; use the next isolating
  experiment as its immediate task.
