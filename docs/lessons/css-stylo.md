---
title: CSS Parsing and Style Resolution Seams
type: lesson
status: approved
owner: maintainers
created: 2026-09-16
updated: 2026-09-27
sidebar_position: 6
tags: [css, stylo, parsing, escaping, ua-css]
---

# CSS parsing and style resolution seams

## Context

Typeanvil has engine-owned passes beside Stylo's computed-style pipeline. A property can parse correctly in one pass and be absent from another.

## Durable practices

- **Hand-rolled declaration passes read raw CSS, so escapes are unprocessed** (verified): `@page`/margin-box content reached the parser as `Literal("Line 1\\aLine 2")` and painted the literal characters `\` and `a`; stylo never sees these rules. Any paged or author pass that parses a string token needs its own css-syntax-3 §4.3.7 unescape. Signature: an escape or a `data:` URI in a declaration behaving as literal text.
- **A CSS hex escape takes the longest run** (verified): `"\ab"` is U+00AB, not a newline followed by `b`. Write `"\a "` (the trailing space is consumed by the escape) or `"\aX"` with a non-hex `X` when a newline is meant. When a new test disagrees with the engine here, read css-syntax-3 before "fixing" the engine.
- **The stylo seam ignores inline `style=""` declarations for most properties** (verified 2026-08-25): `TyElement::style_attribute()` returns None, and the hand-rolled inline pass in `css.rs` carries only `break-*` declarations. WPT fixtures that set position, insets, or other properties inline (common in refs) silently do not get them; the same property via a stylesheet class works. When a test does not respond to a fix, probe whether the fixture sets the property inline before suspecting the engine. Widening the inline seam is still open work.
- **UA_CSS `//` line comments poison stylo's rule stream; rules after them never parse** (fixed `048b33a`, 2026-09-14): the UA sheet's `body { margin: 0 }` sat below a `//` comment block. Stylo treats `//` as invalid CSS and silently drops the rest of the sheet, so the UA body margin was never parsed; the initial `0` matched by accident. A comment that contains a literal `/* */` pair closes early and poisons from the first `*/`. Rules in UA_CSS need `/* */` comments with no inner comment-marker pairs, and any reversal (such as 0→8px) must verify the rule actually parses by probing the computed value.
- **The paged pass's `:root` pseudo matched the root node, never an element** (fixed `048b33a`, 2026-09-14): `Some(StructuralPseudo::Root) => id == dom.root`, but `dom.root` has kind Root, which fails the `NodeKind::Element` check upstream, so `:root { writing-mode: vertical-rl }` never applied and vertical documents silently ran `horizontal-tb`. The `d984e2e` "stylo doesn't compute writing-mode" seam was two seams: stylo and the engine's own matcher. Fix: `:root` is the element whose parent is the document root.
- **Stylo feature prefs gate whole property families** (verified): `display: grid` is behind `layout.grid.enabled` in the servo build; without `static_prefs::set_pref!("layout.grid.enabled", true)` at stylesheet parse, grid rules parse but compute as `block`. When a property family silently no-ops, check preferences.toml for a pref gate first.
- **Author-CSS re-parsing passes must cover every display value the engine supports** (verified): the paged-props pass re-parses the UA sheet with its own display parser; a missing value (grid) let the UA `div { display: block }` overwrite the author rule at UA origin. When adding a `Display` variant, grep every `"display" =>` match arm.
- **`collect_items` must list every block-level `Display` variant** (verified): a container missing from the block-level list folds as inline and its children are lifted to the parent; the container fragment vanishes and its layout dispatch never fires.
- **UA-versus-author paged-rule priority: renumber UA rules, do not shift by `len()`** (verified 2026-09-03): `parse_rules` bumps `order` for every block it scans, including rules the paged parser drops (font-size, margins), so pushed UA rules carry order values around 20+ while `ua_rules.len()` is only 6. Shifting author orders by `len()` left them below the UA values and UA silently beat author rules. Fix: renumber UA rules to `0..n` first, then shift author orders by `n`. Symptom: an author `X { bookmark-level: N }` (or any paged-declaration property) loses to a same-specificity UA rule. Comment text inside UA_CSS or raw CSS must never contain a literal `/*` marker pair; `strip_comments` cuts at the first `*/`, so text like `/* */` in a comment truncates the sheet mid-comment.
- **Author-CSS passes split declaration bodies with a naive `split(';')`** (verified 2026-09-15): four passes in `css.rs` (`breaks`, `borders`, `paged_props`, `viewport_units`) plus `breaks::parse_inline_decls` did. A data URI contains `;base64,`, so `content: url(data:image/png;base64,...)` reached the parser as `url(data:image/png`, an unbalanced path that interns as a broken 0x0 image and paints nothing. `split_top_level_decls` already existed for the `src` list; use it. Signature: an image that interns fine but paints 0x0.

## Verification

For a suspected parsing seam, test the same declaration through the relevant source forms and inspect the resolved value before measuring layout. Add a regression test that exercises the complete path from source text to computed style.

## References

- [Issue evidence and diagnosis](../conventions/issue-evidence.md)
- [CSS standards alignment](../conventions/css-standards-alignment.md)
