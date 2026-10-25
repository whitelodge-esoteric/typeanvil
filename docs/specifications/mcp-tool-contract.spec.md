---
title: MCP Tool Contract (local stdio and cloud remote)
slug: /specifications/mcp-tool-contract
type: spec
status: draft
owner: maintainers
created: 2026-10-24
updated: 2026-10-24
sidebar_position: 73
tags: [mcp, ai-native, cli, cloud, contract]
spec_id: mcp-tool-contract
issue_id: CORE-574
applies_to:
  - schemas/mcp/v1/
dependencies:
  - cli-surface
  - diagnostics
  - pdf-metadata
  - capability-manifest
---

# MCP Tool Contract (local stdio and cloud remote)

## Overview

TypeAnvil reaches an agent in two ways. The local server (`typeanvil mcp`)
runs as a child process over stdio. The cloud server (for example
`mcp.typeanvil.com`) runs as a remote MCP server over HTTPS.

Both servers shall expose the same tools with the same names, inputs and
result shapes. A prompt or workflow written against one works against the
other. The user picks a connection, not a different product.

This spec defines the tool contract. It ships no code. The local server
(a child of CORE-570) and the Platform cloud endpoint implement it.

Machine-readable schemas are in `schemas/mcp/v1/` (JSON Schema 2020-12).
The schemas are normative for field names and types. This document is
normative for behavior.

## Goals

1. One tool set, one schema set, two transports.
2. Inputs that never drift: path, inline content or URL, in every tool.
3. A result shape that does not change with the environment.
4. Every result names the environment that made it.
5. Stable error codes that agents can branch on.
6. A contract test suite that both servers pass.

## Non-Goals

1. Transport details beyond what the contract needs. MCP defines stdio and
   Streamable HTTP.
2. Cloud authentication design (API key, OAuth). The Platform repo owns it.
3. The `typeanvil mcp --cloud` proxy mode. See Deferred options.
4. Cross-environment byte equality. See Environment identity.

## Contract version

1. The contract version is `MAJOR.MINOR`. This spec defines `1.0`.
2. A change that adds an optional input field, an optional result field, a
   tool, or an error code is a MINOR change.
3. A change that removes or renames a tool, field or error code, makes an
   optional input required, or changes a type or meaning is a MAJOR change.
4. Schema files live under `schemas/mcp/v<MAJOR>/`. A MAJOR change adds a
   new directory. The old directory stays until its removal is announced.
5. Clients shall ignore unknown result fields. Servers shall reject unknown
   input fields with `INVALID_INPUT` (the input schemas set
   `additionalProperties: false`).
6. Every result carries `environment.contract_version`.

## Tools

Tool names are exact. Each tool has an input schema and an output schema:

| Tool | Purpose | Input schema | Output schema |
|---|---|---|---|
| `render` | HTML or Markdown to PDF | `render.input` | `render.output` |
| `validate` | Render report and assertions, no PDF returned | `validate.input` | `validate.output` |
| `inspect_page` | Layout boxes and a PNG of one page | `inspect_page.input` | `inspect_page.output` |
| `capabilities` | What this server supports and its limits | `capabilities.input` | `capabilities.output` |
| `explain` | Computed style and page placement for a selector | `explain.input` | `explain.output` |

Schema file names are `<name>.input.schema.json` and
`<name>.output.schema.json`. Shared definitions are in `common.schema.json`.

### Behavior

1. A server shall register all five tools, using the exact names above.
2. A server shall publish the input schema of each tool as the MCP tool
   `inputSchema` and the output schema as `outputSchema`.
3. A successful result shall return the output object as MCP
   `structuredContent`. It shall also return the same object as JSON text in
   `content[0]`, for clients that do not read `structuredContent`.
4. `render` shall return a PDF through `artifact` (see Output forms). The
   PDF shall be the same bytes the CLI `render` command makes for the same
   input and options in the same environment.
5. `validate` shall run the full layout pass and return no PDF. It shall
   return `ok: false` when any diagnostic has severity `error` or any
   assertion fails. A failed assertion is a normal result, not a tool error.
6. `validate` diagnostics shall use the event codes of `diagnostics.spec.md`.
7. `inspect_page` shall return the boxes of one page in document order. When
   `image.include` is true it shall return a PNG at the requested DPI. A page
   number above `page_count` shall return `PAGE_OUT_OF_RANGE`.
8. `inspect_page` shall set `truncated: true` when it dropped boxes because
   of `max_boxes`.
9. `capabilities` shall report the limits that apply to the caller. A limit
   with no enforcement shall be `null`, never missing.
   The CSS support data behind `css` is the capability manifest of
   `capability-manifest.spec.md` (`typeanvil capabilities --json`). A later
   MINOR version may embed it; 1.0 links to the support guide only.
10. `explain` shall return matches in document order, up to `max_matches`.
    A selector that does not parse shall return `SELECTOR_INVALID`. A valid
    selector with zero matches shall return a normal result with
    `match_count: 0`.
11. `explain.placements` shall list one entry per page fragment, so an
    element split across pages has several entries.
12. Tools shall be stateless. Two calls share nothing. Each call carries its
    full input. (A later version may add sessions. That is a MINOR change.)

## Input forms

Every document input (`document`, each item of `stylesheets`) is a
`ContentSource`. It has exactly one of:

- `path`: a file path
- `content`: inline text
- `url`: an `http` or `https` URL

Asset inputs (`assets`) are `path`, `url` or `content_base64`.

### Behavior

1. A server shall accept `content` and `url` in every environment.
2. The local server shall accept `path`. A relative path resolves against
   the working directory of the server process.
3. The cloud server shall NOT accept `path`. It shall return
   `PATH_INPUT_UNSUPPORTED` with `field` set to the JSON pointer of the
   offending input. It shall not read any file for the caller.
4. A `ContentSource` with zero or more than one of `path`, `content`, `url`
   shall return `INPUT_SOURCE_AMBIGUOUS`. The server shall check this rule
   BEFORE it validates the rest of the input against the published
   `inputSchema`. The `oneOf` in the schema is a hint for clients. A request
   that breaks it shall get `INPUT_SOURCE_AMBIGUOUS`, never `INVALID_INPUT`.
5. `format` selects `html` or `markdown`. When absent the server infers it
   from the file extension or URL path (`.md`, `.markdown` mean Markdown).
   Inline content with no `format` is HTML.
6. Relative references in inline content resolve against `base_url`. When
   `base_url` is absent they resolve against nothing, and a reference that
   needs a base shall produce a diagnostic, not a failure.
7. `assets` keys are references as written in the document. A matching
   asset entry shall win over network or file lookup.
8. A local server may restrict readable paths to configured roots. A path
   outside the roots shall return `PATH_OUTSIDE_ROOT`.
9. A missing file shall return `PATH_NOT_FOUND`.
10. A URL fetch failure shall return `URL_FETCH_FAILED`. A URL the server
    refuses by policy (the cloud server shall refuse private, loopback and
    link-local addresses) shall return `URL_BLOCKED`.
11. The cloud server shall apply the same input-form rules to `assets`.
    Asset `path` returns `PATH_INPUT_UNSUPPORTED` there.

## Output forms

`render.output.artifact` has a `kind`:

| kind | Fields | Allowed in |
|---|---|---|
| `path` | `path` | local |
| `bytes` | `data_base64` | local, cloud (within output size limit) |
| `link` | `url`, `expires_at` | cloud; local may use a `file://` URL |

### Behavior

1. The request field `output.mode` selects the kind. `auto` is the default.
   `auto` means `bytes` locally and `link` in the cloud.
2. A server that cannot give the requested mode shall return
   `OUTPUT_FORM_UNSUPPORTED`. The cloud server returns it for `path`.
3. `mode: path` requires `output.path`. The local server writes the file and
   returns the absolute path.
4. Every `render` result shall carry `page_count`, `byte_size` and `sha256`
   of the PDF, whatever the kind. A client can check a download against
   `sha256`.
5. A cloud `link` shall stay valid for at least 15 minutes. `expires_at` is
   the exact end time.
6. The rest of the result is the same in every mode.

## Environment identity

Every successful result and every error carries `environment`:

```json
{
  "contract_version": "1.0",
  "engine_version": "2026.10.2",
  "font_bundle_version": "liberation-2.1.5+tf1",
  "runtime": "local"
}
```

### Behavior

1. `engine_version` is the engine release version string. A development
   build shall append the short commit hash.
2. `font_bundle_version` identifies the exact font bundle the engine
   resolves bundled faces from. It shall change when any bundled font file
   changes.
3. `runtime` is `local` or `cloud`.
4. Output is deterministic within one environment: same input, same
   options, same `engine_version`, same `font_bundle_version`, same host
   fonts (when `fonts.system_fonts` is true) gives the same PDF bytes.
5. Output between environments may differ. When it does, an agent shall
   compare the two `environment` objects first. Differences in
   `engine_version` or `font_bundle_version` explain it.
6. The cloud server shall report `fonts.system_fonts: false`. The local
   server reports what the host allows. System fonts are the most common
   cause of local and cloud differences, so `capabilities` states it.

## Limits and errors

Errors are MCP tool results with `isError: true`. The payload follows
`common.schema.json#/$defs/Error`: `code`, `message`, `retryable`,
`environment`, and optionally `field` and `limit`. A tool error result shall
validate against that `Error` schema. It shall not be checked against the
tool `outputSchema`, which describes successful results only.

Protocol failures (unknown tool, malformed JSON-RPC) use MCP protocol errors.
Everything about a document or a limit uses a tool error.

### Error codes

| Code | Meaning | Retryable |
|---|---|---|
| `INVALID_INPUT` | Input fails its schema | no |
| `INPUT_SOURCE_AMBIGUOUS` | Not exactly one of path, content, url | no |
| `PATH_INPUT_UNSUPPORTED` | Path given to a server with no file access (cloud) | no |
| `PATH_NOT_FOUND` | File missing or unreadable | no |
| `PATH_OUTSIDE_ROOT` | Path outside configured roots | no |
| `URL_FETCH_FAILED` | Fetch error, non-2xx, or fetch timeout | yes |
| `URL_BLOCKED` | URL refused by policy | no |
| `OUTPUT_FORM_UNSUPPORTED` | Requested output mode unavailable | no |
| `PARSE_FAILED` | Input cannot be parsed into a document | no |
| `RENDER_FAILED` | Engine failure while rendering | no |
| `PAGE_OUT_OF_RANGE` | `page` above `page_count` | no |
| `SELECTOR_INVALID` | Selector does not parse | no |
| `SELECTOR_NO_MATCH` | Reserved. `explain` does not use it in 1.0 | no |
| `LIMIT_TIMEOUT` | Call exceeded `timeout_seconds` | yes |
| `LIMIT_INPUT_SIZE` | Input above `max_input_bytes` | no |
| `LIMIT_OUTPUT_SIZE` | Output above `max_output_bytes` | no |
| `LIMIT_PAGE_COUNT` | Document above `max_pages` | no |
| `LIMIT_ASSET_COUNT` | Assets above `max_assets` | no |
| `QUOTA_EXCEEDED` | Account quota used up (cloud) | no |
| `RATE_LIMITED` | Too many calls (cloud) | yes |
| `UNAUTHENTICATED` | Missing or bad credentials (cloud) | no |
| `FORBIDDEN` | Credentials lack permission (cloud) | no |
| `INTERNAL` | Server fault | yes |

### Behavior

1. The set of codes is closed per contract version. A server shall not emit
   a code outside this table.
2. A `LIMIT_*` error shall include `limit` with `name`, `value`, `unit` and,
   when known, `observed`. `name` equals the matching key of
   `capabilities.limits`.
3. Limit values differ by environment. The local server may set all limits
   to `null`. The cloud server shall publish its values in `capabilities`.
4. `QUOTA_EXCEEDED` and `RATE_LIMITED` are cloud-only. The local server
   shall not emit them.
5. `UNAUTHENTICATED` and `FORBIDDEN` are cloud-only. The local stdio server
   has no credentials.
6. The `message` is for people and may change. Agents shall branch on
   `code`.
7. `retryable: true` means the same call may succeed later with no change.

## Naming when both are configured

1. Docs and installers shall recommend two server names in the agent
   config: `typeanvil-local` and `typeanvil-cloud`.
2. A single-server setup may use `typeanvil`.
3. Tool names do not carry a prefix. The client adds the server name.
4. The server `name` field in the MCP `initialize` result shall be
   `typeanvil`. The environment is in `runtime`, not in the server name.

## Deferred options

- `typeanvil mcp --cloud`: the local process proxies tool calls to the cloud
  server. Not part of 1.0. It would need a rule for `path` inputs (upload,
  or reject). A later version of this spec decides it.
- Stateful sessions (upload a document once, query it many times).
- Streaming progress for long renders.

## Conformance

A contract test suite shall exist and both servers shall pass it.

### Behavior

1. The suite lives in the repo as language-neutral cases:
   `tests/mcp-conformance/cases/*.json`. Each case has an `id`, a `tool`,
   an `input`, the `runtimes` it applies to (`local`, `cloud` or both) and an
   `expect` block (`ok` or an exact error `code`, plus JSON-pointer checks).
2. A runner takes a server target (a stdio command or an HTTPS URL plus
   credentials) and runs every case that applies to its `runtime`.
3. The runner shall validate every result against the output schema, and
   every tool listing against the input schema.
4. A server passes when every applicable case passes and every result
   validates.
5. The suite shall run in CI for the local server, and in the Platform repo
   CI for the cloud server.
6. The minimum case set:
   - `tools/list` returns the five tool names with schemas equal to the files
     in `schemas/mcp/v1/`.
   - `render` with inline `content`, `url`, and (local only) `path`.
   - `render` with `path` on cloud returns `PATH_INPUT_UNSUPPORTED`.
   - `render` with two sources returns `INPUT_SOURCE_AMBIGUOUS`.
   - `render` with `output.mode: path` on cloud returns
     `OUTPUT_FORM_UNSUPPORTED`.
   - Same `render` twice in one environment gives equal `sha256`.
   - `validate` with passing and failing assertions.
   - `inspect_page` with a valid page and a page out of range.
   - `explain` with a match, no match and an invalid selector.
   - `capabilities` agrees with observed behavior: `input_forms.path` equals
     whether a path input works.
   - Each limit a server sets in `capabilities.limits` produces its
     `LIMIT_*` code when exceeded.
   - Every result and error carries a valid `environment`.
7. The schema files shall pass JSON Schema 2020-12 meta-validation. That
   check runs in the repo gate when the suite lands.

## Acceptance criteria

1. Given the spec, when `scripts/validate_docs.py` runs (PyYAML installed),
   then it passes.
2. Given `schemas/mcp/v1/`, when each file is loaded as JSON, then it
   parses and carries a `$id` under `https://typeanvil.com/schemas/mcp/v1/`.
3. Given the five tool names, when the schema directory is listed, then each
   has an `.input.schema.json` and an `.output.schema.json` file.
4. Given a cloud `render` call with `document.path`, when the contract is
   followed, then the result is `PATH_INPUT_UNSUPPORTED` with `field`.
5. Given any result, then `environment` has all four fields.
6. Given the error table, then each code in it appears in
   `common.schema.json#/$defs/ErrorCode`, and the reverse.

Criteria 2, 3 and 6 are checked by `tests/mcp-conformance` when it lands.
They are checked by hand for this spec.

## Implementation pointers

- Local server: child issue under CORE-570 (`typeanvil mcp`, stdio).
- Cloud endpoint: Platform repo. The Platform team implements this spec and
  the schemas in `schemas/mcp/v1/`, and runs the conformance suite.

## References

- CORE-570 (AI-native epic), CORE-574 (this spec)
- `docs/specifications/cli-surface.spec.md`
- `docs/specifications/diagnostics.spec.md`
- MCP specification: https://modelcontextprotocol.io/specification
