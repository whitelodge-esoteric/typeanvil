# Task: Walking skeleton for Typeanvil (CORE-50)

Build the thinnest end-to-end HTML→PDF pipeline in Rust: `engine/` in this repo.
The repo already contains a Python WPT conformance harness at `harness/` (CORE-49,
done) — your engine will eventually be driven by it, but for THIS task you only need
the CLI contract it expects.

## CLI contract (fixed — do not change)

```
typeanvil render <input.html> --page-width 5in --page-height 3in \
  --margin-top 0.5in --margin-right 0.5in --margin-bottom 0.5in --margin-left 0.5in \
  -o out.pdf
```

This is the harness's `CliEngine` contract (see `harness/engine.py`). Also accept
`--page-size <WxH>` in inches as a convenience. Print usage + exit non-zero on bad args.

## Pipeline (crates to wrap)

1. **HTML parsing** — `html5ever` (tendril-based DOM tree; you own the tree structs)
2. **CSS parse + cascade** — **stylo 0.20** (`https://crates.io/api/v1/crates/stylo`,
   published 2026-08-04, default features = servo). THIS IS THE RISK: stylo needs you to
   implement its `TElement`/`TNode`/`ComputedValues` trait machinery (Servo-style DOM
   traits). Prototype the binding FIRST with the smallest possible surface:
   - Implement the trait set for a minimal element/node wrapper over your html5ever tree
   - Drive `stylo::style::style_element` for a document with a handful of rules
   - If the trait surface proves unworkable within a reasonable effort, STOP and report
     back with the exact blocker (don't silently swap in a different engine).
   Support a realistic subset: tag/class/id selectors, descendant combinators, property
   inheritance for font-size/font-family/color/margins. Ignore @media (assume print),
   ignore @import, ignore most properties — default values fine.
3. **Text shaping** — `rustybuzz` + `ttf-parser` (wrap harfbuzz behavior; no C FFI).
   Load system fonts via `fontdb` (load a default sans font; don't over-engineer
   font matching).
4. **Layout** — hand-written minimal block layout in this crate: boxes with
   width/height/margin/padding from computed style, inline text as shaped runs,
   block flow with y-cursor. Floats/abs-pos/tables: NOT in scope. Fragmentation:
   overflow to new pages (walking skeleton only — the real fragmentation core is
   CORE-51). This is the only code we "build" in this task.
5. **PDF output** — `krilla` (production backend used by Typst; supports tagged PDF).
   Draw text glyphs and rectangles; one page per page-break.

## Arithmetic decision (record it)

The Architecture note (vault `brain/Projects/Typeanvil/Architecture.md`) lists an open
question: fixed-point TeX-style scaled points vs controlled f64. Decide for the skeleton:
prefer **f64 with explicit rounding at page/output boundaries** unless you hit a concrete
determinism problem during this task. Append a `DECIDED 2026-08-15:` bullet to the
Architecture note's Open Questions section stating what you chose and why. (Layout code
must not be blocked on this.)

## Structure

```
engine/
  Cargo.toml          # name = "typeanvil", edition 2021, deps: html5ever, stylo=0.20,
                      # rustybuzz, ttf-parser, fontdb, krilla, clap (or hand-rolled args)
  src/main.rs         # CLI entry
  src/dom.rs          # html5ever tree + stylo TNode/TElement impls (the risk)
  src/style.rs        # stylo driving: parse stylesheet -> cascade -> computed style
  src/text.rs         # shaping via rustybuzz -> positioned glyph runs
  src/layout.rs       # minimal block layout + pagination
  src/pdf.rs          # krilla writer
tests/
  (integration test: render fixture -> assert PDF bytes non-empty, page count, and
   that a known string's glyphs are present — decode via pdf text extraction if easy,
   otherwise assert krilla didn't error and output has expected page size)
fixtures/sample.html  # simple doc: <h1> + <p> with class + a table-free layout
```

Also: `uv run python -m harness run --engine chromium --filter css-page --limit 5`
must still work after your changes (do NOT touch `harness/`; the CLI contract is what
ties them). Add a `CliEngine` smoke test in `harness/tests/` only if it's trivial —
otherwise skip; the contract check is the CLI help text matching `harness/engine.py`.

## Verification (run all of it before finishing)

1. `cargo build --release` — clean build. First stylo compile is SLOW (minutes); that's
   expected.
2. `cargo test` — your integration test passes.
3. `./target/release/typeanvil render fixtures/sample.html --page-width 5in --page-height 3in
   --margin-* 0.5in -o /tmp/sample.pdf && ls -la /tmp/sample.pdf` — file exists, non-trivial
   size. Verify PDF validity: `python3 -c "import pypdfium2; print(pypdfium2.PdfDocument('/tmp/sample.pdf').page_count)"`
   (the repo venv at `.venv/bin/python` has pypdfium2).
4. Report exact outputs: page count, PDF size, which stylo traits you had to implement,
   and any deviations from this spec (with reasons).
