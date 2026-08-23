---
title: Licensing Resolution and Watermark Seam
slug: /specifications/licensing-resolution
type: spec
status: draft
owner: elijah
created: 2026-08-21
updated: 2026-08-21
sidebar_position: 24
tags: [engine, licensing, cli, pdf, distribution]
spec_id: licensing-resolution
issue_id: CORE-115
applies_to: engine 0.x
dependencies: []
---

# Licensing Resolution and Watermark Seam

## Overview

Typeanvil ships as a commercial closed-source binary. Customers activate a
self-hosted install by providing a license file; unlicensed installs still
render but stamp a visible watermark on every page (Prince's model — see the
research brief `docs/research/licensing/licensing-and-distribution.md`).

This spec covers **iteration one only**: the license *resolution* paths, a
mocked verifier (no cryptographic verification yet), and the watermark seam
in the PDF emitter. Real Ed25519-signed license files are a later issue;
this issue exists so the call-site shape, lookup order, error semantics,
and watermark rendering land early and get exercised by tests before any
commercial release depends on them.

Two properties make this low-risk now:

- The engine is never compiled from source by customers, so
  `#[cfg(debug_assertions)]` gives developers a fully licensed build while
  no customer ever holds a debug build.
- The watermark lives at PDF-emission level (`pdf.rs`), drawn after page
  content on every page, where author CSS cannot suppress or cover it.

## Goals / Non-Goals

**Goals**

- One resolution function with a fixed lookup order:
  `--license <path>` CLI arg → `TYPEANVIL_LICENSE` env var →
  `license.dat` adjacent to the executable → not found.
- Iteration one mocks verification: resolution always yields a valid
  license regardless of what it finds. The types, call site, and error
  path exist and are tested; only the body is fake.
- Debug builds (`#[cfg(debug_assertions)]`) bypass resolution entirely
  and are treated as licensed.
- Unlicensed release builds render normally except for a watermark line
  on every page ("Unlicensed — TypeAnvil").
- A *malformed* license file (unparsable) fails loudly with a nonzero
  exit code. A *missing* license does not fail.
- Determinism preserved: identical input + identical license state →
  byte-identical output.
- The harness CLI contract stays stable; `--license` is additive.

**Non-Goals** (deferred)

- Cryptographic license verification (Ed25519 signature, embedded public
  key), expiry checks, edition/seat fields — later issue.
- Phone-home activation of any kind — explicitly rejected for broad use;
  possibly opt-in enterprise-only much later.
- Private registry distribution gating — separate layer, separate decision.
- Watermark styling beyond a single fixed line (position, tiling,
  diagonal stamps).
- A license management subcommand (`typeanvil license activate ...`).

## Behavior

The engine shall:

1. **Resolve the license through one function.** `licensing::resolve()`
   inspects sources in this exact order and stops at the first hit:
   1. `--license <path>` named argument on the `render` subcommand.
   2. `TYPEANVIL_LICENSE` environment variable holding a filesystem path.
   3. `license.dat` in the same directory as the running executable
      (resolved from `std::env::current_exe()`, never the working
      directory).
   4. Nothing found → `LicenseState::Missing`.
2. **Mock verification in iteration one.** `resolve()` shall return a
   hardcoded valid license (`LicenseState::Valid(MockLicense)`) for any
   found source, and `Valid(MockLicense)` for `Missing` as well — the
   mock treats every install as licensed. The enum shapes the real
   branches so swapping in signature verification touches only this
   function's body.
3. **Bypass in debug builds.** When compiled with `debug_assertions`,
   resolution short-circuits to licensed without touching the
   filesystem or environment. Release builds enforce.
4. **Watermark unlicensed output.** When resolution yields
   `LicenseState::Missing` in a release build, rendering proceeds
   normally and the PDF emitter draws the text `Unlicensed — TypeAnvil`
   once per page, after page content, at a fixed position near the page
   bottom-right margin corner. The watermark is paint-level, not CSS:
   author stylesheets cannot remove, recolor, or reposition it.
5. **Fail loudly on malformed licenses.** If a source was found but its
   contents cannot be read or parsed into a license structure, the CLI
   prints `typeanvil: error: <reason>` and exits with a failure code.
   Missing vs broken are distinct outcomes: missing → watermarked render,
   broken → hard error.
6. **Keep determinism.** The watermark string, face, size, and position
   are constants. No timestamp, no randomness. Identical input and
   identical license state produce byte-identical PDFs.
7. **Preserve the existing CLI contract.** All current flags keep their
   meaning. `--license <path>` is new and optional. The Python harness
   adapter (`harness/engine.py`) passes no license flag today and must
   continue working unchanged.

## Interfaces

**New module** `engine/src/licensing.rs`:

```text
pub enum LicenseState {
    Valid(License),
    Missing,
}

pub struct License {
    pub customer: String,        // mock: "development"
    pub edition: Edition,        // mock: Edition::Trial
}

pub enum Edition { Trial, Standard, Enterprise }  // fields reserved; unused in v1

pub fn resolve(cli_path: Option<&Path>) -> Result<LicenseState, LicenseError>
pub enum LicenseError {
    Unreadable { path: PathBuf, reason: String },
    Malformed { path: PathBuf, reason: String },
}
```

`resolve()` takes the already-parsed `--license` value rather than reading
global argv, so tests drive it directly.

**`engine/src/main.rs`** — `RenderArgs` gains `license: Option<PathBuf>`;
the render entry calls `resolve()` and threads the result into
`pdf::render` via a small options struct (additive parameter; existing
callers updated in the same PR).

**`engine/src/pdf.rs`** — the emitter accepts a watermark flag. When set,
after painting each page's fragment tree it draws the constant watermark
line using the regular Arial face already embedded (no new font source).

**Environment**: `TYPEANVIL_LICENSE` — absolute or relative path to a
license file; relative resolves against the process CWD (documented
deviation from rule 1.3's exe-relative default, matching how users type
paths).

## Acceptance Criteria

All live in a new `engine/tests/licensing.rs` unless noted.

1. **Lookup order — arg beats env beats adjacent.** Given all three
   sources present, `resolve(Some(arg))` reports the arg's source;
   given env + adjacent file, it reports the env source; given only an
   adjacent file, it reports the adjacent source. (Sources are
   distinguishable via the returned provenance field on `LicenseState`
   in test builds.)
2. **Adjacent file uses the executable directory.** With CWD set to a
   temp dir containing `license.dat` and the executable elsewhere, no
   adjacent-file hit occurs. (Test runs resolution against an injected
   exe-path parameter; the production wrapper passes `current_exe()`.)
3. **Debug build bypass.** A test compiled under `debug_assertions`
   asserts resolution returns licensed without any source present.
4. **Missing license renders watermarked.** Render a minimal document
   with forced-missing license state; extract each page's text layer
   (`pypdfium2`) and assert the watermark string appears exactly once
   per page.
5. **Licensed output is clean.** Same document, valid license: no
   watermark string anywhere in the text layer.
6. **Malformed license fails loudly.** A found-but-garbage file yields
   `LicenseError::Malformed`; the CLI exits nonzero printing
   `typeanvil: error:`.
7. **Determinism with watermark.** Two watermarked renders of the same
   input produce byte-identical files.
8. **Harness contract unchanged.** Existing engine acceptance suites
   pass without modification (they exercise debug-build bypass).

## Edge Cases

- `--license` pointing at a directory → `Unreadable`, hard error.
- `TYPEANVIL_LICENSE` set to an empty string → treated as unset.
- Executable path unavailable (`current_exe()` errors) → skip the
  adjacent-file probe, fall through to `Missing`.
- License file containing valid UTF-8 garbage (parses as text, not a
  license) → `Malformed`, hard error — never silently watermarked,
  because the user believed they were licensed.
- Future signed-license swap must keep these semantics: the enum and
  error shapes here are the stable surface.

## References

- Research brief: `docs/research/licensing/licensing-and-distribution.md`
  (scheme rationale, Prince precedent, dependency-license constraints).
- CLI contract: module doc comment in `engine/src/main.rs`; mirrored by
  `harness/engine.py`.
