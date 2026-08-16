# CORE-50: Walking skeleton — html5ever → stylo → block layout → krilla

You are implementing the **walking skeleton** for Typeanvil, a PrinceXML-competitor HTML/CSS→PDF typesetting engine. Repo: `/Users/elijah/workspace/typeanvil`. This is the FIRST Rust code in the repo (everything so far is a Python test harness in `harness/`). Task: thinnest possible end-to-end pipe — parse HTML, cascade CSS, trivial block layout, write PDF. The whole thing must render a styled "Hello, page 1" through the pipe.

## Non-negotiable CLI contract (defined by the existing harness)

The harness at `harness/engine.py` (`CliEngine`) will invoke the engine exactly like this — your binary MUST satisfy it:

```
typeanvil render <input.html> \
    --page-width 5in --page-height 3in \
    --margin-top 0.5in --margin-right 0.5in \
    --margin-bottom 0.5in --margin-left 0.5in \
    --base-url http://127.0.0.1:PORT/ \
    -o <output.pdf>
```

Requirements:
- **Deterministic and offline**: identical input → identical PDF bytes. No network fetches except via `--base-url` (for the WPT harness; ignore it beyond parsing the flag in this skeleton).
- **Fixed geometry**: honour `--page-width/--page-height/--margin-*` exactly (parse `5in`, `0.5in` etc.).
- **Paginated output**: emit one PDF page per laid-out page.
- Accept `--base-url` (may be empty string; just parse and ignore).
- Exit 0 on success, non-zero on error, write PDF to `-o` path.

## Build location & structure

Create a Rust binary crate at **`/Users/elijah/workspace/typeanvil/engine/`**:

```
engine/
├── Cargo.toml          # binary name "typeanvil" ([[bin]] name = "typeanvil")
├── src/
│   ├── main.rs         # CLI arg parsing (use clap or hand-rolled — your call)
│   ├── dom.rs          # minimal DOM types (html5ever output → our own node structs)
│   ├── css.rs          # cascade: stylesheets → computed styles per element
│   ├── layout.rs       # trivial block layout: block boxes stacked vertically
│   └── pdf.rs          # krilla export: draw text at computed positions/colors
└── tests/
    └── smoke.rs        # integration test: render a fixture HTML → PDF exists, non-empty
```

Do NOT touch the existing Python harness or tests. Add `engine/target/` and `engine/Cargo.lock` decisions to `.gitignore` as appropriate (Cargo.lock SHOULD be committed for a binary crate).

## Dependencies (add to engine/Cargo.toml)

- `html5ever` — WHATWG-compliant HTML parsing (Servo). Parse into a simple DOM.
- `cssparser` — CSS parsing (Servo).
- `selectors` — selector matching (Servo).
- **stylo** — THIS IS THE SPIKE. The published `stylo` crate (MPL-2.0) gives a web-grade cascade: parse, selector match, specificity, inheritance, computed values. It requires implementing `TElement`/`TNode` traits for our DOM.
- `krilla` — PDF output (Typst's production backend; tagged PDF, PDF/A, subsetting). Sits on `pdf-writer`.
- `kurbo` (krilla's geometry dep, likely needed for paths/points).
- `anyhow` or `thiserror` for errors; `clap` optional for CLI.

## Stage 1: THE STYLO SPIKE (do this FIRST — it's the project's biggest risk)

The whole point of this issue is to de-risk the stylo binding. Attempt it in earnest:

1. Parse HTML with html5ever → build a minimal DOM tree (your own `Node`/`Element` structs; use `Rc`-based tree or arena — whatever is simplest).
2. Parse a `<style>` element (and/or a linked stylesheet via `--base-url` if trivial) with cssparser.
3. Implement `stylo`'s `TElement`/`TNode`/related traits for your DOM (study how Blitz does it; stylo ships `stylo_traits`).
4. Run the cascade to compute computed styles for at least: `color`, `font-size`, `font-family`, `display`, `margin`/`padding`, `background-color`.

**If stylo proves unworkable within this run** (huge dep tree, build failure, trait surface too deep for the time budget, MPL concerns), then **fall back** to the Architecture doc's documented fallback: hand-rolled cascade on `cssparser` + `selectors` (parse rules, match selectors, specificity sort, inherit `color`/`font-size`). This is a *sanctioned* fallback, not failure — but you MUST:
- State clearly in your summary which path you took (stylo or fallback) and why.
- If fallback: keep the code structured so stylo could be swapped in later (e.g., a `ComputedStyle` struct + a `cascade()` trait boundary).

Do not let the spike consume the whole task — timebox it. The deliverable is a working skeleton, not a perfect cascade. A hand-rolled cascade handling inline `<style>` rules + a few selectors (element, class, id, descendant) is perfectly acceptable for "styled text".

## Stage 2: Trivial block layout

- Compute style for the document.
- Lay out block boxes top-to-bottom within the content box (page size minus margins).
- For each block: position at current y, height from its content (text line height for now; `margin`/`padding` collapse NOT required — just stack).
- Text: break text into lines greedily at available width (simple first-fit is fine; Knuth-Plass is a LATER issue). Shaping via `rustybuzz`/`harfrust` is NOT required for the skeleton — use krilla's text API with a built-in/embedded font (e.g., embed a system font or ship one) and simple byte-string text.
- Track vertical position; when a block would overflow the page, start a new page (this is the seed of the fragmentation model — keep it trivial).

## Stage 3: krilla PDF export

- Open a `krilla::Document`, create a page per laid-out page, set size from CLI flags (convert `5in` → points: 1in = 72pt).
- Draw each block: `background-color` as filled rect, text at its position with its `color`/`font-size` (approximate font-size scaling — krilla's text API will define this).
- Use krilla's high-level text drawing (it wraps pdf-writer). Font: embed a simple TTF — check what krilla's examples use (they typically embed a font from assets); if no font ships with the crate, use a system font path or `fontdb` to load one (e.g., `/System/Library/Fonts/Supplemental/Arial.ttf` or similar on macOS — verify the path exists on THIS machine first with `ls`).

## Stage 4: Arithmetic decision — RECORD IT

The Architecture note (`/Users/elijah/notes/brain/Projects/Typeanvil/Architecture.md`) has an open question: fixed-point TeX-style scaled points vs. strictly-controlled f64. **Decide it for the walking skeleton and record it.**

My recommended decision (adopt unless you find a strong reason not to): **strictly-controlled f64 in a `Scalar` newtype wrapper** (mirroring Typst's approach — they use f64 wrapped in `Scalar`), with the rule: no fused-multiply-add, no `-C target-cpu` floating-point reassociation, pin the crate to IEEE-754 semantics; revisit fixed-point only if cross-platform bit-identical output proves to need it. **Append to `Architecture.md` under "Core Decisions" a line: `DECIDED 2026-08-16: Arithmetic = strictly-controlled f64 in a Scalar newtype (Typst-style); fixed-point deferred unless cross-platform divergence appears.`** (Also update the Open Questions checkbox.)

## Verification (run these yourself before finishing)

1. `cd /Users/elijah/workspace/typeanvil/engine && cargo build --release` — must succeed (if stylo dep tree is too heavy for release build time, `cargo build` debug is acceptable but note it).
2. `cargo test` — your smoke test must pass.
3. Create a fixture `/tmp/hello.html`:
   ```html
   <html><head><style>h1 { color: #e63946; font-size: 28px; } p { color: #06d6a0; font-size: 14px; }</style></head>
   <body><h1>Hello, page 1</h1><p>This is the Typeanvil walking skeleton.</p></body></html>
   ```
4. Run the EXACT CLI contract:
   ```
   ./target/release/typeanvil render /tmp/hello.html --page-width 5in --page-height 3in --margin-top 0.5in --margin-right 0.5in --margin-bottom 0.5in --margin-left 0.5in --base-url http://127.0.0.1:9/ -o /tmp/hello.pdf
   ```
   Verify exit 0, `/tmp/hello.pdf` exists and is non-empty (>1KB).
5. **Determinism check**: run it twice, `shasum` both PDFs — must be identical.
6. Report the file sizes and shasums in your summary.

## Conventions

- Rust 2021 edition. Format with `cargo fmt`. No unsafe unless absolutely required (avoid it).
- Pure Rust, no C FFI. No `unsafe` blocks unless a dep forces it.
- Errors: `anyhow::Result` is fine for a skeleton.
- Keep it SIMPLE — this is a walking skeleton, not the real engine. Resist gold-plating (no flex/grid, no Knuth-Plass, no fragmentation beyond page-overflow, no incremental relayout).
- If you deviate from this prompt for a load-bearing reason, document the deviation in a "Deviation" section in your final summary and explain why.
