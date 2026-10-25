---
title: Agent Authoring Guide
slug: /specifications/agent-authoring-guide
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 50
tags: [ai-native, docs, mcp, capabilities]
spec_id: agent-authoring-guide
issue_id: CORE-579
applies_to:
  - engine/src/guide.rs
  - engine/src/guide/authoring.md
  - engine/src/mcp/mod.rs
  - docs/guides/agent-authoring.md
  - skills/typeanvil-authoring/SKILL.md
dependencies:
  - capability-manifest
  - render-report
  - mcp-server
---

# Agent Authoring Guide

## Overview

An agent that writes HTML for TypeAnvil needs a short guide: what CSS works,
which paged-media idioms to use, what to avoid, and how to fix render-report
findings. This spec defines that guide and its three published forms.

## Goals

1. One prompt-ready guide whose supported and unsupported lists come from the
   capability manifest (`capability-manifest.spec.md`).
2. Three forms from one source: a docs page, an agent skill file, and an MCP
   resource and prompt.
3. CI fails when a committed form differs from the generated guide.

## Non-Goals

1. A full CSS reference. `docs/guides/css-support.md` stays the long form.
2. Per-element behavior notes beyond the manifest's own notes.

## Behavior

1. The guide prose SHALL live in `engine/src/guide/authoring.md`. The
   placeholders `{{SCHEMA}}`, `{{GENERATED}}` and `{{AVOID}}` SHALL be
   replaced from `capabilities::manifest()` by `guide::body()`.
2. `{{GENERATED}}` SHALL list every `supported` property, `@page` descriptor,
   at-rule, selector feature and input format, and every `partial` entry with
   its note. `{{AVOID}}` SHALL be a table of every `unsupported` entry with
   its note and alternative.
3. The guide SHALL cover: the workflow (write, validate, fix, render), a
   document skeleton, `@page`, margin boxes, running headers, footnotes,
   `break-*`, a table of render-report codes with fixes, and a done check.
4. The docs page SHALL be `docs/guides/agent-authoring.md` (slug
   `/guides/agent-authoring`). The docs-site `llms.txt` plugin lists every
   published page, so the page appears in `llms.txt` with no extra step.
5. The skill file SHALL be `skills/typeanvil-authoring/SKILL.md` with
   `name` and `description` frontmatter.
6. `typeanvil mcp` SHALL declare the `resources` and `prompts` capabilities
   and SHALL serve: resource `typeanvil://guide/authoring`
   (`text/markdown`, via `resources/list` and `resources/read`) and prompt
   `write_document` (via `prompts/list` and `prompts/get`, with an optional
   `task` argument). An unknown resource URI SHALL return error `-32002`; an
   unknown prompt name SHALL return `-32602`.
7. Regeneration: `UPDATE_GUIDE=1 cargo test --test it authoring_guide`
   rewrites the docs page and the skill file. Without `UPDATE_GUIDE`, the same
   tests fail when a committed file differs from the generated text. The
   tests run in the CI `engine` job (`cargo test`), so a manifest change
   without a regeneration fails CI.
8. The guide's skeleton stylesheet SHALL draw no diagnostics.

## Acceptance Criteria

1. Given a manifest change, when the committed docs page or skill file is not
   regenerated, then `authoring_guide::docs_page_is_current` or
   `skill_file_is_current` fails.
2. Given the manifest, the guide names every supported and unsupported
   property (`guide_lists_every_manifest_entry`).
3. Given the skeleton stylesheet, `analyze` returns no events
   (`skeleton_css_has_no_diagnostics`).
4. Given an MCP client, `resources/list`, `resources/read`, `prompts/list` and
   `prompts/get` return the guide (`mcp_exposes_resource_and_prompt`).
5. Spot check: a 2-page report that uses only the guide's idioms renders with
   `--diagnostics json` and reports 0 warnings, 2 pages (fixture
   `probe/core579/report.html`, recorded on CORE-579).

## References

- CORE-579, CORE-570 (AI-native epic), CORE-572 (capability manifest)
- `llms-txt.spec.md`, `mcp-server.spec.md`
