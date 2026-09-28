---
title: Issue Evidence and Diagnosis
type: convention
status: approved
owner: maintainers
created: 2026-09-15
updated: 2026-09-27
sidebar_position: 4
tags: [conventions, evidence, triage, testing]
---

# Issue evidence and diagnosis

Every implementation decision shall use current evidence. A report can describe a valid failure while its proposed cause is wrong. An unknown cause is acceptable during investigation.

Use the [issue evidence review runbook](../operations/issue-evidence-review.md) for the procedure. The documentation validator checks document structure; it does not establish that a diagnosis is correct.

## Evidence fields

Keep these fields separate:

| Field | Required content |
| --- | --- |
| Observed behavior | Exact source and fixture revisions, command, expected result, actual result, and evidence location. |
| Expected behavior | Governing specification section and a plain-language statement of correct behavior. Record unresolved interpretation. |
| Suspected cause | A hypothesis and an experiment that could confirm or reject it. Use `unknown` when unsupported. |
| Confirmed cause | An isolating probe or failing test that supports the causal explanation. State the tested boundary and remaining uncertainty. |
| Acceptance criteria | Observable correct behavior mapped to checks. Keep implementation mechanisms provisional until supported. |

A source-code match, browser disagreement, or summary alone does not confirm a cause. A symptom reproduction proves the failure; an isolating comparison or trace is needed to confirm the cause. Mark another contributor's results as reported until independently checked.

For rendered output, record renderer and rasterizer versions, fonts, DPI, page geometry, print preferences, WPT revision, and exact test and reference IDs. One status can hide a defect shared by both documents rendered through the same engine.

## Gate 1: before implementation

1. Read the complete issue record and all available comments. Follow supersession links to the canonical capability.
2. Inspect later landings and existing branches or worktrees. Preserve live work.
3. Build the current branch point and run the smallest relevant reproduction. Record source and fixture revisions and runner settings. A cached binary is insufficient evidence for a new source revision.
4. Check that the fixture exercises the suspected property and code path. Inspect both sides of a reftest and measure the behavior directly.
5. Compare the result with the governing specification. Correct a stale premise before coding. Resolve contradictory normative requirements before implementing them.
6. Record the result, evidence provenance, cause confidence, and next bounded step.

A documentation-only task checks current document and source evidence. It does not require an engine build when no engine behavior is involved. If reproduction is blocked, record the exact blocker. Do not invent a diagnosis or successful gate.

## Gate 2: before creating a follow-up

1. Remeasure the residual at the landed source revision. Label candidate measurements clearly.
2. Check later work and existing ownership. Confirm that the problem remains.
3. Keep a same-capability boundary case on the canonical capability record.
4. For a separate defect, record the evidence fields and obtain a scope review before transferring ownership.
5. Link related records and read back the result in the tracking system used by the project.

An urgent report can be recorded before local reproduction. Mark it unverified, preserve the supplied input, and make reproduction the next task.

## Correcting a diagnosis

Update the active description and acceptance criteria when evidence changes. Preserve the superseded explanation in marked history with the reason for correction. Preserve original reproductions, useful experiments, and landed regression tests.

Keep raw logs and rendered artifacts in a durable location that another contributor can access. A temporary directory inside a removed container is insufficient. Keep credentials and private document data out of public artifacts.

## Compact template

```markdown
## Observed behavior
- Source and fixture revisions:
- Reproduction command and runner settings:
- Actual result:
- Evidence location and provenance:

## Expected behavior
- Specification section:
- Expected result and unresolved questions:

## Diagnosis
- Status: unknown / suspected / confirmed
- Suspected cause:
- Isolating evidence:
- Rejected hypotheses and limits:

## Acceptance criteria
- Observable behavior and mapped checks:
- Existing canaries and full change gate:

## Ownership and review
- Capability owner and related record:
- Evidence reviewer and result:
```

## Review and completion

A reviewer shall check evidence provenance, active-description corrections, and scope ownership before accepting implementation or a follow-up. Record which acceptance checks ran. A targeted probe supports pickup; it does not replace the full baseline/candidate gate before landing.

Use the [release gate](../operations/release-gate.md) for direct page, geometry, text, orientation, paint, and link checks when pair equality cannot establish correctness. Apply [CSS standards alignment](css-standards-alignment.md) to expected behavior.

## References

- [Development guidelines](development-guidelines.md)
- [Issue evidence review](../operations/issue-evidence-review.md)
- [Documentation conventions](doc-conventions.md)
- [CSS standards alignment](css-standards-alignment.md)
