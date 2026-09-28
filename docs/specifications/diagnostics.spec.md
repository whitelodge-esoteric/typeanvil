---
title: Structured Machine-Readable Diagnostics
type: spec
status: approved
owner: maintainers
created: 2026-08-24
updated: 2026-09-27
slug: /specifications/diagnostics
sidebar_position: 43
tags: [cli, diagnostics, css-parsing]
spec_id: diagnostics
applies_to:
  - engine/src/diagnostics.rs
  - engine/src/main.rs
dependencies:
  - wpt-conformance-harness
---

# Structured Machine-Readable Diagnostics

## Overview

Prince reports authoring problems as prose in a log. TypeAnvil is AI-first:
the natural consumer of its error output is another program — an agent loop,
a CI gate, the WPT harness. This spec defines a machine-readable diagnostics
channel: which CSS properties the engine does not consume, which at-rules it
does not know, and which declarations it had to drop while parsing, each with
a stable event code and a source location.

Iteration one covers the stylesheet surface (parse/cascade input). The
scanner analyzes the SAME CSS text the cascade consumes, so its findings are
true statements about the render even though it runs as a separate pass.

## Goals

1. `--diagnostics json`: one JSON document on stdout with every diagnostic
   event, schema-versioned and stable.
2. `--diagnostics text`: the same events, human-readable, on stderr.
3. Default behavior unchanged: no flag → zero extra output, byte-stable PDFs.
4. Determinism: identical input → identical diagnostic list, emitted in
   source order. No hash-map iteration anywhere in the path.

## Non-Goals

- **Layout-time events** ("float deferred past page", fallback taken during
  fragmentation) require threading a sink through `layout()`; deferred to a
  follow-up issue. Iteration one reports the stylesheet surface only.
- **Harness integration** (per-test diagnostic counts in WPT reports) —
  deferred to a later harness revision.
- Value-level validation beyond structural checks (a full CSS value grammar
  is stylo's job; the scanner never re-implements it).
- Fixing or working around anything the diagnostics report.

## Behavior

1. The engine SHALL expose `diagnostics::analyze(css: &str) -> Vec<Diagnostic>`
   that scans a stylesheet source string and returns diagnostic events in
   ascending source order (by byte offset).
2. Each `Diagnostic` SHALL carry: `code` (stable string, see §Schema),
   `severity` (`warning` in iteration one), `message` (human sentence), and
   `line`/`column` (1-based coordinates of the event's start in the original
   source, counting lines by `\n`). Comments and quoted strings SHALL be
   skipped, so a property named inside a comment never fires an event.
3. The scanner SHALL emit `unsupported-property` for every declaration whose
   property name is NOT in the engine's consumed-property set (Appendix A:
   properties read by `css.rs::convert`, the break pass, the border pass,
   the paged pass, or `@font-face` descriptors).
4. The scanner SHALL emit `unknown-at-rule` for any at-rule whose keyword is
   not in `{@media, @page, @font-face}`. `@media` bodies SHALL be scanned
   recursively; `@page` bodies SHALL be scanned for declarations, with
   margin-box sub-blocks (`@top-left` … `@bottom-center`) recognized and
   skipped as supported constructs.
5. The scanner SHALL emit `display-fallback` when a `display` declaration's
   value is one of the engine's documented fallbacks: `inline-table`,
   `table-column`, `table-column-group`, `table-caption`, `inline-flex`
   (each renders via a defined fallback; see `css.rs::convert`).
6. The scanner SHALL emit `malformed-declaration` for a declaration fragment
   that has no `:` separator or an empty property name.
7. The CLI SHALL accept `--diagnostics <json|text>`. In `json` mode it SHALL
   print exactly one JSON document (§Schema) to stdout after writing the
   output PDF. In `text` mode it SHALL print one line per event to stderr:
   `LINE:COL warning [code]: message`. Without the flag, nothing is printed
   and the render path is untouched.
8. The analysis SHALL run only when `--diagnostics` is present (zero cost,
   byte-stable PDFs when off).

## Interfaces

```rust
// engine/src/diagnostics.rs
pub struct Diagnostic {
    pub code: String,      // e.g. "unsupported-property"
    pub severity: String,  // "warning"
    pub message: String,
    pub line: usize,       // 1-based
    pub column: usize,     // 1-based, char count (not bytes)
}

/// Scan CSS source; events sorted by source position.
pub fn analyze(css: &str) -> Vec<Diagnostic>;

/// Serialize the versioned JSON document (deterministic key order).
pub fn to_json(events: &[Diagnostic]) -> String;
```

CLI: `typeanvil render <input.html> … [--diagnostics json|text] -o out.pdf`
(added to the contract comment in `main.rs`; mirrored in
`harness/engine.py`'s `CliEngine` docstring).

## JSON Schema

```json
{
  "schema": 1,
  "diagnostics": [
    {
      "code": "unsupported-property",
      "severity": "warning",
      "message": "property `text-indent` is not supported and was ignored",
      "line": 3,
      "column": 5
    }
  ],
  "counts": { "warnings": 1 }
}
```

- `schema` is the integer schema version; breaking changes bump it.
- `diagnostics` is in ascending `(line, column)` order.
- Event codes (stable): `unsupported-property`, `unknown-at-rule`,
  `display-fallback`, `malformed-declaration`.
- `counts.warnings` equals the number of events (all severities are
  `warning` in schema 1).

## Acceptance Criteria

1. Given a fixture whose stylesheet contains an unsupported property, an
   unknown at-rule, a `display: inline-table`, a malformed declaration, and
   supported declarations around them, When analyzed, Then the JSON document
   lists EXACTLY those four events in source order with correct 1-based
   line/column coordinates (test asserts the exact JSON string:
   `engine/tests/diagnostics.rs::fixture_events_exact_json`).
2. Given the same fixture rendered twice with `--diagnostics json`, the two
   stdout documents SHALL be byte-identical (determinism).
3. Given any document rendered WITHOUT `--diagnostics`, the stderr and the
   output PDF bytes SHALL be unchanged versus the baseline binary
   (gated: full `cargo test` suite green; PDF determinism tests cover
   byte-stability).
4. Comments and strings containing property-like text produce NO events
   (`comments_and_strings_are_ignored`).

## Edge Cases

- Unterminated block or comment: scan ends there; no panic, no synthetic
  events.
- `!important` suffixes: stripped before value checks; importance does not
  change support status.
- Properties inside `@page` margin boxes: only `content`-family properties
  are meaningful there; the scanner skips margin-box bodies entirely rather
  than risk false `unsupported-property` noise (margin-box descriptor sets
  overlap the element set imperfectly — documented deviation, revisit when
  layout-time events land).
- Uppercase / mixed-case property names: normalized case-insensitively.
- Vendors (`-webkit-*`) and custom properties (`--*`): never reported (they
  are legal CSS the engine deliberately ignores).

## Appendix A — Consumed-property set

Derived from code (grep of accessors + hand-rolled passes), not memory:

color, background-color, font-family, font-size, font-weight, font-style,
line-height, text-align, margin-top/right/bottom/left, padding-top/right/
bottom/left, width, height, min-width, display, position, top, right, bottom,
left, z-index, float, clear, border, border-width, border-color,
border-top(-width|-color), border-right(-width|-color), border-bottom(-width|
-color), border-left(-width|-color), break-before, break-after, break-inside,
page-break-before, page-break-after, page-break-inside, orphans, widows,
hyphens, string-set, counter-reset, counter-increment, content, page,
column-count, column-width, column-span, column-gap, row-gap, gap,
flex-direction, flex-wrap, flex-grow, flex-shrink, flex-basis, flex,
align-items, align-self, justify-content, order, src (@font-face).

Note: `min-width` is parsed by stylo but the block layout path ignores
height/auto constraints (the current block-layout model); it stays OUT of the reported set —
this scanner reports the stylesheet surface, and per-property layout
fidelity is layout-time diagnostics' job (Non-Goals).

## References

- `engine/src/diagnostics.rs` and `engine/src/main.rs` — implementation seams.
- Char-safe CSS scanning: the scanner iterates code points,
  never raw bytes).
- Future: layout-time diagnostic events, harness per-test counts.
