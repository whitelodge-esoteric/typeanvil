---
title: Issue Evidence and Diagnosis
type: convention
status: approved
owner: elijah
created: 2026-09-15
updated: 2026-09-15
sidebar_position: 4
tags: [conventions, linear, workflow, evidence, testing]
issue_id: CORE-205
---

# Issue evidence and diagnosis

Every implementation decision shall use current evidence. A ticket can report a
valid failure while its proposed cause is wrong. An unknown cause is acceptable
for investigation.

This convention applies when creating, picking up, correcting, or splitting a
TypeAnvil issue. Work is tracked in the **TypeAnvil Core** Linear project, team
**Core**. The project ID is `2949a51f-360e-43a7-a8a2-53295a47b213`.

Use the [issue evidence review runbook](../operations/issue-evidence-review.md)
for commands and review steps. This is a required human/agent review. The docs
validator checks document structure; it cannot establish that a diagnosis is
correct.

## Why this check exists

[CORE-180](https://linear.app/whitelodge/issue/CORE-180) inherited an older
2,136-pixel residual diagnosis. Later work on
[CORE-174](https://linear.app/whitelodge/issue/CORE-174) recorded a 50-pixel
residual and showed that the proposed whitespace trim would restore an error.
Those are historical measurements. A fresh reproduction is required before
acting on the remaining difference.

[CORE-184](https://linear.app/whitelodge/issue/CORE-184) also had an unmerged
implementation and corrected measurements in comments that its description
did not fully represent. Reading the description alone missed useful work.

## Evidence fields

Issue authors shall keep these fields separate:

| Field | Required content |
| --- | --- |
| Observed behavior | What failed, exact source commit, fixture or minimal input, command, expected versus actual result, and evidence location. |
| Expected behavior | Governing specification section and a plain-language statement of correct behavior. Record unresolved interpretation explicitly. |
| Suspected cause | A hypothesis and the experiment that could confirm or reject it. Use `unknown` when no cause is supported. |
| Confirmed cause | An isolating probe or failing test that supports the causal explanation. State the tested boundary and remaining uncertainty. |
| Acceptance criteria | Observable correct behavior and its mapped checks. Keep proposed implementation mechanisms provisional until supported. |

A source-code match, browser disagreement, or agent summary alone does not
confirm a cause. A test that only reproduces the symptom proves the failure;
confirming the cause also requires an isolating comparison or trace. Record
another session's reported results as reported until independently checked.

For rendered output, record the renderer and rasterizer versions, fonts, DPI,
page geometry, browser page-preference flags, WPT revision, and actual test IDs
where relevant. Identify both test and reference inputs. One status can hide a
defect shared by both documents rendered through the same engine.

## Gate 1: before implementation

The implementing agent and lead shall:

1. Read the full issue description and all comment pages. Follow supersession
   links to the canonical capability ticket.
2. Inspect later landings and existing branches/worktrees. Preserve unmerged
   work and avoid another session's live files.
3. Build the current branch-point engine and rerun the smallest relevant
   reproduction before implementation. Record the exact source and fixture
   revisions and runner settings. A shared cached binary needs a fresh build
   for that source revision.
4. Check that the named fixture exercises the suspected property and code
   path. Inspect both sides of a reftest and measure the behavior directly.
5. Compare the result with the active ticket and governing specification. If
   the premise fails, correct the active description before coding. If the
   specification is contradictory or its expected behavior is disputed, get
   the relevant decision before implementing that behavior.
6. Record the gate result in the ticket: evidence links, current cause status,
   and the next bounded step. A reproduced failure with an unknown cause can
   proceed as investigation. Exploratory code remains an experiment until its
   behavior and causal claims pass verification.

A documentation-only task uses current document/source evidence. It does not
require an engine build or render when no engine behavior is involved. If
reproduction is blocked, record the exact blocker and missing evidence. Do not
invent a failure, diagnosis, or successful gate.

## Gate 2: before creating a follow-up

A residual is a failure that remains after a change. Before creating another
implementation ticket, the author and lead shall:

1. Remeasure the residual at the landed commit. Keep an unlanded candidate's
   measurements explicitly labeled as candidate evidence.
2. Read later comments, landings, and existing open issues. Confirm the problem
   remains and identify any existing owner.
3. Decide whether the case belongs to the current capability. Keep a related
   boundary case as a test or checklist item there. A small commit does not
   complete a broader capability.
4. For a genuinely separate defect, record its evidence fields and obtain a
   lead review of the scope transfer. The lead is the accountable project
   owner or parent agent reviewing the worker's evidence. A direct session
   records its own review explicitly; it does not claim independent review.
5. Set the new issue's project, team, owner or ownership decision, appropriate
   parent/relations, and initial Backlog state explicitly. Link both directions
   and read back the target before reporting success.

An urgent or externally reported failure can be recorded before a local
reproduction. Label its evidence as unverified, preserve the supplied input,
and make reproduction the next task. Missing evidence is never a reason to
invent certainty or hide a report.

## Correcting a diagnosis

Update the active description and relevant acceptance criteria when evidence
changes. Put the superseded explanation in clearly labeled history or a
comment with the reason for correction. A correction stored only in comments
leaves the executable instructions stale.

If the governing specification contains conflicting normative statements,
resolve them in the same approved spec change. Appending a new rule while the
opposite rule remains active is incomplete. A worker shall raise unresolved
behavior or scope conflicts to the lead; an existing user-approval requirement
still applies.

Preserve original reproductions, useful experiments, and landed regression
tests. For consolidation, create the replacement's scope-transfer table before
canceling sources as superseded. Use Done only for verified completed work.
Administrative closure does not authorize deletion of unmerged commits, live
worktrees, or evidence still needed by the replacement.

## Compact issue template

Use these headings in the active issue description. Replace placeholders with
measured values or an explicit statement of what is unknown.

```markdown
## Observed behavior
- Source commit and branch:
- Fixture/WPT revision and actual test/reference IDs:
- Reproduction command and runner settings:
- Actual result:
- Evidence location and provenance (measured here / reported):

## Expected behavior
- Specification section:
- Expected result; unresolved questions:

## Diagnosis
- Status: unknown / suspected / confirmed
- Suspected cause:
- Isolating evidence for a confirmed cause:
- Rejected hypotheses and remaining limits:

## Acceptance criteria
- Observable behavior and mapped direct checks:
- Existing canaries and full change gate:

## Ownership and review
- Capability owner, related issue, and any scope transfer:
- Model used:
- Gate 1 evidence/reviewer/result:
- Gate 2 evidence/reviewer/result, if a follow-up:
```

Store commands, minimal inputs, and essential results in the issue or tracked
repository material. Keep raw logs/PDFs in a durable attachment or retained
artifact path that another session can access. A temporary directory inside a
removed container is insufficient. Keep credentials and private document data
out of public artifacts.

## Review and completion

A lead shall check evidence provenance, active-description corrections, and
scope ownership before accepting implementation or follow-up creation. Record
which acceptance checks actually ran. A fresh targeted probe is sufficient for
pickup; it does not replace the full baseline/candidate gate before landing.

For engine work, compare identical test-ID sets with compatible runner settings
and freshly built baseline/candidate binaries. Add direct page-count, geometry,
text-orientation, or paint assertions for behavior that pair equality cannot
establish. Apply [CSS standards alignment](css-standards-alignment.md) and the
user's existing approval rules for any exposed gap or scope change.

## References

- [Development guidelines](development-guidelines.md)
- [Issue evidence review](../operations/issue-evidence-review.md)
- [Documentation conventions](doc-conventions.md)
- [CSS standards alignment](css-standards-alignment.md)
- [CORE-205](https://linear.app/whitelodge/issue/CORE-205) — repository adoption
