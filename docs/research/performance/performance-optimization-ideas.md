---
title: Performance Optimization Ideas
type: research
status: approved
owner: elijah
created: 2026-08-25
updated: 2026-08-25
sidebar_position: 1
tags: [performance, benchmark, prince]
issue_id: CORE-123
---

# Performance optimization ideas (post-CORE-123 profiling)

Baseline (2026-08-25, release build): TypeAnvil renders most corpus docs in
21–32 ms vs Prince's 44–49 ms, and uses 13–18 MiB peak RSS vs Prince's
45–51 MiB. The one loss is table-stress: 226 ms vs Prince's 66 ms.

## Profiling evidence

15× `sample` of the release binary rendering `table-stress.html`:

- ~81% of runtime inside `layout_table_like` → `frozen_table_columns`
  (the CORE-89 freeze fixed point) → `intrinsic_column_widths`.
- HarfRust shaping (`shape_word_with_features`) appears in ~89% of samples
  inclusive — cell text is re-shaped from scratch on every freeze pass and
  row measurement.
- Allocator churn (`realloc` family + RawVec growth) ≈ 8% self time,
  mostly HarfRust buffer growth during repeated shaping.
- Minor: CSS cascade ~15%, PDF emission ~4%, font registry ~17% (one-time).
- No memoization exists anywhere in the measurement path ("memo-free" is
  a stated invariant in layout.rs).

## Ticketed

- **CORE-124** — shape cache: memoize `(text, face, size, features)` →
  advance width inside `shape_word_with_features`. Estimated 1.5–3× on
  table-stress. Byte-determinism must hold.

## Ideas on file (not ticketed)

1. **Per-cell intrinsic width caching within one table layout.** Compute
   min/max per cell once per freeze scope instead of once per pass. Smaller
   win than the shape cache; may be mostly absorbed by it.
2. **HarfRust buffer reuse / bump arena for measurement-only allocations.**
   Attacks the ~8% allocator tax. Medium effort, modest payoff.
3. **Stylo rule cache + bloom filter are disabled** (css.rs comments say
   "no rule cache… selector matching still exact, just uncached"). Enabling
   with deterministic cache conditions would speed large-document cascade.
   Probe-first change — they were skipped deliberately.
4. **Release profile tuning:** try `lto = "thin"`, `codegen-units = 1`.
   Typically 5–15% free; verify byte-determinism after (profile already
   avoids FMA contraction; LTO shouldn't change math but gate it anyway).
5. **Parallelism: not recommended yet.** Pagination is sequential (each
   page depends on its break token) and docs render in 20–50 ms already;
   thread overhead would eat the gain.

## Re-running the benchmark

```bash
cargo build --release --manifest-path engine/Cargo.toml
.venv/bin/python scripts/benchmark.py --runs 5
```

Runbook: `docs/operations/benchmark-engines.md`.
