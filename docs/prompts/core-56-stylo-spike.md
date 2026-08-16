# Task: stylo 0.20 binding spike (CORE-56)

Bind Servo's `stylo` 0.20 as the real CSS cascade engine in the Typeanvil engine,
replacing the hand-rolled cssparser cascade — through the existing `ComputedStyle`
seam so layout/PDF code does not change.

This is a SPIKE with a clear verdict to reach: **can stylo be driven on this seam in
a focused session?** If yes: land it (tests pass, determinism holds). If genuinely
blocked: commit a short `SPIKE.md` documenting the exact blocker and revert to the
hand-rolled cascade — the fallback stays the shipped state until stylo works.

## Ground truth (learned from a previous failed attempt — DO NOT repeat these errors)

A prior run wrote ~1,200 lines of binding and failed on 7 mechanical errors. These are
FIXED facts, verified against docs.rs today:

1. **The crate's lib name is `style`** (crate name `stylo`, version 0.20.0). Docs:
   `https://docs.rs/stylo/0.20.0/style/`. Use `use style::...` and `stylo = "0.20"` in
   Cargo.toml.
2. **`Device` lives at `style::device::Device`** — NOT `style::media_queries::Device`.
3. **`FontMetricsProvider` lives under `style::device::servo::FontMetricsProvider`**
   — NOT `style::servo::media_queries::FontMetricsProvider`.
4. **`ns!` macro is NOT public API.** Build namespaces with `web_atoms::Namespace`
   and `string_cache` directly (e.g. `Namespace::from(ns!(html))` is unavailable —
   construct `web_atoms::ns!` equivalents via `string_cache::Atom`/`Namespace::from`).
5. **Missing deps that are required**: `url`, `mime` (stylo's servo feature pulls
   them; add explicitly). `PointerCapabilities` may have moved — check docs.rs for its
   actual location in 0.20 before using it; if it's gone, use `Default::default()` on
   the device or whatever 0.20 exposes.
6. **`stylo_dom` 0.20.0 companion crate exists** (`dom = { package = "stylo_dom",
   version = "0.20" }`) — it provides DOM state types the trait family needs. Use it.

## What to read BEFORE writing code (mandatory)

- `engine/src/css.rs` — the seam: `ComputedStyle` is the output contract, `cascade`
  is the single entry point. Layout and PDF read ONLY `ComputedStyle`. Do not change
  that contract.
- `engine/src/dom.rs`, `engine/src/geom.rs`, `engine/src/main.rs` — the DOM tree you
  own, the `Scalar` type, the CLI contract.
- `https://docs.rs/stylo/0.20.0/style/` — browse the actual modules: `device`,
  `properties`, `dom`, `servo`, `stylist`, `style_resolver`. Use docs.rs freely;
  pattern-matching stylo internals from memory is exactly how the last attempt broke.
- `https://docs.rs/stylo_dom/0.20.0/stylo_dom/` — the companion crate surface.
- Research brief `research/typeanvil-rust-typesetting-research.md` (stylo section).

## What to build

Replace the hand-rolled cascade's internals with stylo:

1. Implement the `TElement` / `TNode` / `TDocument` trait family (from `style::dom`)
   plus `selectors::Element` over the existing html5ever-backed tree in `dom.rs`.
   Minimal correct surface: tag/class/id, parent/child traversal, attr lookup,
   `is_html_element`, style attribute if trivial.
2. Drive stylo to produce computed values: build a `Device` (print media! —
   `MediaType::print()`), parse the stylesheet with `style::stylesheets::Stylesheet`
   (or `cssparser` → stylo parser), run matching/cascade via
   `style::style_resolver` / `stylist` to get `ComputedValues` per element.
3. Convert the `ComputedValues` fields the engine actually uses (color, font-size,
   font-family, display, margins, padding, background-color) into the existing
   `ComputedStyle` struct. Keep the seam identical.
4. Keep the CLI contract byte-for-byte (render flags, --base-url, -o). Keep output
   deterministic (the `output_is_deterministic` test must pass).
5. The hand-rolled cascade may be deleted ONLY when stylo path passes all tests.

## Scope limits (spike discipline)

- ONLY the cascade is in scope. No @page, no media queries beyond print, no flex/grid,
  no selector features beyond what the current tests + fixture need (element/class/id/
  descendant, inheritance of the six properties the seam carries).
- Do not touch `harness/`, `layout.rs` beyond what the seam forces, or `pdf.rs`.
- If stylo needs a `Vec`-returning traversal or generics that fight the existing tree,
  prefer the smallest local change that keeps the seam contract.

## Verification (run all before finishing)

1. `cargo build --release` — clean, zero warnings if possible.
2. `cargo test --release` — the existing 3 tests pass (non-empty PDF, pagination,
   byte-determinism), plus any new test you add for cascade correctness (e.g. a
   specificity case: `p` vs `.cls` vs `#id`).
3. Render the fixture twice, `shasum` the PDFs, assert identical bytes.
4. Render a fixture that uses each supported selector type + inheritance; confirm the
   styled output visually via `pypdfium2` rasterization if easy (`.venv/bin/python`).
5. Report: which stylo traits you implemented, what moved vs the seam, the verdict
   (landed / blocked with exact reason), and any deviations from this spec.
