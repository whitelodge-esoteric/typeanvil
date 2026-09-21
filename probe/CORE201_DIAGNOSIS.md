# CORE-201 Diagnosis — border-side-color-repaints

Observed behavior and evidence of record for the per-side border colour fix.
Follows `docs/conventions/issue-evidence.md`.

## Observed behavior

`border-bottom-color: cyan` (with a full `border: 10px solid black`
shorthand) repainted ALL FOUR border bands cyan instead of only the bottom
band. Confirmed at the exact fixture of the issue
(`probe/core179-sidecolors.html`), the same probe CORE-179 used.

Command (dev container, worktree):

```
./scripts/dev-container.sh python3 /work/probe/core201-hist.py \
  /work/engine/target/debug/typeanvil /work/probe/core201-before.pdf
```

- **Baseline build**: `engine/target/debug/typeanvil` at
  `git rev-parse HEAD` = worktree branch point
  `ebboston/core-201-engine-per-side-border-colours-border-side-color-repaints`
  (based on `origin/release/2026.9`).
- **Fixture revision**: `probe/core179-sidecolors.html` (200x100px page;
  `.box { width:100px; height:40px; border:10px solid black;
  border-bottom-color: cyan; }`).
- **Output**: `probe/core201-before.pdf` (1 page, 150x75pt at 5in x 3in CLI
  geometry) → rasters to `probe/core201-before.png`.
- **Histogram (pre-fix)**: cyan (255,255,0 in the BGR raster buffer = rgb
  cyan) covered the full box bbox `(0,0)-(89,44)` — all four bands.
  Visual confirmation: `vision_analyze` on `core201-before.png` saw all four
  bands cyan.

## Expected CSS behavior

css-backgrounds-3 §4.5: `border-bottom-color` sets ONE side's color. The
other three sides keep the shorthand's `black` (css-backgrounds-3 §3
`currentColor` resolution — here a declared black). Only the bottom band
paints cyan.

## Suspected cause

Two cooperating seams collapsed per-side colors onto one shared slot:

1. The stylo conversion in `engine/src/css.rs` read only
   `clone_border_top_color()` into `ComputedStyle::border_color`.
2. The hand-rolled `border` author-CSS pass (`borders::parse_decl`) mapped
   `border-<side>-color` to `BorderDecl::Color`, the all-four-sides variant,
   so the per-side longhand clobbered every band with its color
   (`ComputedStyle::border_color` single slot, one cascade winner).

The margin-box path had the same shape: `MarginBoxSpec` already stores
per-side `(width, colour)` pairs, but `layout.rs::border_paint_color`
returned the FIRST declared side's color and `BrandAttachment` painted all
sides with it.

## Isolating evidence (per side)

- **Stylo accessors exist**: all four `clone_border_{top,right,bottom,left}_color`
  confirmed in the generated stylo sources
  (`engine/target/debug/build/stylo-1845d1f92ce6ccda/out/properties.rs`),
  so per-side computed colors were available but discarded.
- **Post-fix histogram**: `probe/core201-after.pdf` → cyan only in the bottom
  band, bbox `(8,37)-(81,44)` (a horizontal strip at the box's bottom edge);
  black covers top/right/left. Raster `core201-after.png` — visual check
  confirmed black top/left/right, cyan bottom only.
- **Unit tests**: `engine/tests/core201_per_side_border_colors.rs` (4 tests)
  — shorthand+longhand, inline same, currentColor fallback, and margin-box
  per-side. All pass post-fix. RED before the fix by construction (the old
  model had no per-side `BorderBox` fields to assert on; every side carried
  the shared color).

## WPT gate (zero regression)

Harness run inside the dev container (`--wpt /main/.wpt`, `--engine cli`,
`--cli-cmd "/work/engine/target/debug/typeanvil render"`, `--workers 1`):

| fixture (filter) | pre-fix baseline | post-fix | delta |
|---|---|---|---|
| `page-orientation-on-portrait` (001/002/003) | 3 PASS | 3 PASS (`core201-after-orient.json`) | none |
| `monolithic-overflow-031-print` | FAIL (page count: test=5 ref=8) | FAIL (page count: test=5 ref=8) | none |

The orientation canaries are the color-sensitive mismatch pairs: their notref
(`page-orientation-on-portrait-002-notref.html`) contains no cyan, so
bottom-only cyan still mismatches as required — zero flips. The mono-031
failure is documented and unrelated to color (page count).

Full engine test suite: `cargo test` exits 0 (45 test binaries green).

## Fix summary

- `css.rs`: `ComputedStyle.border_color` → `border_{top,right,bottom,left}_color`;
  stylo conversion reads all four sides; `BorderDecl::SideColor` variant;
  per-side `Won` cascade-winner slots; shared `apply_decl` for the stylesheet
  and inline loops.
- `frag.rs`: `BorderBox.color` → `{top,right,bottom,left}_color`.
- `layout.rs` + `flex.rs`: per-side construction; html-root ring color follows
  the max-width side; margin-box per-side from `MarginBoxSpec` pairs;
  `border_paint_color` deleted.
- `pdf.rs`: `BorderItem` carries four colors; both stroke loops fill each band
  with its own color.
- Spec: `docs/specifications/paged-media-css.spec.md` — Non-Goals bullet
  removed, Behavior §18 + Acceptance Criterion 33 added, `updated` bumped.