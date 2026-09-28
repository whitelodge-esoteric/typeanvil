---
title: Prince Compatibility Research
type: research
status: approved
owner: maintainers
created: 2026-09-09
updated: 2026-09-27
sidebar_position: 1
tags: [prince, compatibility, migration, css]
---

# Prince Compatibility Research

## Question

How should Typeanvil use PrinceXML as a comparison target without making
Prince-specific behavior a second rendering contract?

## Method

Prince's public paged-media and CSS support documentation was checked on
2026-09-27. The repository's CSS alignment convention and current feature
specifications were checked. Historical local scoreboards and issue-specific
mode proposals are archived and excluded from this current summary.

## Findings

### Prince documents both standards support and deviations

Prince's public CSS support matrix lists CSS Paged Media Level 3 and CSS
Generated Content for Paged Media among its supported areas. The same matrix
also records deviations and extensions, including a different initial value for
`widows` and `orphans`, partial support for writing modes, and Prince-specific
interfaces for some paged-media features.

### Prince's paged-media model is a useful comparison target

Prince's paged-media documentation covers page size, page regions, named pages,
pagination controls, page numbering, and PDF-oriented features. These topics are
useful for migration checks and fixture design. They do not establish Typeanvil
behavior when CSS specifications define a different result.

### A global compatibility mode would duplicate the contract

A rendering switch would need a named rule for every meaningful difference,
separate tests, and a separate release gate. The public support matrix shows that
Prince behavior spans both standardized features and vendor extensions. The
lower-risk approach is to document each material divergence and add a scoped
option only when a concrete use case requires it and the behavior is specified.

## Implications

1. Keep CSS specifications as the governing source for Typeanvil behavior.
2. Use Prince output for migration and visual comparison evidence.
3. Label each difference as spec-correct, engine gap, metric difference, or
   vendor-specific behavior after checking the relevant standard.
4. Pin the Prince version for any reproducible comparison and record its version
   in generated artifacts.

## Sources

- [Prince paged media documentation](https://www.princexml.com/doc/paged/)
- [Prince supported CSS specifications](https://www.princexml.com/doc/css-refs/)
- [CSS Paged Media Level 3](https://www.w3.org/TR/css-page-3/)
- [CSS Generated Content for Paged Media Level 3](https://www.w3.org/TR/css-gcpm-3/)
- `docs/conventions/css-standards-alignment.md`
- `docs/specifications/visual-comparison-demo.spec.md`
