---
title: CSS Standards Alignment
type: convention
status: approved
owner: maintainers
created: 2026-08-24
updated: 2026-10-13
sidebar_position: 3
tags: [css, engine, compatibility, prince, weasyprint]
---

# CSS Standards Alignment

When CSS specifications and PrinceXML disagree, Typeanvil follows the CSS
specification. Always.

## Why

PrinceXML 16 made several breaking changes for exactly this reason. Its release
notes state the intent plainly:

> Changed the default value for the "box-decoration-break" property to "slice"
> to match web browsers and the CSS specification.

That change broke documents that depended on the old default. But it moved
Prince toward the standard, so every future fix gets easier instead of harder.
We take the same position, earlier: we will not inherit Prince's historical
deviations as our own behavior.

## The rule

1. **CSS spec first.** When a CSS specification defines a behavior, implement
   that behavior — even if PrinceXML does something different.
2. **Prince is a comparison target, not an oracle.** We match Prince where it
   agrees with the specs (page counts, line breaking, table measure). Where it
   disagrees with a spec, the spec wins.
3. **Document deviations both ways.** Every spec's References section names the
   governing CSS module. If we knowingly differ from Prince for spec reasons,
   record that in the spec — one sentence is enough.
4. **Breaking changes are acceptable when they move us toward the spec.** Do
   not preserve a non-standard behavior for backward compatibility once it is
   identified as a deviation.
5. **Probe before assuming either way.** Prince's actual behavior is measured,
   not guessed (see the probe discipline in existing specs). A "Prince quirk"
   finding should note whether the quirk is a spec deviation or spec
   compliance.

## Practical effect on parity work

Our demo corpus compares Typeanvil against Prince output: `scripts/build-demo.sh`
renders every corpus document through both engines and scores the pixel diff.
WeasyPrint is a second independent comparison engine (CORE-318): free,
open-source, and reproducible, so the performance benchmark and the WPT triage
can run without a Prince license. It is a comparison target, not an oracle,
exactly like Prince.

Our WPT harness does **not** compare against Prince. It scores test-versus-
reference through a single engine, and Chromium (via Playwright) is its built-in
oracle — WPT references are authored by browser engineers to encode browser
behavior, so a browser is the ground truth for a WPT reftest by construction.
For four-way triage on a failing test, `python -m harness triage <filter>` runs
the same filter through our engine, Chromium, Prince, and WeasyPrint and
reports who is the odd one out. Two readings matter most:

- **Our engine fails where several other engines pass** — we are the odd one
  out, and real engines satisfy the reference. Treat it as our bug.
- **Our engine fails where one print engine passes but Chromium fails** — the
  pair may be contradictory. Check whether another test pins the opposite
  reference before investing; `page-name-003` and `page-name-abspos-002` are
  structurally identical with opposite references, so no engine can pass both.

When a residual diff traces to a place where Prince deviates from the CSS spec:

- Fix toward the spec if the fix is cheap and gated clean on the full WPT
  suite (the usual self-consistency gate still applies).
- Otherwise file it as a known divergence with a note: "spec-correct; Prince
  deviates." Never tune the engine to reproduce a spec violation just to win
  scoreboard percentage points.

## Examples

- Prince 16 changed `box-decoration-break`'s default from `clone` to `slice`
  per css-break. Typeanvil implements the CSS initial value (`slice`), not
  Prince's old default.
- When a Prince-only extension (`-prince-*` properties) conflicts with a
  standardized equivalent, follow the standard. Extensions are additive and
  never override spec-defined behavior.

## Known spec-correct divergences

Rule 3 asks for a one-sentence note wherever we knowingly differ from a
comparison engine for spec reasons. Their visible effect is corpus diff and
visual-QA movement, which is easy to misread as a defect on a later pass — so
the recurring ones are collected here. Every row is measured with the other
engines as comparison targets, never as oracles. (The `box-decoration-break`
case under Examples above is the other recorded instance.)

| Behaviour | Governing spec | Typeanvil | PrinceXML 16.2 | WeasyPrint 70.0 |
|---|---|---|---|---|
| Top page-margin boxes align on the vertical centre of their band | css-page-3 §6.2, Table 2 | band centre | whole band painted; running head wraps vertically | flush to the content edge |

### Page-margin box vertical alignment

css-page-3 §6.2 fixes the page-margin-box defaults in a table: `top-left`,
`top-center` and `top-right` each default to `vertical-align: middle`. Typeanvil
implements that table directly, so a margin box shorter than its band is centred
in the band.

The visible consequence is in `library/reports/weasyprint-report.html`. The
sample's `@top-center` is a 1.42pt background rule that its author drew to sit
beneath the running head. Under the spec's `middle` default it lands on the
running head's centre line instead, so the rule crosses the text. WeasyPrint
keeps the rule clear of the text by placing the boxes flush to the page edge,
which deviates from the table.

Measured 2026-10-13 on `release/2026.10`. Reproduce with a 200mm × 60mm page and
20mm margins, declaring `@top-left` (`height: 1cm; width: 1cm`), `@top-center`
(`height: .05cm; width: 100%`) and `@top-right`
(`height: 1cm; vertical-align: middle; width: 100%`), then read the box
positions out of each engine's PDF. The top margin band is 56.7pt tall: the
`@top-left` box starts 14.2pt below the page edge in Typeanvil (centred) and
28.3pt below it in WeasyPrint (flush to the content edge). Prince paints the
whole 56.7pt band as one background box.

## References

- [Release Notes for Prince 16](https://www.princexml.com/releases/16/)
- [Prince forum: Prince 16 released](https://www.princexml.com/forum/topic/5122/prince-16-released)
- `docs/conventions/doc-conventions.md` — how docs and specs work here.
