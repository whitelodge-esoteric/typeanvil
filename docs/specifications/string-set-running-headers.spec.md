---
title: Running Headers — string-set and string()
slug: /specifications/string-set-running-headers
type: spec
status: draft
owner: elijah
created: 2026-08-22
updated: 2026-08-22
sidebar_position: 16
tags: [engine, css-gcpm, css-page, paged-media]
spec_id: string-set-running-headers
issue_id: CORE-108
applies_to: engine 0.x
dependencies: [paged-media-css, fragmentation-core]
---

# Running Headers — `string-set` and `string()`

## Overview

Running headers are the book/report pattern where each page's margin box shows
the chapter (or section) currently in effect on that page. CSS names the two
halves: `string-set: name content()` captures a value as elements are placed,
and `content: string(name)` reads it in an `@page` margin box
(css-gcpm-3 §7).

The engine already has partial scaffolding:

- `ComputedStyle.string_set` parses `string-set` declarations (`css.rs`,
  `parse_string_set`).
- `layout_box` assigns values into `Flow.running` when a fresh box starts.
- `ContentPiece::StringRef(name)` parses `string(name)` in margin-box
  `content`, and both `resolve_content` and `render_margin_content` read
  `flow.running`.

What is missing is the css-gcpm-3 semantics: the `first` / `start` / `last` /
`first-except` keywords (the parser stores the whole argument list as the name,
so `string(chapter, first)` looks up a string literally named `"chapter,
first"`), and page-scoped assignment tracking. Today every read behaves as
`last`.

This spec defines the completion of the feature. The keyword semantics are not
taken from prose — they were probed against Prince 16.2 (see
`docs/research/css-gcpm/prince-string-keywords-probe.md`) and the probe results
are normative for this implementation.

## Goals

1. All four keywords work in margin boxes, with Prince-matching behavior on
   the probe documents (text-layer equality).
2. Carry-over across pages works (a page with no new assignment shows the most
   recent earlier value).
3. Values captured inside fragmented elements (an `h2` that splits across a
   page boundary) count as assigned on the page where their FIRST fragment
   starts.
4. Deterministic: same input → byte-identical PDF.

## Non-Goals

- `attr()` and counter values inside `string-set` (parser maps them to
  `content()` today; unchanged).
- `string()` in element `content:` resolving with per-element timing (element
  generated content keeps reading the current value; only margin boxes get
  keyword semantics).
- Named strings entering the PDF bookmark/outline structure.

## Behavior

1. **Parsing.** The engine shall parse `string(name)` and
   `string(name, keyword)` where keyword ∈ {`first`, `start`, `last`,
   `first-except`} (case-insensitive). The default keyword shall be `first`.
   A malformed keyword shall fall back to `first`.

2. **Capture.** When an element's box STARTS fresh on a fragmentainer (the
   existing `fresh` branch in `layout_box`), for each of its `string-set`
   pairs the engine shall record `(page_index, name, value)` in a per-page
   assignment log AND update the persistent current-value map. Capture order
   within one page shall be document (pre-order placement) order.

3. **Fragmented capture.** An element that begins on page N and continues on
   page N+1 shall be recorded once, on page N (capture happens in the `fresh`
   branch only).

4. **Resolution — margin boxes.** For margin-box content, let A = the list of
   assignments to `name` made during page P, and C = the carried value at the
   start of page P (value after page P−1; empty before page 1). Then:
   - `last`: last of A if A non-empty, else C.
   - `first`: first of A if A non-empty, else C.
   - `start`: C always (Prince-verified: even an element placed at the very
     top of page P does NOT count for `start`; see research note).
   - `first-except`: empty string if A non-empty, else C.

5. **Carry-over.** C for page N+1 shall equal the value map state after page N
   completes. No assignment anywhere in the document → empty string.

6. **Two-pass convergence.** Margin-box text never affects pagination, so
   string resolution cannot change page counts; the same bounded fixed-point
   argument as `counter(pages)` (CORE-84) applies. Element `content:` reads
   happen during body layout and use the current-value map directly (existing
   behavior); they do not consume the page log.

7. **Named pages interplay.** Assignment logging shall be independent of named
   page context: a `string-set` on an element placed on any named page logs to
   that element's physical page index. Margin boxes resolve against their own
   page's log.

## Interfaces

```rust
// paged.rs
pub enum StringKeyword { First, Start, Last, FirstExcept }
impl Default for StringKeyword { fn default() -> Self { StringKeyword::First } }

// ContentPiece::StringRef gains the keyword:
StringRef(String, StringKeyword)

// layout.rs — Flow gains a page-scoped assignment log:
struct Flow {
    // existing fields ...
    /// Assignments made during the CURRENT page: (name, value) in capture
    /// order. Drained/cleared at each page boundary after margin boxes
    /// attach; used by margin-box keyword resolution.
    page_string_sets: Vec<(String, String)>,
}
```

Margin-box attachment receives the page log + current map and resolves each
`StringRef(name, kw)` per §Behavior 4. `resolve_content` (element path)
treats every keyword as "current value" and logs a deviating-implementation
note.

## Acceptance Criteria

1. **Given** the multi-chapter probe doc, **when** rendered, **then** page 2's
   top margin shows `first`=Chapter Two, `start`=Chapter One, `last`=Chapter
   Two — matching the Prince text layer exactly.
2. **Given** a page containing two chapter headings, **when** rendered,
   **then** `first` shows the earlier heading, `last` the later, default ==
   `first`.
3. **Given** a two-page doc whose chapter heading sits on page 1 only,
   **when** rendered, **then** page 2 shows the carried value for
   `first`/`last` and `first-except` shows the carried value too (empty on
   page 1, carried on page 2).
4. **Given** a heading split across a page break (its second half lands on
   page N+1), **when** rendered, **then** the assignment logs to page N only.

Each criterion maps to a test in `engine/tests/paged_media.rs`
(string-keyword unit tests over a small DOM) plus the probe-doc text-layer
comparison run manually before landing.

## Edge Cases

- `string(name)` referenced before ANY assignment: empty string, all keywords.
- Multiple `string-set` pairs on ONE element (`string-set: a content(), b
  content()`): both log independently, document order.
- Same name assigned twice on one page: `first` vs `last` differ; verified by
  criterion 2's fixture shape (two h2s) and by a dedicated unit test using two
  assignments from one element pair.
- MAX_PAGES truncation: pages beyond the cap don't exist; no resolution is
  attempted for them.

## References

- Probe evidence (normative): `docs/research/css-gcpm/prince-string-keywords-probe.md`
- css-gcpm-3 §7 (generated content for paged media)
- CORE-84 two-pass convergence precedent; CORE-82 named-page context lessons
