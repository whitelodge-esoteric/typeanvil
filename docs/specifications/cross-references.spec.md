---
title: Cross-References — target-counter / target-text
slug: /specifications/cross-references
type: spec
status: in-review
owner: maintainers
created: 2026-09-03
updated: 2026-09-27
sidebar_position: 18
tags: [engine, css-gcpm, cross-references, toc, determinism]
spec_id: cross-references
applies_to: engine 0.x
dependencies: [paged-media-css, hyperlinks]
---

# Cross-References — target-counter / target-text

## Overview

Reports and papers reference their own sections: a TOC entry reads
"Foundations ........ 2", body text reads "see §3.2 on page 14". PrinceXML
expresses these with css-gcpm-3 generated content: `target-counter(url,
counter-name)` and `target-text(url)`. TypeAnvil already ships TOC leaders,
internal link annotations, and the `counter(pages)` two-pass loop;
`target-counter(attr(href), page)` rides the same
loop. This spec completes the pair: the counter-name argument (named document
counters and `pages`) and `target-text()`.

## Goals

- Parse the full argument grammar of both functions.
- Resolve `page`, `pages`, and named counters against the TARGET element's
  state, not the referencing element's.
- Keep the two-pass resolution bounded and deterministic.
- Prince-parity page numbers on the demo report TOC (±0 on the number).

## Non-Goals

- `target-counter(url, counter(style))` list/ordinal styles — decimal only.
- `target-text(..., first)` / `(..., before)` / `(..., after)` content
  selectors (css-gcpm-3 §7.1 variants) — full subtree text only.
- `target-counter` inside `@page` margin boxes (element `content` only;
  margin-box resolution renders these pieces as empty, unchanged).
- CSS `attr()` with namespaces or non-string types.
- `url(#id)` argument form — only `attr(name)` targets.

## Behavior

1. The engine **shall** parse `target-counter(<target>[, <counter-name>])`
   and `target-text(<target>)` in element `content` values, splitting
   arguments on the first comma (same rule as `string(name, keyword)`).
2. The target **shall** be read as `attr(<name>)` on the element carrying the
   `content`; a leading `#` in the attribute value is stripped and the first
   element (document order) with that `id` is the target.
3. `target-counter(..., page)` **shall** resolve to the 1-based page number
   the target element lands on, via the existing bounded multi-pass loop
   (hard cap 3; the same map used by link resolution).
4. `target-counter(..., pages)` **shall** resolve to the document's total
   page count from the previous pass (same value `counter(pages)` yields).
5. `target-counter(..., <name>)` for a named counter **shall** resolve to the
   target element's counter snapshot: the running value of `counter-reset` /
   `counter-increment` folded in document order up to and including the
   target, inheriting the nearest earlier snapshot carrying that counter,
   defaulting to 0 when no element ever declared it.
6. `target-text(...)` **shall** resolve to the target element's full text
   content, read synchronously from the DOM (no second pass needed).
7. A missing target (no matching id, missing attribute, or a target whose
   box is suppressed) **shall** resolve to `?` for `target-counter` and
   `target-text` alike — never a panic, never a bogus number.
8. Resolution **shall** be a pure function of the document: BTreeMap state,
   document-order walks, no clock, no environment reads (determinism rule,
   the determinism contract).
9. The leader-fill reservation **shall** keep treating resolved pieces at the
   0.5em per-character heuristic, so the two-pass TOC convergence property
   (paged-media-css spec §9–10) is preserved.

## Interfaces

- `paged::ContentPiece::TargetCounter { attr: String, counter: String }`
  (the `counter` field is new; `"page"` is the default).
- `paged::ContentPiece::TargetText { attr: String }`.
- `paged::parse_target_attr(args) -> Option<String>` — `attr(name)` splitter.
- `layout::paginate(...)` gains a `target_counters` parameter mirroring
  `target_pages`, and returns the per-pass
  `BTreeMap<NodeId, BTreeMap<String, i32>>` snapshot map.
- `layout::collect_counters(dom, root, styles, values, snapshots)` — the
  document-order fold.

## Acceptance Criteria

1. **TOC page numbers** — Given the demo report TOC with
   `content: ... target-counter(attr(href), page)`, when rendered, each entry
   ends with the target chapter's 1-based page number.
   Test: `engine/tests/paged_media.rs::target_counter_pages_resolves_target_page_number`
   (plus the pre-existing `toc_target_counter` / `report_demo`).
2. **Named counter** — Given `counter-increment: section` on chapter
   wrappers and `target-counter(attr(href), section)` in the TOC, each entry
   reads the TARGET's counter value (1, 2), not the entry element's own (0).
   Test: `target_counter_named_counter_reads_document_counter_state`.
3. **pages counter** — `target-counter(attr(href), pages)` equals the total
   page count. Test: `target_counter_pages_includes_total_pages_counter`.
4. **target-text** — `target-text(attr(href))` renders the target's text.
   Test: `target_text_resolves_target_element_text`.
5. **Missing targets** — both functions render `?` for unknown ids.
   Tests: `target_counter_missing_target_renders_question_mark`,
   `target_text_missing_target_renders_question_mark`.
6. **No regressions** — the full `cargo test` suite and the WPT
   css-page/css-break buckets show zero status flips against the
   pre-change binary.

## Edge Cases

- Target id exists but its box never renders (suppressed) → `?`.
- Counter declared only AFTER the target in document order → the target's
  snapshot has no value; nearest earlier snapshot wins; else `0`.
- `content` on an inline box never applies (pre-existing engine rule);
  cross-reference content behaves like any other generated content there.

## References

- css-gcpm-3 §7 (target-counter, target-text).
- Paged media spec (two-pass machinery): `docs/specifications/paged-media-css.spec.md`.
- Link annotations: `docs/specifications/hyperlinks.spec.md`.
