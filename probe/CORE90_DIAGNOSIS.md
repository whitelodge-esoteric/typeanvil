# CORE-90 — Line-box divergence: diagnosis (probe evidence)

## What the probes proved

Rendered the same fixtures at demo geometry (5in×3in, 0.5in margins → 288×144pt
content) through both engines, measuring baselines from pypdfium2 charboxes.

### 1. Line-box HEIGHTS already match (probes v2, v4)

Single-line "Aq" paragraphs stacked with zero margins; consecutive baseline
deltas = line box height. Identical in both engines at every font-size × factor:

| font | factor | TA Δbase | Pr Δbase |
|------|--------|----------|----------|
| 10pt | 1.0    | 9.996    | 10.0     |
| 10pt | 1.2    | 11.995   | 12.0     |
| 10pt | 1.6    | 15.994   | 16.0     |
| 10pt | 2.0    | 19.992   | 20.0     |
| 15pt | 1.2    | 18.0     | 18.0     |

So the ticket's "TypeAnvil multiplies font-size by the factor directly without
font-metric leading" is FALSE for the box height — both engines produce
`factor × font_size` boxes. Pagination (box-based) is unaffected by the fix.

### 2. The divergence: FIRST-BASELINE PLACEMENT inside the box (probe v8)

One-line block, no preceding content, content top = 36pt:

| fs  | factor | TA first baseline | Prince first baseline |
|-----|--------|-------------------|-----------------------|
| 10  | 1.0    | 46.0  (36+10)     | 44.47                 |
| 10  | 1.2    | 46.0              | 45.47                 |
| 10  | 1.6    | 46.0              | 47.47                 |
| 10  | 2.0    | 46.0              | 49.47                 |
| 15  | 1.0    | 51.0  (36+15)     | 48.7                  |
| 15  | 1.2    | 51.0              | 50.2                  |
| 15  | 1.6    | 51.0              | 53.2                  |
| 15  | 2.0    | 51.0              | 56.2                  |

TA: baseline = box_top + font_size, constant. Prince: baseline = box_top +
`ascent + (line_height − ascent − descent)/2`, slopes up with factor.

### 3. The formula — CSS2.1 §10.8.1, exact to 0.001pt

Arial hhea: ascender 1854/2048 = 0.9053em, descender −434/2048 = −0.2119em
(all four faces identical). Predicted baseline offset from box top:
`ascent + (line_height − ascent − descent)/2`.

| fs  | factor | predicted offset | measured Prince | match |
|-----|--------|------------------|-----------------|-------|
| 10  | 1.0    | 8.467            | 8.47            | ✓     |
| 10  | 1.2    | 9.467            | 9.47            | ✓     |
| 10  | 1.6    | 11.467           | 11.47           | ✓     |
| 10  | 2.0    | 13.467           | 13.47           | ✓     |
| 15  | 1.0    | 12.701           | 12.70           | ✓     |
| 15  | 1.2    | 14.200           | 14.20           | ✓     |
| 15  | 1.6    | 17.200           | 17.20           | ✓     |
| 15  | 2.0    | 20.200           | 20.20           | ✓     |

Crossover (TA baseline == Prince baseline): factor ≈ 1.307 for 10pt — exactly
between the evidence's 1.2 (TA taller → more pages) and 1.6 (Prince taller →
more pages). Slope-cross explained.

### 4. Secondary finding: UA body margin (probe v9, v10 bisect)

prose.html's `body` rule has no `margin:` → TA's UA stylesheet (`body { margin:
8px }`, css.rs UA_CSS) adds 6pt at the top of page 1. Prince's body margin
applies ~3pt (its UA default differs). This is a separate constant push, not
line-box construction; it also shifts the h1 first baseline. OUT OF SCOPE for
CORE-90 (non-goal: "only the line-box construction math") but worth its own
ticket if page counts still diverge after the baseline fix.

## The fix

In `engine/src/layout.rs` (3 sites: generated content line 637, paragraph lines
717, float segments 823) and `engine/src/layout/multicol.rs` (line 565) and
margin boxes (line 2378), replace `baseline = y + font_size` with
`baseline = y + ascent + (line_height − ascent − descent)/2`, where
`ascent`/`descent` come from the face's hhea table scaled by font_size/upem.

Line-box HEIGHT stays `line_height` (already matches Prince; pagination math
`y + lh <= bottom_limit` untouched). Only the glyph baseline inside the box
moves, which is exactly the observable that diverged.

## Probe scripts

`probe/core90_probe.py` (v2: line box heights), `probe/core90_fontsize.py`
(v4: per-font-size heights), `probe/core90_baseline.py` (v8: first-baseline
formula), `probe/core90_variant.py` (v5: prose variants), `probe/core90_bisect.py`
(v10: body-margin bisect), `probe/core90_lines.py` (v7: per-line dump),
`probe/core90_match.py` (v12: line-matched deltas).
