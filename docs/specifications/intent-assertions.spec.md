---
title: Intent Assertions — Layout Goals With a Pass/Fail Verdict
slug: /specifications/intent-assertions
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 46
tags: [cli, diagnostics, layout, ai-native]
spec_id: intent-assertions
issue_id: CORE-573
applies_to:
  - engine/src/assertions.rs
  - engine/src/css.rs
  - engine/src/main.rs
dependencies:
  - render-report
  - diagnostics
---

# Intent Assertions — Layout Goals With a Pass/Fail Verdict

## Overview

An agent can state a goal ("this fits on 2 pages", "the totals table is not
split") but it cannot easily check a PDF for it. `--assert <file.json>` takes
a list of layout goals in a small, closed vocabulary. After the render, the
engine evaluates each goal against the final layout and reports a verdict per
goal with evidence (page, rect, source line and column, element selector).
The verdicts are added to the `--diagnostics` JSON document. A separate
opt-in flag, `--fail-on-assert`, makes the process exit with status 3 when any
goal fails.

Assertions read the same layout facts as the render report
(`render-report.spec.md`). They never change the layout or the PDF.

## Goals

1. A closed vocabulary of eight assertion kinds, each with a defined pass
   condition.
2. One verdict per assertion, in input order, with evidence that points at
   source.
3. Callers that do not pass the new flags see no change: same stdout, same
   exit status, same PDF bytes.
4. Determinism: identical input and assertions give a byte-identical result.

## Non-Goals

- **Auto-fit remedies** (for example shrink-to-fit to meet `max-pages`).
  DECIDED 2026-10-25: v1 reports only. A remedy changes the document, so it
  needs its own spec and its own determinism rules. Follow-up work.
- Assertions on text content, colors, or computed style values.
- The MCP `validate` tool mapping. `mcp-tool-contract.spec.md` defines its own
  kinds; a server maps them onto this engine surface.
- Assertions embedded in the HTML document (for example a `<script>` block).
  The file input is the only input in v1.

## Behavior

### Input

1. `typeanvil render` SHALL accept `--assert <path>`. The file SHALL hold a
   JSON array of objects. Each object SHALL have a `kind` string and MAY have
   an `id` string, which is echoed in the result.
2. The kinds and their fields:

   | `kind` | Fields | Passes when |
   |---|---|---|
   | `max-pages` | `value` (integer ≥ 0) | final page count ≤ `value` |
   | `exact-pages` | `value` (integer ≥ 0) | final page count = `value` |
   | `no-overflow` | — | the render report has no `layout-overflow` event |
   | `no-empty-pages` | — | the render report has no `empty-page` event |
   | `keep-together` | `selector` | every matched element has boxes on exactly one page |
   | `on-page` | `selector`, `page` (integer ≥ 1) | every matched element's first box is on `page` |
   | `outline-covers` | `selector` | every matched element has an entry in the PDF outline |
   | `fonts-resolved` | — | the render report has no `font-fallback` event |

3. The engine SHALL reject, before rendering, a file that is not valid JSON,
   is not an array, holds an unknown `kind`, lacks a required field, has a
   field of the wrong type, or has an unknown field. The rejection SHALL be a
   usage error: a message on stderr and exit status 2. No PDF is written.
4. A `selector` SHALL be parsed by stylo's selector parser (the same parser
   the cascade uses). A selector list (`h1, h2`) matches the union of its
   members. Selectors with a pseudo-element are ignored. A selector that does
   not parse, or that matches no element, SHALL make that assertion FAIL with
   a detail that says so. An assertion about nothing is not a passing goal.

### Evaluation

5. Assertions SHALL be evaluated after the PDF is written, from the final
   `Layout`, the DOM, and the render report (`render_report::analyze_layout`).
6. `keep-together`: an element "has boxes on a page" when a fragment whose
   `source` is that element exists in the page's fragment tree. Fixed-position
   boxes that repeat on every page count as split.
7. `on-page`: an element with no box fails, with the detail `no box`.
8. `outline-covers`: an element is in the outline when its computed
   `bookmark-level` is not `none` and it produced a box. This is the rule
   `build_headings` uses to emit outline entries (`pdf-outlines.spec.md`).
9. Evidence SHALL be a list of objects with `page`, `rect`, `line`, `column`,
   and `element`, in the field meanings of `render-report.spec.md` §3. For
   the event-backed kinds the evidence is the matching events. For the
   selector kinds the evidence is the failing elements, one object per page
   for a split element. A passing assertion carries an empty evidence list.
   `max-pages` and `exact-pages` carry no evidence; their detail names the
   page count.

### Output

10. With `--assert`, the diagnostics JSON document (schema 2) SHALL gain one
    top-level key, `assertions`, placed after `counts` and before `summary`:

    ```json
    "assertions": {"passed": 1, "failed": 1, "results": [
      {"index": 0, "id": "fit", "kind": "max-pages", "passed": true, "detail": "3 pages (max 3)", "evidence": []},
      {"index": 1, "kind": "keep-together", "passed": false, "detail": "1 of 1 matched elements split across pages", "evidence": [
        {"page": 1, "rect": [36, 100, 288, 80], "line": 9, "column": 1, "element": "table.totals"},
        {"page": 2, "rect": [36, 36, 288, 40], "line": 9, "column": 1, "element": "table.totals"}]}
    ]},
    ```

    `id` is present only when the input had one. Without `--assert` the
    document SHALL be byte-identical to the render report document. The
    schema number stays `2`: the key is additive and optional.
11. `--assert` without `--diagnostics` SHALL behave as `--diagnostics json`.
12. `--diagnostics text` SHALL print one stderr line per assertion after the
    other events: `assert[INDEX] PASS|FAIL KIND: DETAIL`.

### Exit status

13. Without `--fail-on-assert` the exit status SHALL NOT depend on
    assertion results (0 on a successful render).
14. With `--fail-on-assert`, when one or more assertions fail, the engine
    SHALL still write the PDF and the diagnostics, print
    `typeanvil: N assertion(s) failed` on stderr, and exit with status 3.
    When all pass, it exits 0.
15. `--fail-on-assert` without `--assert` SHALL be a usage error (exit 2).

| Status | Meaning |
|---|---|
| 0 | Render succeeded (and, with `--fail-on-assert`, every assertion passed) |
| 1 | Render error |
| 2 | Usage error, including an invalid assertions file |
| 3 | `--fail-on-assert`: at least one assertion failed |

## Interfaces

```rust
// engine/src/assertions.rs
pub enum Kind { MaxPages(usize), ExactPages(usize), NoOverflow, NoEmptyPages,
                KeepTogether(String), OnPage(String, usize),
                OutlineCovers(String), FontsResolved }
pub struct Assertion { pub id: Option<String>, pub kind: Kind }
pub struct Evidence { pub page: usize, pub rect: Option<[f64; 4]>,
                      pub line: u32, pub column: u32, pub element: String }
pub struct Outcome { pub index: usize, pub id: Option<String>,
                     pub kind: &'static str, pub passed: bool,
                     pub detail: String, pub evidence: Vec<Evidence> }
pub fn parse(json: &str) -> Result<Vec<Assertion>, String>;
pub fn evaluate(dom: &Dom, stylesheet: &Stylesheet, layout: &Layout,
                report: &RenderReport, list: &[Assertion]) -> Vec<Outcome>;
pub fn to_json_block(outcomes: &[Outcome]) -> String;

// engine/src/css.rs
pub fn select(dom: &Dom, selector: &str) -> Option<Vec<NodeId>>; // None = invalid
```

## Acceptance Criteria

Tests live in `engine/tests/it/assertions.rs`.

1. Each of the eight kinds has a fixture where it passes and one where it
   fails, and the failing result carries the expected evidence
   (`max_pages`, `exact_pages`, `no_overflow`, `no_empty_pages`,
   `keep_together`, `on_page`, `outline_covers`, `fonts_resolved`).
2. An invalid or unmatched selector fails the assertion (`selector_edge_cases`).
3. An unknown kind, a missing field, and malformed JSON are rejected with
   exit status 2 and no PDF (`cli_rejects_bad_file`).
4. Exit status: failing assertions exit 0 without `--fail-on-assert` and 3
   with it; passing assertions exit 0 with it; `--fail-on-assert` alone exits
   2 (`cli_exit_codes`).
5. Two CLI runs give byte-identical stdout, and a run without `--assert`
   prints no `assertions` key (`cli_output_is_deterministic`).

## Edge Cases

- An empty array is valid: `"assertions": {"passed": 0, "failed": 0, "results": []}`.
- `exact-pages: 0` can never pass; the engine always emits at least one page.
- A `display: none` element matched by `keep-together` has no box on any
  page. It is not split, so it passes. The same element fails `on-page`.
- Repeated table header rows are their own elements; asserting
  `keep-together` on a `thead` that repeats fails, which is correct.

## References

- `render-report.spec.md` — layout events and evidence fields.
- `pdf-outlines.spec.md` — which elements get outline entries.
- `mcp-tool-contract.spec.md` — the `validate` tool that consumes this.
- CORE-570 (AI-native epic), CORE-573.
