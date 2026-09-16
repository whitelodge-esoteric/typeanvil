---
title: CSS Parsing, Stylo, and Author-Sheet Seams
type: lesson
status: approved
owner: elijah
created: 2026-09-16
updated: 2026-09-16
sidebar_position: 6
tags: [css, stylo, parsing, escape, UA_CSS]
---

# CSS Parsing, Stylo, and Author-Sheet Seams

Parsing and stylesheet seam lessons verified against real CORE issues. Read this before touching declaration parsing, escapes, UA_CSS, or stylo property computation.

## Lessons

- **Every hand-rolled declaration pass reads RAW CSS, so escapes are unprocessed** (verified CORE-178): `@page`/margin-box content reached the parser as `Literal("Line 1\\aLine 2")` and painted the literal characters `\` and `a` — stylo never sees these rules. Any paged/author pass that parses a string token needs its own css-syntax-3 §4.3.7 unescape. Signature: an escape or a `data:` URI in a declaration behaving as literal text.
- **A CSS hex escape takes the LONGEST run** (verified CORE-178): `"\ab"` is U+00AB, not a newline followed by `b`. Write `"\a "` (the trailing space is consumed by the escape) or `"\aX"` with a non-hex `X` when a newline is meant. When a new test disagrees with the engine here, read css-syntax-3 before "fixing" the engine — the engine was right.
- **The stylo seam ignores inline `style=""` declarations for most properties** (verified CORE-121, 2026-08-25): `TyElement::style_attribute()` returns None, and the hand-rolled inline pass in css.rs carries only break-* declarations. WPT fixtures that set position/insets/other properties INLINE (very common in refs) silently don't get them; the same property via a stylesheet class works. When a test "doesn't respond" to a fix, probe whether the fixture sets the property inline before suspecting the engine. Follow-up: widen the inline seam.
- **UA_CSS `//` line-comments poison stylo's rule stream — rules after them never parse (fixed 048b33a, CORE-153, 2026-09-14):** the UA sheet's `body { margin: 0 }` sat BELOW a `// CORE-92:` comment block. Stylo treats `//` as invalid CSS and silently drops the rest of the sheet, so the UA body margin was NEVER parsed — the initial 0 matched by accident. Even worse: a comment that CONTAINS a literal `/* */` pair closes early and poisons from the FIRST `*/` (the same strip_comments hazard from CORE-128). Rules in UA_CSS need `/* */` comments with NO inner comment-marker pairs, and any "reversal" (like 0→8px) must verify the rule actually parses (probe the computed value).
- **The paged pass's `:root` pseudo matched the Root NODE, never an element (fixed 048b33a, CORE-153, 2026-09-14):** `Some(StructuralPseudo::Root) => id == dom.root` — but `dom.root` is kind Root, which fails the `NodeKind::Element` check upstream, so `:root { writing-mode: vertical-rl }` NEVER applied and vertical docs silently ran horizontal-tb (the d984e2e "stylo doesn't compute writing-mode" seam was actually TWO seams: stylo AND the engine's own matcher). Fix: `:root` = the element whose parent is the doc root.
- **Stylo feature prefs gate whole property families (verified CORE-139):** display:grid is behind `layout.grid.enabled` in the servo build — without `static_prefs::set_pref!("layout.grid.enabled", true)` at stylesheet parse, grid rules parse but compute as block (the CORE-63 column-pref pattern). When a property family silently no-ops, check preferences.toml for a pref gate FIRST.
- **Author-CSS re-parsing passes must cover every display value the engine supports (verified CORE-139):** the paged-props pass re-parses the UA sheet with its OWN display parser; a missing value (grid) let UA `div { display: block }` overwrite the author rule at UA origin. When adding a Display variant, grep every `"display" =>` match arm.
- **collect_items must list every block-level Display variant (verified CORE-139):** a container missing from the block-level list folds as inline and its CHILDREN are lifted to the parent — the container fragment vanishes and its layout dispatch never fires.
- **UA-vs-author paged-rule priority: renumber UA rules, don't shift by len() (verified CORE-128, 2026-09-03).** `parse_rules` bumps `order` for EVERY block it scans — including rules the paged parser drops (font-size, margins) — so pushed UA rules carry order values ~20+ while `ua_rules.len()` is only 6. Shifting author orders by `len()` left them BELOW the UA values and UA silently beat author rules. Fix: renumber UA rules to 0..n first, then shift author orders by n. Symptom signature: an author `X { bookmark-level: N }` (or any paged-decl property) loses to a same-specificity UA rule. Also: comment text inside UA_CSS/raw CSS must never contain a literal `/*` marker pair — `strip_comments` cuts at the FIRST `*/`, so text like `` `/* */` `` in a comment truncates the sheet mid-comment.
- **Author-CSS passes split declaration bodies with a naive `split(';')`** (verified CORE-183, 2026-09-15): four passes in css.rs (`breaks`, `borders`, `paged_props`, `viewport_units`) plus `breaks::parse_inline_decls` did. A data URI contains `;base64,`, so `content: url(data:image/png;base64,...)` reached the parser as `url(data:image/png` — an unbalanced path that interns as a BROKEN 0x0 image and paints nothing. `split_top_level_decls` already existed for the `src` list; use it. Signature of the bug: an image that interns fine but paints 0x0.
