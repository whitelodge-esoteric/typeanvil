---
title: Existing-hyphen breaks render one hyphen
slug: /specifications/existing-hyphen-break
type: spec
status: in-review
owner: elijah
created: 2026-08-20
updated: 2026-08-20
sidebar_position: 16
tags: [engine, typography, hyphenation, line-breaking]
spec_id: existing-hyphen-break
issue_id: CORE-98
applies_to: engine 0.x
dependencies: [typography-layer, hyphenation-line-break-parity]
---

# Existing-hyphen breaks render one hyphen

## Overview

A compound word that already contains a hyphen (e.g. `page-margin`) can break
at that existing hyphen when it does not fit on a line. TypeAnvil rendered
`page--` where the source has one hyphen: the first line carried the source
hyphen (inside the syllable box) AND the K-P break penalty appended a second
hyphen glyph. Prince renders `page-`. Found in the 2026-08-20 demo triage
(letterhead p1, `5afe57d`).

## Goals / Non-Goals

Goals:

- A line broken at an existing source hyphen renders exactly one trailing
  hyphen.
- Real hyphenation breaks (Liang syllable boundaries) keep appending the
  hyphen glyph — `page-mar-` at the `mar|gin` boundary is unchanged.

Non-Goals:

- Changing hyphenation density, penalties, or breakpoint choice.
- Changing the line-end hyphen character (still `-`, extracted as U+FFFE in
  the PDF text layer, same as Prince).
- Handling hyphens inside margin-box / generated `content:` runs (those go
  through `shape_word` directly, not the K-P item stream).

## Behavior

1. `push_word` shall emit, for each Liang break penalty whose source position
   immediately follows an existing `-` character in the word, a penalty with
   `hyphen: None` and zero added width. The source hyphen is already part of
   the preceding syllable box's text, so the line that breaks there must not
   receive a second glyph.
2. `push_word` shall emit `hyphen: Some` penalties (with the hyphen glyph) for
   every other Liang break boundary, unchanged from CORE-97.
3. `materialize_line` shall append the hyphen glyph only when the break item
   carries `hyphen: Some` (unchanged code path; the fix is purely which
   penalties carry a glyph).
4. The text of a line broken at an existing hyphen shall end with exactly one
   `-` character, and the following line shall begin with the remainder of the
   word without a leading hyphen (`page-` / `margin`, never `page--` or
   `page-` / `-margin`).
5. Source-text accounting (`consumed` / `box_ends`) shall be unchanged by this
   fix: no text is lost or duplicated across the break.

## Interfaces

No public API changes. Internal changes only:

- `engine/src/typography.rs` — `push_word`: the `Item::Penalty` constructor
  now checks `word.as_bytes().get(syl_start - start - 1) == Some(&b'-')`
  before deciding `hyphen` / `width`.
- `Item::Penalty` (same file) — unchanged shape.

## Acceptance Criteria

Given a paragraph containing `page-margin` broken at a width where the word
breaks at its own hyphen (e.g. 39pt, demo geometry):

- When the engine breaks the paragraph with hyphenation enabled,
  Then the first line's text is exactly `page-` (one hyphen),
  And no line in the paragraph contains `--`,
  And the next line begins `margin` (no leading hyphen),
  And the line's glyph run contains a glyph whose range maps to the trailing
  `-` in the line text.

Mapped test: `engine/tests/typography.rs`
`existing_hyphen_break_single_hyphen` (CORE-98 regression).

Given the letterhead demo fixture rendered through the CLI:

- When the PDF text layer of page 1 is extracted,
  Then it contains `page-` exactly like Prince (never `page--`).

## Edge Cases

- **Real hyphenation break inside a compound** (`page-mar-` at `mar|gin`):
  the break is a Liang boundary, the preceding syllable does not end in `-`,
  so the penalty keeps its glyph. Verified: 52–60pt probe renders
  `page-mar-` / `gin`.
- **Word with hyphen not at a break position**: if the K-P DP never breaks at
  the existing hyphen, no penalty at that boundary is ever materialized; the
  word renders whole (`page-margin`).
- **Multiple hyphens** (`well-known`, `semi-permeable`): each existing hyphen
  that becomes a break boundary is treated the same way; each line carries
  exactly its own source hyphen.
- **`hyphenate: false`**: `push_word` takes the single-box path; no penalties
  are emitted, no behavior change.

## References

- CORE-97 spec: `hyphenation-line-break-parity` (hypher syllable pipeline,
  `LEFT_HYPHEN_MIN`, K-P penalty semantics).
- `engine/src/typography.rs` — `push_word`, `materialize_line`.
- Demo triage 2026-08-20 at `5afe57d` (letterhead p1 `page--`).
