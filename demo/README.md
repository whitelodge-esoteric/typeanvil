# Demo — two render tracks

Both tracks render the same engine and publish a browsable gallery. They differ
in geometry, in whether a reference engine is involved, and in audience.

| Track | Directory | Engines | Geometry | Gallery |
|---|---|---|---|---|
| Comparison | [`demo/corpus/`](corpus/) | TypeAnvil **and** Prince | 5in × 3in @ 96 DPI | [`corpus/README.md`](corpus/README.md) |
| Showcase | [`demo/showcase/`](showcase/) | TypeAnvil only | US Letter @ 300 DPI | [`showcase/README.md`](showcase/README.md) |

The comparison track is the measurement instrument: identical CLI flags to both
engines, per-page pixel diffs, a scoreboard, and a triage bucket per document.
The showcase track renders the same engine at the geometry documents actually
print at, and targets prospects rather than a diff.

## Rebuild

```sh
scripts/build-demo.sh      # comparison: renders through both engines
scripts/build-showcase.sh  # showcase: TypeAnvil only, 300 DPI
```

Each track owns its own directory: every fixture, manifest, asset, and
generated page lives inside it, and a build rewrites only its own README's
generated section. Prince is used by the comparison track alone, under its free
non-commercial license — install and license notes are in
[`demo/corpus/README.md`](corpus/README.md).

## Contracts

- `docs/specifications/visual-comparison-demo.spec.md` — comparison track
- `docs/specifications/showcase-render.spec.md` — showcase track
