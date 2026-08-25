# Engine benchmark — TypeAnvil vs PrinceXML

Generated 2026-08-25T03:16:52Z · TypeAnvil `b595351` (release profile) · Prince 16.2

Environment: Apple M4, 16 GB RAM, macOS (macOS-26.5-arm64-arm-64bit )

Method: each fixture rendered at the standard demo geometry (5in x 3in pages, 0.5in margins); 1 warmup run discarded, then 5 timed runs; wall-clock is the median, memory is the max peak RSS across timed runs (`/usr/bin/time -l`). Process spawn cost included for both engines.

## Render speed (median wall-clock) (s)

| Fixture | ta | prince |
|---|---|---|
| Invoice | 0.026 | 0.044 | (×0.60)
| Quarterly Report | 0.021 | 0.046 | (×0.47)
| Academic Paper | 0.032 | 0.049 | (×0.66)
| Letterhead | 0.022 | 0.049 | (×0.45)
| Inventory Ledger — Table Fragmentation Stress | 0.226 | 0.066 | (×3.42)
| Float Showcase — Figure & Pull Quote | 0.024 | 0.048 | (×0.50)
| Prose Showcase | 0.029 | 0.049 | (×0.59)

## Peak memory (MiB)

| Fixture | ta | prince |
|---|---|---|
| Invoice | 13.4 | 45.4 | (×0.30)
| Quarterly Report | 13.9 | 46.2 | (×0.30)
| Academic Paper | 14.5 | 48.9 | (×0.30)
| Letterhead | 13.1 | 49.5 | (×0.26)
| Inventory Ledger — Table Fragmentation Stress | 17.7 | 50.7 | (×0.35)
| Float Showcase — Figure & Pull Quote | 14.6 | 50.9 | (×0.29)
| Prose Showcase | 14.6 | 48.9 | (×0.30)

## Page counts (sanity)

| Fixture | ta | prince |
|---|---|---|
| Invoice | 5 | 5 |
| Quarterly Report | 10 | 10 |
| Academic Paper | 11 | 11 |
| Letterhead | 8 | 8 |
| Inventory Ledger — Table Fragmentation Stress | 43 | 45 |
| Float Showcase — Figure & Pull Quote | 8 | 8 |
| Prose Showcase | 11 | 11 |

