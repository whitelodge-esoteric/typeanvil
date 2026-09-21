---
title: Issue Pickup Workflow Optimization
type: research
status: approved
owner: elijah
created: 2026-09-21
updated: 2026-09-21
sidebar_position: 1
tags: [workflow, evidence, tooling, performance]
---

# Issue pickup workflow optimization

## Question

How can issue pickup reach a useful regression test sooner while preserving
current reproduction, causal evidence, CSS correctness, and landing verification?

## Method

The audit inspected the current evidence convention, review runbook, Docker
wrapper, harness capture/direct-check paths, and original tool records from
CORE-180, CORE-210, and CORE-235. Repository source was inspected at release
commit `b2e9bf88c5ae23ac0cf60847ee137032c9db3fce`. Main was at
`3ed9f429b7b0dfa611044a4879b5c9f90af7537a`.

This was a workflow and source audit. It ran no fresh engine reproduction or
performance benchmark. Historical rendering results remain attributed to their
original sessions. Timestamp differences were calculated from original messages;
they include tool orchestration and waiting. They do not measure model reasoning
time. No general speedup is claimed.

## Findings

### Avoidable setup delays are directly observed

| Session | Operation | Observed elapsed time |
| --- | --- | --- |
| CORE-210 | Synchronous semantic indexing and status | 203.84 seconds |
| CORE-210 | Wait for a container binary at a host path | 420.14 seconds |
| CORE-210 | Cargo compilation, reported by Cargo separately | 46.80 seconds |
| CORE-180 | Wait for a container binary at a host path | 203.48 seconds |

CORE-210's first two calls were non-overlapping and span 10.40 minutes. The
host-path wait could never establish container build readiness. The Docker
wrapper mounts a named target volume at `/work/engine/target`. The corresponding
host directory may be empty after a successful build. Process age at a later
status query is also distinct from compilation duration.

CORE-180 created its worktree from main and then recreated it at the release
commit. CORE-210 also omitted the explicit release start point. An implicit
branch point adds revalidation risk and can omit recent fixes.

The delegated CORE-235 review identified retries caused by host/container path
confusion, outputs lost in container `/tmp`, and guessed probe interfaces. It
also found a document described as missing after looking in main even though
it existed on the release branch. These are historical observations; this audit
did not replay the engine experiments.

### The evidence checks remain useful

The CORE-210 session's fragment and size traces redirected its proposed fix from
image drawing to sizing. The CORE-235 session needed a specification scope decision
and a comparison separating missing content from content covered by border paint.
Removing these checks would risk implementing an incorrect explanation.

The evidence convention already permits a fresh targeted probe at pickup and
reserves the full change gate for landing. An unknown cause is valid investigation
work. Exploratory instrumentation can proceed as a labeled experiment. The process
does not require a complete subsystem explanation before every useful experiment.

### Reuse has correctness limits

The wrapper reuses the target volume associated with a worktree. Its shared Cargo
registry/git caches avoid repeated downloads; they do not populate a new
worktree's compiled target. A fresh `cargo build` can be incremental in a compatible
warm target. The actual saving from a proposed warm-baseline workflow is unmeasured.

Ordinary run reports and history lack the capture format's complete source/binary
identity fields. A source SHA supplied by the caller does not independently prove
that a binary was built from it. Capture compatibility deliberately permits
different source and binary identities, and currently records fonts as unknown.
A document hash alone also omits external asset dependencies. A safe build or
render cache needs a stronger identity than a compatible gate verdict.

### There is secondary tooling overhead

Every `scripts/dev-container.sh` invocation starts ownership-repair containers
and recursively changes ownership across the cache and target volumes. Its cost
was not benchmarked.

`harness/capture.py` renders unique documents and then calls the pair runner,
which renders test/reference inputs again. `harness/direct.py` renders per check,
even when several checks name one input. Run-local reuse is a plausible
optimization, subject to unchanged WPT and direct-check semantics.

## Corrections already applied

The executing profile's workflow instructions were corrected to:

1. Set In Progress when investigation begins and read back the state.
2. Create worktrees from an explicit release commit and verify the resulting HEAD.
3. Use container execution paths and collect the actual background process result.
4. Preserve output under the mounted worktree instead of container `/tmp`.
5. Initialize semantic indexing only when semantic code discovery is needed.
6. Reuse one fresh reproduction across pickup and premise review while source,
   fixtures, settings, and build provenance remain unchanged.
7. Give further research a specific unresolved question. Proceed to a bounded
   fix when failure, expected behavior, and causal evidence support it.
8. Reuse documented probe interfaces and convert isolating probes to meaningful
   regression assertions where possible.
9. Replace the project guide's duplicate pickup procedure with a single workflow
   reference.

The exact profile-specific changes are archived outside `docs/` in
`.hermes/skill-patches/`. Committing that archive does not install it into other
profiles. Broader historical instruction cleanup is still pending.

## Recommended work

### 1. Provide a tested pickup command

Build a small entry point around the existing container wrapper and harness.
Write its specification before implementation. The command should:

- Resolve the intended release ref and exact SHA explicitly. Check source state
  and ownership before creating or reusing a worktree. Fail clearly on ambiguity.
- Invoke a fresh build early. Read its real exit status before rendering. Avoid
  pipelines that conceal build failures and avoid polling host binary paths.
- Run the smallest requested reproduction. Support a concrete HTML input and a
  WPT selection with exact expected IDs. Reject empty or unexpectedly broad
  selections. Verify test and reference coverage.
- Write separate build/run logs and statuses, persistent PDFs, a measured result,
  and a compact evidence record to a non-overwriting directory.
- Record source SHA and dirty-state/patch identity as applicable, fixture/WPT and
  asset identities, binary hash, container/toolchain, fonts, page settings,
  rasterizer, DPI, commands, and timestamps. Mark unknown fields explicitly.
- Distinguish expected test failure, successful reproduction, build failure,
  crash, missing dependency, and empty selection. Never treat a completed batch
  as an implementation or release-gate pass.
- Skip engine execution for documentation-only tasks. Provide a documented
  status/progress path suitable for non-interactive use.

Keep causal judgment and specification decisions with the agent. Reuse the
existing capture/direct-check formats where suitable and document their limits.

### 2. Retain a reusable diagnosis record

Keep the exact input, command, expected measurement and specification citation,
actual measurement, test/reference mapping, rejected explanations, cause
confidence, next unresolved question, and durable evidence location together.
Update the active issue description when evidence changes. Archive artifacts
before worktree cleanup.

At pickup, read the full issue and comments, inspect later work, build and rerun
the smallest probe. Reuse the experiment design and established context. Changed
inputs, uncertain provenance, or changed relevant source invalidate reuse.
Historical captures can inform investigation but cannot replace the mandatory
fresh pickup build and reproduction.

### 3. Use bounded diagnosis and reusable probes

Start with the cheapest measurement that distinguishes hypotheses: page count
for pagination, geometry for sizing/position, a targeted paint region for paint,
and text extraction only where the PDF provides a usable text layer. Inspect
both sides of reftests. Distinguish geometry loss from overpainting.

Maintain tested interfaces for HTML rendering, existing-PDF inspection, and
fragment traces. Read usage before invoking a helper. After an interface error,
inspect the API before retrying. Convert a useful isolating probe into an invariant
assertion before the fix where possible. An unconditional panic is diagnostic
output and does not prove a meaningful RED test.

Batch independent issue/document reads and overlap bounded reading with the
build. Serialize heavy Docker builds, suites, and gates. Delegate independent
bounded evidence collection only when it saves work; the parent still verifies
causal claims. A time checkpoint should expose an unresolved question and next
experiment, without forcing an unsupported fix when time expires.

### 4. Pilot a warm isolated baseline

Use an exclusively available baseline worktree for the initial fresh build and
probe, then create the implementation tree at that recorded SHA. Coordinate
ownership through the entire build/probe interval and check source identity
before and after. Never reset or edit another session's live tree.

Keep the fresh build invocation. Rebuild and remeasure when the selected branch
point or relevant environment changes. Never share a writable Cargo target
between concurrent worktrees. Optional target seeding would need separate
compatible, quiescent copies, a fresh build afterward, and measured evidence;
it is an experiment rather than a required implementation mechanism.

### 5. Reduce instruction size and contradictions

Keep one current pickup procedure, concise task-specific navigation, and topic
references for historical engine knowledge. Preserve lessons and mark superseded
advice. Reconcile remaining main-versus-release, host-versus-container, gate,
cleanup, and validator instructions against the current repository contract.
Measure loaded context size and failed tool calls before and after. File size
alone does not establish latency or quality gains.

### 6. Measure and reduce container startup cost

Measure startup and ownership-repair cost separately from compilation and
rendering. If material, use a tested initialization/version check with an explicit
repair path, or batch build and minimal reproduction in one guarded invocation.
Retain separate logs and statuses. Preserve non-root access, memory limits,
permissions on fresh and existing volumes, and safe interrupted-run recovery.

### 7. Consider run-local PDF reuse

Share captured PDFs/rasters between compatible measurements within a run and
between multiple direct checks of one input. Preserve match/mismatch, fuzzy,
page-selection, multiple-reference, reference-chain, and failure behavior.
Test invalidation when inputs, assets, binary, or rendering settings change.
Do not reuse outputs across incompatible environments or silently approve
changed expectations. Measure render counts, runtime, and artifact size before
claiming a gain. Keep this lower priority than reliable pickup execution.

### 8. Ensure future sessions discover the improvements

Keep tooling and detailed docs on the active release branch. Require the entry
instructions to locate that release copy even when the initial workspace is on
main. Test discovery from a main workspace, the release worktree, and a newly
created issue worktree. Existing worktrees do not automatically acquire new
commits. Installed profile skills require explicit updates; a Git push alone
does not change them. Do not update other profiles or promote main implicitly.

## Verification plan

- Exercise a cold target, a warm target, wrong branch selection, busy/dirty
  worktree, failed build, missing dependency, interrupted run, empty/extra WPT
  selection, successful reproduction, and expected failing reproduction.
- Prove artifacts survive container exit and can be retained before cleanup.
- Run existing harness semantics tests and the appropriate change gate for code
  changes. Run the docs validator for documentation changes.
- Recheck current source before implementation. This audit's commit is historical.
- For the next five pickups, record pickup-to-first-reproduction,
  setup/tool-error waiting, reproduction-to-isolating-evidence, approval waiting,
  and time to a meaningful regression assertion. Record total completion time,
  review/repair work, model, issue type, and cache state too.
- Compare similar work and publish raw phase measurements. Keep claims about
  total improvement provisional until measured. Retain the evidence safeguards.

## Implications and scope

Prioritize a reliable pickup command, reusable evidence, and concise instructions.
Warm-baseline, startup, and render-cache work should follow measurement. These
changes optimize execution of the CORE-205 evidence contract and reuse CORE-206
harness capabilities. CORE-212 continues to own demo pipeline engine/Python
selection; avoid duplicating its delivery scope.

This work does not authorize engine behavior changes, weaker evidence or landing
gates, automatic expectation updates, other-profile edits, live-worktree cleanup,
upstream issue filings, or main promotion.

## Sources

- [Issue evidence convention](../../conventions/issue-evidence.md)
- [Issue evidence review runbook](../../operations/issue-evidence-review.md)
- [Dev container runbook](../../operations/dev-container.md)
- [Release gate runbook](../../operations/release-gate.md)
- [Harness release-gate specification](../../specifications/harness-release-gate.spec.md)
- `scripts/dev-container.sh`: target isolation and per-invocation volume repair.
- `harness/capture.py`: document capture, pair evaluation, and identity fields.
- `harness/direct.py`: per-check rendering.
- `harness/runner.py`, `harness/report.py`, `harness/release_gate.py`: comparison
  semantics, ordinary report metadata, and compatibility checks.
- [CORE-205](https://linear.app/whitelodge/issue/CORE-205), full description and comments.
- [CORE-206](https://linear.app/whitelodge/issue/CORE-206), full description and all comments,
  including later promotion, command correction, and residual ownership records.
- [CORE-212](https://linear.app/whitelodge/issue/CORE-212), full description and comments.
- Historical workflow timing evidence: original CORE-210 tool calls 23668/23739,
  23924/23994, and result 24006; CORE-180 calls 22927/22993. Raw session records
  remain local. Only the relevant derived measurements are published here.
