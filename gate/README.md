# Gate artifacts

Review records and this directory's policy are tracked. Captures and gate output
are not: they are large, and their identity is the evidence.

| Path | Tracked | Holds |
|---|---|---|
| `policy.json` | yes | acknowledged identity limits, allowed errors/skips/render failures |
| `reviews/<candidate-label>.json` | yes | one disposition per reviewed output change |
| `captures/<label>.json` | no | the recorded render evidence for one side |
| `out/` | no | gate reports and per-change side-by-side images |

`policy.json` acknowledges `fonts`: the engine reports no font inventory, so font
identity is recorded as `unknown` on both sides. Removing that acknowledgement
makes the gate refuse to pass, which is intentional.

See [the release gate runbook](../docs/operations/release-gate.md).
