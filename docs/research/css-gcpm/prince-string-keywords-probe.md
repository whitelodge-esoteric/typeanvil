---
title: "Prince 16.2 string() keyword semantics — probe evidence"
type: research
status: approved
owner: maintainers
created: 2026-08-22
updated: 2026-08-22
sidebar_position: 2
tags: [css-gcpm, paged-media, prince, probe]
---

# Prince 16.2 `string()` keyword semantics — probe evidence

This study records a historical Prince 16.2 probe. It is retained as comparison
 evidence only; the normative behavior is defined by [CSS Generated Content for
 Paged Media Level 3](https://www.w3.org/TR/css-gcpm-3/).

Three probe docs in this directory (`core-108-string-probe.html`,
`core-108-except-probe.html`, `core-108-midpage-probe.html`), rendered with
Prince 16.2 (`prince <doc>.html -o out.pdf`), text layer dumped via
pypdfium2 `get_text_range()`.

## Verified behavior table

| Keyword | Page HAS ≥1 assignment | Page has NO assignment |
|---|---|---|
| *(none)* = `first` | first assignment on the page | carried-over value |
| `first` | first assignment on the page | carried-over value |
| `start` | **carried-over value** — even when the assigned element sits at the very top of the page | carried-over value |
| `last` | last assignment on the page | carried-over value |
| `first-except` | empty string | carried-over value |

## Evidence lines

Probe 1 (chapters One..Four; page 2 starts with "Chapter Two" at top,
page 3 contains "Chapter Three" AND "Chapter Four"):

- page 1: `F:Chapter One S: L:Chapter One` / `D:Chapter One X:` —
  page 1 has assignments, so `start` is empty (nothing to carry) and
  `first-except` is empty.
- page 2: `F:Chapter Two S:Chapter One L:Chapter Two` — `start` shows the
  PREVIOUS chapter even though "Chapter Two" is the first thing on the page.
- page 3: `F:Chapter Three S:Chapter Two L:Chapter Four` — two assignments;
  default/`first` → Three, `start` → carried Two, `last` → Four.

Probe 2 (single chapter, 2 pages):

- page 1: `F:Only Chapter X:` (has assignment)
- page 2: `F:Only Chapter X:Only Chapter` (no assignment → both carry)

Probe 3 ("Beta" starts MID-page on page 2, not at top):

- page 2: `F:Beta S:Alpha L:Beta` — `start` still shows Alpha. Combined with
  probe 1's page 2, `start` == value entering the page in BOTH placements
  (top-of-page and mid-page). This contradicts a naive reading of css-gcpm-3
  §7 ("value of the first assignment on or before the start of the page"
  interpreted as including a top-of-page element): Prince excludes any element
  placed on that page, wherever it lands.

## Design consequences

- `start` is implementable as exactly "the value at the moment the page's
  layout begins" — capture state must be snapshotted at page start.
- Carry-over falls out of keeping one persistent current-value map plus a
  per-page list of assignments made during that page.

## References

- [CSS Generated Content for Paged Media Level 3](https://www.w3.org/TR/css-gcpm-3/) — normative named-string definitions.
- [Prince generated content documentation](https://www.princexml.com/doc/gen-content/) — comparison-engine behavior.
- [WPT css-gcpm directory](https://github.com/web-platform-tests/wpt/tree/master/css/css-gcpm) — public test corpus.
