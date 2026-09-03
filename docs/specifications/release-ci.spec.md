---
title: Release CI — tag to cross-platform binaries to GitHub Releases
type: spec
status: in-review
owner: Elijah Boston
created: 2026-09-03
updated: 2026-09-03
slug: /specifications/release-ci
sidebar_position: 45
tags: [release, ci, distribution, core-135, calver]
spec_id: SPEC-CORE-135-release-ci
issue_id: CORE-135
applies_to:
  - .github/workflows/release.yml
dependencies:
  - SPEC-CORE-134-cli-surface
---

# Release CI: tag → cross-platform binaries → GitHub Releases (CORE-135)

Parent epic: CORE-133 (installable runtime). The npm package (CORE-136) and the
Homebrew formula (CORE-137) both download release artifacts produced by this
pipeline, so this ticket gates both.

## Goals

1. A push of a tag `v*` builds release binaries for the target matrix and
   attaches them to a GitHub Release.
2. Artifacts carry the version from the git tag. The binary's `--version`
   output matches the tag (verified in CI).
3. SHA256SUMS are attached for the Homebrew formula to consume.

## Non-Goals

- No npm publish or Homebrew tap update in this workflow (CORE-136/137).
- No macOS signing or notarization (CORE-138 decides that).
- No `latest` moving tag management beyond what GitHub Releases provides.

## Decision: hand-rolled matrix (not cargo-dist)

DECIDED 2026-09-03: hand-rolled build matrix. cargo-dist would scaffold the
npm/Homebrew publish steps, but it dictates the artifact layout, wraps the
build in its own release branch flow, and adds a config surface we would have
to learn to debug. The matrix is 5 targets and simple; transparency wins while
the project is private and the release cadence is manual. Revisit if the
matrix grows past ~8 targets or per-target quirks multiply.

## Versioning: CalVer

Releases use calendar versioning `YYYY.M.PATCH` (e.g. `2026.9.0`), decided on
CORE-133. Tags are `vYYYY.M.PATCH` (e.g. `v2026.9.0`).

Correction (2026-09-03, found during CI bring-up): the epic's original
"zero-padded month" idea is wrong. node-semver rejects leading zeros, so
`2026.09.0` would be invalid on npm; the plain month `2026.9.0` is the valid
form and matches the epic's own examples.

The workflow stamps the version from the tag at build time:

- `engine/Cargo.toml` `[package].version` is rewritten from the tag before
  `cargo build` (never hand-edited).
- After build, the binary's `--version` output must equal
  `typeanvil <version-from-tag>`. Mismatch fails the target.

## Build matrix

| Target | Runner | Notes |
|---|---|---|
| `aarch64-apple-darwin` | macos-14+ | Apple Silicon; primary dev platform |
| `x86_64-apple-darwin` | macos-latest | Intel macs |
| `x86_64-unknown-linux-gnu` | ubuntu-22.04 | glibc baseline pinned to 22.04 |
| `x86_64-unknown-linux-musl` | ubuntu-22.04 | static binary for thin servers/containers |
| `aarch64-unknown-linux-gnu` | ubuntu-22.04 | cross-compiled via cargo-zigbuild |

## Behavior

1. The workflow shall trigger on push of a tag matching `v*`.
2. The workflow shall derive the version by stripping the leading `v` from the
   tag (e.g. tag `v2026.9.0` → version `2026.9.0`), and shall fail fast if the
   remainder is not a valid CalVer (`YYYY.0M.PATCH`).
3. The workflow shall stamp `engine/Cargo.toml` `[package].version` with the
   version from the tag before building, and shall not require any
   hand-edited version anywhere in the pipeline.
4. Each target shall build in release mode (`cargo build --release`) with the
   repository's `[profile.release]` settings: `lto = "thin"`, no fast-math or
   reassociation flags. The workflow shall not override these settings.
5. Each target shall attach a `.tar.gz` named
   `typeanvil-<version>-<target>.tar.gz` containing the `typeanvil` binary and
   a short README.
6. The workflow shall generate and attach a `SHA256SUMS` file covering all
   attached `.tar.gz` artifacts.
7. After building, each target shall verify the binary reports
   `typeanvil <version>` on `--version`; a mismatch shall fail that target.
8. A failed target shall not prevent other targets from building or
   attaching (per-target jobs with `fail-fast: false`).
9. The release shall be created (or reused) with the generated artifacts and
   SHA256SUMS attached.

## Acceptance Criteria

- AC1: Pushing a test tag `v*` runs the workflow green on all 5 targets.
- AC2: The GitHub Release lists 5 `.tar.gz` artifacts plus `SHA256SUMS`.
- AC3: Each artifact's binary prints the tag version on `--version`.
- AC4: A deliberately wrong version stamp would fail the target (verified by
  the check's logic review; not exercised on a real tag).

## Done when

- Workflow builds all 5 targets on a test tag; artifacts + SHA256SUMS attached
  to the release.
- `--version` output matches tag (CI check green).
- Decision (cargo-dist vs hand-rolled) recorded with a DECIDED marker.
- Determinism constraints documented in the workflow comments.
