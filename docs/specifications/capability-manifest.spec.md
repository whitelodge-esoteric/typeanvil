---
title: Capability Manifest
slug: /specifications/capability-manifest
type: spec
status: approved
owner: maintainers
created: 2026-10-24
updated: 2026-10-24
sidebar_position: 45
tags: [cli, diagnostics, ai-native]
spec_id: capability-manifest
issue_id: CORE-572
applies_to:
  - engine/src/capabilities.rs
  - engine/src/diagnostics.rs
  - engine/src/main.rs
dependencies:
  - diagnostics
---

# Capability Manifest

## Overview

LLMs write CSS that the engine ignores. `typeanvil capabilities --json`
prints a schema-versioned manifest of what the engine supports. The manifest
is generated from the same tables the diagnostics scanner
(`diagnostics.spec.md`) uses to decide "not consumed". A property is
`supported` in the manifest exactly when the scanner stays silent on it, so
the two cannot disagree.

## Goals

1. One deterministic JSON document listing CSS properties, `@page`
   descriptors, `display` values, at-rules, selector features, and input
   formats, each with a status, a short note, and (where one exists) a
   recommended alternative.
2. No drift between the manifest and the diagnostics scanner, enforced by a
   test.
3. Diagnostics warnings name the manifest entry and its alternative.

## Non-Goals

- Per-value support for every property. Value-level entries exist only for
  `display`, the one property where the scanner judges values.
- A WPT-pass-rate definition of `partial`. The harness is a
  self-consistency gate (test and reference render through the same engine),
  so a bucket pass rate does not measure feature support. Revisit when the
  harness reports per-feature oracle scores.
- Layout-time fidelity (a property that is consumed but rendered imperfectly
  stays `supported`). Layout-time events are a diagnostics Non-Goal too.

## Decisions

- DECIDED 2026-10-24 (agent): **granularity** is property-level, plus
  value-level entries for `display` (kind `display`). Why: the scanner only
  distinguishes values for `display`; finer entries would not be backed by
  engine tables.
- DECIDED 2026-10-24 (agent): **`partial`** means "the engine renders a
  documented fallback instead of the value". Today that is exactly the
  scanner's `DISPLAY_FALLBACKS` table (`display-fallback` events). Why: it is
  objective and machine-checked; a WPT bucket rate is not (see Non-Goals).

## Behavior

1. The CLI SHALL accept `typeanvil capabilities --json` and print the
   manifest to stdout, exit 0. Any other argument list SHALL be a usage error
   (exit 2). `--help` SHALL list the subcommand.
2. Entries of kind `property` with status `supported` SHALL be exactly the
   scanner's consumed-property set. Kind `page-descriptor` SHALL be the
   scanner's `@page` descriptor set. Kind `display` (status `partial`) SHALL
   be the scanner's display-fallback table. Supported `at-rule` entries SHALL
   be the scanner's at-rule set.
3. Curated `unsupported` entries (properties and at-rules with a note and an
   alternative) SHALL never override a status: an entry the scanner consumes
   is omitted.
4. Any property or at-rule absent from the manifest SHALL be treated as
   `unsupported` (`default_status`).
5. Output SHALL be deterministic: entries sorted by `(kind, name)`, fixed key
   order, no version or time fields.
6. When the scanner emits `unsupported-property` or `unknown-at-rule` for a
   name whose manifest entry has an alternative, the message SHALL end with
   ` (capability <kind>:<name>; alternative: <text>)`.

## Interfaces

```rust
// engine/src/capabilities.rs
pub const SCHEMA: u32 = 1;
pub enum Status { Supported, Partial, Unsupported }
pub struct Capability { kind, name, status, note, alternative: Option<&str> }
pub fn manifest() -> Vec<Capability>;      // sorted by (kind, name)
pub fn lookup(id: &str) -> Option<Capability>; // "property:text-indent"
pub fn to_json() -> String;
```

## JSON Schema

```json
{
  "schema": 1,
  "default_status": {"property": "unsupported", "at-rule": "unsupported"},
  "capabilities": [
    {"id": "property:text-indent", "kind": "property", "name": "text-indent",
     "status": "unsupported", "note": "first-line indentation is not applied",
     "alternative": "use `padding-left` or `margin-left` on the block"}
  ]
}
```

Kinds: `at-rule`, `display`, `input-format`, `page-descriptor`, `property`,
`selector`. `alternative` is present only when one exists.

## Acceptance Criteria

1. Given the binary, When `typeanvil capabilities --json` runs, Then it exits
   0 and prints `to_json()`; `--help` lists it
   (`engine/tests/it/capabilities.rs::cli_prints_manifest`,
   `help_lists_capabilities`, `cli_without_json_flag_is_usage_error`).
2. Given every manifest entry and a sweep of common properties, When fed to
   the scanner, Then the scanner flags a name exactly when the manifest says
   `unsupported` or `partial` (`scanner_and_manifest_agree`).
3. Two runs produce byte-identical output; entries are strictly sorted
   (`manifest_is_deterministic_and_sorted`).
4. `p { text-indent: 2em }` produces a warning that names
   `capability property:text-indent` and its alternative
   (`diagnostics_reference_manifest_alternative`).

## Edge Cases

- The scanner looks properties up by binary search. The drift test caught an
  unsorted table (`float` before `flex*`) that made the scanner flag the
  whole flex family; `flex_family_is_not_flagged` guards it.
- `@page` body declarations get no alternative suffix: their descriptor set
  differs from the element set.

## References

- `docs/specifications/diagnostics.spec.md` (CORE-112).
- CORE-570 (AI-native epic), CORE-572.
