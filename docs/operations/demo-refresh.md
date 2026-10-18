---
title: Demo refresh and rendering triage
type: runbook
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 9
tags: [library, showcase, triage, regression, process]
trigger: When you re-render the document library to find rendering defects after engine landings
---

# Demo refresh and rendering triage

A refresh measures the engine against the document library, finds what renders wrong, and recommends issues. It does not commit renders. Rendered images live in the public `typeanvil-showcase` repo (see `docs/operations/showcase.md`). CORE-495 removed them from this repo.

## The flow

1. **Read the nightly report.** `.github/workflows/nightly.yml` runs every library document and the visual QA checks on the newest `release/*` tip (`docs/operations/nightly.md`). Its job summary lists changed documents, visual QA failures, and the slowest documents. A red run opens or updates the `nightly-failure` issue, and the `nightly-evidence` artifact holds the logs and changed-page PNGs. The fingerprint baselines in `baselines/` (see `docs/operations/regression-gate.md`) carry the machine-checked state. A pull request that moves a rendered page already shows before and after images in its description (`docs/operations/pr-visual-diff.md`).
2. **Triage.** Take each new or changed signal. Reduce it to a minimal repro of 10 to 20 lines of HTML. Render it with TypeAnvil and at least one oracle (Prince, WeasyPrint, or Chromium) at the document's geometry. A finding is confirmed only when at least one oracle disagrees with TypeAnvil. Read the governing spec first: a spec-correct behavior is not a defect (`docs/conventions/css-standards-alignment.md`).
3. **Recommend issues.** Search Linear first. For each confirmed defect, write a recommended issue with the repro file, the measured numbers from at least two engines, and the expected CSS behavior (`docs/conventions/issue-evidence.md`). Post the triage table as a comment on the refresh issue. Create the recommended issues only after the owner approves.

## Local run

Run the builds when you need local renders for triage. All output goes to the ignored `library/out/` tree.

```bash
PY=~/workspace/typeanvil/.venv/bin/python ./scripts/build-demo.sh --validate
PY=~/workspace/typeanvil/.venv/bin/python ./scripts/build-showcase.sh
```

The demo scripts need a host-native binary (`cargo build --manifest-path engine/Cargo.toml`), not the container binary. For the release-profile benchmark, see `docs/operations/benchmark-engines.md`.

## Rules

- Do not commit anything under `library/out/`. Do not run `git add -f` on it. A refresh produces a Linear comment and, when a document's expectation changed, an edit to `library/manifest.json`.
- Do not commit rendered PNGs anywhere in the repo. The CI visual diff and the showcase publish workflow cover that need.
- Record a page-count move that is intended in the manifest entry (`expected_change`), with the reason.
- Never file anything on an external tracker.
