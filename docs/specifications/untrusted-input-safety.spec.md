---
title: Untrusted input safety — fetch policy and resource limits
slug: /specifications/untrusted-input-safety
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
sidebar_position: 76
tags: [security, ai-native, cli, mcp, limits]
spec_id: untrusted-input-safety
issue_id: CORE-578
applies_to:
  - engine/src/safety.rs
  - engine/src/pipeline.rs
  - engine/src/images.rs
  - engine/src/css.rs
  - engine/src/layout.rs
  - engine/src/main.rs
  - engine/src/mcp/tools.rs
  - engine/tests/it/core578_untrusted_input.rs
dependencies:
  - cli-surface
  - diagnostics
  - mcp-server
  - mcp-tool-contract
---

# Untrusted input safety: fetch policy and resource limits

## Overview

Agent-written HTML is untrusted. It can name any local file (`../`,
absolute paths, symlinks, `file://`) or any URL, and it can force very long
layouts. This spec adds a fetch policy and resource limits to `render`, and
makes the MCP server use the strict local profile.

Without a safety flag the CLI behaves exactly as before. The harness
contract does not change.

## Goals

1. Confine document-referenced reads to one root directory, with no `..`
   or symlink escape.
2. Report every blocked read as a diagnostic. A blocked read is never
   silent.
3. Stop a render that exceeds a limit with a stable error code, a stable
   exit code and no PDF.
4. Keep the determinism promise: a limit decides success or failure. It
   never changes the bytes of a PDF that is written.

## Non-Goals

- No network fetch in the renderer. The engine stays offline for document
  references. `allow-list` marks a remote reference as permitted; it does
  not download it. The MCP `url` input form keeps its own `curl` fetch
  (`mcp-tool-contract.spec.md`, Input forms).
- No memory budget. Process memory limits belong to the host (container
  limits, `ulimit`). `--max-image-pixels` and `--max-input-bytes` bound the
  two inputs that scale memory most.
- No sandbox for the input file and `--css` files themselves. The caller
  names those; the policy covers what the DOCUMENT references.

## Behavior

### Fetch policy

1. `render` shall accept `--fetch <mode>`, where mode is `none`, `local`,
   `allow-list` or `unrestricted`. Without the flag the mode is
   `unrestricted`.
2. `unrestricted` shall keep the legacy behavior: any local path is read;
   remote references are skipped without a diagnostic.
3. `local` shall read a referenced file only when its real path (symlinks
   resolved) lies under the root. The root is `--base-url` when given, else
   the input file's directory.
4. `none` shall read no referenced file. `data:` URIs still decode, because
   they are part of the document.
5. `allow-list` shall act as `local` and permit remote references to the
   hosts in `--fetch-hosts` (comma-separated, case-insensitive). Host
   matching uses the URL authority without userinfo or port.
6. Under `none`, `local` and `allow-list`, a remote or `file:` reference
   that is not permitted shall be blocked.
7. The policy shall cover `<img src>`, `<link rel=stylesheet href>`,
   `<iframe src>`, `@font-face url()` and SVG `<image href>`. Under a
   restricted mode an SVG `<image href>` that names a file or URL loads
   nothing.
8. Each blocked read shall emit `typeanvil: warning: [fetch-blocked] <msg>`
   on stderr, once per distinct reference, and the render shall continue as
   if the resource were missing. With `--diagnostics`, each block is also
   a `fetch-blocked` warning event.

### Limits

9. `render` shall accept `--max-pages <n>`, `--timeout <seconds>`,
   `--max-input-bytes <n>` and `--max-image-pixels <n>`. Without them
   there is no limit (the internal `MAX_PAGES` safety net remains).
10. A render that would need more than `n` pages shall stop with
    `LIMIT_PAGE_COUNT`.
11. A render that runs past the time budget shall stop with
    `LIMIT_TIMEOUT`. The budget is checked between pages and between
    pipeline stages.
12. A document source above `n` bytes shall stop with `LIMIT_INPUT_SIZE`
    before parsing.
13. An image whose header dimensions exceed `n` pixels (width × height)
    shall stop with `LIMIT_IMAGE_SIZE` before it is decoded.
14. A stopped render shall write no PDF, print
    `typeanvil: error: [<CODE>] <message>` on stderr, print the
    `fetch-blocked` lines collected before the stop, and exit 4.
15. **Determinism:** a timeout is an error, never a different PDF. A limit
    shall never truncate output: either the full PDF is written, or none.
    So a written PDF is the same bytes with or without limits.
16. An invalid flag value (unknown mode, `allow-list` without hosts,
    `--fetch-hosts` without `allow-list`, a non-positive count or time)
    shall be a usage error (exit 2).

### MCP default

17. The MCP `render`, `validate`, `inspect_page` and `explain` tools shall
    use the `local` profile. Roots are the document's base directory, the
    call's scratch directory (materialized `assets`) and every `--root`.
    Blocked reads appear as `fetch-blocked` diagnostics in the result.
    Reason: MCP input is agent-written by definition.
18. A limit error from the pipeline shall map to the tool error with the
    same code (`LIMIT_TIMEOUT`, `LIMIT_PAGE_COUNT`, ...). The MCP server
    sets no limit by default in this slice; `capabilities.limits` stays
    `null`.

## Interfaces

```text
typeanvil render <in> ... [--fetch none|local|allow-list|unrestricted]
    [--fetch-hosts a.com,b.com] [--max-pages N] [--timeout SECONDS]
    [--max-input-bytes N] [--max-image-pixels N]
```

```rust
// engine/src/safety.rs
pub enum FetchPolicy { Unrestricted, None, Local, AllowList(Vec<String>) }
pub struct Policy { fetch, roots, max_pages, deadline, max_image_pixels }
pub enum LimitKind { PageCount, Timeout, InputSize, ImageSize }   // .code()
pub struct LimitError { kind, message, blocked }
pub fn install(policy: Policy) -> Guard;         // per render, thread-local
// pipeline::RenderInput gains `policy` and `max_input_bytes`;
// pipeline::Rendered gains `blocked: Vec<String>`.
```

| Exit code | Meaning |
|---|---|
| 0 | PDF written |
| 2 | Usage error |
| 3 | `--fail-on-assert` failure |
| 4 | Limit reached (`LIMIT_*` on stderr), no PDF |

## Acceptance Criteria

Tests: `engine/tests/it/core578_untrusted_input.rs` and unit tests in
`engine/src/safety.rs`.

1. Given `--fetch local` and an `<iframe src="../secret.html">` and a
   `<link href="../secret.css">`, the render succeeds and stderr and the
   diagnostics JSON report `fetch-blocked` for both
   (`path_escape_is_blocked_and_reported`). A symlink inside the root to a
   file outside is blocked (`symlink_escape_is_blocked`).
2. Given `--fetch local` or `none` and `<img src="https://...">` /
   `file:///etc/passwd`, both are reported blocked; an allow-listed host is
   not (`remote_url_is_blocked_and_reported`); `none` blocks a file inside
   the root (`fetch_none_blocks_local_reads`).
3. Given a 5-page document, `--max-pages 5` succeeds and `--max-pages 2`
   exits 4 with `[LIMIT_PAGE_COUNT]` and no PDF
   (`page_limit_yields_error_code`).
4. Given a tiny `--timeout`, the render exits 4 with `[LIMIT_TIMEOUT]` and
   no PDF (`timeout_yields_error_code`).
5. `--max-input-bytes` and `--max-image-pixels` yield `LIMIT_INPUT_SIZE`
   and `LIMIT_IMAGE_SIZE` (`input_size_limit_yields_error_code`,
   `image_size_limit_yields_error_code`).
6. Without flags, the PDF is byte-identical to `--fetch unrestricted` and to
   a render with generous limits, and stderr is empty (`no_flag_is_unchanged`).
7. Bad values exit 2 (`bad_values_are_usage_errors`).
8. The MCP conformance suite stays green with the `local` profile.

## Edge Cases

- A missing file outside the root is still reported blocked (the lexical
  path is checked), so an escape attempt is visible even when it would fail.
- WPT root-absolute links (`/fonts/ahem.css`) walk up past the input
  directory; under `local` they are blocked unless `--base-url` names the
  checkout root. The harness passes no `--fetch`, so it is unaffected.
- A limit that trips during layout stops pagination early; the partial
  layout is discarded, never written.

## References

- CORE-578 (this issue), CORE-570 (AI-native epic), CORE-575 (MCP server).
- `mcp-tool-contract.spec.md` — error codes `LIMIT_*`, `URL_BLOCKED`.
- `docs/conventions/css-standards-alignment.md` — determinism promise.
