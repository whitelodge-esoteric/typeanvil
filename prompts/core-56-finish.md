# Task: finish CORE-56 stylo spike — wire the trait impls into the cascade

A previous run wrote `engine/src/stylo_dom.rs` (813 lines): the `TDocument` /
`TNode` / `TElement` / `TShadowRoot` + `selectors::Element` trait family over the
arena DOM. It is **orphaned** — `engine/src/main.rs` declares `mod css` only, and
`engine/src/css.rs` is still the hand-rolled cssparser cascade. Cargo.toml was
updated (stylo + stylo_dom + stylo_traits + selectors + url + mime; cssparser
REMOVED), so the crate currently fails to build with 2 `cssparser` errors.

**Your job: complete the integration so the crate builds, tests pass, and stylo is
the cascade engine — or write `engine/SPIKE.md` and revert cleanly if it truly can't
work (do not fake a win).**

## Current state (verified)

- `engine/src/stylo_dom.rs` — trait impls. Read it fully first; it documents its own
  design (TyElement = Copy (NodeId, &TyBackend), atom interning at backend
  construction, UnsafeCell-backed element-data map).
- `engine/src/dom.rs` — the arena DOM (`Dom`, `NodeId`, `NodeKind`).
- `engine/src/css.rs` — the seam: `pub struct ComputedStyle` (the ONLY contract
  layout/pdf read) and `pub fn cascade(...)` entry point. The hand-rolled cascade
  lives here and must be replaced while keeping `ComputedStyle`'s fields identical.
- `engine/src/main.rs` — `mod css;` at line 14; CLI contract (render, page flags,
  --base-url, -o). Do NOT change the CLI contract.
- `engine/src/geom.rs` — `Scalar` newtype (f64), `px_to_pt`.

## Ground truth (from the CORE-56 issue — do not repeat these errors)

1. Lib name is `style` (crate stylo 0.20); `use style::...`.
2. `Device` = `style::device::Device` (NOT media_queries). Print media:
   `MediaType::print()`.
3. `FontMetricsProvider` = `style::device::servo::FontMetricsProvider`.
4. `web_atoms::ns!` is not public — namespaces via `Namespace::from(...)`.
5. Add back `cssparser = "0.35"` to Cargo.toml IF the hand-rolled cascade stays
   during transition (it is still referenced). If you fully replace css.rs's
   parsing, remove the dependency instead — but the build must pass either way.

## Steps

1. Make the build green first (add cssparser back if needed) so you can iterate.
2. Wire `mod stylo_dom;` and the cascade: replace the hand-rolled cascade's guts in
   css.rs with stylo — build a `Device` (print), parse the stylesheet with
   `style::stylesheets`, run matching/cascade to `ComputedValues`, convert the
   fields the engine uses (color, font-size, font-family, display, margins,
   padding, background-color) into `ComputedStyle`. Keep the seam contract.
3. Keep selector support: element / .class / #id / descendant + inheritance.
4. Delete the hand-rolled cascade code ONLY once the stylo path passes all tests.
5. If you hit a real wall, write `engine/SPIKE.md` with the exact blocker and
   restore a building state (revert to hand-rolled), then stop — that's a valid
   spike outcome.

## Verification (run all)

1. `cargo build --release` — clean.
2. `cargo test --release` — 3 existing tests (non-empty PDF, pagination,
   byte-determinism) + add a specificity test (`p` vs `.cls` vs `#id`).
3. Render fixture twice, `shasum` both PDFs, assert identical bytes.
4. Report: what you wired, what moved vs the seam, verdict (landed / blocked),
   deviations.
