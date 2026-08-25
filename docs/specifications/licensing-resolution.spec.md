---
title: Licensing and Distribution (Open Core / AGPL)
slug: /specifications/licensing-resolution
type: spec
status: draft
owner: elijah
created: 2026-08-21
updated: 2026-08-25
sidebar_position: 24
tags: [engine, licensing, agpl, distribution]
spec_id: licensing-resolution
issue_id: CORE-115
applies_to: engine 0.x
dependencies: []
---

# Licensing and Distribution (Open Core / AGPL)

## Overview

Typeanvil is distributed as **free, open-source software under the GNU
Affero General Public License v3 (AGPL-3.0)**. The runtime renders with no
watermark, no license check, and no feature gating. Revenue comes from the
hosted cloud service (`docs/specifications/typeanvil-cloud-service.spec.md`),
not from selling the binary.

This spec supersedes the previous commercial closed-source design
(license resolution chain + unlicensed watermark), decided against on
2026-08-25. The prior watermark seam work from CORE-115 iteration one is
dropped; this issue now covers the distribution model instead.

**Why AGPL:** the license adds a network-use clause to GPL — anyone who
modifies Typeanvil and offers it as a service must publish their modified
source. This prevents a competitor from forking the engine and hosting a
proprietary clone of our own cloud product. As copyright holder, we can
also offer a separate commercial license to companies that need to embed
the engine in proprietary software (dual licensing) — optional, later.

## Goals / Non-Goals

**Goals**

- Single license file (`LICENSE`, AGPL-3.0 text) at repo root.
- Copyright headers carry the AGPL notice in `engine/` source files.
- The engine renders identically regardless of environment: no license
  lookup code paths exist anywhere.
- `Cargo.toml` declares `license = "AGPL-3.0-only"` (or `-or-later` — see
  Open Questions).
- Distribution via crates.io / GitHub releases of source; container images
  published publicly for cloud use.

**Non-Goals**

- License resolution, license files, Ed25519 verification, activation —
  all dropped with the commercial model.
- Watermark rendering in `pdf.rs` — dropped.
- Phone-home telemetry of any kind.
- A `typeanvil license` subcommand — dropped.

## Behavior

1. **No enforcement code.** The engine shall contain no license resolution,
   watermarking, or gating logic. Rendering output depends only on input.
2. **AGPL compliance surface.** The repository shall ship: the full AGPL
   license text, a README section stating the license and pointing at the
   cloud service, and source notices per the AGPL's requirements.
3. **Determinism preserved unchanged.** Identical input → byte-identical
   PDF, with no environment-dependent branches introduced by licensing
   (there are none).
4. **Dependency audit.** All engine dependencies shall remain compatible
   with AGPL distribution (Apache-2.0/MIT/BSD/MPL are compatible; no
   additional constraints were imposed by the old model either).
5. **Trademark separation.** "Typeanvil" name/logo are not licensed under
   AGPL; forks shall not use the marks (standard open-core trademark
   carve-out, enforced informally until trademark registration).

## Acceptance Criteria

1. Repo root contains the complete AGPL-3.0 license text; the validator
   or CI checks its presence and first-line hash prefix.
2. `cargo build && cargo test` pass with zero licensing-related modules;
   grep confirms no `licensing` module, no `--license` flag, no watermark
   string exists in `engine/src/`.
3. Rendering a minimal document produces byte-identical output before and
   after the licensing-code removal PR (regression proof that no behavior
   changed).
4. `cargo package --list` succeeds and `cargo publish --dry-run` reports
   `license = "AGPL-*"` metadata.

## Edge Cases

- Contributor CLA: needed only if we later want to dual-license or
  relicense; decide before outside contributions arrive.
- Cloud service itself: we are the copyright holder, so AGPL's network
  clause never obligates us to publish our platform modifications.
- Enterprise embedding requests: handled case-by-case under a future
  commercial license; does not change the OSS default.

## References

- Cloud service spec: `docs/specifications/typeanvil-cloud-service.spec.md`
- Prior research brief (historical context):
  `docs/research/licensing/licensing-and-distribution.md`
- AGPL-3.0 text: https://www.gnu.org/licenses/agpl-3.0.txt
