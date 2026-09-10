---
title: Selector Matching — Structural Pseudo-Classes Across Text Nodes
slug: /specifications/selector-matching
type: spec
status: approved
owner: elijah
created: 2026-09-09
updated: 2026-09-09
sidebar_position: 26
tags: [engine, css, selectors, stylo, cascade]
spec_id: selector-matching
issue_id: CORE-155
applies_to: engine 0.x
dependencies: []
---

# Selector Matching — Structural Pseudo-Classes Across Text Nodes

## Overview

The engine hands its DOM to the `selectors` crate through the
`SelectorsElement` trait in `engine/src/stylo_dom.rs`. The crate drives every
structural selector through a small set of sibling and child accessors; if one
of them reports a non-element node, the crate's traversal stops and the
selector silently fails to match. This spec records the contract those
accessors must satisfy, and the defect that violated it.

## Goals

1. `:nth-of-type()`, `:nth-last-of-type()`, `:first-of-type`, `:last-of-type`,
   `:only-of-type`, and adjacent-sibling (`` + ``) selectors shall match the
   same elements whether or not text nodes (whitespace or otherwise) sit
   between the element siblings.
2. Sibling/child accessors shall expose element structure only — text and
   comment nodes shall not terminate a traversal.

## Non-Goals

1. Fully conformant `selectors` performance work (bloom filters, selector
   caching). The trait implementation is a thin DOM adapter.
2. Text-node-relative pseudo-classes (`:empty`, `::first-line`). Those read
   text content directly and are unaffected by sibling accessors.

## Behavior

1. **Element-sibling accessors skip non-elements.** `prev_sibling_element()`
   and `next_sibling_element()` shall return the nearest sibling ELEMENT in the
   requested direction, walking past any number of text or comment nodes, and
   `None` only when no element sibling exists. Returning a raw adjacent
   sibling that happens to be a text node is a contract violation.
2. **Type-counting is element-based.** `:nth-of-type(n)` counts only siblings
   with the same element name, so whitespace between them must not change the
   index at which the selector matches.
3. **First/last element children.** `first_element_child()` and
   `last_element_child()` shall likewise skip leading/trailing text nodes.

## Interfaces

- `engine/src/stylo_dom.rs`: `TyNode::sibling_element(delta)`, used by
  `impl SelectorsElement for TyElement`: `prev_sibling_element`,
  `next_sibling_element`.

## Acceptance Criteria

Each maps to a live test in `engine/tests/selectors.rs`, and to the WPT
print-reftest named where applicable.

- Given `<div>a</div>\n<div>b</div>` with `div:nth-of-type(2) { … }`, exactly
  the second div matches (`nth_of_type_matches_across_whitespace_text_nodes`;
  WPT `css-page/page-orientation-on-square-001`).
- Given the same markup with and without whitespace between the divs, the set
  of matched elements is identical
  (`nth_of_type_parity_with_and_without_whitespace`).
- Given three divs separated by whitespace and `div:last-of-type { … }`,
  exactly the third div matches (`last_of_type_matches_only_the_last_sibling`).

## Edge Cases

- Multiple consecutive text nodes between elements: the walk continues until an
  element is found or the sibling chain ends.
- Leading/trailing whitespace inside a parent: the first/last ELEMENT child is
  still found.
- A parent whose only children are text nodes: both accessors return `None`
  (no element sibling exists).

## References

- CSS Selectors Level 4 §4.3 (`:nth-of-type()`), §4.4 (`:last-of-type`),
  §5.1 (next-sibling combinator) — https://www.w3.org/TR/selectors-4/
- Servo's own `SelectorsElement` implementation for its DOM (the reference
  behavior this adapter mirrors).
- `selectors` crate 0.40 `matching.rs` — `:nth-of-type` traversal.
- Linear: CORE-155.
