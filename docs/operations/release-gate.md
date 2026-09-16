---
title: Run the Release Gate
slug: /operations/release-gate
type: runbook
status: approved
owner: elijah
created: 2026-09-15
updated: 2026-09-16
sidebar_position: 8
tags: [harness, wpt, release, gate, testing]
trigger: Before landing an engine change, and before promoting the release branch
issue_id: CORE-206
---

# Run the release gate

## When to run

Run the gate in two situations:

1. **Before landing an engine change** that affects layout, pagination, or
   typography. Use the engine-change tier.
2. **Before promoting the release branch to `main`.** Pass the engine-change
   tier, then record the full-selection delta as a residual (see
   [Promotion](#promotion)).

The gate exists because the WPT pair comparison is self-consistent. When a change
moves a test and its reference the same way, the pair still matches and the score
stays flat while the output changed. The gate compares each document with its own
earlier output and asserts expected PDF properties directly.

The gate does not replace the WPT score. Both run.

## Promotion

A promotion passes the **engine-change tier** and records the full-selection
delta as a residual. It does not require a disposition for every changed
document: a long-lived release branch changes most of the corpus, and approving
that wholesale would defeat the gate.

1. Capture and gate the engine-change tier (direct checks plus
   `--filter css-page/margin-boxes`, 37 tests) for the baseline and the
   candidate. It must print `GATE PASSED`.
2. Capture the full selection for both sides and run the gate with `--reviews`
   pointing at an empty directory. Every changed document is then reported as
   `unreviewed_change`: that list IS the delta. Keep the report.
3. Record the delta in
   `docs/research/wpt-harness/promotion-delta-<date>.md`: the WPT status
   movement per test, the changed-document counts, the regressions, and the
   follow-up issues they need.
4. Reference the record from the promotion commit and the Linear issue.

Record dispositions only for what a reviewer actually examined, and state the
evidence in the reason. A family-level approval must say that it is family-level
rather than imply a per-document diff.

## Prerequisites

- An isolated worktree at the branch point, with the shared WPT checkout linked
  (`ln -sfn ~/workspace/typeanvil/.wpt <worktree>/.wpt`).
- The dev container (see [dev container](dev-container.md)). Build and test inside
  it, never on the host.
- Two freshly built binaries:
  - **baseline** — the current release tip, or `main` if you compare against it;
  - **candidate** — your branch.
- The repo venv python for the harness itself:
  `~/workspace/typeanvil/.venv/bin/python`.
- The spec: [harness release gate](../specifications/harness-release-gate.spec.md).

## Steps

### 1. Build both binaries

```bash
# baseline
git -C <baseline-worktree> rev-parse HEAD          # record this
scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml -j 4

# candidate
git -C <candidate-worktree> rev-parse HEAD         # record this
scripts/dev-container.sh cargo build --manifest-path /work/engine/Cargo.toml -j 4
```

### 2. Capture the baseline

A capture is an explicit operation. It cannot report a gate pass.

Captures run inside the container, from the worktree whose binary you are
capturing. `--wpt` is a top-level flag and goes before the subcommand.

```bash
cd <baseline-worktree>          # the release tip, or main
SHA=$(git rev-parse HEAD)       # the engine source this binary was built from
scripts/dev-container.sh bash -c \
  "python3 -m harness --wpt /main/.wpt baseline \
   --engine cli --cli-cmd '/work/engine/target/debug/typeanvil render' \
   --label baseline-$SHA --out /work/gate/captures --source-commit $SHA"
```

`--source-commit` is required here. A linked worktree's `.git` is a pointer file
to a host path, so inside the container `git rev-parse HEAD` cannot resolve and
the capture would record `source_commit: unknown`. Read the commit on the host,
where the worktree is real, and pass it in.

Inside the container the current worktree is `/work` and the shared WPT checkout
is `/main/.wpt` (read-only). Output under `/work` is persistent; `/tmp` is not.

The command prints `BASELINE CAPTURED — not a gate result`. A capture with any
render failure is incomplete and exits nonzero.

### 3. Capture the candidate

Repeat step 2 with the candidate binary and `--label candidate-<commit>`.

### 4. Run the gate

The gate reads captures only; it does not render, so it runs on the host or in
the container. Run it from the candidate worktree with the host venv python.

```bash
~/workspace/typeanvil/.venv/bin/python -m harness gate \
  --baseline gate/captures/baseline-<commit>.json \
  --candidate gate/captures/candidate-<commit>.json \
  --direct harness/direct_manifest.json \
  --reviews gate/reviews --policy gate/policy.json \
  --out gate/out
```

Copy the baseline capture into the candidate worktree's `gate/captures/` when the
two sides were captured from different worktrees.

The command prints both identities, the compatibility verdict, every blocking
condition, every changed document, and every direct-check result. It exits 0 only
when all layers pass.

### 5. Review the changes

Read `<out>/<candidate-label>-gate.json`, then the side-by-side images under
`<out>/changes/` (baseline on the left, candidate on the right, from the
thumbnails stored in each capture). For each changed document, record one disposition in
`gate/reviews/<candidate-label>.json`:

| Kind | Use when |
|---|---|
| `correction` | the new output is the correct one, with a specification citation |
| `variation` | the output differs for a justified rendering reason; `provenance` is required |
| `regression` | the change is unwanted; the gate keeps failing |

A disposition binds to the exact baseline and candidate fingerprints. Change
either side and the old record no longer applies, so the gate blocks again. Never
widen a tolerance or edit expected values to make the gate pass.

### 6. Record the evidence

Put the commands, both source and binary identities, the report, the review
record, the measured runtime, and the artifact size on the Linear issue. Follow
[issue evidence](../conventions/issue-evidence.md).

### 7. Prove the gate on a fault (when the gate itself changes)

A gate that has never caught a real fault is unproven. Keep the fault out of the
shipped engine: patch the engine in the worktree, build, capture, then restore
the source with `git checkout -- <file>` before you build anything else.

```bash
git diff > /tmp/fault.patch          # the deliberate fault, kept as evidence
git checkout -- engine/src/<file>.rs # restore immediately after capturing
```

Record the fault patch, both binary hashes, and the gate verdict on the issue.
The capture from a faulted binary is evidence about the gate. It is not an
engine conformance result.

## Verification

The gate passed:

- exit status 0, and the output ends with `GATE PASSED`;
- no blocking conditions;
- every direct check reports a measured value and passes;
- the WPT status counts equal the baseline counts for the same test ids.

For a promotion this is the engine-change tier. The full-selection delta is a
separate artifact (see [Promotion](#promotion)) and is not a pass.

A gate pass writes no new state. The captured JSON and the review record are
retained as evidence.

## Rollback

The gate changes nothing outside its output directory. To discard a run, delete
`gate/out/`. To abandon a capture, delete `gate/captures/<label>.json` and
recapture. Nothing in the engine or the WPT checkout is modified.

## Troubleshooting

- **`unknown_identity_unacknowledged` for `fonts`** — the engine reports no font
  inventory, so font identity is `unknown` on both sides. List `fonts` under
  `acknowledged_unknown` in the policy file to accept that limit explicitly. Do
  not record a guessed font hash.
- **`incompatible_environment`** — the two captures differ in page geometry,
  DPI, engine kind, WPT revision, or rasterizer. Rebuild and recapture both sides
  from the same WPT revision with the same flags.
- **`incomplete_capture`** — a document failed to render. Read the capture's
  `message` for that document, fix the cause, and recapture. Do not gate on a
  partial capture.
- **`coverage_mismatch`** — the two selections differ. Use the same `--filter`
  and `--limit` on both sides.
- **`unsupported_schema`** — the capture predates the current schema, or the file
  is not a capture. The condition carries the loader's reason. Recapture; the gate
  does not infer missing metadata.
- **`source_commit: unknown` in a capture** — the capture ran where git metadata
  was unavailable (the dev container). Recapture with `--source-commit` set to the
  host's `git rev-parse HEAD` for the tree that built the binary.
- **`unreviewed_change`** — a document changed and no disposition covers it. Read
  the change artifact and record a disposition. A stale record shows up as
  `stale_review`.
- **A direct check fails on the current branch** — the fixture is wrong or the
  engine has a real gap. Confirm which one with a targeted probe before touching
  the manifest. A real gap follows the issue-evidence rules: record it, keep the
  check, and fix the engine or move the check to the capability ticket that owns
  the behavior. Do not delete the check to make the gate pass.
- **`--wpt` is a GLOBAL flag, for `gate` as well as for captures** — when the
  worktree's `.wpt` symlink points at an absolute host path, every direct check
  whose `input_source` is `wpt` fails to render inside the container and reports
  a `CalledProcessError`. Pass `--wpt /main/.wpt` before the subcommand and all
  thirteen checks run.
- **`--reviews` takes a directory or a file.** The documented value is the
  `gate/reviews` directory holding `<candidate-label>.json`; an empty directory
  means nothing is reviewed, so every changed document blocks.
- **A capture runs long** — captures render every document in the selection,
  including references. Reuse a capture whose identity still matches instead of
  recapturing, and use the engine-change tier for rapid iteration.
