---
title: "Prince-compatibility rendering mode — feasibility analysis"
type: research
status: approved
owner: elijah
created: 2026-09-09
updated: 2026-09-09
sidebar_position: 2
tags: [prince, compatibility, architecture, product]
---

# Prince-compatibility rendering mode — feasibility analysis

**Question.** Should the engine expose a user-facing switch that selects
"align with Chromium" or "align with PrinceXML" rendering, and is it a heavy
lift?

**Recommendation: no global switch.** Add named, bounded, individually-tested
options instead, and ship a compatibility matrix for migrating users. The
measurements and the concrete cost are below.

## Where the divergences actually are

Measured, not assumed.

**Against Prince (the demo corpus, `demo/out/scoreboard.json`):** 6 of 7
fixtures already match on page count. The residual is a per-page pixel diff of
8–21% (letterhead 7.77%, quarterly report 7.87%, prose 10.52%, academic paper
13.19%, invoice 19.17%, float showcase 20.88%). The one page-count mismatch is
the inventory ledger at 43 vs 45, which is the recorded Prince quirk of
rendering that table 7.7pt wider than its content box.

Pixel-level residuals are not a mode. They come from hundreds of independent
micro-decisions: font metric adoption, UA stylesheet values, hyphenation
density, letter-spacing, margin collapse details. A switch cannot toggle "be 12%
closer to Prince" — each divergence has to become its own conditional.

**Against Chromium (the WPT suite):** the divergences found in the CORE-157
boundary study are not preference differences. On `page-name-002` the engine
fails while **both** Chromium and Prince pass — that is a defect, and a switch
would only let a user opt into keeping it.

**Genuinely contested cases are vanishingly rare.** Across the whole CORE-157
study, exactly one place showed Chromium and Prince disagreeing where it
mattered: `page-name-003` against `page-name-abspos-002`, which are structurally
identical tests with opposite references. That is a contradictory WPT pair, not
a real-world rendering decision.

## Why the lift is heavier than it looks

Threading a mode enum from the CLI through layout, pagination, and the PDF
emitter is the cheap part — roughly a day. The cost is in making the flag
*mean* something:

1. **Every divergence point must become conditional.** Each needs the divergent
   behavior isolated, both branches implemented, and coverage on both sides.
2. **The regression gate stops working.** The harness scores test-versus-
   reference through one engine, and WPT references encode browser behavior. A
   Prince-aligned mode would *fail* those references wherever Prince differs, so
   the existing 283-test gate cannot validate the mode — it actively penalises
   it.
3. **A Prince mode needs a differently-shaped gate.** The honest check is
   our-engine-in-prince-mode versus Prince's own output — cross-engine
   comparison, which the harness spec lists as a Non-Goal and does not
   implement. That gate has to be built first.
4. **The target moves.** Prince is not a specification. Its behavior changes
   between versions — the CSS alignment convention already records Prince 16
   changing `box-decoration-break`'s default. Pinning rendering to Prince means
   pinning to a version, which means a version matrix and re-baselining.
5. **Every decision surface doubles, and bug reports get ambiguous.** "Which
   mode were you in?" becomes a required question for every layout defect, and
   the spec-driven workflow has to describe two behaviors for one spec.

## The strategic cost

`AGENTS.md` states the positioning plainly: Typeanvil follows the CSS
specification, and CSS wins over PrinceXML always. A global compatibility mode
reverses that stance. If the product offers "render like Prince", the
differentiator disappears and the offer becomes a less mature clone of a
twenty-year-old engine — competing on imitation rather than on standards
correctness, which is where the wedge lives.

## What to do instead

**1. Use three-way triage (shipped).** `python -m harness triage <filter>`
reports which leg is the odd one out on a failing test, so a divergence is
classified before anyone decides how to handle it.

**2. For genuine Prince-parity requests, add named options one at a time.** The
right shape is a bounded, documented, individually-tested choice rather than a
global mode. Candidates that are real and testable in this form:

- **Print UA stylesheet profile** — Prince's `html.css` print defaults versus
  browser defaults. Partially adopted already (CORE-95 mirrored Prince's
  fixed-pt heading sizes and margins; CORE-92 zeroed the body margin).
- **Default `widows`** — the engine already sets 1 to match Prince (CORE-97).
- **`box-decoration-break` default** — currently the CSS initial value
  (`slice`); Prince 16 matches.

Each is one decision with one test, recorded in the spec that governs it. That
keeps the spec-driven model intact.

**3. Ship a compatibility matrix.** For migrating users, the artifact that
actually helps is a documented list of known divergences, each labelled
"spec-correct; Prince deviates" or "engine gap, tracked as CORE-N". That answers
the migration question better than a switch, and it is already the convention's
rule 3 ("document deviations both ways").

## When to revisit

Reconsider a scoped mode only if a concrete customer requirement names a
specific behavior, that behavior cannot be reached by a named option, and it is
confirmed against a pinned Prince version. A general rendering switch has no
such trigger.

## References

- `docs/conventions/css-standards-alignment.md` — CSS spec wins; Prince is a
  comparison target.
- `docs/research/css-page/named-page-boundary-model.md` — the three-way study
  that produced the divergence data above.
- `demo/out/scoreboard.json` — per-fixture Typeanvil vs Prince measurements.
- Prince 16 release notes — https://www.princexml.com/releases/16/
