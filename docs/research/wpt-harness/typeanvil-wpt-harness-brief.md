---
title: WPT Print-Reftest Harness Research
type: research
status: approved
owner: maintainers
created: 2026-08-14
updated: 2026-09-27
sidebar_position: 1
tags: [wpt, harness, conformance, testing]
---

# WPT Print-Reftest Harness Research

## Question

Which public WPT reftest rules should guide a PDF conformance harness, and what
should remain in the Typeanvil specification?

## Method

The public WPT documentation and public WPT CSS directories were checked on
2026-09-27. The current repository specification and harness implementation were
also checked. Historical issue triage and dated scoreboards are archived with
the internal project notes; they are not used as current public evidence.

## Findings

### Print reftests have explicit pagination semantics

WPT documents print reftests as ordinary reftests rendered to paginated output
and compared page by page. The documented default page box is 5 by 3 inches with
0.5-inch margins. A test can select pages with `reftest-pages`, and fuzzy
matching applies to each image comparison.

### Reftest metadata defines the comparison contract

WPT's reftest documentation defines `rel=match` and `rel=mismatch`, reference
links, multiple-reference behavior, and fuzzy thresholds for maximum per-channel
difference and total differing pixels. These rules are stable public semantics
that a harness can implement or cite.

### The public WPT corpus is the fixture source

The public `css-page` and `css-gcpm` directories contain paged-media fixtures,
references, and metadata. The repository's harness specification narrows the
selection to the supported engine surface and defines the local CLI, artifact,
and release-gate behavior.

### Chromium is useful as a separate oracle

Chromium's public LayoutNG documentation describes printing and block
fragmentation as a fragment-tree-based layout system. A browser run can provide
an independent comparison for diagnosis, but a cross-engine comparison does not
replace WPT's same-engine test-versus-reference conformance score.

## Implications

1. Keep WPT semantics in the harness specification and implementation tests.
2. Cite WPT's public rules instead of retaining issue-specific score reports as
   public research.
3. Treat browser comparisons as diagnostic evidence. Record the browser version,
   page settings, and fixture revision for any new measurement.
4. Keep dated triage, private artifact paths, and issue disposition records in
   the external archive.

## Sources

- [WPT reftests](https://web-platform-tests.org/writing-tests/reftests.html)
- [WPT print reftests](https://web-platform-tests.org/writing-tests/print-reftests.html)
- [WPT css-page directory](https://github.com/web-platform-tests/wpt/tree/master/css/css-page)
- [WPT css-gcpm directory](https://github.com/web-platform-tests/wpt/tree/master/css/css-gcpm)
- [Chromium LayoutNG block fragmentation](https://developer.chrome.com/docs/chromium/renderingng-fragmentation)
- `docs/specifications/wpt-conformance-harness.spec.md`
- `docs/specifications/harness-release-gate.spec.md`
