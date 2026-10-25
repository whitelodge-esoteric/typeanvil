# MCP contract conformance suite

Language-neutral cases for the MCP tool contract
(`docs/specifications/mcp-tool-contract.spec.md`, schemas in `schemas/mcp/v1/`).
Both servers run them: the local server (`typeanvil mcp`) in this repo's CI,
the cloud server in the Platform repo's CI.

## Layout

- `cases/*.json` — one case per file.
- `fixtures/` — documents the cases read. The runner starts the server with
  this directory as its working directory, so a relative `path` input such as
  `fixtures/simple.html` resolves here.

## Case format

```json
{
  "id": "render-inline-content",
  "tool": "render",
  "runtimes": ["local", "cloud"],
  "input": { "document": { "content": "<p>Hi</p>" } },
  "repeat": 2,
  "expect": {
    "ok": true,
    "checks": [{ "pointer": "/page_count", "equals": 1 }],
    "same_across_repeats": ["/sha256"]
  }
}
```

- `tool` is a contract tool name, or `tools/list` for the listing case.
- `runtimes` lists where the case applies (`local`, `cloud`).
- `input` is the tool's `arguments`. The string `{{BASE_URL}}` is replaced
  with the runner's fixture HTTP server (it serves `fixtures/`).
- `expect.ok: true` means a success result; `expect.error: "<CODE>"` means a
  tool error with that exact code.
- Each check has a JSON `pointer` into the result and one of `equals`,
  `exists` (bool), `min`, `max`, `contains` (array or string contains).
- `repeat` runs the same call N times; `same_across_repeats` lists pointers
  whose values must be equal in every run.

## What the runner checks on every case

1. Success results validate against the tool's output schema.
2. Error results validate against `common.schema.json#/$defs/Error`.
3. Every result and error carries a valid `environment` whose `runtime`
   equals the target's runtime.
4. `tools/list` returns the six tools, and each `inputSchema` and
   `outputSchema` accepts and rejects exactly what the files in
   `schemas/mcp/v1/` accept and reject (the published schemas may be
   dereferenced copies).
5. Each limit the server sets (non-null) in `capabilities.limits` has a case
   with id `limit-<name>`.

The local runner is `engine/tests/it/mcp_conformance.rs`
(`cargo test --test it mcp_conformance`).
