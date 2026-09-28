---
title: Container distribution — docker run CLI, user fonts, open-source fallback
type: spec
status: draft
owner: maintainers
created: 2026-09-27
updated: 2026-09-27
slug: /specifications/container-distribution
sidebar_position: 47
tags: [container, distribution, docker, fonts, release]
spec_id: container-distribution
applies_to:
  - engine/src/fonts.rs
  - .github/workflows/release.yml
  - docker/
dependencies:
  - release-ci
  - cli-surface
  - font-resolution
---

# Container distribution: `docker run` CLI, user fonts, open-source fallback

The container reuses the release artifacts. It wraps the existing Linux binary;
it adds no new compile matrix.

## Overview

Ship the engine as a container image so users render without installing a
binary:

```bash
docker run --rm -v "$PWD:/work" -v "$HOME/.fonts:/usr/share/fonts/user:ro" \
  ghcr.io/typeanvil/typeanvil render /work/in.html -o /work/out.pdf
```

The image bundles an open-source font set so fallback text renders with valid
font bytes on Linux. Users mount their own fonts; the engine discovers them
through fontdb and gives them precedence over the bundled set.

## Goals

1. A `docker run` invocation renders an HTML file to a PDF with the same CLI
   contract and exit codes as the native binary.
2. A user-supplied font directory mounted at a documented path takes precedence
   over the bundled set for family resolution.
3. With no user fonts, the engine falls back to a bundled open-source set on
   Linux — never empty font bytes.
4. The image's `--version` prints `typeanvil <tag>`.
5. The container build wraps the release binary unchanged (no RUSTFLAGS, no
   rebuild semantics change).

## Non-Goals

- No Homebrew tap or macOS signing.
- No engine logic in the image beyond the shipped binary.
- No network font loading; `@font-face` `https://` sources fail resolution
  deterministically (unchanged from `font-resolution`).
- No change to macOS fallback behavior. The bundled Arial set stays the macOS
  fallback; the open-source set is the Linux fallback.
- No `latest` moving-tag management beyond what GHCR provides.

## Decision: bundled font set

DECIDED 2026-09-27: bundle **Liberation Sans** (regular, bold, italic, bold
italic) as the Linux fallback set.

- Liberation is metric-compatible with Arial, so documents that resolve to the
  bundled Arial set on macOS render with near-identical metrics on Linux. This
  keeps the GENERIC GATE re-baseline minimal: the four generic families and the
  Arial/Helvetica name path resolve to faces whose metrics match the macOS
  Arial bundle.
- Liberation is licensed under the SIL Open Font License 1.1, which permits
  redistribution in a container image. The license text ships in the image.
- DejaVu and Noto were rejected: DejaVu is not metric-compatible with Arial
  (wider metrics re-baseline every unstyled doc), and Noto Sans is a larger
  download with no metric-compatibility benefit for this use.

The bundled set lives at a known path in the image, e.g.
`/usr/share/fonts/typeanvil/`. The engine reads it as a fallback when the
macOS Arial paths are absent (Linux).

## Decision: user-font mount contract

DECIDED 2026-09-27: the documented mount path is `/usr/share/fonts/user`
(read-only). fontdb's `load_system_fonts()` scans the standard fontconfig
directories on Linux, which include `/usr/share/fonts`; a mount into
`/usr/share/fonts/user` is discovered automatically with no engine change.

The explicit `--font-dir` flag was considered and rejected for this issue:
fontdb already discovers mounted fonts with zero engine surface, and the mount
path is a stable, documented contract. A `--font-dir` flag remains a possible
follow-up if a user needs a non-standard path.

## Decision: resolution order

DECIDED 2026-09-27: user fonts → bundled open-source set → bundled Arial
fallback (unchanged on macOS).

- User fonts (system faces discovered by fontdb) resolve first for named
  families and for the generic families once the GENERIC GATE is re-pointed.
- The bundled open-source set is the Linux fallback for the generic families
  and for the Arial/Helvetica name path.
- The bundled Arial set remains the macOS fallback, unchanged.

## Behavior

1. The container image shall run `typeanvil render` with the same CLI contract
   as the native binary, exit codes included.
2. A user-supplied font directory mounted at `/usr/share/fonts/user` shall take
   precedence over the bundled set for family resolution.
3. With no user fonts, the engine shall fall back to the bundled open-source
   set on Linux (never empty font bytes).
4. The image's `--version` shall print `typeanvil <tag>`.
5. The container build shall wrap the release binary unchanged (no RUSTFLAGS,
   no rebuild semantics change).
6. The image shall run as a non-root user and contain no shell in the final
   layer.
7. The image shall be multi-arch (amd64 + arm64) via buildx and pushed to GHCR
   tagged with the CalVer tag.

## Interfaces

### Engine: bundled fallback paths

`engine/src/fonts.rs` `BUNDLED_PATHS` (lines 48–53) hardcodes the macOS Arial
paths. On Linux those paths do not exist, so `face_bytes()` (line 444) reads
empty bytes for the fallback. The engine shall resolve the bundled fallback
set from a platform-appropriate path:

- On macOS: the existing `/System/Library/Fonts/Supplemental/Arial*.ttf` paths
  (unchanged).
- On Linux: the bundled open-source set at a known path, e.g.
  `/usr/share/fonts/typeanvil/LiberationSans-*.ttf`.

The GENERIC GATE (`family_candidates`, ~line 386) pins serif/sans-serif/
monospace/cursive/fantasy to the bundled Arial set. This issue re-points the
gate to the bundled open-source set on Linux, with an approved re-baseline per
`docs/conventions/css-standards-alignment.md`.

### Container image

```text
ghcr.io/typeanvil/typeanvil:<tag>
  /usr/bin/typeanvil          # the musl release binary
  /usr/share/fonts/typeanvil/ # bundled Liberation Sans (4 faces) + OFL license
  /usr/share/fonts/user/      # user mount point (documented contract)
  non-root user, no shell
```

## Acceptance Criteria

- **AC1 (spec committed):** This specification has valid frontmatter before
  engine or CI changes; the docs validator is green.
- **AC2:** Linux fallback renders with valid font bytes; the GENERIC GATE
  re-points to the bundled set with an approved re-baseline per
  `docs/conventions/css-standards-alignment.md`.
- **AC3:** User fonts override the bundled set (verified by rendering with a
  distinctive user font).
- **AC4:** Release CI container job is green on a test tag; image pushed to
  GHCR with a multi-arch manifest; `--version` matches the tag; smoke render
  matches the native Linux binary's output.
- **AC5:** Non-root execution verified; container renders with no host font
  installation.

## Edge Cases

- **No user fonts, no bundled set readable:** the engine falls back to the
  bundled Arial set (macOS) or the bundled open-source set (Linux). If neither
  is readable, the render fails with a clear error — never empty font bytes
  silently.
- **User font directory empty:** fontdb discovers nothing; the bundled set
  stands.
- **User font shadows a bundled family name:** user faces resolve first
  (Behavior 2); the bundled set is the fallback.
- **Arial/Helvetica name path on Linux:** resolves to the bundled open-source
  set (metric-compatible with Arial), not empty bytes.
- **Multi-arch pull on an unsupported platform:** GHCR serves the matching
  manifest; a platform without a manifest fails with a clear registry error.

## References

- Release workflow: `.github/workflows/release.yml`.
- Container definition: `docker/Dockerfile`.
- `font-resolution.spec.md` — the GENERIC GATE and bundled Arial fallback.
- `release-ci.spec.md` — determinism constraints the container job must not
  violate.
- `cli-surface.spec.md` — the `--version` / `render` contract the image
  preserves.
- `docs/conventions/css-standards-alignment.md` — the re-baseline rule.
- Liberation Sans: SIL Open Font License 1.1.
