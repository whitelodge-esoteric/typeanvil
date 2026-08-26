---
title: Typeanvil Cloud Service
slug: /specifications/typeanvil-cloud-service
type: spec
status: draft
owner: elijah
created: 2026-08-25
updated: 2026-08-25
sidebar_position: 25
tags: [cloud, platform, api, monetization, agpl]
spec_id: typeanvil-cloud-service
issue_id: CORE-115
applies_to: platform 0.x
dependencies: [licensing-resolution]
---

# Typeanvil Cloud Service

## Overview

Typeanvil pivots from commercial closed-source distribution to an
**open-core model**: the engine runtime is free and open source under the
AGPL, and revenue comes entirely from a paid hosted service built on it.

The rationale: adoption is the bottleneck against PrinceXML and WeasyPrint.
A free OSS runtime removes the adoption tax a paid binary imposes; the cloud
service monetizes convenience and scale rather than artificial limits. AGPL
protects the service — a competitor who forks the engine and hosts their own
cloud must publish their modifications.

This spec defines the v1 cloud offering. The runtime stays genuinely useful
standalone; the cloud wins on convenience, scale, and integration.

## Goals / Non-Goals

**Goals**

- Hosted rendering API: submit HTML/Markdown + assets, receive PDF.
- Parallel rendering: batch jobs fanned out across container workers.
- MCP support: expose rendering as MCP tools so agents and LLM workflows
  call PDF generation directly (the differentiation wedge).
- Team/org support: shared templates, fonts, brand assets per organization.
- Preview UI: browser WYSIWYG preview before committing to a full render.
- Infrastructure on Cloudflare Containers (Workers Paid plan): scale-to-zero,
  10ms active billing, free egress. Cost model:
  `docs/research` note + vault "Pricing Strategy" (≈$0.00004/page steady state).

**Non-Goals**

- Any watermark or feature gating in the open-source runtime.
- Phone-home telemetry or license enforcement of any kind.
- GPU rendering, JS execution in documents (future considerations).
- Self-hosted commercial licensing tiers — replaced by dual AGPL/commercial
  licensing if embedding demand appears (see licensing spec).

## Service Tiers

| Tier | Price | Includes |
|---|---|---|
| Free | $0 | Preview UI, N renders/mo (limit TBD), low priority queue |
| Pro | ~$19–49/mo | Higher volume, parallel batch, API keys, MCP access |
| Org | TBD | Teams: shared templates/fonts/assets, roles, usage pooling |

Pricing by value tier, not per-page cost — infra floor is near zero
(≥95% gross margin at any plausible rate). Final pricing tracked in vault
`brain/Projects/Typeanvil/Pricing Strategy`.

## Behavior

The platform shall:

1. **Expose a hosted render API.** `POST /v1/render` accepts HTML/Markdown
   plus inline or referenced assets and returns a PDF (or a job handle for
   async renders). API-key authenticated, per-org metering.
2. **Fan out parallel renders.** Batch submissions split into per-document
   jobs across Cloudflare Container instances; results reassemble in order;
   partial failures report per-document status without failing the batch.
3. **Provide MCP tool endpoints.** An MCP server exposes render, preview,
   template, and org tools with the same auth model as the REST API.
4. **Run each render in an isolated container.** One job per container
   instance invocation; no shared mutable state between customer jobs.
5. **Preserve determinism guarantees server-side.** Identical input +
   identical engine version produce byte-identical output; the API pins
   and reports the engine version used for every render.
6. **Keep the OSS runtime fully capable standalone.** The cloud adds no
   engine capability that is withheld from the open-source build; paid
   value is hosting, parallelism, collaboration, and integrations.
7. **Apply free-tier watermarks at the cloud layer, never in the runtime.**
   Preview/free renders get a watermark injected by the platform (an
   injected `@page` margin-box rule or post-process), enabled per plan.
   Paid tiers render clean. Author-requested watermarks are a normal CSS
   `@page` margin-box feature and need no platform involvement.
8. **Enforce entitlements only at the API gateway.** There is no license
   check in the runtime; AGPL governs the free engine and API keys govern
   the paid service. Commercial licensing (if offered later) is a legal +
   billing channel, not a runtime feature.

## Acceptance Criteria

1. Given a valid HTML document, an authenticated `POST /v1/render`
   returns a valid PDF whose byte hash matches a local CLI render of the
   same input on the same pinned engine version.
2. Given a 100-document batch, parallel fan-out completes with all 100
   PDFs returned in submission order, or a per-document error report.
3. An MCP client can list and call the render tool end-to-end with an
   API key.
4. Two orgs' jobs share no filesystem or cache state (isolation test).
5. Free-tier metering enforces the monthly limit and returns a
   machine-readable quota-exceeded response.

## Edge Cases

- Asset fetch failures mid-render → job fails with the specific asset URL.
- Engine version skew between API regions → version pinning required in
  the request contract; unpinned requests use the current default and the
  response reports which was used.
- Very large batches → chunked submission with resumable job handles.

## References

- Licensing model: `docs/specifications/licensing-resolution.spec.md`
  (AGPL pivot supersedes the watermark design).
- Infra cost research: vault `brain/Projects/Typeanvil/Pricing Strategy.md`.
- Platform notes: vault `brain/Projects/Typeanvil/Platform/`.
