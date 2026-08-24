---
title: CSS Standards Alignment
type: convention
status: approved
owner: elijah
created: 2026-08-24
updated: 2026-08-24
sidebar_position: 3
tags: [css, engine, compatibility, prince]
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

Our demo corpus and WPT harness compare Typeanvil against Prince output. When
a residual diff traces to a place where Prince deviates from the CSS spec:

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

## References

- [Release Notes for Prince 16](https://www.princexml.com/releases/16/)
- [Prince forum: Prince 16 released](https://www.princexml.com/forum/topic/5122/prince-16-released)
- `docs/conventions/doc-conventions.md` — how docs and specs work here.
