---
title: "Page box growth from declared @page width/height — Chromium ground truth"
type: research
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 2
tags: [css-page, page-box, dimensions, chromium, probe]
---

# Page box growth from declared @page width/height

## Verdict

With no `size:` declaration, a declared `@page { width; height }` sets the
page box to the declared area plus the margins. An explicit `size:` wins.

## Measurements

Chromium via playwright, `prefer_css_page_size=True`, measured 2026-09-28
(CORE-258):

| Fixture | Declared | Page box |
| -- | -- | -- |
| dimensions-004 test | `margin: 4em 5em 8em 7em; width: 20em; height: 15em` | 384x324pt |
| dimensions-004 ref | `margin: 0; width: 32em; height: 27em` | 384x324pt |
| dimensions-005/006 pairs | same pattern | 384x324pt |

Check: (20+5+7)em = 32em = 384pt wide; (15+4+8)em = 27em = 324pt high.

## Why the older note was wrong

The harness oracle prints with `prefer_css_page_size=False`. That setting
ignores the stylesheet page size, so it reported "Chromium keeps the
requested size". It does not measure the CSS rule.

## Engine result

The engine grows the page box in `resolve_page_spec` (`engine/src/paged.rs`)
when no matching rule declares `size:`. Unit test:
`paged::tests::declared_area_without_size_grows_page_box`.
