---
title: "Named-page break boundaries — Chromium oracle study"
type: research
status: approved
owner: maintainers
created: 2026-09-09
updated: 2026-09-09
sidebar_position: 1
tags: [css-page, named-pages, fragmentation, chromium, probe]
---

# Named-page break boundaries — Chromium oracle study

## Verdict

| Test | Status | Conclusion |
|---|---|---|
| `page-name-002` | **Fixable** — real engine defect | Chromium renders its test and reference identically (8 pages, exact). The engine runs 3 pages. One model extension is required. |
| `page-name-003` | **Not worth fixing** — irreducible suite contradiction | It and `page-name-abspos-002` are structurally identical tests with opposite references. No engine can pass both. |

The engine currently passes `page-name-abspos-002` and fails `page-name-003`.
Chromium does the reverse. Both outcomes score the same, so the swap buys no
conformance.

## Method

Nine minimal probe documents. Each was rendered twice:

1. Through the engine — `typeanvil render <probe.html> --page-width 5in
   --page-height 3in --margin-* 0.5in`.
2. Through Chromium — Playwright `page.pdf()` with the harness's exact settings
   (`width=5in`, `height=3in`, `margin=0.5in`, `print_background=True`,
   `prefer_css_page_size=False`, `--font-render-hinting=none`).

Evidence is the page count plus the extracted text of every page
(pypdfium2 `get_text_bounded`). The four disputed WPT fixtures were then run
through the harness on both legs.

Page geometry matters: the harness renders every fixture at 5x3in with 0.5in
margins, whatever the fixture asks for.

## Probe results

A = `<div style="page:a">`, B = `<div style="page:b">`, X/Y = bare text runs.

| Probe | Body markup | Engine | Chromium |
|---|---|---|---|
| P1 | `A` `B` | 2 (`A ⋮ B`) | 2 (`A ⋮ B`) |
| P2 | `A` `B` `A` | 3 (`A ⋮ B ⋮ C`) | 3 (`A ⋮ B ⋮ C`) |
| P3 | `A` X `A` | 1 (`AXC`) | **3** (`A ⋮ X ⋮ C`) |
| P4 | `A` X | 1 (`AX`) | **2** (`A ⋮ X`) |
| P5 | X `A` | 1 (`XA`) | **2** (`X ⋮ A`) |
| P6 | `A` X `B` | 1 (`AXB`) | **3** (`A ⋮ X ⋮ B`) |
| P7 | `A`(`B` text) | 1 (`B text`) | **2** (`B ⋮ text`) |
| P8 | `A`(`B` text) `A` | 1 | **2** (`B ⋮ text C`) |
| P9 | `A` X `B` Y | 1 (`AXBY`) | **4** (`A ⋮ X ⋮ B ⋮ Y`) |

The engine and Chromium agree on exactly the two probes whose boundaries are
block-to-block (P1, P2).

## The rule Chromium implements

A page-context change forces a page break at **every** boundary between
adjacent in-flow content, including boundaries adjacent to a bare text run. A
bare text run takes the page context of its containing block. Bare text
directly inside `body` takes the default page.

P3 demonstrates the consequence: two blocks that both declare `page:a` sit on
separate pages when a body-level text run separates them, because the sequence
of contexts is a → default → a.

## The rule the engine implements

The engine breaks only between two adjacent **block-level elements**. Any
boundary that involves a bare text run never breaks. The page-change comparison
treats a bare text run as contextless and stops there.

This narrower rule is why P3 through P9 all collapse to one page.

## page-name-002

The test interleaves page-declaring blocks with bare text runs. The reference
expects 8 pages.

Engine output, 3 pages:

| Page | Content |
|---|---|
| 1 | 1st page |
| 2 | 2nd page · 3rd page · Also 3rd page · 4th page · 5th page |
| 3 | 6th page · 7th page · Also 7th page · 8th page |

Chromium output: 8 pages, one item per page, matching the reference exactly
(`max_difference=0` on all 8 pages).

The two breaks the engine does fire are the block-to-block ones (1st→2nd and
5th→6th). The five missing breaks are all adjacent to bare text: 2nd→3rd
(b→a across bare text inside the wrapper), Also 3rd→4th (a→default),
4th→5th (default→a), 6th→7th (b→a), Also 7th→8th (a→default).

**Required change.** Bare text runs must carry their containing block's
effective page context, act as a page-boundary position, and take part in the
page-change comparison. The comparison must no longer stop at a bare text run.
This lives in the in-flow item loop of `engine/src/layout.rs`, in the same code
that two earlier landed slices already modified.

**Risk.** An earlier attempt recorded a 38-test regression when breaks were forced on
page-name changes, so this area is historically hazardous. Several `page-name-*`
tests currently pass under the narrower rule and may flip in either direction.
Gate the change on the full 283-test suite and expect to accept some flips as
corrections rather than treat every flip as a regression.

## page-name-003 and page-name-abspos-002 contradict each other

The two test bodies are the same shape. Both put an absolutely-positioned
wrapper around two children that declare `page:a` and `page:b`.

| | Test body | Reference | Chromium on the test |
|---|---|---|---|
| `page-name-003` | abspos wrapper, children `page:a` and `page:b` | 2 pages (`1st page`, then a `break-before:page` div with `2nd page`) | 2 pages → **PASS** |
| `page-name-abspos-002` | abspos wrapper, children `page:a` and `page:b` | 1 page (abspos wrapper holding two plain divs, no page declarations) | 2 pages vs 1-page reference → **FAIL** |

One reference requires a break between the `a` child and the `b` child inside
the abspos wrapper. The other reference requires no break for the same markup.
The suite therefore contains an unsatisfiable pair, and an engine must choose a
side.

The engine chooses the no-break side (abspos subtrees suppress page-change
breaks), which matches `page-name-abspos-002` and fails `page-name-003`.
Chromium chooses the break side. Recommendation: keep the current side, record
the contradiction, and take no action on `page-name-003`.

## Correction to a recorded Non-Goal

`docs/specifications/named-pages.spec.md` Non-Goal 1 treats `fixedpos-010` as
evidence that bare-text breaks must not fire, and calls the pair
(`fixedpos-010`, `page-name-002`) contradictory. That framing does not survive
probe.

`fixedpos-010` declares its own page sizes (`@page { size: 400px }` and
`@page large { size: 500px 400px }`) while the harness forces 5x3in. The
fixture cannot be judged at harness geometry, and Chromium fails it there for a
pixel difference rather than a page-count difference. Its reference is also
consistent with the full model: the trailing bare text after the `page:large`
block needs a large→default break to reach page 4, which is what the reference
shows.

With that confound removed, `page-name-002` has no clean contradictory partner.
The bare-text boundary rule can be implemented without contradicting any
fixture that has browser ground truth.

## Reproduction

Probe generation and both render legs:

```bash
# engine leg (per probe)
typeanvil render probes/P4_a_then_text.html \
  --page-width 5in --page-height 3in \
  --margin-top 0.5in --margin-right 0.5in \
  --margin-bottom 0.5in --margin-left 0.5in -o out.pdf

# Chromium leg
python3 - <<'PY'
from playwright.sync_api import sync_playwright
with sync_playwright() as pw:
    b = pw.chromium.launch(args=["--font-render-hinting=none"])
    pg = b.new_context().new_page()
    pg.goto("file:///abs/path/probe.html")
    open("out.pdf","wb").write(pg.pdf(width="5in", height="3in",
        margin={"top":"0.5in","right":"0.5in","bottom":"0.5in","left":"0.5in"},
        print_background=True, prefer_css_page_size=False))
    b.close()
PY
```

Both legs of every disputed fixture, with the harness:

```bash
.venv/bin/python -m harness --wpt <checkout>/.wpt run \
  --engine chromium --filter page-name-002 \
  --report /tmp/cr-002.json --db /tmp/h.sqlite --artifacts /tmp/art-cr

.venv/bin/python -m harness --wpt <checkout>/.wpt run \
  --engine cli --cli-cmd "<worktree>/engine/target/debug/typeanvil render" \
  --filter page-name-002 \
  --report /tmp/cli-002.json --db /tmp/h2.sqlite --artifacts /tmp/art-cli
```

## References

- [CSS Paged Media Module Level 3](https://www.w3.org/TR/css-page-3/), §8.1, named-page value propagation and forced breaks.
- [WPT print reftests](https://web-platform-tests.org/writing-tests/print-reftests.html), default geometry and page-by-page comparison.
- [WPT css-page directory](https://github.com/web-platform-tests/wpt/tree/master/css/css-page), public fixture corpus.
- `docs/specifications/named-pages.spec.md` — implementation contract.
- `docs/specifications/paged-media-css.spec.md` — paged-media scope and limitations.
