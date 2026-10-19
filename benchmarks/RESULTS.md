# Engine benchmark — TypeAnvil vs PrinceXML vs WeasyPrint vs Headless Chrome

Generated 2026-10-19T05:20:27Z · TypeAnvil `599a143` (release profile) · Prince 16.2 · WeasyPrint version 70.0 · Google Chrome for Testing 154.0.8037.57

Environment: Apple M4, 16 GB RAM, macOS (macOS-26.5-arm64-arm-64bit-Mach-O )

Method: each fixture rendered at the geometry its `library/manifest.json` entry declares (the small corpus uses 5in x 3in pages, 0.5in margins; heavy and visual documents use their real page size). Release-time measurement: same host, TypeAnvil release profile. WeasyPrint uses a matching @page stylesheet; Headless Chrome receives a prepared copy of the fixture with a `<base href>` for relative assets and the same @page rule appended. 1 warmup run discarded, then 5 timed runs; wall-clock is the median, single-process memory is `/usr/bin/time -l`, process-tree memory is sampled over the whole process group. Process spawn cost included for all engines.

Skipped: WeasyPrint / Book Chapter — The Cartography of Rivers — WeasyPrint 70.0 AssertionError in float_layout (upstream)

## Render speed (median wall-clock) (s)

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 0.019 | 0.038 | 0.388 | 0.129 |
| Quarterly Report | 0.018 | 0.042 | 0.409 | 0.135 |
| Academic Paper | 0.020 | 0.044 | 0.474 | 0.136 |
| Letterhead | 0.017 | 0.043 | 0.441 | 0.130 |
| Running Letterhead | 0.019 | 0.037 | 0.374 | 0.130 |
| Inventory Ledger — Table Fragmentation Stress | 0.074 | 0.061 | 0.717 | 0.160 |
| Float Showcase — Figure & Pull Quote | 0.018 | 0.051 | 0.456 | 0.131 |
| Prose Showcase | 0.017 | 0.044 | 0.442 | 0.130 |
| Invoice — Statement of Account | 0.025 | 0.042 | 0.419 | 0.136 |
| Consolidated Financial Statements | 0.025 | 0.041 | 0.438 | 0.135 |
| Monthly Analytics Digest | 0.019 | 0.042 | 0.421 | 0.138 |
| Board Report — Q4 FY2026 | 0.021 | 0.043 | 0.420 | 0.130 |
| Sales Summary by Region | 0.022 | 0.040 | 0.416 | 0.131 |
| Pro-Forma Invoice | 0.020 | 0.043 | 0.455 | 0.132 |
| Credit Note | 0.018 | 0.040 | 0.409 | 0.145 |
| Multi-Currency Invoice | 0.021 | 0.040 | 0.412 | 0.133 |
| Recurring Billing Statement | 0.019 | 0.039 | 0.405 | 0.132 |
| Book Chapter — The Cartography of Rivers | 0.017 | 0.042 | — | 0.130 |
| Whitepaper — Deterministic Document Rendering | 0.017 | 0.047 | 0.452 | 0.129 |
| Thesis Excerpt — Sediment Transport | 0.018 | 0.042 | 0.459 | 0.132 |
| Annual Operations Report — Long Report | 0.018 | 0.040 | 0.414 | 0.139 |
| Journal Paper — Stage–discharge hysteresis | 0.020 | 0.040 | 0.446 | 0.135 |
| Conference Paper — Sparse Sampling of Bedload | 0.017 | 0.043 | 0.457 | 0.132 |
| Preprint — Convergence of Iterative Page Breaking | 0.017 | 0.042 | 0.444 | 0.129 |
| Technical Report — Capacity Planning | 0.018 | 0.042 | 0.420 | 0.136 |
| Product Manual — Northwind 2400 Pallet Truck | 0.017 | 0.050 | 0.417 | 0.139 |
| Spec Sheet — Northwind 2400 Series | 0.018 | 0.039 | 0.401 | 0.143 |
| Price Catalog 2026 — Meridian Trading | 0.020 | 0.040 | 0.403 | 0.141 |
| Data Sheet — Fernwood Cloud Platform | 0.018 | 0.042 | 0.409 | 0.134 |
| Business Letter — Meridian Trading Co. | 0.014 | 0.038 | 0.384 | 0.130 |
| Internal Memo — Platform Migration | 0.015 | 0.037 | 0.387 | 0.130 |
| Cover Letter — Application | 0.014 | 0.039 | 0.420 | 0.130 |
| Formal Notice — Service of Notice | 0.015 | 0.039 | 0.379 | 0.130 |
| Journal Article — LLaMA (arXiv 2302.13971) | 0.257 | 0.140 | 1.884 | 0.297 |
| Dissertation (generated, ~140 pp) | 0.883 | 0.461 | 3.296 | 0.495 |
| Book — Pride and Prejudice (Gutenberg #1342) | 0.754 | 1.021 | 4.061 | 0.522 |
| Field Manual — FM 21-76 Survival | 0.542 | 0.826 | 3.362 | 0.486 |
| Math Sample | 0.017 | 0.039 | 0.365 | 0.151 |
| Rich Media Print | 0.011 | 0.160 | 0.470 | 0.229 |
| Concert Ticket | 0.012 | 0.052 | 0.383 | 0.137 |
| Event Poster | 0.012 | 0.058 | 0.384 | 0.144 |
| Product Flyer | 0.012 | 0.055 | 0.398 | 0.146 |
| WeasyPrint Invoice | 0.017 | 0.035 | 0.413 | 0.130 |
| WeasyPrint Letter | 0.013 | 0.064 | 0.445 | 0.162 |
| WeasyPrint Poster | 0.005 | 0.090 | 0.427 | 0.199 |
| WeasyPrint Report | 0.023 | 0.059 | 0.655 | 0.173 |
| WeasyPrint Ticket | 0.016 | 0.045 | 0.612 | 0.142 |

## Peak memory (MiB)

Largest single process. Under-reports multi-process engines (Chrome) — see the process-tree tables.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 14.5 | 44.9 | 79.4 | 93.7 |
| Quarterly Report | 15.2 | 46.7 | 81.9 | 99.6 |
| Academic Paper | 15.8 | 50.9 | 92.5 | 93.4 |
| Letterhead | 15.6 | 50.2 | 88.2 | 95.2 |
| Running Letterhead | 14.6 | 44.3 | 77.5 | 90.7 |
| Inventory Ledger — Table Fragmentation Stress | 22.2 | 50.4 | 101.3 | 122.6 |
| Float Showcase — Figure & Pull Quote | 15.5 | 49.8 | 87.8 | 97.7 |
| Prose Showcase | 15.6 | 49.0 | 87.5 | 91.5 |
| Invoice — Statement of Account | 16.4 | 46.8 | 81.7 | 99.5 |
| Consolidated Financial Statements | 15.6 | 46.7 | 82.5 | 98.9 |
| Monthly Analytics Digest | 15.2 | 46.7 | 82.6 | 102.8 |
| Board Report — Q4 FY2026 | 16.0 | 47.5 | 80.7 | 95.4 |
| Sales Summary by Region | 15.2 | 45.7 | 81.5 | 96.0 |
| Pro-Forma Invoice | 14.9 | 45.9 | 82.4 | 98.5 |
| Credit Note | 14.5 | 45.9 | 79.8 | 95.2 |
| Multi-Currency Invoice | 15.2 | 45.9 | 80.6 | 96.3 |
| Recurring Billing Statement | 14.9 | 45.9 | 80.3 | 96.2 |
| Book Chapter — The Cartography of Rivers | 15.7 | 48.6 | — | 96.3 |
| Whitepaper — Deterministic Document Rendering | 15.2 | 49.5 | 87.7 | 95.5 |
| Thesis Excerpt — Sediment Transport | 14.9 | 47.7 | 88.0 | 96.7 |
| Annual Operations Report — Long Report | 15.0 | 45.9 | 80.4 | 95.8 |
| Journal Paper — Stage–discharge hysteresis | 36.7 | 48.5 | 88.6 | 99.5 |
| Conference Paper — Sparse Sampling of Bedload | 14.8 | 48.6 | 88.9 | 101.8 |
| Preprint — Convergence of Iterative Page Breaking | 15.7 | 48.6 | 88.5 | 96.5 |
| Technical Report — Capacity Planning | 14.8 | 46.8 | 82.4 | 101.7 |
| Product Manual — Northwind 2400 Pallet Truck | 14.7 | 48.8 | 88.7 | 109.1 |
| Spec Sheet — Northwind 2400 Series | 14.9 | 45.7 | 79.9 | 95.7 |
| Price Catalog 2026 — Meridian Trading | 15.3 | 45.9 | 80.0 | 96.9 |
| Data Sheet — Fernwood Cloud Platform | 14.7 | 46.5 | 81.9 | 101.3 |
| Business Letter — Meridian Trading Co. | 14.3 | 44.5 | 78.1 | 91.8 |
| Internal Memo — Platform Migration | 14.2 | 44.5 | 78.5 | 95.4 |
| Cover Letter — Application | 13.8 | 46.1 | 86.1 | 91.6 |
| Formal Notice — Service of Notice | 14.4 | 44.8 | 78.1 | 93.4 |
| Journal Article — LLaMA (arXiv 2302.13971) | 63.5 | 115.7 | 224.2 | 251.4 |
| Dissertation (generated, ~140 pp) | 127.4 | 114.0 | 260.6 | 247.0 |
| Book — Pride and Prejudice (Gutenberg #1342) | 143.9 | 145.4 | 251.2 | 200.5 |
| Field Manual — FM 21-76 Survival | 121.8 | 141.1 | 197.1 | 205.2 |
| Math Sample | 13.4 | 45.3 | 78.5 | 97.5 |
| Rich Media Print | 13.0 | 67.6 | 140.4 | 201.9 |
| Concert Ticket | 13.5 | 47.8 | 84.6 | 103.7 |
| Event Poster | 13.5 | 49.9 | 92.1 | 111.6 |
| Product Flyer | 13.7 | 49.3 | 78.7 | 116.0 |
| WeasyPrint Invoice | 35.6 | 52.3 | 79.5 | 93.5 |
| WeasyPrint Letter | 13.2 | 45.2 | 84.1 | 102.6 |
| WeasyPrint Poster | 10.2 | 52.2 | 90.8 | 118.7 |
| WeasyPrint Report | 19.7 | 58.2 | 152.4 | 167.1 |
| WeasyPrint Ticket | 34.5 | 101.6 | 168.8 | 95.4 |

## Peak memory, whole process tree — summed RSS (MiB)

Sum of resident memory across every process the render spawned; shared pages are counted once per process, so this over-counts multi-process engines.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 14.5 | 44.9 | 81.5 | 359.5 |
| Quarterly Report | 15.2 | 46.7 | 84.1 | 362.1 |
| Academic Paper | 15.8 | 50.9 | 94.7 | 361.6 |
| Letterhead | 15.6 | 50.2 | 90.4 | 358.5 |
| Running Letterhead | 14.6 | 44.3 | 79.7 | 350.8 |
| Inventory Ledger — Table Fragmentation Stress | 22.2 | 50.4 | 103.5 | 400.4 |
| Float Showcase — Figure & Pull Quote | 15.5 | 49.8 | 90.0 | 367.0 |
| Prose Showcase | 15.6 | 49.0 | 89.7 | 356.9 |
| Invoice — Statement of Account | 16.4 | 46.8 | 83.9 | 370.1 |
| Consolidated Financial Statements | 15.6 | 46.7 | 84.7 | 368.2 |
| Monthly Analytics Digest | 15.2 | 46.7 | 83.8 | 367.5 |
| Board Report — Q4 FY2026 | 16.0 | 47.5 | 82.8 | 363.8 |
| Sales Summary by Region | 15.2 | 45.7 | 83.6 | 365.6 |
| Pro-Forma Invoice | 14.9 | 45.9 | 84.5 | 360.0 |
| Credit Note | 14.5 | 45.9 | 81.9 | 359.8 |
| Multi-Currency Invoice | 15.2 | 45.9 | 82.2 | 369.0 |
| Recurring Billing Statement | 14.9 | 45.9 | 82.5 | 364.6 |
| Book Chapter — The Cartography of Rivers | 15.7 | 48.6 | — | 361.3 |
| Whitepaper — Deterministic Document Rendering | 15.2 | 49.5 | 89.9 | 360.4 |
| Thesis Excerpt — Sediment Transport | 14.9 | 47.7 | 90.2 | 361.7 |
| Annual Operations Report — Long Report | 15.0 | 45.9 | 82.7 | 360.5 |
| Journal Paper — Stage–discharge hysteresis | 36.7 | 48.5 | 90.8 | 368.8 |
| Conference Paper — Sparse Sampling of Bedload | 14.8 | 48.6 | 91.1 | 367.2 |
| Preprint — Convergence of Iterative Page Breaking | 15.7 | 48.6 | 90.7 | 358.8 |
| Technical Report — Capacity Planning | 14.8 | 46.8 | 84.5 | 363.7 |
| Product Manual — Northwind 2400 Pallet Truck | 14.7 | 48.8 | 91.0 | 381.8 |
| Spec Sheet — Northwind 2400 Series | 14.9 | 45.7 | 82.0 | 369.8 |
| Price Catalog 2026 — Meridian Trading | 15.3 | 45.9 | 82.2 | 370.7 |
| Data Sheet — Fernwood Cloud Platform | 14.7 | 46.5 | 84.2 | 361.1 |
| Business Letter — Meridian Trading Co. | 14.3 | 44.5 | 80.2 | 355.4 |
| Internal Memo — Platform Migration | 14.2 | 44.5 | 80.6 | 356.2 |
| Cover Letter — Application | 13.8 | 46.1 | 88.3 | 359.4 |
| Formal Notice — Service of Notice | 14.4 | 44.8 | 80.4 | 360.7 |
| Journal Article — LLaMA (arXiv 2302.13971) | 63.5 | 115.7 | 226.5 | 537.6 |
| Dissertation (generated, ~140 pp) | 127.4 | 114.0 | 262.8 | 587.6 |
| Book — Pride and Prejudice (Gutenberg #1342) | 143.9 | 145.4 | 253.2 | 521.5 |
| Field Manual — FM 21-76 Survival | 121.8 | 141.1 | 199.2 | 517.8 |
| Math Sample | 13.4 | 45.3 | 80.5 | 363.6 |
| Rich Media Print | 13.0 | 67.6 | 142.6 | 481.1 |
| Concert Ticket | 13.5 | 47.8 | 86.8 | 366.8 |
| Event Poster | 13.5 | 49.9 | 94.3 | 384.1 |
| Product Flyer | 13.7 | 49.3 | 80.8 | 388.7 |
| WeasyPrint Invoice | 35.6 | 52.3 | 81.7 | 357.6 |
| WeasyPrint Letter | 13.2 | 45.2 | 86.4 | 374.4 |
| WeasyPrint Poster | 10.2 | 52.2 | 92.7 | 393.7 |
| WeasyPrint Report | 19.7 | 58.2 | 154.6 | 425.4 |
| WeasyPrint Ticket | 34.5 | 101.6 | 170.9 | 357.1 |

## Peak memory, whole process tree — physical footprint (MiB)

Sum of each process's private physical footprint (the figure Activity Monitor shows as Memory); excludes shared libraries. The fairest single cross-engine memory comparison.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 8.3 | 20.4 | 93.4 | 81.0 |
| Quarterly Report | 8.9 | 21.8 | 95.0 | 82.2 |
| Academic Paper | 9.8 | 25.9 | 100.5 | 79.5 |
| Letterhead | 9.5 | 24.8 | 98.6 | 80.1 |
| Running Letterhead | 8.7 | 19.9 | 90.7 | 73.8 |
| Inventory Ledger — Table Fragmentation Stress | 16.0 | 25.8 | 115.4 | 122.7 |
| Float Showcase — Figure & Pull Quote | 9.4 | 24.0 | 98.2 | 83.8 |
| Prose Showcase | 9.6 | 24.0 | 98.2 | 74.6 |
| Invoice — Statement of Account | 10.1 | 21.5 | 95.1 | 87.7 |
| Consolidated Financial Statements | 9.4 | 22.1 | 96.0 | 89.4 |
| Monthly Analytics Digest | 8.9 | 21.8 | 96.0 | 87.2 |
| Board Report — Q4 FY2026 | 9.7 | 23.0 | 93.7 | 82.8 |
| Sales Summary by Region | 9.0 | 21.3 | 94.7 | 84.9 |
| Pro-Forma Invoice | 8.7 | 21.2 | 96.5 | 83.6 |
| Credit Note | 8.3 | 21.3 | 93.1 | 80.9 |
| Multi-Currency Invoice | 9.0 | 21.3 | 94.0 | 84.9 |
| Recurring Billing Statement | 8.7 | 21.3 | 94.1 | 84.0 |
| Book Chapter — The Cartography of Rivers | 9.6 | 23.5 | — | 82.8 |
| Whitepaper — Deterministic Document Rendering | 8.9 | 24.8 | 98.3 | 82.0 |
| Thesis Excerpt — Sediment Transport | 8.7 | 23.0 | 99.1 | 81.2 |
| Annual Operations Report — Long Report | 8.8 | 21.3 | 94.4 | 81.1 |
| Journal Paper — Stage–discharge hysteresis | 30.5 | 22.9 | 98.8 | 86.4 |
| Conference Paper — Sparse Sampling of Bedload | 8.5 | 23.7 | 99.9 | 86.4 |
| Preprint — Convergence of Iterative Page Breaking | 9.5 | 23.5 | 98.8 | 79.4 |
| Technical Report — Capacity Planning | 8.6 | 22.0 | 95.5 | 85.4 |
| Product Manual — Northwind 2400 Pallet Truck | 8.5 | 24.0 | 101.9 | 96.5 |
| Spec Sheet — Northwind 2400 Series | 8.7 | 21.1 | 94.5 | 83.3 |
| Price Catalog 2026 — Meridian Trading | 9.0 | 21.3 | 93.1 | 84.8 |
| Data Sheet — Fernwood Cloud Platform | 8.5 | 21.7 | 95.1 | 84.9 |
| Business Letter — Meridian Trading Co. | 8.2 | 20.0 | 91.4 | 76.3 |
| Internal Memo — Platform Migration | 8.0 | 20.0 | 91.7 | 78.0 |
| Cover Letter — Application | 7.9 | 21.5 | 97.0 | 76.0 |
| Formal Notice — Service of Notice | 8.1 | 20.3 | 91.3 | 78.4 |
| Journal Article — LLaMA (arXiv 2302.13971) | 57.1 | 67.8 | 188.6 | 264.4 |
| Dissertation (generated, ~140 pp) | 110.0 | 90.9 | 255.7 | 322.1 |
| Book — Pride and Prejudice (Gutenberg #1342) | 131.4 | 122.4 | 246.4 | 261.0 |
| Field Manual — FM 21-76 Survival | 115.6 | 116.5 | 207.7 | 259.5 |
| Math Sample | 7.1 | 21.3 | 90.9 | 80.7 |
| Rich Media Print | 6.9 | 43.0 | 154.5 | 200.7 |
| Concert Ticket | 7.2 | 23.0 | 98.8 | 86.9 |
| Event Poster | 7.2 | 25.2 | 106.4 | 100.6 |
| Product Flyer | 7.4 | 24.7 | 92.6 | 106.3 |
| WeasyPrint Invoice | 29.2 | 29.5 | 92.8 | 76.8 |
| WeasyPrint Letter | 6.7 | 22.4 | 97.6 | 102.0 |
| WeasyPrint Poster | 3.9 | 29.7 | 104.5 | 119.1 |
| WeasyPrint Report | 12.9 | 35.7 | 164.6 | 155.5 |
| WeasyPrint Ticket | 27.9 | 53.7 | 138.7 | 77.1 |

Processes per render: ta 1, prince 1, weasy 4, chrome 5.

## Page counts (sanity)

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 5 | 5 | — | 5 |
| Quarterly Report | 13 | 11 | — | 11 |
| Academic Paper | 12 | 11 | — | 12 |
| Letterhead | 8 | 8 | — | 5 |
| Running Letterhead | 5 | 5 | — | 4 |
| Inventory Ledger — Table Fragmentation Stress | 26 | 45 | — | 35 |
| Float Showcase — Figure & Pull Quote | 8 | 8 | — | 8 |
| Prose Showcase | 11 | 11 | — | 11 |
| Invoice — Statement of Account | 5 | 5 | — | 6 |
| Consolidated Financial Statements | 12 | 11 | — | 12 |
| Monthly Analytics Digest | 8 | 8 | — | 8 |
| Board Report — Q4 FY2026 | 12 | 13 | — | 13 |
| Sales Summary by Region | 7 | 7 | — | 7 |
| Pro-Forma Invoice | 8 | 15 | — | 7 |
| Credit Note | 7 | 10 | — | 7 |
| Multi-Currency Invoice | 7 | 7 | — | 8 |
| Recurring Billing Statement | 6 | 7 | — | 7 |
| Book Chapter — The Cartography of Rivers | 9 | 8 | — | 9 |
| Whitepaper — Deterministic Document Rendering | 10 | 12 | — | 12 |
| Thesis Excerpt — Sediment Transport | 9 | 9 | — | 9 |
| Annual Operations Report — Long Report | 9 | 9 | — | 10 |
| Journal Paper — Stage–discharge hysteresis | 9 | 6 | — | 5 |
| Conference Paper — Sparse Sampling of Bedload | 9 | 9 | — | 8 |
| Preprint — Convergence of Iterative Page Breaking | 8 | 8 | — | 8 |
| Technical Report — Capacity Planning | 10 | 10 | — | 10 |
| Product Manual — Northwind 2400 Pallet Truck | 9 | 9 | — | 9 |
| Spec Sheet — Northwind 2400 Series | 5 | 5 | — | 5 |
| Price Catalog 2026 — Meridian Trading | 7 | 4 | — | 6 |
| Data Sheet — Fernwood Cloud Platform | 7 | 7 | — | 7 |
| Business Letter — Meridian Trading Co. | 6 | 6 | — | 7 |
| Internal Memo — Platform Migration | 6 | 6 | — | 6 |
| Cover Letter — Application | 6 | 6 | — | 6 |
| Formal Notice — Service of Notice | 7 | 6 | — | 6 |
| Journal Article — LLaMA (arXiv 2302.13971) | 45 | 13 | — | 10 |
| Dissertation (generated, ~140 pp) | 150 | 150 | — | 156 |
| Book — Pride and Prejudice (Gutenberg #1342) | 350 | 349 | — | 363 |
| Field Manual — FM 21-76 Survival | 636 | 213 | — | 220 |
| Math Sample | 4 | 1 | — | 1 |
| Rich Media Print | 1 | 1 | — | 1 |
| Concert Ticket | 1 | 1 | — | 1 |
| Event Poster | 1 | 1 | — | 1 |
| Product Flyer | 2 | 2 | — | 2 |
| WeasyPrint Invoice | 1 | 1 | — | 1 |
| WeasyPrint Letter | 1 | 1 | — | 2 |
| WeasyPrint Poster | 1 | 1 | — | 2 |
| WeasyPrint Report | 26 | 8 | — | 7 |
| WeasyPrint Ticket | 3 | 1 | — | 1 |

## Startup cost (empty document)

One-paragraph document, same process spawn as every other run. Per-page figures below subtract this per engine.

| Engine | Median wall-clock (ms) | Peak RSS (MiB) |
|---|---|---|
| TypeAnvil | 10 | 9.5 |
| PrinceXML | 30 | 38.9 |
| WeasyPrint | 333 | 74.8 |
| Headless Chrome | 129 | 87.4 |

## Heavy and visual documents at real geometry

Each document renders at the page size its manifest entry declares. Cells: median wall-clock seconds / pages / ms per page (startup subtracted) / peak footprint MiB (whole process tree).

| Document | Geometry | TypeAnvil | PrinceXML | WeasyPrint | Headless Chrome |
|---|---|---|---|---|---|
| Quarterly Report | 5in×3in, 0.5in | 0.02 s / 13 pp / 0.6 ms/pp / 9 MiB | 0.04 s / 11 pp / 1.1 ms/pp / 22 MiB | 0.41 s / None pp / — / 95 MiB | 0.14 s / 11 pp / 0.5 ms/pp / 82 MiB |
| Academic Paper | 5in×3in, 0.5in | 0.02 s / 12 pp / 0.8 ms/pp / 10 MiB | 0.04 s / 11 pp / 1.2 ms/pp / 26 MiB | 0.47 s / None pp / — / 100 MiB | 0.14 s / 12 pp / 0.6 ms/pp / 80 MiB |
| Inventory Ledger — Table Fragmentation Stress | 5in×3in, 0.5in | 0.07 s / 26 pp / 2.5 ms/pp / 16 MiB | 0.06 s / 45 pp / 0.7 ms/pp / 26 MiB | 0.72 s / None pp / — / 115 MiB | 0.16 s / 35 pp / 0.9 ms/pp / 123 MiB |
| Prose Showcase | 5in×3in, 0.5in | 0.02 s / 11 pp / 0.6 ms/pp / 10 MiB | 0.04 s / 11 pp / 1.3 ms/pp / 24 MiB | 0.44 s / None pp / — / 98 MiB | 0.13 s / 11 pp / 0.0 ms/pp / 75 MiB |
| Consolidated Financial Statements | 5in×3in, 0.5in | 0.02 s / 12 pp / 1.2 ms/pp / 9 MiB | 0.04 s / 11 pp / 1.0 ms/pp / 22 MiB | 0.44 s / None pp / — / 96 MiB | 0.13 s / 12 pp / 0.5 ms/pp / 89 MiB |
| Board Report — Q4 FY2026 | 5in×3in, 0.5in | 0.02 s / 12 pp / 0.9 ms/pp / 10 MiB | 0.04 s / 13 pp / 1.0 ms/pp / 23 MiB | 0.42 s / None pp / — / 94 MiB | 0.13 s / 13 pp / 0.0 ms/pp / 83 MiB |
| Whitepaper — Deterministic Document Rendering | 5in×3in, 0.5in | 0.02 s / 10 pp / 0.7 ms/pp / 9 MiB | 0.05 s / 12 pp / 1.4 ms/pp / 25 MiB | 0.45 s / None pp / — / 98 MiB | 0.13 s / 12 pp / 0.0 ms/pp / 82 MiB |
| Technical Report — Capacity Planning | 5in×3in, 0.5in | 0.02 s / 10 pp / 0.8 ms/pp / 9 MiB | 0.04 s / 10 pp / 1.2 ms/pp / 22 MiB | 0.42 s / None pp / — / 96 MiB | 0.14 s / 10 pp / 0.6 ms/pp / 85 MiB |
| Journal Article — LLaMA (arXiv 2302.13971) | 8.5in×11in, 0.75in | 0.26 s / 45 pp / 5.5 ms/pp / 57 MiB | 0.14 s / 13 pp / 8.4 ms/pp / 68 MiB | 1.88 s / None pp / — / 189 MiB | 0.30 s / 10 pp / 16.8 ms/pp / 264 MiB |
| Dissertation (generated, ~140 pp) | 210mm×297mm, 25mm | 0.88 s / 150 pp / 5.8 ms/pp / 110 MiB | 0.46 s / 150 pp / 2.9 ms/pp / 91 MiB | 3.30 s / None pp / — / 256 MiB | 0.49 s / 156 pp / 2.3 ms/pp / 322 MiB |
| Book — Pride and Prejudice (Gutenberg #1342) | 6in×9in, 0.75in | 0.75 s / 350 pp / 2.1 ms/pp / 131 MiB | 1.02 s / 349 pp / 2.8 ms/pp / 122 MiB | 4.06 s / None pp / — / 246 MiB | 0.52 s / 363 pp / 1.1 ms/pp / 261 MiB |
| Field Manual — FM 21-76 Survival | 8.5in×11in, 1in | 0.54 s / 636 pp / 0.8 ms/pp / 116 MiB | 0.83 s / 213 pp / 3.7 ms/pp / 116 MiB | 3.36 s / None pp / — / 208 MiB | 0.49 s / 220 pp / 1.6 ms/pp / 260 MiB |
| Math Sample | 8.5in×11in, 0.75in | 0.02 s / 4 pp / 1.7 ms/pp / 7 MiB | 0.04 s / 1 pp / 9.1 ms/pp / 21 MiB | 0.37 s / None pp / — / 91 MiB | 0.15 s / 1 pp / 22.3 ms/pp / 81 MiB |
| Rich Media Print | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 1.2 ms/pp / 7 MiB | 0.16 s / 1 pp / 129.5 ms/pp / 43 MiB | 0.47 s / None pp / — / 154 MiB | 0.23 s / 1 pp / 99.9 ms/pp / 201 MiB |
| Concert Ticket | 8in×3.2in, 0in | 0.01 s / 1 pp / 1.8 ms/pp / 7 MiB | 0.05 s / 1 pp / 21.4 ms/pp / 23 MiB | 0.38 s / None pp / — / 99 MiB | 0.14 s / 1 pp / 7.8 ms/pp / 87 MiB |
| Event Poster | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 1.8 ms/pp / 7 MiB | 0.06 s / 1 pp / 27.4 ms/pp / 25 MiB | 0.38 s / None pp / — / 106 MiB | 0.14 s / 1 pp / 14.6 ms/pp / 101 MiB |
| Product Flyer | 8.5in×11in, 0.75in | 0.01 s / 2 pp / 0.7 ms/pp / 7 MiB | 0.05 s / 2 pp / 12.2 ms/pp / 25 MiB | 0.40 s / None pp / — / 93 MiB | 0.15 s / 2 pp / 8.3 ms/pp / 106 MiB |
| WeasyPrint Invoice | 8.5in×11in, 0.75in | 0.02 s / 1 pp / 6.3 ms/pp / 29 MiB | 0.04 s / 1 pp / 4.8 ms/pp / 30 MiB | 0.41 s / None pp / — / 93 MiB | 0.13 s / 1 pp / 0.5 ms/pp / 77 MiB |
| WeasyPrint Letter | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 2.6 ms/pp / 7 MiB | 0.06 s / 1 pp / 33.4 ms/pp / 22 MiB | 0.45 s / None pp / — / 98 MiB | 0.16 s / 2 pp / 16.2 ms/pp / 102 MiB |
| WeasyPrint Poster | 8.5in×11in, 0.75in | 0.01 s / 1 pp / 0.0 ms/pp / 4 MiB | 0.09 s / 1 pp / 60.2 ms/pp / 30 MiB | 0.43 s / None pp / — / 104 MiB | 0.20 s / 2 pp / 34.7 ms/pp / 119 MiB |
| WeasyPrint Report | 8.5in×11in, 0.75in | 0.02 s / 26 pp / 0.5 ms/pp / 13 MiB | 0.06 s / 8 pp / 3.6 ms/pp / 36 MiB | 0.66 s / None pp / — / 165 MiB | 0.17 s / 7 pp / 6.3 ms/pp / 156 MiB |
| WeasyPrint Ticket | 8.5in×11in, 0.75in | 0.02 s / 3 pp / 2.0 ms/pp / 28 MiB | 0.04 s / 1 pp / 14.8 ms/pp / 54 MiB | 0.61 s / None pp / — / 139 MiB | 0.14 s / 1 pp / 12.6 ms/pp / 77 MiB |

## Scaling series

Generated plain-flow book (justified text, a chapter heading every 10 pages) at 6in×9in, 0.75in margins, built at roughly 10, 50, 150, 300 pages. Only length changes between rows. Cells: median seconds / actual pages / ms per page (startup subtracted).

| Target | TypeAnvil | PrinceXML | WeasyPrint | Headless Chrome |
|---|---|---|---|---|
| ~10 pp | 0.02 s / 10 pp / 0.8 ms/pp | 0.04 s / 10 pp / 1.2 ms/pp | 0.42 s / None pp / — | 0.13 s / 11 pp / 0.1 ms/pp |
| ~50 pp | 0.04 s / 50 pp / 0.6 ms/pp | 0.08 s / 50 pp / 1.1 ms/pp | 0.75 s / None pp / — | 0.17 s / 55 pp / 0.7 ms/pp |
| ~150 pp | 0.10 s / 150 pp / 0.6 ms/pp | 0.20 s / 150 pp / 1.1 ms/pp | 1.59 s / None pp / — | 0.26 s / 165 pp / 0.8 ms/pp |
| ~300 pp | 0.20 s / 300 pp / 0.6 ms/pp | 0.41 s / 300 pp / 1.3 ms/pp | 2.90 s / None pp / — | 0.43 s / 330 pp / 0.9 ms/pp |

Growth exponent (slope of log time vs log pages over the rows of 50+ pages, startup subtracted; 1.00 = linear, above 1.00 = worse than linear): TypeAnvil 1.00, PrinceXML 1.09, Headless Chrome 1.18.

