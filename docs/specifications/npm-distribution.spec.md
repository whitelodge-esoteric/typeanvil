---
title: npm distribution — typeanvil package for Node and Bun
type: spec
status: draft
owner: Elijah Boston
created: 2026-09-04
updated: 2026-09-16
slug: /specifications/npm-distribution
sidebar_position: 46
tags: [npm, distribution, node, bun, core-136, release]
spec_id: SPEC-CORE-136-npm-distribution
issue_id: CORE-136
applies_to:
  - npm/
  - .github/workflows/release.yml
dependencies:
  - SPEC-CORE-135-release-ci
---

# npm distribution: `typeanvil` (Node + Bun) (CORE-136)

Parent epic: CORE-133 (installable runtime). Blocked by CORE-135 (release
artifacts), which is Done. One npm package covers both runtimes: Bun installs
npm packages natively, so Node ≥ 18 and Bun both consume the same artifact.

## Goals

1. `npm install typeanvil` works on Node ≥ 18 and Bun, on the 5 release
   targets from CORE-135.
2. The package is a thin wrapper: it resolves the native binary and spawns it.
   No engine logic in JavaScript.
3. Publishing is wired into the existing tag-triggered release workflow —
   no separate release path.

## Non-Goals

- No Homebrew tap (CORE-137).
- No macOS signing/notarization (CORE-138).
- No engine logic, patching, or bundling in JavaScript.
- Publishing does not gate on the repo being public; the npm package itself is
  public-facing regardless.

## Decision: optionalDependencies platform packages (esbuild pattern)

The package uses the esbuild/Biome layout: a JS wrapper package plus one
platform package per release target, each shipping its own binary.

- `typeanvil` — wrapper: resolves + spawns the binary, exposes the CLI
  (`bin`) and a programmatic `render()`.
- `@typeanvil/darwin-arm64`, `@typeanvil/darwin-x64`,
  `@typeanvil/linux-x64-gnu`, `@typeanvil/linux-x64-musl`,
  `@typeanvil/linux-arm64-gnu` — each ships only its platform binary.

npm picks the matching platform package via its `os`/`cpu`/`libc` fields. A
missing match surfaces a clear "unsupported platform" error from the wrapper
(the `--omit=optional` install case included).

DECIDED 2026-09-04. Alternatives rejected: postinstall download from GitHub
Releases (fails behind corporate proxies, needs a second artifact store,
breaks `npm ci` reproducibility); WASM build (engine is native-first, no WASM
target exists). esbuild's layout is the proven pattern for exactly this
problem.

## Package layout

```
npm/
  typeanvil/            — wrapper package (bin + programmatic render())
    package.json
    bin.js              — CLI entry (chmod +x)
    index.js            — render() programmatic API
    resolve.js          — binary resolution + error surfacing
    README.md
  typeanvil-darwin-arm64/    — @typeanvil/darwin-arm64
  typeanvil-darwin-x64/      — @typeanvil/darwin-x64
  typeanvil-linux-x64-gnu/   — @typeanvil/linux-x64-gnu
  typeanvil-linux-x64-musl/  — @typeanvil/linux-x64-musl
  typeanvil-linux-arm64-gnu/ — @typeanvil/linux-arm64-gnu
```

Each platform package holds `bin/typeanvil` (the raw binary, executable bit
set) and a `package.json` with:

- `os`/`cpu` matching the Rust target (musl adds `libc: ["musl"]`).
- `version` matching the git tag.
- `repository`/`license` fields (AGPL-3.0-only).

Platform dirs are named `typeanvil-<platform>` so a directory maps 1:1 to its
package name.

## Wrapper resolution order

1. `process.env.TYPEANVIL_BIN` if set (override for dev/CI).
2. `require.resolve('@typeanvil/<platform>/bin/typeanvil')` for the current
   platform (os × arch × libc).
3. Fallback: scan `typeanvil`'s own `node_modules` for any installed
   `@typeanvil/*` platform package (covers hoisted/symlinked installs such as
   Bun's global store).
4. No binary found → print an "unsupported platform / omitted optional deps"
   error naming the platform and exit non-zero.

## Behavior

1. `npx typeanvil render in.html -o out.pdf` shall pass all arguments through
   to the native binary and exit with its exit code (spawn with `stdio:
   inherit`, forward the exit code, handle `SIGINT`/`SIGTERM` by killing the
   child).
2. Programmatic API: `import { render } from 'typeanvil'` shall spawn the
   binary with the given arguments and resolve to the PDF as a `Uint8Array`;
   non-zero exit rejects with the binary's stderr attached. The CLI
   passthrough is the must-have; `render()` is the nice-to-have.
3. Package `version` shall match the git tag / GitHub Release version —
   single source of truth. Automation rewrites `version` in every
   `npm/**/package.json` from the tag at publish time; no hand-edited
   versions.
4. The wrapper shall error clearly (non-zero exit, human-readable message)
   when no platform binary is present — including the `npm install
   --omit=optional` case.
5. Every package.json shall set `license: "AGPL-3.0-only"`, `repository`, and
   `homepage` pointing at the GitHub repo.
6. Platform packages shall declare `os`, `cpu` (and musl `libc`) so npm skips
   non-matching platforms at install time rather than at first run.
7. The release workflow shall skip the npm publish steps and report a notice
   when the `NPM_TOKEN` secret is absent, and shall publish only when the
   secret is present. A tag without the secret still produces GitHub Release
   binaries and a green workflow run.

## Publishing flow (release workflow)

The tag-triggered release workflow (CORE-135) gains an `npm-publish` job
after `release`:

1. Download the built binaries (same artifacts the release job attaches).
2. Stamp every `npm/**/package.json` `version` (and each platform package's
   optionalDependency entry) from the tag.
3. Copy each binary into its platform package dir as `bin/typeanvil`.
4. `npm publish` each platform package, then the wrapper (wrapper last: its
   optionalDependencies must resolve at publish time).
5. The wrapper package's version is stamped identically to the tag.

`NODE_AUTH_TOKEN` comes from a repository secret (`NPM_TOKEN`). The secret is
optional: when it is absent the job publishes nothing and reports a notice, so
the release stays green and the npm packages stay unpublished until the token
is added. Nothing else in the workflow depends on it.

## Private-repo note

Platform packages ship the binary INSIDE the npm tarball, so `npm install`
never needs GitHub artifacts. npm publishing itself works from a private
repo; only the `repository` URL resolution is affected until the repo flips
public. Local dev + `npm pack` dry-runs proceed before that.

## Acceptance Criteria

- AC1: `npm pack` dry-runs succeed for the wrapper and all 5 platform
  packages with correctly staged binaries.
- AC2: From the packed wrapper, `npx typeanvil --version` prints the version
  on both Node ≥ 18 and Bun.
- AC3: `render()` resolves to a PDF `Uint8Array` for a minimal HTML input on
  both runtimes.
- AC4: Installing with `--omit=optional` (no binary present) produces the
  clear unsupported-platform error, not a stack trace.
- AC5: `npm publish --dry-run` for each package reports the expected files
  and no secret material.
- AC6: Release workflow's `npm-publish` job stamps versions from the tag and
  publishes wrapper + 5 platform packages (verified by workflow review on a
  test tag when the repo/npm org is ready).

## Done when

- `npm/` packages exist and pack cleanly (AC1–AC5 verified locally).
- Release workflow extended with the `npm-publish` job (AC6).
- Spec landed same-PR.
