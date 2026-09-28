---
title: Rust Typesetting Architecture Brief
type: research
status: approved
owner: maintainers
created: 2026-08-14
updated: 2026-09-27
sidebar_position: 1
tags: [rust, ecosystem, typst, layout, pdf]
---

# Rust Typesetting Architecture Brief

## Question

Which public Rust projects provide reusable foundations for an HTML/CSS-to-PDF
engine, and which parts remain Typeanvil-specific work?

## Method

This refresh checks first-party project documentation and current public crate
metadata on 2026-09-27. It records architecture and scope claims. It does not
make a license or dependency decision; those require a current package review
at implementation time.

## Findings

### Typst provides a useful staged compiler model

Typst's public architecture document describes four phases: parsing, evaluation,
layout into frames, and export to PDF or raster output. It also describes an
incremental compiler based on `comemo`. This supports studying a staged pipeline
and explicit layout output. Typst is a document language rather than an HTML/CSS
engine, so its data model is an architectural reference, not a drop-in layout
backend.

### Taffy covers selected CSS layout algorithms

The `taffy` crate documents Flexbox, Grid, and Block algorithms. Its public API
uses a tree of UI nodes with styles as input and positions and sizes as output;
measure functions integrate other layout systems. The documented scope is UI
layout. It does not provide Typeanvil's HTML parsing, CSS cascade, inline text
layout, pagination, or PDF emission. Treat it as an algorithm reference or a
bounded integration candidate after measuring interface cost.

### Stylo is a reusable CSS engine with integration cost

The public Stylo repository describes it as the CSS engine used by Servo and
Firefox, and publishes the `stylo`, selector, and related crates. Its repository
shows an MPL-2.0 license and a multi-crate release process. Stylo can inform
cascade and selector work, but integration requires a compatible DOM abstraction,
style data plumbing, and a license review against the repository's AGPL policy.

### Parley and Krilla address different layers

Parley is a public rich-text layout library. It is relevant to shaping, text
runs, bidi, and line layout, but it is not a paged HTML/CSS layout engine.
Krilla is a public high-level Rust PDF library. Its repository documents PDF
construction, tests, a no-unsafe-code policy for its main crate, and dual MIT
and Apache-2.0 licensing. A PDF library can consume Typeanvil's resolved
fragments; it does not replace layout or pagination.

### WeasyPrint is a useful comparison point

WeasyPrint's public contributor documentation describes a Python project with
pytest tests and a source/documentation tree. Its architecture and performance
remain useful comparison topics, but a public contributor page alone does not
support the old study's stronger claims about table performance or complexity.
Those claims are excluded here until measured against a cited source and a
reproducible version.

## Implications

1. Keep HTML parsing, CSS cascade integration, inline layout, fragmentation, and
   PDF output as separate boundaries in Typeanvil's design.
2. Evaluate external crates by the exact layer they cover. A UI layout solver or
   PDF writer cannot be treated as a complete paged-media engine.
3. Recheck crate versions, licenses, MSRV, and feature coverage when selecting a
   dependency. Public research does not replace that review.
4. Preserve Typeanvil's fragment and pagination contracts as the primary design
   surface. External projects can supply components only where their input and
   output models match those contracts.

## Sources

- [Typst compiler architecture](https://github.com/typst/typst/blob/main/docs/dev/architecture.md)
- [Taffy crate documentation](https://docs.rs/taffy)
- [Stylo repository](https://github.com/servo/stylo)
- [Parley repository](https://github.com/linebender/parley)
- [Krilla repository](https://github.com/LaurenzV/krilla)
- [WeasyPrint contributor documentation](https://doc.courtbouillon.org/weasyprint/stable/contribute.html)
- `docs/specifications/typography-layer.spec.md`
- `docs/specifications/fragmentation-core.spec.md`
- `docs/specifications/licensing-resolution.spec.md`
