# Typeanvil

Production-grade HTML/Markdown → PDF typesetting engine. A PrinceXML competitor built AI-first.

**Wedge:** paged-media CSS excellence (`@page`, fragmentation, running headers) for reports and invoices — not full CSS parity. Win the niche, expand outward.

**First deliverable:** WPT-based conformance harness, built *before* the engine. The test oracle is the moat.

**Typography differentiator:** Knuth-Plass total-fit line breaking + character protrusion as defaults — nobody in the HTML-to-PDF space ships this, not even Prince.

## Project notes (Obsidian vault)

- `brain/Projects/Typeanvil/Project Overview.md` — founding research, market gap, naming, architecture principles
- `brain/Projects/Typeanvil/Pricing Strategy.md` — pricing vs Prince XML ($3,800/server) and DocRaptor
- `brain/Projects/Typeanvil/Architecture.md` — architecture sketch (Rust, wrap-vs-build, fragmentation-first)
- `research/` (this repo) — the three underlying research briefs: LayoutNG fragmentation, Rust ecosystem, WPT harness

## Status

Named 2026-08-13. Pre-code. Open next steps:

- [ ] Trademark + domain + package-registry clearance for "Typeanvil"
- [ ] Architecture sketch: fragmentation-first core; wrap-vs-build boundaries (HarfBuzz, ICU)
- [ ] WPT conformance harness design
- [ ] Prototype Knuth-Plass line breaker + protrusion (early demo candidate)
