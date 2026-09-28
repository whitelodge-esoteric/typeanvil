---
title: Licensing and Distribution
type: spec
status: approved
owner: maintainers
created: 2026-08-21
updated: 2026-09-27
sidebar_position: 24
tags: [engine, licensing, agpl, distribution]
spec_id: licensing-resolution
applies_to: engine and engine tooling
slug: /specifications/licensing-resolution
dependencies: []
---

# Licensing and Distribution

## Overview

Typeanvil's engine and engine tooling are free software under the
[AGPL-3.0-only](https://www.gnu.org/licenses/agpl-3.0.html) license. The
repository ships the license text and contribution terms. The engine has no
license state, activation flow, watermark, phone-home behavior, or feature gate.

The repository documents the engine and its tooling. Hosted-service behavior is
outside this specification. Public hosted-service documentation will be linked
here when it is available.

## Goals / Non-Goals

**Goals**

- Keep the engine usable offline without registration or activation.
- Preserve deterministic rendering regardless of distribution context.
- State the repository license and contribution terms clearly.
- Keep engine and tooling distribution separate from any hosted service.

**Non-Goals**

- License resolution, license files, activation, or runtime entitlement checks.
- Watermark rendering for unlicensed use.
- Phone-home telemetry or mandatory network access.
- Hosted-service plans, billing, authentication, quotas, or watermark policy.
- A commercial licensing program for the engine.

## Behavior

1. **License declaration.** The repository shall declare AGPL-3.0-only in the
   root license file and engine package metadata.
2. **No runtime enforcement.** The engine shall contain no license resolution,
   activation, watermarking, phone-home, or feature-gating logic.
3. **Offline operation.** Rendering shall not require registration, a license
   file, a network request, or a hosted-service account.
4. **Determinism.** Identical input, engine version, dependency data, and font
   data shall produce identical output. Licensing context shall not affect it.
5. **Contribution terms.** The repository shall publish `CLA.md` and shall link
   to it from contributor-facing documentation.
6. **Dependency compatibility.** Engine dependencies shall remain distributable
   under the repository's license and their own license terms. License review
   shall use current package metadata rather than an old research note.
7. **Service boundary.** Engine and tooling documentation shall not describe
   hosted-service behavior as an engine feature. When public service
   documentation exists, repository docs may link to it.

## Interfaces

The relevant distribution interfaces are repository files and package metadata:

- `LICENSE` — AGPL-3.0-only license text.
- `CLA.md` — contribution terms.
- `engine/Cargo.toml` — package license metadata.
- `README.md` — public license and scope statement.

The engine CLI has no license or activation subcommand.

## Acceptance Criteria

1. The repository root contains the complete AGPL-3.0-only license text.
2. `engine/Cargo.toml` declares `license = "AGPL-3.0-only"`.
3. A source and test search finds no engine license-check, activation,
   watermark, or phone-home path.
4. A minimal offline render does not inspect a license file or contact a
   network service.
5. The README identifies the engine and tooling license, links `CLA.md`, and
   leaves hosted-service documentation as a future public link.
6. Documentation validation passes with PyYAML installed.

## Edge Cases

- A contributor may use the engine offline. The render result shall not change.
- A deployment that adds authentication or quotas is outside the engine and
  shall be documented by that deployment.
- A future hosted service may have separate terms. Those terms shall not be
  represented as engine runtime behavior.
- A dependency license change requires a new review before release.

## References

- [GNU AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html)
- `LICENSE` — repository root license text.
- `CLA.md` — repository contribution terms.
- [Typeanvil architecture](../architecture/overview.md)
- [Container distribution specification](container-distribution.spec.md)
- [Documentation conventions](../conventions/doc-conventions.md)