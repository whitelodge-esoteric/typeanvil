---
title: "CORE-239 handoff notes"
type: runbook
status: draft
owner: worker
created: 2026-10-16
updated: 2026-10-16
sidebar_position: 99
tags: [handoff, core-239]
---

# CORE-239 handoff notes

The coordinator posts these entries to Linear.

## Landed on this branch

- `engine/src/layout.rs`, text-run arm: a leading forced-break sentinel is skipped when the run continues an atomic-only line. A `<br>` right after a tall inline-block no longer opens an extra empty line below the box.
- Test: `engine/tests/core239_br_after_tall_inline_block.rs`. Pre-fix 133.65 against 119.25 (FAIL). Post-fix PASS.
- Fixture `css-page/monolithic-overflow-015` flips FAIL to PASS. Canaries 008 and 012 stay PASS.
- Gate review record: `gate/reviews/candidate-6057922.json`. The branch was rebased, so the reviewer shall re-key it to the final candidate SHA and re-capture base and candidate.

## Decisions

- DECIDED 2026-10-16 (agent): fix the 015 cause first. Cause CONFIRMED by probe: base ink on page 4 sits 15pt lower than the reference (y 125..134 against 110..119).
- DECIDED 2026-10-16 (agent): split the remaining six fixtures from this ticket. They have different causes, and one design change does not cover them.

## Split write-ups (for new TypeAnvil Core issues)

1. monolithic-overflow-014 (inline-block). The reference uses a float, so the first text line repeats beside the box on pages 1-3. The inline-block path in `layout.rs` (about lines 3646-3810) advances the cursor past the box on each page. The following text run is placed only after the box ends, on page 4. Needs per-page line-box continuation for a fragmented atomic box. Design change.
2. monolithic-overflow-005 (flex column). Page 4 yellow ink ends at y=107 against 122 in the reference. The text sits 15pt too high.
3. monolithic-overflow-021. Page 2 green area is 21600 px against 41472 px in the reference. Fixture uses `contain: size` plus `border-bottom: 50vh`.
4. monolithic-overflow-007 (grid text node). NOT probed to a confirmed cause. Hypothesis: the grid item collector drops direct text nodes. css-grid requires anonymous grid items.
5. css-break/transform-023, css-break/break-nested-float-in-table-001, css-page/media-queries-002. Not probed.
