---
title: Local MCP server and JSON request mode
slug: /specifications/mcp-server
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 74
tags: [mcp, ai-native, cli, contract]
spec_id: mcp-server
issue_id: CORE-575
applies_to:
  - engine/src/mcp/
  - engine/src/pipeline.rs
  - engine/src/main.rs
  - tests/mcp-conformance/
dependencies:
  - mcp-tool-contract
  - cli-surface
  - render-report
---

# Local MCP server and JSON request mode

## Overview

`typeanvil mcp` runs the MCP tool contract (`mcp-tool-contract.spec.md`)
as a child process over stdio. `typeanvil call <tool>` runs one tool call
from a shell: one JSON request on stdin, one JSON result on stdout. Both are
subcommands of the existing binary, so `npx typeanvil mcp` works through the
npm packages with no extra install.

This spec covers what the contract leaves to the local server: transport,
defaults, path roots, URL fetch, assets, the raster and the exit codes.
The contract stays normative for tool names, fields and error codes.

## Goals

1. Every contract tool, with the exact contract shapes, over MCP stdio.
2. The same shapes without MCP, for scripts (`typeanvil call`).
3. A `render` through MCP makes the same PDF bytes as the CLI.
4. One binary: no helper process, no runtime dependency for rasterization.
5. A conformance suite that both servers can run.

## Non-Goals

1. The cloud server and `typeanvil mcp --cloud` (contract Deferred options).
2. Limits. The local server enforces none and reports every limit as `null`.
3. Streamable HTTP transport. The local server speaks stdio only.
4. MCP resources, prompts, sampling and progress notifications.

## Behavior

### Transport

- **S1.** `typeanvil mcp` shall read JSON-RPC 2.0 messages from stdin, one
  per line, and write one reply per line to stdout. It shall write nothing
  else to stdout. It shall exit 0 at end of input.
- **S2.** The server shall answer `initialize`, `ping`, `tools/list` and
  `tools/call`. It shall echo the client's `protocolVersion` when it supports
  it (`2025-06-18`, `2025-03-26`, `2024-11-05`) and answer `2025-06-18`
  otherwise. `serverInfo.name` shall be `typeanvil` (contract Naming 4).
- **S3.** An unknown method shall return JSON-RPC error `-32601`, an unknown
  tool `-32602`, and a line that is not JSON `-32700`. The server shall keep
  running after any of them. Notifications get no reply.

### Tools

- **S4.** `tools/list` shall publish each contract schema with every
  `common.schema.json` reference replaced by the definition it names, so a
  client never fetches a second file. The published schema shall accept and
  reject exactly what the schema file accepts and rejects.
- **S5.** A call shall check the source rule first (`INPUT_SOURCE_AMBIGUOUS`)
  and the input schema second (`INVALID_INPUT`, `field` = the JSON pointer of
  the first violation), as contract Input forms Behavior 4 requires.
- **S6.** With no `options`, a document renders on a US Letter page
  (`8.5in` × `11in`) with zero margins. `options.tagged` defaults to false.
  Author `@page` rules override these defaults, as in the CLI.
- **S7.** `render` and the CLI `render` command shall call the same pipeline
  function (`pipeline::run`). For the same document file and the same page
  options, title, author and tagging, the PDF bytes shall be identical.
- **S8.** `render` returns `artifact.kind: bytes` for `auto` and `bytes`, and
  writes the file for `path` (the returned path is absolute). `link` returns
  `OUTPUT_FORM_UNSUPPORTED`. `capabilities.output_modes` is
  `["path", "bytes"]`.
- **S9.** `inspect_page` rasterizes the page from the exact PDF bytes that
  `render` returns, inside the engine, with the pure-Rust `hayro` rasterizer.
  The PNG width is `round_down(page_width_pt × dpi / 72)`. Boxes are the
  page's fragments in pre-order (document content, then margin boxes in paint
  order), with page-absolute top-left coordinates in points, rounded to
  0.01 pt.
- **S10.** `validate` reports the diagnostics a `--diagnostics json` render
  reports (Markdown, stylesheet, bookmark and layout events) and the faces
  that painted text. All faces are embedded, because the PDF writer embeds
  every face it draws with.
- **S11.** `explain` matches with the engine's stylo selector parser and
  matcher. A selector that does not parse returns `SELECTOR_INVALID` before
  the render. Without `properties`, `computed_style` lists the reported
  properties whose value differs from the root element's value.
- **S12.** A panic inside one call shall return `INTERNAL` for that call;
  the server keeps serving.

### Inputs

- **S13.** A relative `path` resolves against the server's working directory
  (contract Input forms Behavior 2). Relative references in a path document
  resolve against the document's directory.
- **S14.** `typeanvil mcp --root <dir>` (repeatable) restricts every read
  path, `base_url` directory and output path to the roots. A path outside
  them returns `PATH_OUTSIDE_ROOT`. With no `--root`, any path is allowed.
- **S15.** `url` inputs are fetched with the system `curl` (http and https
  only, 30 s timeout). Another scheme returns `URL_BLOCKED`. A failure or a
  non-2xx status returns `URL_FETCH_FAILED`.
- **S16.** Each asset is written to a per-call scratch directory. A `src` or
  `href` attribute, or a CSS `url()` in a `<style>` element or a request
  stylesheet, that equals an asset key points at that file. The scratch
  directory is removed when the call ends.
- **S17.** For inline and URL documents with no `base_url`, relative
  references resolve against the empty scratch directory, so they resolve to
  nothing (contract Input forms Behavior 6). A `base_url` that is an absolute
  path or a `file://` URL is the base directory.

### Environment

- **S18.** `engine_version` is the crate version. An unstamped build (crate
  version `0.0.1`) reports `0.0.1+<TYPEANVIL_COMMIT>` when that variable was
  set at build time, else `0.0.1+dev`. `font_bundle_version` is the render
  report's bundle fingerprint.
- **S19.** `capabilities` reports `system_fonts: true`, every limit `null`
  and `css.support_guide` = `https://typeanvil.dev/guides/css-support`.

### JSON request mode

- **S20.** `typeanvil call <tool> [--root <dir>]...` reads the whole of stdin
  as the tool's input object (empty input = `{}`) and prints the result
  object as one line of JSON. The object is the MCP `structuredContent`.
- **S21.** Exit code 0 means a success result, 1 a tool error result (the
  printed object is the `Error` payload), 2 a usage error (no tool name or an
  unknown flag). Invalid JSON or an unknown tool prints `INVALID_INPUT`.

## Interfaces

```text
typeanvil mcp [--root <dir>]...
typeanvil call <tool> [--root <dir>]... < request.json
```

```rust
// engine/src/pipeline.rs
pub fn run(input: RenderInput) -> anyhow::Result<Rendered>;
// engine/src/mcp/mod.rs
impl Server {
    pub fn new(roots: Vec<PathBuf>) -> Server;
    pub fn serve(&self, reader: impl BufRead, writer: impl Write) -> io::Result<()>;
    pub fn call(&self, tool: &str, args: Value) -> (Value, bool);
    pub fn call_json(&self, tool: &str, body: &str) -> (Value, bool);
}
// engine/src/css.rs
pub fn select_elements(dom: &Dom, selector: &str) -> Option<Vec<NodeId>>;
```

## Acceptance criteria

Every criterion maps to a test in `engine/tests/it/mcp_conformance.rs` or
`engine/src/mcp/` unit tests.

1. Given `tests/mcp-conformance/cases/`, when the suite runs against
   `typeanvil mcp`, then every local case passes and every result validates
   against its output schema (or the `Error` schema) with an independent
   JSON Schema 2020-12 validator
   (`conformance_cases_pass_against_local_server`).
2. Given `tools/list`, then it returns the five tools in contract order and
   each published schema agrees with its file on a probe set
   (S4; same test, `tools-list` case).
3. Given a document file and page options, when it renders through the CLI
   and through MCP (`bytes` and `path`), then the PDF bytes are identical,
   tagged and untagged (S7; `mcp_render_bytes_equal_cli_render`).
4. Given `typeanvil call`, then stdout is one JSON line with the MCP shape,
   exit 0 on success and 1 on a tool error (S20, S21;
   `call_mode_matches_mcp_shapes`).
5. Given `--root`, then a path outside the roots, a `..` escape and an
   output path outside the roots return `PATH_OUTSIDE_ROOT` (S14;
   `roots_restrict_paths`).
6. Given an unknown tool, an unknown method and a malformed line, then the
   server returns JSON-RPC errors and keeps serving (S3;
   `protocol_errors_are_jsonrpc_errors`).
7. Given `inspect_page` on a 2in page at 72 dpi, then the PNG is 144 px
   square and the boxes carry page-absolute rects (S9;
   `inspect_page_png_shows_page_content`).
8. Given the schema files, then each passes 2020-12 meta-validation and the
   error-code enum equals the contract's error table
   (`schema_files_are_valid_and_complete`, `error_codes_match_spec_table`).
9. Given the existing CLI `render` contract, then the full engine and WPT
   gates are unchanged (the pipeline extraction moves code without changing
   behavior).

## Edge cases

- A Markdown document from a URL is detected from the URL path extension.
- A `stylesheets` entry applies after the document's own sheets. For
  Markdown it is a `--css` sheet (after the default sheet).
- A `base_url` that is a remote URL cannot serve local reads; relative
  references resolve to nothing, with no error.
- `max_boxes` counts document boxes first, then margin boxes.

## References

- `docs/specifications/mcp-tool-contract.spec.md` (CORE-574)
- `docs/specifications/render-report.spec.md` (CORE-571)
- `docs/guides/mcp.md` — client configuration
- MCP specification: https://modelcontextprotocol.io/specification
