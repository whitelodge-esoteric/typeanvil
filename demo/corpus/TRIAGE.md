# Triage archive — historical record (frozen 2026-09-15)

This file records the demo-triage passes from 2026-08 to 2026-09. It is an
archive, and **every engine bug it reports has since been fixed**. Read it for
method and attribution evidence, not for current status.

## Where current state lives instead

- `demo/corpus/README.md` — the generated gallery: a scoreboard (engine commit,
  per-doc diff %, bucket) and the benchmark section, grouped `Open gaps` /
  `Closed — regression fixtures`.
- `demo/corpus/manifest.json` — per-fixture expectations, known limitations,
  and the reason each doc's number is what it is.
- Linear — the tracker. New triage findings become issues; nothing is appended
  to this file any more.

## The passes

| Pass | Date | Filed | State |
|---|---|---|---|
| CORE-79 — second triage, re-baseline post CORE-62/63/64/74/78 | 2026-08-19 | CORE-80 … CORE-86 | All landed |
| CORE-93 — float-showcase page-count divergence (TA 11 vs PR 8) | 2026-08-20 | CORE-95 (plus CORE-94 / CORE-53 scope) | Landed |
| CORE-100 — table backgrounds dropped when a border is present | 2026-08-20 | CORE-100 | Landed |
| CORE-110 — float-showcase + academic-paper residuals | 2026-08-20 | CORE-117, CORE-118 | Both landed |
| CORE-146 — corpus feature refresh + benchmark layer | 2026-09-07 | benchmark fixtures CORE-130/140/141/143 | All landed |
| CORE-209 — invoice-statement fixture, corpus diff drift | 2026-09-15 | CORE-209 (done), CORE-210, CORE-211 | Two still open |

Checked 2026-09-15: every issue id above except CORE-210 and CORE-211 carries a
landing commit on `release/2026.9`.

## Sections that specs cite — keep these anchors

- `§CORE-110` — minimal-probe evidence for margin collapsing, cited by
  `docs/specifications/margin-collapse.spec.md`.
- `§Driver 2` (inside the CORE-93 section) — the Prince-vs-TypeAnvil hyphenation
  packing probe, cited by `docs/specifications/hyphenation-line-break-parity.spec.md`.
- `§CORE-100` — the background pixel-scan technique and its root cause, cited by
  `docs/specifications/table-backgrounds.spec.md`.
- This file as a whole is the attribution-convention reference for
  `docs/specifications/wpt-conformance-harness.spec.md`.

---

> **Historical.** Every finding in this section has landed — see the status
> index at the top of this file.

# CORE-79 — Second Demo Triage: Re-baseline post CORE-62/63/64/74/78

Run: `bash scripts/build-demo.sh` at commit `eeafd44` (current main).
Prince 16.2 non-commercial. Geometry: 5in×3in, 0.5in margins, 96 DPI raster.
Scoreboard regenerated (numbers identical to the `f7d2501` commit — CORE-78's
table×multicol fix does not touch the corpus, which has no multicol).

## Scoreboard (current main, 2026-08-19)

| Doc | Overall diff | TA pages | PR pages | Bucket (revised) |
|---|---|---|---|---|
| Invoice | 30.0% | 4 | 5 | **engine-bug (bold/weight)** |
| Letterhead | 14.7% | 8 | 8 | **engine-bug (margin-box cover) + cosmetic** |
| Academic Paper | 27.5% | 9 | 9 | **engine-bug (italic) + line-break** |
| Prose Showcase | 28.5% | 9 | 8 | **engine-bug (bold) + line-break** |
| Quarterly Report | 17.7% | 10 | 10 | **engine-bug (bold) + cosmetic** |
| Table Stress | 33.5% | 20 | 45 | **engine-bug (bold) + table column-width** |

## Method

1. Rebuilt gallery on current main (venv python, pipeline determinism passes).
2. Built side-by-side montages for all 60 shared pages
   (`demo/scripts/make_montages.py`, TA|PR labeled).
3. Vision-triaged representative page pairs per doc; then **verified every
   visual hypothesis with hard PDF evidence** (embedded-font lists, text-layer
   extraction, char-box geometry, minimal repro renders through both engines).

## Findings — this triage REVERSES CORE-72's conclusion

CORE-72 said "zero engine bugs; diffs = font substitution" and filed CORE-73
(font pinning). **That was wrong.** After pinning, the scoreboard did not move
(28.8→30.0, 15.8→14.7, 34.0→33.5). Root cause: the engine never honored the
font stack's weight/style anyway.

### Confirmed engine bugs (each verified, not eyeballed)

1. **`font-weight` and `font-style` are ignored entirely.** `engine/src/pdf.rs`
   hardcodes `/System/Library/Fonts/Supplemental/Arial.ttf` (regular) as the
   only font; there is no bold/italic face selection anywhere in the render
   path (grep: no font-weight/font-style reads; `ComputedStyle` carries
   `font_family` only). Verification: minimal repro `<div class="bold">` /
   `.italic` — TA PDF embeds ONE font (`ArialMT`), Prince embeds three
   (`ArialMT`, `Arial-BoldMT`, `Arial-ItalicMT`); bold line width 257.5pt
   (Prince) vs no bold variant in TA. **Affects every doc**: every `h1`/`h2`,
   `thead th`, `.total-row`, `.brand`, `.sig .name`, `.abstract`, `blockquote`
   renders regular weight/upright. This is the single largest visible diff
   driver. Filed as **CORE-80**.
2. **Named-page margin-box suppression not honored.** Letterhead p1 (cover,
   `@page cover { @top-left { content: none } … }`) — TA renders "Northwind
   Systems www.northwind.example Page 1 of 0" in the margin; Prince renders
   none. Filed as **CORE-82**.
3. **Non-ASCII in margin-box content strings → mojibake.** `@top-center
   content: "… — 2026"` renders as "â□□" (text layer shows UTF-8 bytes
   `E2 80 94` decoded as Latin-1). Visible on table-stress and invoice
   headers. Filed as **CORE-83**.
4. **Body-text ToUnicode map is garbage.** Glyphs render correctly (vision
   read "Anvil, standard (150 lb)" fine) but the PDF text layer extracts as
   control chars (`\x01nventory`, `\x1co\x0fpany`). Copy/paste, search, and
   accessibility are broken in every TA PDF. Invisible in the raster gallery,
   but a real product defect. Filed as **CORE-85**.

### Structural divergence (engine algorithm, not a single bug)

5. **Auto table-layout column-width distribution differs.** Table Stress: TA
   fits 4 data rows/page (row pitch 14.8pt), Prince fits 1–3 (pitch 31.5pt —
   2.1×) because Prince gives the Description column less width so cells wrap
   to 2 lines and "On hand" wraps. This is the 20 vs 45 page blowup and the
   33.5%. Manifest already flags "columns scale from content, not fixed
   widths" as a known delta. Filed as **CORE-81**.
6. **Margin-box font default differs.** Prince renders margin-box text in
   TimesNewRomanPSMT (its default), TA in its single Arial. Corpus does not
   pin margin-box `font-family`. Cheap fix on the corpus side. Filed as
   **CORE-86** (Low, corpus).

### Known gaps, now ticketed

7. **counter(pages)** — "Page 1 of 0" on letterhead footer. Tracked in the
   manifest since CORE-70 but never filed. Filed as **CORE-84**.

### Not TypeAnvil defects (do not ticket)

- **Prince tfoot on table-stress p1**: Prince repeats the total row early
  (its own table-fragmentation behavior, as noted in CORE-72).
- **Prince watermark logo** (non-commercial license): constant small
  contributor on every Prince page.
- **Line-break/hyphenation positions** (paper, prose): different hyphenation
  dictionaries / breaking algorithms; expected per manifest `expected_deltas`,
  not a bug. Will converge only via algorithm parity work, deliberately out of
  scope for this triage.

## Revised diff-driver map (per doc)

- Invoice 30.0: bold (h1, thead, total-row) + table column-width + em-dash
- Letterhead 14.7: cover margin boxes + counter(pages) + bold (brand)
- Paper 27.5: italic (abstract, blockquote) + bold (h1/h2) + line-breaks
- Prose 28.5: bold (h1/h2) + line-breaks + hyphenation
- Report 17.7: bold (h1, thead, total-row) + line-breaks
- Table Stress 33.5: bold (thead, tfoot) + auto table column-width + em-dash

**Single highest-leverage fix: CORE-80 (font weight/style).** It touches every
doc and is the difference between "the demo shows bugs" and "the demo shows a
wedge". Expect a large jump in convergence from it alone; CORE-81 is the
second lever (page-count parity on tables).

## Follow-up tickets filed (all children of CORE-79)

- CORE-80 (High, engine): font-weight/font-style ignored — single hardcoded
  Arial regular face
- CORE-81 (High, engine): auto table-layout column-width distribution
- CORE-82 (Medium, engine): named-page margin-box `content: none` not honored
- CORE-83 (Medium, engine): non-ASCII in margin-box content strings → mojibake
- CORE-84 (Medium, engine): counter(pages) total-page counter unsupported
- CORE-85 (Medium, engine): body-text ToUnicode/text-extraction broken
- CORE-86 (Low, corpus): pin margin-box font-family in corpus fixtures

## Re-verification plan

After CORE-80 lands: rebuild gallery, expect letterhead/report to drop toward
single digits; table-stress stays high until CORE-81. Do NOT re-triage from
vision alone — check embedded-font lists and char-box geometry (scripts under
`/tmp` were one-shot; the durable technique is in the skill reference).

---

> **Historical.** Every finding in this section has landed — see the status
> index at the top of this file.

# CORE-93 — Third Demo Triage: Float fixture page-count divergence (TA 11 vs PR 8)

Run: `bash scripts/build-demo.sh --keep-work` at commit `983b2aa` (current
main, CORE-76 + CORE-90 + CORE-91 landed). Prince 16.2 non-commercial.
Geometry: 5in×3in, 0.5in margins, 96 DPI raster. Scoreboard regenerated at
`983b2aa`. **Update:** CORE-92 landed while this triage was in flight and
already fixed the body-margin half — scoreboard at `3be65c5` shows Float
Showcase 11→10 pages, matching the attribution below (remaining 10 vs 8 =
line-breaking parity, CORE-94 In Progress).

## Scoreboard (current main, 2026-08-20)

| Doc | Overall diff | TA pages | PR pages | Bucket |
|---|---|---|---|---|
| Float Showcase | 24.40% | 11 | 8 | **engine (UA margins) + algorithm (hyphenation)** |
| Invoice | 25.32% | 5 | 5 | cosmetic + engine (residual) |
| Letterhead | 13.18% | 8 | 8 | cosmetic |
| Academic Paper | 25.12% | 11 | 11 | cosmetic |
| Prose Showcase | 21.86% | 10 | 11 | engine (UA margins) + line-box |
| Quarterly Report | 16.70% | 10 | 10 | cosmetic |
| Table Stress | 29.11% | 42 | 45 | engine (table) — CORE-89 residual |

## Method

1. Rendered float-showcase through both engines (identical flag set), extracted
   per-line geometry + full text per page (pypdfium2 `get_text_bounded` +
   `search`-anchored charboxes; the rect-index API is unreliable — text is
   grouped per text object, not reading order).
2. Isolated hypotheses with minimal probes (no-float packing probe, body-margin
   probe, h1-margin probe, float-placement probe) — every hypothesis verified
   with hard evidence, not montage eyeballing.
3. Attributed page-count + line-count deltas per driver.

## Findings

### Driver 1 (engine bug): UA stylesheet is screen-defaults, not print

`engine/src/css.rs` `UA_CSS` (line ~389) applies the HTML screen defaults:
`body { margin: 8px }` (line 408), `h1 { margin: 0.67em 0 }` (line 400),
`p { margin: 1em 0 }` (line 406), etc. In print (Prince's UA sheet), the body
margin is zero and heading/paragraph margins are smaller.

Evidence:
- `body { margin: 8px }` = 6pt each side → TA content lines span
  x=[42, 314] (272pt wide) vs Prince x=[36, 324] (287pt). Same 288pt page
  content box, but TA's text block is inset ~8pt each side.
- Minimal probe (no floats, identical text): with `body { margin: 0 }` TA
  drops from 10 lines/2 pages to **9 lines/1 page** — matches Prince's 8
  lines/1 page (1 line residual = hyphenation).
- Float fixture: `body { margin: 0 }` TA drops 11→10 pages, 84→80 lines
  (Prince: 8 pages, 75 lines). The 6pt horizontal inset alone costs ~4 lines
  and 1 page.
- `h1 { margin: 0.67em 0 }` (13.4pt top + bottom at 20pt font) pushes
  unstyled headings down ~27pt. Float-placement probe: with `h1 { margin: 0 }`
  the short float fits on page 1 (matches Prince); without it, the float is
  deferred to page 2 because `y + fh > bottom_limit` by 3.6pt.

Effect: every corpus fixture's lines are narrower and headings taller than
Prince intends. Docs near a page boundary flip pages (float-showcase,
prose); docs with slack do not (paper, report, letterhead, invoice).

Filed as: **CORE-95** (engine: print-adapted UA stylesheet).

### Driver 2 (algorithm parity, not a bug): hyphenation density / line breaking

Same text, same font (both embed ArialMT/Arial-BoldMT/Arial-ItalicMT), same
14.0pt line pitch, same width — but Prince takes 9 hyphen breaks where TA
takes 2 (packing probe), fitting 12–14 words/line vs TA's 10–13. Total: PR 49
body lines vs TA 56 (margin-zeroed probe). HYPHEN_PENALTY = 135 (Typst/TeX
default); TA's K-P prefers a clean space break when it fits with modest
badness, Prince hyphenates eagerly. This is the CORE-53 typography-tuning
question, magnitude larger than the other fixtures at line-height 1.4.
**Cross-reference:** CORE-94 (In Progress, filed from CORE-90 close-out)
tracks the same line-breaking divergence at 1.6/2.0 and notes the direction
flips with line-height — the shared root is the justification glue model
(stretch/shrink tolerances), not hyphenation alone.

Effect: ~5 extra lines per fixture doc; 2 of the 3 float-showcase extra pages.

Recorded in manifest `expected_deltas` (already present: "Prince hyphenation
dictionary may differ"). Not filed as a bug — tracked in CORE-53 scope.

### NOT a float bug

Float placement itself is correct: `fits = y + fh <= bottom_limit`, suspend
when it doesn't fit, resume at next page top, text wraps beside it (verified:
float-placement probe with equalized margins renders 1 page in TA matching
Prince's 1 page, float on page 1 with wrapping). The figure lands on page 3
in TA vs page 2 in Prince purely because Drivers 1+2 make the preceding text
taller/longer — the float-placement cascade is a symptom, not a defect.
CORE-91 (glyph duplication) remains fixed.

## Attribution (float-showcase, 11 vs 8 pages)

| Driver | Pages | Lines | Type |
|---|---|---|---|
| UA body margin 8px (narrower lines) | 1 | 4 | engine bug (CORE-95) |
| UA h1/h2 margins (fixture authors these; probe-only for unstyled) | 0–1 | ~1 | engine bug (CORE-95) |
| Hyphenation density (PR hyphenates 4× more) | 2 | 5 | algorithm parity (CORE-53) |
| **Total** | **3** | **9** | |

## Follow-up tickets filed

- **CORE-95** (High, engine): print-adapted UA stylesheet — body margin 8px +
  heading margins must not apply in print (Prince parity). **Note: CORE-92
  already landed the body-margin half** (UA_CSS body margin 8px → 0,
  scoreboard at 3be65c5: Float Showcase 11→10, Letterhead 13.18→10.53, Prose
  21.86→19.70, Report 16.70→14.54). CORE-95's remaining scope: heading/other
  UA margins in print (h1 0.67em etc.) — probe-only effect on unstyled docs.
- Hyphenation-density parity tracked by **CORE-94** (In Progress, line
  breaking at high line-height; same root as this triage's driver 2).

## Re-verification plan

After CORE-95 lands: rebuild gallery, expect float-showcase to converge
toward PR 8 pages, prose toward 11, and every fixture's text block to span
the full 36→324 content box. Re-check with char-box geometry, not vision.

---

> **Historical.** Every finding in this section has landed — see the status
> index at the top of this file.

# CORE-100 — Fourth triage follow-up: table backgrounds dropped (2026-08-20)

Run at `5afe57d` (same build as the CORE-96–99 scoreboard, 2026-08-20).
Found while triaging the residual table diffs: invoice/table-stress/report
all render their colored table headers and total rows as WHITE in TypeAnvil.

## Finding: `background-color` on table cells vanishes when a border is present

- Invoice p1 header: TA has **0** `#a8dadc` pixels, Prince 4,016.
- Table-stress p2 header: TA 0, Prince 10,577; total-row band: TA 0,
  Prince 6,072 (`#457b9d`).
- Report p6 table band: TA 0 `#e9f0f5`, Prince 6,575.
- Minimal repros isolate the trigger: bg WITHOUT border renders fine
  (`/tmp/bgtest2.html` shows `#a8dadc` + `#ffdd88`); bg WITH border drops
  everything (`/tmp/bgtest.html` / `bgtest3.html` — borders only).
- Row-level `tr { background-color }` never paints (`#00ff00` absent).

**Root cause:** `engine/src/layout.rs` `layout_table_cell` (~1871) calls
`layout_box` (sets `FragmentContent::Background`, line 1278) then, when any
border width > 0, UNCONDITIONALLY overwrites `res.fragment.content` with
`FragmentContent::Border` (1902-1910). One fragment = one content kind
today; background is lost. Every corpus table uses `th, td { border: … }`
AND `background-color`, so all hit it.

## Follow-up ticket

- **CORE-100** (High, engine, codex): table cell/row backgrounds dropped
  when border present. Background + border must coexist (background under,
  border stroke on top — pdf.rs two-pass order already supports it); add
  row/group-level bg (`tr`/`thead`/`tfoot`).

## Verification technique: background pixel-scan

When a doc "looks flat" or a table band is missing color, count exact-color
pixels in a rasterized band instead of trusting vision (10pt color at 96
DPI is unreliable):

```python
from PIL import Image
import collections
img = Image.open('demo/out/images/<doc>/page-NNN-ta.png').convert('RGB')
px = img.load()
c = collections.Counter()
for y in range(y0, y1):
    for x in range(img.width):
        c[px[x, y]] += 1
print(c.most_common(4))  # target color present ⇒ background painted
```

Compare TA vs Prince on the same band; a target hex present in Prince and
absent in TA = the engine dropped the fill. Reusable for any
background/border/color-convergence question (see the skill's
`pdf-verification-techniques` reference for the sibling char-box/ToUnicode
techniques).

---

> **Historical.** Every finding in this section has landed — see the status
> index at the top of this file.

# CORE-110 — Fifth triage: float-showcase (22.31%) + academic-paper (22.22%) residuals

Run: `bash scripts/build-demo.sh --keep-work` at commit `9d22125` (current
main, CORE-111 landed). Prince 16.2 non-commercial. Geometry: 5in×3in,
0.5in margins, 96 DPI raster. Scoreboard regenerated at `9d22125`: Float
Showcase 22.31% (worst p5 33.1%), Academic Paper 22.22% (uniform 22–26%
across pp. 2–10), invoice 13.32, letterhead 9.25, prose 14.45, report 12.42,
table-stress 25.14. All seven page counts match.

Method: char-box line clustering (`pypdfium2` get_charbox, baseline window
±2.5pt) on every shared page pair; minimal repro probes through both engines;
exact-color pixel scan of the pull-quote band. Vision confirmation was
attempted at 216 DPI but skipped (vision backend 502s during the session);
every finding below rests on char-box or pixel evidence.

## Attribution table

| Driver | Bucket | Affects | Evidence |
|---|---|---|---|
| D1. `@top-center`/`@bottom-center` clamps to start-align when text is wider than the middle-third slot | **engine bug → CORE-117** | paper, float-showcase, prose, invoice, table-stress (5 of 7 docs) | layout.rs `horizontal_slot` gives center boxes a 96pt slot (content 288pt / 3) and layout.rs ~3847 centers via `(slot_w − text_w).max(0)` — zero offset for wide heads, so they render left-aligned at the third-slot origin. Measured centers: paper TA 200.6 vs PR 180.2; float 206.7/147.7w vs 180.2; invoice 206.1/148.1w vs 179.0; prose 231.7/249.7w vs 180.0 (TA overflows past the right content edge, x=330.7 > 324); table-stress 212.4/159.4w vs 180.1. Prince centers on the true midline regardless of width. |
| D2. Adjacent vertical margins sum instead of collapsing to the max (CSS 2.1 §8.3.1) | **engine bug → CORE-118** | paper (blockquote +6pt, h2 boundaries +7pt), float-showcase, prose, report — any doc with styled sibling blocks | Minimal probe (`collapse.html`, p{mb:7pt} + blockquote{mt:6pt}): TA p→quote baseline gap 28.10pt vs Prince 22.11pt (= max(7,6)+line remainder). Engine code path: layout.rs ~2197 advances the parent cursor by `box_height + margin_bottom`; the next box then adds its own full `margin_top` (~1302–1315). No `max()` collapse exists. In-corpus: paper p4 quote block sits 6.0pt higher in TA (y=66.07 vs 72.03) with identical x-span. |
| D3. Line-break choice divergence (Prince hyphenates more eagerly; TA prefers space breaks) | **algorithm parity — accepted (CORE-53/CORE-94 scope)** | every body-text page | Paper p4: identical pitch (15.00 vs 15.03 median), identical x-span, same line count through line 6, then TA breaks one word earlier per line and Prince hyphenates ("end‑" at the measure). Paper p1 is GLYPH-IDENTICAL between engines except D1's head shift — proving font metrics are converged; what remains is choice, not measurement. |
| D4. Cross-page continuation-line count differs by one (consequence of D3) | **consequence of D3** | float-showcase mid pages | FS p5/p6: Prince carries one extra wrapped line onto the page ("— or when the page runs out." at y=169.5), shifting the whole body block up ~14–21pt for the rest of the page. Same words, same styling — pure vertical cascade. |
| D5. Pull-quote styling/margins diverge (CORE-110 starting hypothesis) | **disproven — pull-quote renders correctly** | float-showcase | Exact-color pixel scan of the left-half float band on FS p4: TA contains the #333 gray (104 px) and anti-aliased ramp matching Prince's (143 px); italics present both sides. CORE-101 float fragmentation machinery places and styles the pull quote correctly; the mid-page 33% diffs are D1+D2+D4. |

### Hypothesis from the ticket: table-stress-style x-shift — RULED OUT

Body text x-spans match between engines within 0.5pt on every sampled page of
both docs ([36, 323.x] both sides, paper and float-showcase alike). The only
horizontal divergence is D1's margin-box band. The positional-shift family
does NOT explain these residuals.

## Acceptance criteria for accepted residuals

D3/D4 are accepted until the glue-model parity work (CORE-53 scope, tracked
by CORE-94 for line-height interaction) changes the breaker's tie-breaking:

- Per-page body line count within ±1 of Prince (currently met on 16 of 19
  shared pages across the two docs).
- Baseline pitch within 0.15pt (met: ≤0.11 observed).
- Glyph positions identical when the wrap choice matches (proven by paper p1).
- Accepted residual target after CORE-117+CORE-118 land: both docs ≤15%
  overall (the head band + collapse shifts are worth roughly half the
  measured diff by pixel share; re-measure after those fixes).

## Follow-up tickets filed

- **CORE-117** (engine): margin-box center alignment clamps to start-align
  for text wider than the third-slot — center must be the true page midline
  (Prince-verified), overflow allowed into adjacent slots.
- **CORE-118** (engine): vertical margin collapsing between sibling blocks —
  adjacent margins must collapse to max(), not sum; adjoinence rules for
  empty boxes/padding can stay out of scope if documented.

## Re-verification plan

After CORE-117 and CORE-118 land: rebuild gallery; expect the running-head
band to go dark-diff on five docs and blockquote/h2-following pages to drop
several points. Then re-triage whatever remains of the two headline docs with
char-box evidence only.



> **Historical.** Every finding in this section has landed — see the status
> index at the top of this file.

# CORE-146 — Sixth refresh: corpus features landed post-CORE-79 + benchmark layer (2026-09-07)

Run: `PY=$HOME/workspace/typeanvil/.venv/bin/python bash scripts/build-demo.sh`
at the core-146 worktree (base 26cc06f). Prince 16.2 non-commercial.
Geometry: 5in×3in, 0.5in margins, 96 DPI raster.

## What changed in the corpus

- `paper.html`: real footnotes (`float: footnote`, CORE-107) replace the
  old "footnotes unsupported" note. Two calls in the Results chapter.
- `report.html`: chart image via `<img>` (CORE-106) + explicit hyperlinks
  (internal anchors + one external URI, CORE-104) in the Findings chapter.
- `letterhead.html`: `@font-face` (CORE-103) with the OFL Liberation Serif
  cut committed under `demo/corpus/assets/` (license file included).
- SVG stays OUT of the public corpus for now: CORE-131 is only on
  `release/2026.9`, not main. The issue's rule is "feat: commits in main".
- `demo/corpus/assets/` now carries committed fixture assets; build-demo.sh
  renders from the corpus dir so relative URLs resolve identically in both
  engines (on main, url()/src resolve against the process CWD — CORE-140
  threads --base-url properly).

## Scoreboard after refresh (7 docs)

| Doc | TA | PR | Diff% | Note |
|---|---|---|---|---|
| float-showcase | 8 | 8 | 20.88 | unchanged fixture |
| invoice | 5 | 5 | 19.17 | unchanged fixture |
| letterhead | 8 | 8 | 7.86 | @font-face brand line added |
| paper | 11 | 11 | 14.36 | footnotes added |
| prose | 11 | 11 | 10.52 | unchanged fixture |
| report | 11 | 11 | 7.51 | chart + links added |
| table-stress | 43 | 45 | 20.94 | unchanged fixture (known mismatch) |

Page counts match Prince on all six non-table docs; table-stress keeps its
known 43v45 gap (CORE-96 residual). No regressions: unchanged fixtures moved
≤0.11pp; refreshed docs landed at 14.36% (paper) and 7.51% (report) — both
inside the cosmetic bucket, no structural divergence introduced by the new
features.

## New residual: footnote call-marker attribution in floated-wrap runs

While refreshing paper.html, a REAL footnote bug surfaced (found by the
fixture, verified by charbox/text extraction, fixed in the same PR):

- Symptom: a note whose call marker sits in a paragraph that wraps BESIDE a
  float (the segmented text path) never attached to its call page; a later
  note attached to the wrong page. Two wrongs made a single-call case pass,
  masking the bug until a second call in one paragraph exposed it.
- Root cause: in the segmented path the marker window was computed as
  `src_offset + Σconsumed[..li]`, but `src_offset` already advanced by those
  same lines — the window start drifted to exactly 2× the true byte offset,
  so markers after the segment's first line never fell inside a window.
- Fix: capture the segment's base offset (`seg_base`) before the line loop
  and compute windows from it. Engine suite 215 passed / 0 failed.
- Follow-up candidates: none required; regression covered by the spanning-
  paragraph probe promoted to `engine/tests/core146_probe.rs` (renamed to
  `footnotes_spanning.rs` before landing).

## Benchmark layer (new, CORE-146)

`demo/corpus/bench/` + `benchmark_manifest.json`: one fixture per open
engine issue (CORE-143 monolithic overflow, CORE-141 margin-box per-box
decls, CORE-140 page-context vh/vw, CORE-130 page floats). All four
verified safe-to-render through BOTH engines (TA and Prince terminate, 1-2
pages each). The gallery shows them in a separate "Benchmark" section with
the current TA render, the tracked issue, and the expectation. Fixtures
flip `pending` → `resolved` when their issue lands.


> **Current.** The fixture work in this section landed; the diff drift it
> records is still open as CORE-211.

# CORE-209 — Seventh refresh: invoice-statement fixture; corpus diff drift recorded (2026-09-15)

Run: `PY=$HOME/workspace/typeanvil/.venv/bin/python bash scripts/build-demo.sh` in the
release worktree (`release/2026.9`, tip `702e427`). Prince 16.2 non-commercial.
Geometry: 5in×3in, 0.5in margins, 96 DPI raster. Corpus: 8 docs (one added).

**Build note (CORE-168):** host cargo builds are banned, so the engine binary lives
in the Docker volume `dev-target-release`. Prince exists only on the host. The TypeAnvil
side was therefore rendered by the container-built binary through a container-backed
shim at `engine/target/debug/typeanvil` (path-mapped to `/work`), and the host ran the
Prince side, rasterization, and diff. Both pipelines otherwise ran unmodified.

## What changed

- **New fixture** `invoice-statement.html` — business document: line items table
  (22 rows) then a running-balance statement of account, each table repeating its own
  header row per page, plus a `@top-center` running header and accented glyphs
  (`Zürich`, `Säntis`) that exercise the ToUnicode path.
- **First comparison build since 2026-09-07** (CORE-146). Roughly 40 engine commits
  landed in between, so every pre-existing doc's diff moved. See the drift finding below.

## Scoreboard at `702e427` (8 docs)

| Doc | TA | PR | Diff% | vs CORE-146 |
|---|---|---|---|---|
| float-showcase | 8 | 8 | 23.15 | +2.27 |
| **invoice-statement (new)** | **5** | **5** | **27.69** | — |
| invoice | 5 | 5 | 17.12 | −2.05 |
| letterhead | 8 | 8 | 12.26 | +4.40 |
| paper | 12 | 11 | 23.52 | +9.16 (page count now mismatches) |
| prose | 11 | 11 | 20.40 | +9.88 |
| report | 11 | 11 | 14.29 | +6.78 |
| table-stress | 47 | 45 | 22.21 | +1.27 (43→47 vs PR 45) |

## New fixture triage: `invoice-statement.html` — bucket **cosmetic**

Evidence, all from the rasterized pair plus PDF inspection (not vision):

- Page counts match (5 = 5); no fragmentation divergence despite two fragmenting tables.
- Per-page diffs 24.8 / 25.9 / 24.6 / 29.7 / 33.5 — uniform, no single structural page.
- Embedded faces: TypeAnvil `ArialMT` + `Arial-BoldMT`; Prince the same plus
  `CourierNewPSMT`. The fixture's `.mono` date column therefore resolves to a monospace
  face in Prince and to the sans face in TypeAnvil (no monospace face available) — the
  dominant substitution driver for this doc.
- Table header fill `#a8dadc` paints on both sides (TA 4,840 px vs PR 4,700 px on p1), so
  CORE-100's fix holds.
- Text lines per page: TA `[41, 42, 41, 42, 44]` vs PR `[40, 38, 41, 41, 50]` — within
  ±2 on pages 1–4; Prince fits 6 more lines on the final page (the terms paragraph wraps
  differently). Content present and placed on both sides.

No engine bug was filed from this fixture. Two previously-filed engine bugs are confirmed
**fixed** by this build: CORE-80 (`font-weight`/`font-style` ignored — `Arial-BoldMT` is
now embedded) and CORE-100 (table cell background dropped when a border is present).

## Open finding: corpus diff drift is unattributed — do NOT read it as a regression

Six of seven pre-existing docs moved **away** from Prince versus the 2026-09-07 baseline
(+2.3 to +9.9 pp), and `paper` now mismatches page count (12 vs 11) where it previously
matched at 11 vs 11. `table-stress` also worsened slightly (43 → 47 pages against Prince's
unchanged 45).

This refresh does not attribute that drift, and nobody should infer a regression from the
numbers alone. The ~40 landings since `cb98dc5` include changes that deliberately diverge
from Prince because the CSS specification wins over accidental parity (standing ruling,
CORE-140), so part of the drift may be intended. Others may be real regressions.

Required before any conclusion: a dedicated triage with char-box geometry per doc,
comparing each side at the OLD baseline commit against the new one (the CORE-185 lesson —
measure both sides at baseline AND candidate; a movement can be an exposed gap rather than
a regression). Until then the drift stays recorded here as an open item.
