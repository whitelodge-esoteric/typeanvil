---
title: MCP server and JSON requests
type: guide
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 8
tags: [guide, mcp, ai-native, cli]
slug: /guides/mcp
---

# MCP server and JSON requests

TypeAnvil includes an MCP server. An agent such as Claude, Cursor or Hermes
can then render, check and inspect documents itself. The server is a
subcommand of the normal binary, so it needs no separate install:

```bash
npx -y typeanvil mcp
```

The server talks MCP over stdio. It offers five tools:

| Tool | What it does |
|---|---|
| `render` | HTML or Markdown to PDF. Returns the PDF as base64, or writes it to a path. |
| `validate` | Lays the document out and checks assertions (page count, overflow, text on a page, fonts). Returns no PDF. |
| `inspect_page` | Returns the layout boxes of one page and a PNG of it. |
| `capabilities` | Tells the agent what this server supports. |
| `explain` | Returns the computed style and page placement of the elements that match a CSS selector. |

The full contract is in
[MCP tool contract](../specifications/mcp-tool-contract.spec.md).

## Connect a client

Each snippet starts the server with `npx`. To use an installed binary, put
`typeanvil` in `command` and use `["mcp"]` as the arguments.

**Claude Code:**

```bash
claude mcp add typeanvil -- npx -y typeanvil mcp
```

**Claude Desktop** (`claude_desktop_config.json`):

```json
{ "mcpServers": { "typeanvil": { "command": "npx", "args": ["-y", "typeanvil", "mcp"] } } }
```

**Cursor** (`.cursor/mcp.json` in the project, or `~/.cursor/mcp.json`):

```json
{ "mcpServers": { "typeanvil": { "command": "npx", "args": ["-y", "typeanvil", "mcp"] } } }
```

**Hermes** (`~/.hermes/config.yaml`):

```yaml
mcp_servers: { typeanvil: { command: npx, args: ["-y", "typeanvil", "mcp"] } }
```

When you configure both the local server and the TypeAnvil cloud server,
name them `typeanvil-local` and `typeanvil-cloud`. The tools have the same
names on both servers.

## Restrict file access

By default the server reads and writes any path that the user who started it
can access. To limit it to a folder, add `--root` (you can use it more than
once):

```bash
npx -y typeanvil mcp --root ~/reports
```

A path outside every root then returns the error `PATH_OUTSIDE_ROOT`.

## Inputs and outputs

A document is a `path`, inline `content`, or a `url`. Use exactly one:

```json
{ "document": { "path": "report.html" } }
{ "document": { "content": "# Notes\n\nHello", "format": "markdown" } }
{ "document": { "url": "https://example.com/invoice.html" } }
```

- A relative `path` resolves against the directory the server started in.
- `.md` and `.markdown` files are Markdown. Inline content is HTML unless
  `format` says `markdown`.
- Without `options`, pages are US Letter (`8.5in` × `11in`) with no margins.
  An `@page` rule in the document overrides this. Lengths use the CLI units
  (`in`, `pt`, `px`, `cm`, `mm`).
- `assets` supplies files that inline content refers to, for example
  `{"logo.png": {"content_base64": "..."}}`. A `src`, `href` or CSS `url()`
  that equals the key uses the asset.
- `render` returns the PDF as base64 by default. To write a file, set
  `"output": {"mode": "path", "path": "out.pdf"}`.

Every result carries `environment`. It names the engine version and the font
bundle that made the output. The same input and the same `environment` give
the same PDF bytes. A `render` through MCP gives the same bytes as
`typeanvil render` for the same file and options.

## JSON requests without MCP

`typeanvil call <tool>` runs one tool from a shell or a script. Put the tool
input on stdin. The result object comes back as one line of JSON on stdout:

```bash
echo '{"document":{"path":"report.html"},"assertions":[{"kind":"page_count","max":3}]}' \
  | typeanvil call validate
```

The request and result objects are the same as for the MCP tool. The exit
code is 0 for a result, 1 for a tool error (the output is then the error
object, with a `code` to branch on), and 2 for a usage error.

## Verify the server

`scripts/mcp_smoke.py` connects with the official MCP Python SDK. It lists
the tools, renders a document and checks that the bytes match a CLI render:

```bash
pip install mcp
python3 scripts/mcp_smoke.py --bin engine/target/debug/typeanvil
```

It prints `MCP SMOKE PASSED` on success. The contract conformance suite runs
in `cargo test` (`engine/tests/it/mcp_conformance.rs`, cases in
`tests/mcp-conformance/`).
