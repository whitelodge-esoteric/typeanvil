---
title: "white-space: pre — preformatted text preservation"
type: spec
status: in-review
owner: maintainers
created: 2026-09-13
updated: 2026-09-27
sidebar_position: 10
tags: [engine, css-text, white-space, fragmentation]
spec_id: SPEC-WHITESPACE-PRE
slug: /specifications/white-space-pre
---

# white-space: pre — preformatted text preservation

## Motivation

The engine's computed-style seam now maps stylo's white-space longhands to an
internal `WhiteSpace` value. This specification defines preservation and
fragmentation behavior for `pre`, `pre-wrap`, `pre-line`, and `break-spaces`.

## Standards basis

css-text-3 §5 (white space processing) and css-text-4 (white-space shorthand
split into `white-space-collapse` + `text-wrap-mode`). stylo 0.20 computes the
shorthand into those two longhands on the inherited-text style struct; the
engine's `ComputedStyle` seam must map them.

## Behavior

The engine **shall** compute white space handling per element from stylo's
`white-space-collapse` and `text-wrap-mode` longhands into an internal
`WhiteSpace` value (normal, pre, nowrap, pre-wrap, pre-line, break-spaces).

The UA stylesheet **shall** declare `white-space: pre` on `pre` elements.

For elements whose white space value preserves breaks (pre, pre-wrap,
pre-line, break-spaces), the engine **shall**:

- treat each newline in the source text as a forced line break;
- preserve sequences of white space instead of collapsing them;
- not distribute justification glue on those lines (spaces keep their natural
  width).

For `pre` (no soft wrap) lines **shall** not soft-wrap: a line longer than the
content box overflows the box rather than reflowing. For `pre-wrap` and
`pre-line` lines **shall** soft-wrap at the content-box width.

Lines produced by newline breaks **shall** fragment across pages like any
other line sequence: a long pre block splits at line boundaries, not mid-line.

## Acceptance criteria

1. The report fixture's code block renders as 6 lines.
2. A probe fixture with `white-space: pre` yields line count = newline count
   + 1; interior multi-space runs keep their width.
3. `<pre>` preserves its source line structure without any author CSS.
4. A long `white-space: pre` block spanning more than one page fragments at
   line boundaries; page count = ceil(lines × line-height / content height).
5. WPT gate: zero status flips on the full 283-test suite.
6. `cargo test` green in the engine crate.
7. `python3 scripts/validate_docs.py` passes (this spec ships in the same PR).

## Non-goals

- `white-space: break-spaces` space-width semantics beyond preservation.
- Tab character advance-width handling (tabs render as-is; no tab stops).
- Segmented-line byte-offset interaction with footnotes inside pre blocks
  (footnote markers inside pre follow the segmented-line rule; a pre
  segment boundary is a hard break, and call markers spanning two pre lines
  inherit the existing segmented logic).
