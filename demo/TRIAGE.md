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
