---
title: Use-Case Research — Document Classes, Weighted Features, Corpus Expansion
type: research
status: approved
owner: maintainers
created: 2026-09-22
updated: 2026-09-28
sidebar_position: 1
tags: [roadmap, use-cases, corpus, document-classes, features]
---

# Use-Case Research — Document Classes, Weighted Features, Corpus Expansion

## 1. Purpose

This doc is the roadmap source for Typeanvil. It answers one question: **what
documents do real customers actually render, and which engine features make
those documents correct?**

It replaces WPT conformance as the driver of the roadmap. WPT is a quality net
(regression detector + test source), not a target — no engine meets it
(Chromium fails 29 of our 283 print-reftests; PrinceXML scores ~40%). The
roadmap is driven by document classes and the features they load-bear.

## 2. Document classes

Six classes cover the documents a print-CSS engine is asked to produce. Each
row lists the class, the concrete documents in it, and the features that make
or break it.

| # | Class | Example documents | Load-bearing features |
|---|---|---|---|
| 1 | **Reports & statements** | quarterly report, financial statement, account statement, analytics digest | tables, running headers/footers, page numbers, TOC, `@page` geometry, stable pagination |
| 2 | **Invoices & billing** | invoice, statement of account, pro-forma, credit note | tables, running headers, currency alignment, `@page` geometry, page-count stability |
| 3 | **Letters & memos** | letterhead, business letter, memo, cover letter | `@page` geometry, margin boxes, running headers, simple flow |
| 4 | **Books & chapters** | book chapter, thesis, whitepaper, long-form report | footnotes, cross-references, running headers, TOC, hyphenation, justification, stable pagination |
| 5 | **Manuals & catalogs** | product manual, spec sheet, price catalog, data sheet | tables, floats (figures), multi-column, running headers, TOC, index |
| 6 | **Academic papers** | journal paper, conference paper, preprint | footnotes, cross-references, tables, floats (figures), hyphenation, justification, two-column |

## 3. Weighted feature map

Weight = how often the feature appears in real documents × how badly a defect
in it degrades the result. **5 = load-bearing, appears in most documents of
several classes; 1 = niche, rarely decisive.**

| Feature | Weight | Classes it serves | Notes |
|---|---|---|---|
| **Tables: fragmentation + repeated headers** | 5 | 1, 2, 5, 6 | The single most common real-world feature. A table that breaks wrong or loses its header is unusable. |
| **`@page` geometry (size, margins, named pages)** | 5 | all | Every document sets page geometry. Wrong margins = wrong document. |
| **Running headers/footers + page numbers** | 5 | 1, 2, 4, 5 | Present in nearly every multi-page document. |
| **Margin boxes** | 4 | 1, 2, 3, 4, 5 | Where running content lives. Biggest WPT bucket AND most common real feature. |
| **Hyphenation + justification quality** | 4 | 4, 6 | Long-form text. Bad hyphenation is visible on every line. |
| **Stable pagination** | 4 | 1, 2, 4 | Page count must not drift between runs/versions. A 3-page shift breaks a customer's workflow. |
| **Footnotes** | 3 | 4, 6 | Books and papers. |
| **Cross-references (target-counter, target-text)** | 3 | 4, 5, 6 | "See §2.3", "Figure 4". |
| **TOC with leaders** | 3 | 1, 4, 5 | Reports, books, manuals. |
| **Floats (figures, pull quotes)** | 3 | 5, 6 | Manuals and papers. |
| **Multi-column** | 2 | 5, 6 | Catalogs, some papers. |
| **Index** | 2 | 5 | Manuals. |
| **PDF output features (bookmarks, metadata, links)** | 2 | all | Nice-to-have, increasingly expected. |

### How the map drives the roadmap

- **Work on the 5s and 4s first.** Tables, `@page`, running content, margin
  boxes, hyphenation/justification, pagination stability. These are where a
  customer's document is won or lost.
- **A WPT issue is work only if it overlaps a weighted feature.** A margin-box
  WPT test maps to a 4-weight feature → it is work. A niche CSS2 float
  interaction that no real document uses → it stays parked in the WPT epic.
- **The 2s and 3s are follow-on**, once the load-bearing features are solid.

## 4. Corpus expansion: 7 → ~20-30 documents

The current comparison corpus (`demo/corpus/manifest.json`) has 8 fixtures:
Invoice, Quarterly Report, Academic Paper, Letterhead, Inventory Ledger
(table-stress), Float Showcase, Prose Showcase, Invoice — Statement of Account.
It is feature-driven (one fixture per feature) rather than document-driven.

The expansion makes the corpus **document-driven**: real-style documents that
cover the table of classes × features, so a defect in a load-bearing feature
shows up as a wrong render of a document a customer would actually produce.

### Target: 24 documents (4 per class)

| Class | Documents to add |
|---|---|
| **Reports & statements** | financial statement (multi-table), analytics digest (charts + tables), board report (TOC + running headers), sales summary (grouped tables) |
| **Invoices & billing** | pro-forma invoice, credit note, multi-currency invoice (currency alignment), recurring-billing statement |
| **Letters & memos** | business letter (letterhead + body), memo (internal), cover letter, formal notice |
| **Books & chapters** | book chapter (footnotes + cross-refs), whitepaper (long-form + TOC), thesis excerpt (footnotes + tables), long report (running headers + page numbers) |
| **Manuals & catalogs** | product manual (floats + tables), spec sheet (dense tables), price catalog (multi-column + tables), data sheet (tables + floats) |
| **Academic papers** | journal paper (two-column + footnotes), conference paper (floats + tables), preprint (cross-refs + footnotes), technical report (tables + figures) |

### Coverage check

The 24-document set exercises every weighted feature at least twice across
different classes, so a regression in a 5-weight feature (e.g. table
fragmentation) is caught by multiple documents, not one synthetic fixture.

### Build order

1. Add the 4 documents per class in weight order — start with the classes that
   use the 5s (Reports, Invoices), then Books/Academic (4s), then Manuals.
2. Each new document gets a manifest entry with `wedge_features` and
   `expected_deltas` (same schema as today), so the comparison track measures
   it against Prince from day one.
3. Keep the existing feature-driven fixtures (table-stress, float-showcase,
   prose) as stress tests — they isolate a single feature and are easier to
   bisect than a full document.

## 5. Measurement

- **Page-count parity with Prince** per document (the scoreboard already does
  this) — the primary stability signal.
- **Visual QA** (`scripts/visual-qa.sh`) on every document — overlap, overflow,
  text round-trip, declared-fill.
- **Per-page pixel diff vs Prince** on the comparison track — the correctness
  signal, with `expected_deltas` explaining known, accepted differences.

## 6. Status

Approved 2026-09-22. The class/feature table and the 24-document corpus target
are signed off. Next step: start the corpus expansion in weight order
(Reports/Invoices first, then Books/Academic, then Manuals). The WPT
conformance epic is the parking lot for conformance issues that do not overlap
a weighted feature.
