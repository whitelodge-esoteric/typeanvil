# Engine benchmark — TypeAnvil vs PrinceXML vs WeasyPrint vs Headless Chrome

Generated 2026-10-18T02:34:51Z · TypeAnvil `9380271` (release profile) · Prince 16.2 · WeasyPrint version 70.0 · Google Chrome for Testing 154.0.8037.57

Environment: Apple M4, 16 GB RAM, macOS (macOS-26.5-arm64-arm-64bit )

Method: each fixture rendered at the geometry its `library/manifest.json` entry declares (the small corpus uses 5in x 3in pages, 0.5in margins; heavy and visual documents use their real page size). Release-time measurement: same host, TypeAnvil release profile. WeasyPrint uses a matching @page stylesheet; Headless Chrome receives a prepared copy of the fixture with a `<base href>` for relative assets and the same @page rule appended. 1 warmup run discarded, then 3 timed runs; wall-clock is the median, single-process memory is `/usr/bin/time -l`, process-tree memory is sampled over the whole process group. Process spawn cost included for all engines.

Skipped: WeasyPrint / Book Chapter — The Cartography of Rivers — WeasyPrint 70.0 AssertionError in float_layout (upstream)

## Render speed (median wall-clock) (s)

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 0.017 | 0.037 | 0.375 | 0.132 |
| Quarterly Report | 0.016 | 0.042 | 0.396 | 0.143 |
| Academic Paper | 0.028 | 0.042 | 0.456 | 0.130 |
| Letterhead | 0.017 | 0.043 | 0.427 | 0.129 |
| Running Letterhead | 0.013 | 0.036 | 0.358 | 0.130 |
| Inventory Ledger — Table Fragmentation Stress | 0.069 | 0.060 | 0.685 | 0.158 |
| Float Showcase — Figure & Pull Quote | 0.018 | 0.042 | 0.445 | 0.130 |
| Prose Showcase | 0.021 | 0.042 | 0.426 | 0.129 |
| Invoice — Statement of Account | 0.026 | 0.040 | 0.405 | 0.136 |
| Consolidated Financial Statements | 0.031 | 0.040 | 0.424 | 0.132 |
| Monthly Analytics Digest | 0.018 | 0.041 | 0.404 | 0.133 |
| Board Report — Q4 FY2026 | 0.019 | 0.042 | 0.404 | 0.137 |
| Sales Summary by Region | 0.019 | 0.039 | 0.401 | 0.130 |
| Pro-Forma Invoice | 0.017 | 0.041 | 0.438 | 0.131 |
| Credit Note | 0.016 | 0.038 | 0.396 | 0.136 |
| Multi-Currency Invoice | 0.018 | 0.039 | 0.396 | 0.130 |
| Recurring Billing Statement | 0.017 | 0.039 | 0.395 | 0.130 |
| Book Chapter — The Cartography of Rivers | 0.018 | 0.041 | — | 0.130 |
| Whitepaper — Deterministic Document Rendering | 0.019 | 0.045 | 0.434 | 0.130 |
| Thesis Excerpt — Sediment Transport | 0.018 | 0.041 | 0.444 | 0.130 |
| Annual Operations Report — Long Report | 0.017 | 0.039 | 0.396 | 0.131 |
| Journal Paper — Stage–discharge hysteresis | 0.019 | 0.040 | 0.431 | 0.134 |
| Conference Paper — Sparse Sampling of Bedload | 0.016 | 0.043 | 0.440 | 0.134 |
| Preprint — Convergence of Iterative Page Breaking | 0.016 | 0.047 | 0.427 | 0.129 |
| Technical Report — Capacity Planning | 0.017 | 0.042 | 0.405 | 0.132 |
| Product Manual — Northwind 2400 Pallet Truck | 0.016 | 0.049 | 0.401 | 0.138 |
| Spec Sheet — Northwind 2400 Series | 0.017 | 0.038 | 0.393 | 0.128 |
| Price Catalog 2026 — Meridian Trading | 0.018 | 0.038 | 0.385 | 0.130 |
| Data Sheet — Fernwood Cloud Platform | 0.016 | 0.040 | 0.396 | 0.131 |
| Business Letter — Meridian Trading Co. | 0.013 | 0.037 | 0.370 | 0.130 |
| Internal Memo — Platform Migration | 0.013 | 0.036 | 0.371 | 0.149 |
| Cover Letter — Application | 0.016 | 0.038 | 0.403 | 0.130 |
| Formal Notice — Service of Notice | 0.013 | 0.037 | 0.369 | 0.129 |
| Math Sample | 0.016 | 0.037 | 0.355 | 0.133 |
| Rich Media Print | 0.011 | 0.159 | 0.457 | 0.224 |
| Concert Ticket | 0.011 | 0.049 | 0.372 | 0.135 |
| Event Poster | 0.011 | 0.056 | 0.371 | 0.142 |
| Product Flyer | 0.011 | 0.053 | 0.385 | 0.143 |
| WeasyPrint Invoice | 0.017 | 0.033 | 0.395 | 0.130 |
| WeasyPrint Letter | 0.012 | 0.060 | 0.434 | 0.160 |
| WeasyPrint Poster | 0.005 | 0.088 | 0.412 | 0.196 |
| WeasyPrint Report | 0.022 | 0.058 | 0.634 | 0.169 |
| WeasyPrint Ticket | 0.015 | 0.043 | 0.600 | 0.139 |
| Journal Article — LLaMA (arXiv 2302.13971) | 0.803 | 0.135 | 1.824 | 0.292 |
| Dissertation (generated, ~140 pp) | 2.137 | 0.441 | 3.193 | 0.491 |
| Book — Pride and Prejudice (Gutenberg #1342) | 68.787 | 0.967 | 3.979 | 0.510 |
| Field Manual — FM 21-76 Survival | 1.397 | 0.802 | 3.291 | 0.478 |

## Peak memory (MiB)

Largest single process. Under-reports multi-process engines (Chrome) — see the process-tree tables.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 14.1 | 44.9 | 79.3 | 93.8 |
| Quarterly Report | 14.8 | 46.7 | 82.0 | 102.9 |
| Academic Paper | 15.3 | 50.9 | 92.4 | 93.3 |
| Letterhead | 15.2 | 50.2 | 88.3 | 95.2 |
| Running Letterhead | 14.2 | 44.3 | 77.5 | 90.5 |
| Inventory Ledger — Table Fragmentation Stress | 21.5 | 50.3 | 101.2 | 122.6 |
| Float Showcase — Figure & Pull Quote | 15.3 | 49.5 | 87.6 | 95.3 |
| Prose Showcase | 15.2 | 48.5 | 87.4 | 92.9 |
| Invoice — Statement of Account | 16.0 | 46.8 | 81.6 | 99.5 |
| Consolidated Financial Statements | 15.6 | 47.0 | 82.4 | 98.9 |
| Monthly Analytics Digest | 14.9 | 46.7 | 82.5 | 102.8 |
| Board Report — Q4 FY2026 | 15.6 | 47.6 | 80.7 | 95.4 |
| Sales Summary by Region | 14.7 | 45.6 | 81.4 | 95.9 |
| Pro-Forma Invoice | 14.5 | 45.8 | 82.3 | 98.4 |
| Credit Note | 14.2 | 45.8 | 79.8 | 95.4 |
| Multi-Currency Invoice | 14.8 | 45.8 | 80.5 | 96.3 |
| Recurring Billing Statement | 14.6 | 45.9 | 80.3 | 96.2 |
| Book Chapter — The Cartography of Rivers | 15.4 | 48.6 | — | 96.2 |
| Whitepaper — Deterministic Document Rendering | 14.9 | 49.5 | 87.8 | 95.4 |
| Thesis Excerpt — Sediment Transport | 14.6 | 47.7 | 87.9 | 96.5 |
| Annual Operations Report — Long Report | 14.4 | 45.8 | 80.3 | 95.8 |
| Journal Paper — Stage–discharge hysteresis | 36.3 | 48.5 | 88.6 | 99.5 |
| Conference Paper — Sparse Sampling of Bedload | 14.3 | 48.6 | 88.8 | 101.7 |
| Preprint — Convergence of Iterative Page Breaking | 15.1 | 48.7 | 88.6 | 96.2 |
| Technical Report — Capacity Planning | 14.4 | 46.7 | 82.3 | 101.7 |
| Product Manual — Northwind 2400 Pallet Truck | 14.3 | 48.8 | 88.7 | 109.1 |
| Spec Sheet — Northwind 2400 Series | 14.6 | 45.7 | 80.0 | 95.7 |
| Price Catalog 2026 — Meridian Trading | 14.9 | 45.8 | 79.9 | 96.8 |
| Data Sheet — Fernwood Cloud Platform | 14.2 | 46.5 | 81.9 | 101.2 |
| Business Letter — Meridian Trading Co. | 13.8 | 44.5 | 78.1 | 91.8 |
| Internal Memo — Platform Migration | 13.7 | 44.4 | 78.5 | 95.4 |
| Cover Letter — Application | 13.4 | 46.9 | 86.2 | 91.6 |
| Formal Notice — Service of Notice | 14.0 | 44.8 | 77.9 | 92.2 |
| Math Sample | 13.1 | 45.3 | 78.6 | 95.9 |
| Rich Media Print | 12.6 | 67.6 | 140.1 | 202.0 |
| Concert Ticket | 13.1 | 47.7 | 84.5 | 103.6 |
| Event Poster | 13.1 | 49.8 | 92.1 | 111.5 |
| Product Flyer | 13.3 | 49.3 | 78.7 | 113.1 |
| WeasyPrint Invoice | 35.3 | 52.2 | 79.4 | 93.2 |
| WeasyPrint Letter | 13.1 | 45.2 | 84.3 | 99.4 |
| WeasyPrint Poster | 9.9 | 52.3 | 90.7 | 118.7 |
| WeasyPrint Report | 19.3 | 58.3 | 152.2 | 167.1 |
| WeasyPrint Ticket | 34.0 | 101.5 | 168.6 | 95.4 |
| Journal Article — LLaMA (arXiv 2302.13971) | 62.2 | 115.5 | 223.9 | 251.2 |
| Dissertation (generated, ~140 pp) | 125.8 | 114.0 | 260.6 | 247.0 |
| Book — Pride and Prejudice (Gutenberg #1342) | 142.8 | 143.3 | 251.0 | 200.4 |
| Field Manual — FM 21-76 Survival | 120.9 | 141.1 | 197.0 | 205.2 |

## Peak memory, whole process tree — summed RSS (MiB)

Sum of resident memory across every process the render spawned; shared pages are counted once per process, so this over-counts multi-process engines.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 14.1 | 44.9 | 81.6 | 362.0 |
| Quarterly Report | 14.8 | 46.7 | 84.0 | 363.1 |
| Academic Paper | 15.3 | 50.9 | 94.6 | 358.3 |
| Letterhead | 15.2 | 50.2 | 90.5 | 353.9 |
| Running Letterhead | 14.2 | 44.3 | 79.0 | 341.0 |
| Inventory Ledger — Table Fragmentation Stress | 21.5 | 50.3 | 103.5 | 397.2 |
| Float Showcase — Figure & Pull Quote | 15.3 | 49.5 | 89.8 | 360.7 |
| Prose Showcase | 15.2 | 48.5 | 89.6 | 358.1 |
| Invoice — Statement of Account | 16.0 | 46.8 | 83.6 | 368.3 |
| Consolidated Financial Statements | 15.6 | 47.0 | 84.7 | 369.2 |
| Monthly Analytics Digest | 14.9 | 46.7 | 84.8 | 372.1 |
| Board Report — Q4 FY2026 | 15.6 | 47.6 | 82.9 | 350.7 |
| Sales Summary by Region | 14.7 | 45.6 | 83.7 | 358.6 |
| Pro-Forma Invoice | 14.5 | 45.8 | 84.5 | 359.6 |
| Credit Note | 14.2 | 45.8 | 82.1 | 362.2 |
| Multi-Currency Invoice | 14.8 | 45.8 | 82.0 | 358.6 |
| Recurring Billing Statement | 14.6 | 45.9 | 82.3 | 352.6 |
| Book Chapter — The Cartography of Rivers | 15.4 | 48.6 | — | 361.4 |
| Whitepaper — Deterministic Document Rendering | 14.9 | 49.5 | 90.0 | 363.0 |
| Thesis Excerpt — Sediment Transport | 14.6 | 47.7 | 90.1 | 366.0 |
| Annual Operations Report — Long Report | 14.4 | 45.8 | 82.5 | 366.6 |
| Journal Paper — Stage–discharge hysteresis | 36.3 | 48.5 | 90.8 | 367.3 |
| Conference Paper — Sparse Sampling of Bedload | 14.3 | 48.6 | 91.0 | 372.5 |
| Preprint — Convergence of Iterative Page Breaking | 15.1 | 48.7 | 90.8 | 364.7 |
| Technical Report — Capacity Planning | 14.4 | 46.7 | 84.6 | 370.3 |
| Product Manual — Northwind 2400 Pallet Truck | 14.3 | 48.8 | 90.9 | 363.9 |
| Spec Sheet — Northwind 2400 Series | 14.6 | 45.7 | 82.2 | 363.0 |
| Price Catalog 2026 — Meridian Trading | 14.9 | 45.8 | 82.1 | 365.1 |
| Data Sheet — Fernwood Cloud Platform | 14.2 | 46.5 | 84.1 | 362.7 |
| Business Letter — Meridian Trading Co. | 13.8 | 44.5 | 80.1 | 353.6 |
| Internal Memo — Platform Migration | 13.7 | 44.4 | 80.5 | 362.0 |
| Cover Letter — Application | 13.4 | 46.9 | 88.4 | 354.2 |
| Formal Notice — Service of Notice | 14.0 | 44.8 | 80.1 | 359.5 |
| Math Sample | 13.1 | 45.3 | 80.3 | 361.6 |
| Rich Media Print | 12.6 | 67.6 | 142.2 | 477.7 |
| Concert Ticket | 13.1 | 47.7 | 86.7 | 358.2 |
| Event Poster | 13.1 | 49.8 | 94.2 | 384.2 |
| Product Flyer | 13.3 | 49.3 | 80.8 | 389.2 |
| WeasyPrint Invoice | 35.3 | 52.2 | 81.6 | 357.6 |
| WeasyPrint Letter | 13.1 | 45.2 | 86.5 | 374.1 |
| WeasyPrint Poster | 9.9 | 52.3 | 92.5 | 393.9 |
| WeasyPrint Report | 19.3 | 58.3 | 154.5 | 421.4 |
| WeasyPrint Ticket | 34.0 | 101.5 | 170.8 | 345.0 |
| Journal Article — LLaMA (arXiv 2302.13971) | 62.2 | 115.5 | 226.1 | 533.6 |
| Dissertation (generated, ~140 pp) | 125.8 | 114.0 | 262.9 | 578.2 |
| Book — Pride and Prejudice (Gutenberg #1342) | 142.8 | 143.3 | 253.3 | 507.9 |
| Field Manual — FM 21-76 Survival | 120.9 | 141.1 | 199.1 | 521.7 |

## Peak memory, whole process tree — physical footprint (MiB)

Sum of each process's private physical footprint (the figure Activity Monitor shows as Memory); excludes shared libraries. The fairest single cross-engine memory comparison.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 8.2 | 20.4 | 92.6 | 81.1 |
| Quarterly Report | 8.7 | 21.9 | 94.8 | 82.1 |
| Academic Paper | 9.6 | 25.8 | 100.3 | 79.9 |
| Letterhead | 9.2 | 24.8 | 97.9 | 76.8 |
| Running Letterhead | 8.5 | 19.9 | 90.5 | 70.0 |
| Inventory Ledger — Table Fragmentation Stress | 15.6 | 25.7 | 114.0 | 122.8 |
| Float Showcase — Figure & Pull Quote | 9.4 | 23.7 | 98.7 | 79.0 |
| Prose Showcase | 9.4 | 23.5 | 97.9 | 79.3 |
| Invoice — Statement of Account | 9.9 | 21.5 | 94.8 | 87.8 |
| Consolidated Financial Statements | 9.7 | 22.5 | 96.1 | 89.2 |
| Monthly Analytics Digest | 8.7 | 21.8 | 95.1 | 91.3 |
| Board Report — Q4 FY2026 | 9.5 | 23.0 | 93.7 | 76.4 |
| Sales Summary by Region | 8.8 | 21.1 | 94.6 | 81.5 |
| Pro-Forma Invoice | 8.4 | 21.2 | 95.5 | 83.6 |
| Credit Note | 8.1 | 21.2 | 92.3 | 82.9 |
| Multi-Currency Invoice | 8.8 | 21.3 | 93.9 | 84.8 |
| Recurring Billing Statement | 8.6 | 21.3 | 93.4 | 77.9 |
| Book Chapter — The Cartography of Rivers | 9.4 | 23.4 | — | 81.9 |
| Whitepaper — Deterministic Document Rendering | 8.8 | 24.8 | 98.3 | 82.3 |
| Thesis Excerpt — Sediment Transport | 8.6 | 23.1 | 98.9 | 84.8 |
| Annual Operations Report — Long Report | 8.5 | 21.2 | 93.5 | 83.5 |
| Journal Paper — Stage–discharge hysteresis | 30.4 | 22.8 | 98.5 | 86.1 |
| Conference Paper — Sparse Sampling of Bedload | 8.3 | 23.7 | 99.1 | 88.9 |
| Preprint — Convergence of Iterative Page Breaking | 9.1 | 23.6 | 98.6 | 82.4 |
| Technical Report — Capacity Planning | 8.5 | 21.9 | 95.5 | 89.3 |
| Product Manual — Northwind 2400 Pallet Truck | 8.4 | 24.0 | 101.6 | 93.5 |
| Spec Sheet — Northwind 2400 Series | 8.7 | 21.1 | 93.3 | 83.3 |
| Price Catalog 2026 — Meridian Trading | 8.9 | 21.2 | 93.4 | 85.0 |
| Data Sheet — Fernwood Cloud Platform | 8.3 | 21.7 | 95.1 | 84.8 |
| Business Letter — Meridian Trading Co. | 7.9 | 20.0 | 91.4 | 76.0 |
| Internal Memo — Platform Migration | 7.7 | 19.9 | 91.4 | 82.4 |
| Cover Letter — Application | 7.7 | 22.4 | 96.8 | 75.8 |
| Formal Notice — Service of Notice | 7.9 | 20.3 | 90.8 | 78.0 |
| Math Sample | 7.0 | 21.3 | 91.5 | 80.1 |
| Rich Media Print | 6.9 | 43.0 | 153.8 | 200.6 |
| Concert Ticket | 7.0 | 23.0 | 98.3 | 87.3 |
| Event Poster | 7.0 | 25.2 | 106.0 | 100.9 |
| Product Flyer | 7.2 | 24.7 | 92.4 | 102.0 |
| WeasyPrint Invoice | 29.1 | 29.5 | 92.1 | 77.1 |
| WeasyPrint Letter | 6.7 | 22.4 | 97.5 | 95.8 |
| WeasyPrint Poster | 3.7 | 29.8 | 104.5 | 119.0 |
| WeasyPrint Report | 12.8 | 35.8 | 164.5 | 151.4 |
| WeasyPrint Ticket | 27.7 | 53.8 | 137.6 | 70.3 |
| Journal Article — LLaMA (arXiv 2302.13971) | 56.0 | 67.7 | 187.4 | 262.8 |
| Dissertation (generated, ~140 pp) | 109.9 | 90.9 | 255.0 | 320.8 |
| Book — Pride and Prejudice (Gutenberg #1342) | 130.8 | 120.3 | 245.4 | 260.9 |
| Field Manual — FM 21-76 Survival | 114.8 | 116.5 | 207.3 | 262.7 |

Processes per render: ta 1, prince 1, weasy 4, chrome 5.

## Page counts (sanity)

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 5 | 5 | 5 | 5 |
| Quarterly Report | 13 | 11 | 11 | 11 |
| Academic Paper | 12 | 11 | 13 | 12 |
| Letterhead | 8 | 8 | 8 | 5 |
| Running Letterhead | 5 | 5 | 6 | 4 |
| Inventory Ledger — Table Fragmentation Stress | 26 | 45 | 46 | 35 |
| Float Showcase — Figure & Pull Quote | 8 | 8 | 9 | 8 |
| Prose Showcase | 11 | 11 | 11 | 11 |
| Invoice — Statement of Account | 5 | 5 | 6 | 6 |
| Consolidated Financial Statements | 12 | 11 | 11 | 12 |
| Monthly Analytics Digest | 8 | 8 | 8 | 8 |
| Board Report — Q4 FY2026 | 12 | 13 | 13 | 13 |
| Sales Summary by Region | 7 | 7 | 7 | 7 |
| Pro-Forma Invoice | 8 | 15 | 17 | 7 |
| Credit Note | 7 | 10 | 11 | 7 |
| Multi-Currency Invoice | 7 | 7 | 8 | 8 |
| Recurring Billing Statement | 6 | 7 | 7 | 7 |
| Book Chapter — The Cartography of Rivers | 9 | 8 | — | 9 |
| Whitepaper — Deterministic Document Rendering | 10 | 12 | 12 | 12 |
| Thesis Excerpt — Sediment Transport | 9 | 9 | 10 | 9 |
| Annual Operations Report — Long Report | 9 | 9 | 10 | 10 |
| Journal Paper — Stage–discharge hysteresis | 9 | 6 | 6 | 5 |
| Conference Paper — Sparse Sampling of Bedload | 9 | 9 | 9 | 8 |
| Preprint — Convergence of Iterative Page Breaking | 8 | 8 | 8 | 8 |
| Technical Report — Capacity Planning | 10 | 10 | 10 | 10 |
| Product Manual — Northwind 2400 Pallet Truck | 9 | 9 | 9 | 9 |
| Spec Sheet — Northwind 2400 Series | 5 | 5 | 5 | 5 |
| Price Catalog 2026 — Meridian Trading | 7 | 4 | 4 | 6 |
| Data Sheet — Fernwood Cloud Platform | 7 | 7 | 7 | 7 |
| Business Letter — Meridian Trading Co. | 6 | 6 | 7 | 7 |
| Internal Memo — Platform Migration | 6 | 6 | 6 | 6 |
| Cover Letter — Application | 6 | 6 | 6 | 6 |
| Formal Notice — Service of Notice | 7 | 6 | 6 | 6 |
| Math Sample | 4 | 1 | 1 | 1 |
| Rich Media Print | 1 | 1 | 1 | 1 |
| Concert Ticket | 1 | 1 | 1 | 1 |
| Event Poster | 1 | 1 | 1 | 1 |
| Product Flyer | 2 | 2 | 2 | 2 |
| WeasyPrint Invoice | 1 | 1 | 1 | 1 |
| WeasyPrint Letter | 1 | 1 | 1 | 2 |
| WeasyPrint Poster | 1 | 1 | 1 | 2 |
| WeasyPrint Report | 26 | 8 | 8 | 7 |
| WeasyPrint Ticket | 3 | 1 | 1 | 1 |
| Journal Article — LLaMA (arXiv 2302.13971) | 45 | 13 | 13 | 10 |
| Dissertation (generated, ~140 pp) | 150 | 150 | 155 | 156 |
| Book — Pride and Prejudice (Gutenberg #1342) | 350 | 349 | 358 | 363 |
| Field Manual — FM 21-76 Survival | 636 | 213 | 219 | 220 |

## Startup cost (empty document)

One-paragraph document, same process spawn as every other run. Per-page figures below subtract this per engine.

| Engine | Median wall-clock (ms) | Peak RSS (MiB) |
|---|---|---|
| TypeAnvil | 10 | 9.2 |
| PrinceXML | 29 | 39.0 |
| WeasyPrint | 324 | 74.9 |
| Headless Chrome | 130 | 87.4 |

## Heavy and visual documents at real geometry

Each document renders at the page size its manifest entry declares. Cells: median wall-clock seconds / pages / ms per page (startup subtracted) / peak footprint MiB (whole process tree).

| Document | Geometry | TypeAnvil | PrinceXML | WeasyPrint | Headless Chrome |
|---|---|---|---|---|---|
| Quarterly Report | 5in×3in, 0.5in | 0.02 s / 13 pp / 0.5 ms/pp / 9 MiB | 0.04 s / 11 pp / 1.2 ms/pp / 22 MiB | 0.40 s / 11 pp / 6.5 ms/pp / 95 MiB | 0.14 s / 11 pp / 1.2 ms/pp / 82 MiB |
| Academic Paper | 5in×3in, 0.5in | 0.03 s / 12 pp / 1.5 ms/pp / 10 MiB | 0.04 s / 11 pp / 1.2 ms/pp / 26 MiB | 0.46 s / 13 pp / 10.1 ms/pp / 100 MiB | 0.13 s / 12 pp / 0.0 ms/pp / 80 MiB |
| Inventory Ledger — Table Fragmentation Stress | 5in×3in, 0.5in | 0.07 s / 26 pp / 2.3 ms/pp / 16 MiB | 0.06 s / 45 pp / 0.7 ms/pp / 26 MiB | 0.68 s / 46 pp / 7.8 ms/pp / 114 MiB | 0.16 s / 35 pp / 0.8 ms/pp / 123 MiB |
| Prose Showcase | 5in×3in, 0.5in | 0.02 s / 11 pp / 1.0 ms/pp / 9 MiB | 0.04 s / 11 pp / 1.1 ms/pp / 24 MiB | 0.43 s / 11 pp / 9.3 ms/pp / 98 MiB | 0.13 s / 11 pp / 0.0 ms/pp / 79 MiB |
| Consolidated Financial Statements | 5in×3in, 0.5in | 0.03 s / 12 pp / 1.8 ms/pp / 10 MiB | 0.04 s / 11 pp / 1.0 ms/pp / 22 MiB | 0.42 s / 11 pp / 9.0 ms/pp / 96 MiB | 0.13 s / 12 pp / 0.2 ms/pp / 89 MiB |
| Board Report — Q4 FY2026 | 5in×3in, 0.5in | 0.02 s / 12 pp / 0.8 ms/pp / 10 MiB | 0.04 s / 13 pp / 1.0 ms/pp / 23 MiB | 0.40 s / 13 pp / 6.1 ms/pp / 94 MiB | 0.14 s / 13 pp / 0.5 ms/pp / 76 MiB |
| Whitepaper — Deterministic Document Rendering | 5in×3in, 0.5in | 0.02 s / 10 pp / 0.9 ms/pp / 9 MiB | 0.04 s / 12 pp / 1.3 ms/pp / 25 MiB | 0.43 s / 12 pp / 9.2 ms/pp / 98 MiB | 0.13 s / 12 pp / 0.0 ms/pp / 82 MiB |
| Technical Report — Capacity Planning | 5in×3in, 0.5in | 0.02 s / 10 pp / 0.7 ms/pp / 8 MiB | 0.04 s / 10 pp / 1.3 ms/pp / 22 MiB | 0.40 s / 10 pp / 8.0 ms/pp / 96 MiB | 0.13 s / 10 pp / 0.3 ms/pp / 89 MiB |
| Math Sample | 8.5in×11in, 0.75in | 0.02 s / 4 pp / 1.5 ms/pp / 7 MiB | 0.04 s / 1 pp / 7.7 ms/pp / 21 MiB | 0.36 s / 1 pp / 30.7 ms/pp / 92 MiB | 0.13 s / 1 pp / 3.6 ms/pp / 80 MiB |
| Rich Media Print | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 0.7 ms/pp / 7 MiB | 0.16 s / 1 pp / 129.8 ms/pp / 43 MiB | 0.46 s / 1 pp / 132.1 ms/pp / 154 MiB | 0.22 s / 1 pp / 93.8 ms/pp / 201 MiB |
| Concert Ticket | 8in×3.2in, 0in | 0.01 s / 1 pp / 1.2 ms/pp / 7 MiB | 0.05 s / 1 pp / 19.8 ms/pp / 23 MiB | 0.37 s / 1 pp / 47.6 ms/pp / 98 MiB | 0.14 s / 1 pp / 5.5 ms/pp / 87 MiB |
| Event Poster | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 0.8 ms/pp / 7 MiB | 0.06 s / 1 pp / 26.5 ms/pp / 25 MiB | 0.37 s / 1 pp / 46.8 ms/pp / 106 MiB | 0.14 s / 1 pp / 12.6 ms/pp / 101 MiB |
| Product Flyer | 8.5in×11in, 0.75in | 0.01 s / 2 pp / 0.6 ms/pp / 7 MiB | 0.05 s / 2 pp / 12.1 ms/pp / 25 MiB | 0.39 s / 2 pp / 30.3 ms/pp / 92 MiB | 0.14 s / 2 pp / 6.8 ms/pp / 102 MiB |
| WeasyPrint Invoice | 8.5in×11in, 0.75in | 0.02 s / 1 pp / 6.9 ms/pp / 29 MiB | 0.03 s / 1 pp / 3.9 ms/pp / 30 MiB | 0.40 s / 1 pp / 70.7 ms/pp / 92 MiB | 0.13 s / 1 pp / 0.4 ms/pp / 77 MiB |
| WeasyPrint Letter | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 2.1 ms/pp / 7 MiB | 0.06 s / 1 pp / 31.3 ms/pp / 22 MiB | 0.43 s / 1 pp / 109.9 ms/pp / 98 MiB | 0.16 s / 2 pp / 15.0 ms/pp / 96 MiB |
| WeasyPrint Poster | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 0.0 ms/pp / 4 MiB | 0.09 s / 1 pp / 59.0 ms/pp / 30 MiB | 0.41 s / 1 pp / 87.1 ms/pp / 104 MiB | 0.20 s / 2 pp / 33.2 ms/pp / 119 MiB |
| WeasyPrint Report | 8.5in×11in, 0.75in | 0.02 s / 26 pp / 0.5 ms/pp / 13 MiB | 0.06 s / 8 pp / 3.5 ms/pp / 36 MiB | 0.63 s / 8 pp / 38.7 ms/pp / 164 MiB | 0.17 s / 7 pp / 5.6 ms/pp / 151 MiB |
| WeasyPrint Ticket | 8.5in×11in, 0.75in | 0.02 s / 3 pp / 1.8 ms/pp / 28 MiB | 0.04 s / 1 pp / 13.8 ms/pp / 54 MiB | 0.60 s / 1 pp / 275.5 ms/pp / 138 MiB | 0.14 s / 1 pp / 9.0 ms/pp / 70 MiB |
| Journal Article — LLaMA (arXiv 2302.13971) | 8.5in×11in, 0.75in | 0.80 s / 45 pp / 17.6 ms/pp / 56 MiB | 0.14 s / 13 pp / 8.2 ms/pp / 68 MiB | 1.82 s / 13 pp / 115.3 ms/pp / 187 MiB | 0.29 s / 10 pp / 16.2 ms/pp / 263 MiB |
| Dissertation (generated, ~140 pp) | 210mm×297mm, 25mm | 2.14 s / 150 pp / 14.2 ms/pp / 110 MiB | 0.44 s / 150 pp / 2.7 ms/pp / 91 MiB | 3.19 s / 155 pp / 18.5 ms/pp / 255 MiB | 0.49 s / 156 pp / 2.3 ms/pp / 321 MiB |
| Book — Pride and Prejudice (Gutenberg #1342) | 6in×9in, 0.75in | 68.79 s / 350 pp / 196.5 ms/pp / 131 MiB | 0.97 s / 349 pp / 2.7 ms/pp / 120 MiB | 3.98 s / 358 pp / 10.2 ms/pp / 245 MiB | 0.51 s / 363 pp / 1.0 ms/pp / 261 MiB |
| Field Manual — FM 21-76 Survival | 8.5in×11in, 1in | 1.40 s / 636 pp / 2.2 ms/pp / 115 MiB | 0.80 s / 213 pp / 3.6 ms/pp / 116 MiB | 3.29 s / 219 pp / 13.5 ms/pp / 207 MiB | 0.48 s / 220 pp / 1.6 ms/pp / 263 MiB |

## Scaling series

Generated plain-flow book (justified text, a chapter heading every 10 pages) at 6in×9in, 0.75in margins, built at roughly 10, 50, 150, 300 pages. Only length changes between rows. Cells: median seconds / actual pages / ms per page (startup subtracted).

| Target | TypeAnvil | PrinceXML | WeasyPrint | Headless Chrome |
|---|---|---|---|---|
| ~10 pp | 0.02 s / 10 pp / 0.9 ms/pp | 0.04 s / 10 pp / 1.1 ms/pp | 0.41 s / 11 pp / 7.7 ms/pp | 0.13 s / 11 pp / 0.0 ms/pp |
| ~50 pp | 0.05 s / 50 pp / 0.8 ms/pp | 0.08 s / 50 pp / 1.1 ms/pp | 0.72 s / 55 pp / 7.3 ms/pp | 0.16 s / 55 pp / 0.6 ms/pp |
| ~150 pp | 0.15 s / 150 pp / 0.9 ms/pp | 0.20 s / 150 pp / 1.1 ms/pp | 1.56 s / 165 pp / 7.5 ms/pp | 0.26 s / 165 pp / 0.8 ms/pp |
| ~300 pp | 0.26 s / 300 pp / 0.8 ms/pp | 0.40 s / 300 pp / 1.2 ms/pp | 2.85 s / 330 pp / 7.7 ms/pp | 0.42 s / 330 pp / 0.9 ms/pp |

Growth exponent (slope of log time vs log pages over the rows of 50+ pages, startup subtracted; 1.00 = linear, above 1.00 = worse than linear): TypeAnvil 1.03, PrinceXML 1.08, WeasyPrint 1.03, Headless Chrome 1.19.

