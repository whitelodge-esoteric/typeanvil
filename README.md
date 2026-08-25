# Typeanvil

Production-grade HTML/Markdown → PDF typesetting engine. A PrinceXML competitor built AI-first.

**Wedge:** paged-media CSS excellence (`@page`, fragmentation, running headers) for reports and invoices — not full CSS parity. Win the niche, expand outward.

**First deliverable:** WPT-based conformance harness, built *before* the engine. The test oracle is the moat.

**Typography differentiator:** Knuth-Plass total-fit line breaking + character protrusion as defaults — nobody in the HTML-to-PDF space ships this, not even Prince.

## Documentation

`docs/` is the engineering source of truth — conventions, feature specs,
architecture, operations runbooks, lessons, and research. Start at
`docs/README.md`; the rules for writing docs are in
`docs/conventions/doc-conventions.md`.

## Project notes (Obsidian vault)

- `brain/Projects/Typeanvil/Project Overview.md` — founding research, market gap, naming, architecture principles
- `brain/Projects/Typeanvil/Pricing Strategy.md` — pricing vs Prince XML ($3,800/server) and DocRaptor
- `brain/Projects/Typeanvil/Architecture.md` — architecture sketch (Rust, wrap-vs-build, fragmentation-first)
- `docs/research/` — research briefs (LayoutNG fragmentation, Rust ecosystem, WPT harness)

## License

Typeanvil is free, open-source software under the **GNU Affero General
Public License v3.0 (AGPL-3.0-only)**. You may use, modify, and host it
freely; modifications offered as a network service must be published under
the same license. See [LICENSE](LICENSE) and
[docs/specifications/licensing-resolution.spec.md](docs/specifications/licensing-resolution.spec.md).

Contributions are welcome — by opening a pull request you agree to the
terms in [CLA.md](CLA.md).

## Status

Named 2026-08-13. Pre-code. Open next steps:

- [ ] Trademark + domain + package-registry clearance for "Typeanvil"
- [ ] Architecture sketch: fragmentation-first core; wrap-vs-build boundaries (HarfBuzz, ICU)
- [ ] WPT conformance harness design
- [ ] Prototype Knuth-Plass line breaker + protrusion (early demo candidate)
