---
title: CSS text-transform
type: spec
status: approved
owner: maintainers
created: 2026-10-16
updated: 2026-10-16
slug: /specifications/text-transform
sidebar_position: 60
tags: [typography, css-text, text-transform]
spec_id: text-transform
issue_id: CORE-359
applies_to:
  - engine/src/css.rs
  - engine/src/layout.rs
  - engine/src/table.rs
dependencies: []
---

# CSS text-transform

## Overview

CSS Text 3 §2.1 defines `text-transform`. The engine shall apply `uppercase`,
`lowercase` and `capitalize` to text before shaping and line breaking, so the
PDF text layer carries the transformed text.

Path chosen: stylo. `text-transform` compiles in the servo build and stylo
computes cascade and inheritance. Verified 2026-10-16:
`clone_text_transform().case()` in `engine/src/css.rs`.

## Goals

- `uppercase`, `lowercase` and `capitalize` change the laid-out text.
- Non-ASCII letters transform (`é` to `É`).

## Non-Goals

- `full-width` and `full-size-kana` compute to `none`.
- Language-tailored mappings (Turkish dotted i, Lithuanian). The style layer
  carries no `lang`.
- `::first-letter` and generated content interaction.
- Table cell measurement transforms the whole cell text as one string. A
  `capitalize` cell with several inline children can measure a few points
  wrong at element boundaries. Painted text is exact.

## Behavior

1. The engine shall read the computed case keyword into
   `ComputedStyle.text_transform`. The value is inherited.
2. A text node shall transform with its parent element's value.
3. `uppercase` and `lowercase` shall use Unicode full case mapping
   (`ß` becomes `SS`).
4. `capitalize` shall uppercase the first alphanumeric character of each word.
   A letter that follows an alphanumeric, `'` or `’` is not a word start. The
   check spans inline element boundaries inside one block.
5. `none` shall leave the text unchanged.

## Acceptance Criteria

| Given | Then | Test |
|---|---|---|
| `uppercase` on "Théodore flight" | text is "THÉODORE FLIGHT" | `core359_text_transform::uppercase_non_ascii` |
| `lowercase` on "ÉCOLE Flight" | text is "école flight" | `lowercase_non_ascii` |
| `capitalize` on "théodore o'neil-smith (x)" | "Théodore O'neil-Smith (X)" | `capitalize_words` |
| parent `uppercase`, child `none` | child text unchanged, inline sibling upper | `inherited_and_overridden_by_child` |
| `capitalize` over `ab<span>cd</span> ef` | "Abcd Ef" | `capitalize_does_not_restart_mid_word_across_inline` |
| no `text-transform` | text unchanged | `none_is_unchanged` |

## References

- CSS Text Module Level 3 §2.1, https://www.w3.org/TR/css-text-3/#text-transform-property
- CORE-359
