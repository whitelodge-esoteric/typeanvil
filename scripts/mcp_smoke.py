#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""MCP smoke test with the official MCP Python SDK (CORE-575).

Starts ``typeanvil mcp`` over stdio as a real MCP client would, lists the
tools, renders a document, and checks that the PDF bytes equal a CLI render
of the same file.

Usage::

    python3 scripts/mcp_smoke.py --bin engine/target/debug/typeanvil

Needs the ``mcp`` package (``pip install mcp``). Exit 0 = pass.
Runbook: docs/guides/mcp.md § Verify the server.
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

EXPECTED_TOOLS = ["render", "validate", "inspect_page", "capabilities", "explain"]


def field(obj: object, snake: str, camel: str) -> object:
    """Read a result field. SDK 2.x uses snake_case, 1.x uses camelCase."""
    return getattr(obj, snake) if hasattr(obj, snake) else getattr(obj, camel)

DOC = """<!DOCTYPE html>
<html><head><title>MCP smoke</title></head>
<body><h1>Smoke test</h1><p>Rendered through a real MCP client.</p></body></html>
"""


async def run(binary: str, workdir: Path) -> int:
    doc = workdir / "smoke.html"
    doc.write_text(DOC, encoding="utf-8")
    cli_pdf = workdir / "cli.pdf"
    subprocess.run(
        [binary, "render", str(doc), "--page-width", "8.5in", "--page-height", "11in",
         "--no-tagged", "-o", str(cli_pdf)],
        check=True,
    )

    params = StdioServerParameters(command=binary, args=["mcp"], cwd=str(workdir))
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            init = await session.initialize()
            info = field(init, "server_info", "serverInfo")
            print(f"server: {info.name} {info.version}, "  # type: ignore[attr-defined]
                  f"protocol {field(init, 'protocol_version', 'protocolVersion')}")
            if info.name != "typeanvil":  # type: ignore[attr-defined]
                print("FAIL: serverInfo.name must be typeanvil", file=sys.stderr)
                return 1
            listed = await session.list_tools()
            names = [t.name for t in listed.tools]
            print(f"tools: {names}")
            if names != EXPECTED_TOOLS:
                print(f"FAIL: expected {EXPECTED_TOOLS}", file=sys.stderr)
                return 1
            result = await session.call_tool("render", {"document": {"path": "smoke.html"}})
            if field(result, "is_error", "isError"):
                print(f"FAIL: render returned an error: {result.content}", file=sys.stderr)
                return 1
            out: dict = field(result, "structured_content", "structuredContent")  # type: ignore[assignment]
            pdf = base64.b64decode(out["artifact"]["data_base64"])
            print(f"render: {out['page_count']} page(s), {out['byte_size']} bytes, "
                  f"sha256 {out['sha256'][:16]}..., runtime {out['environment']['runtime']}")
            if hashlib.sha256(pdf).hexdigest() != out["sha256"]:
                print("FAIL: sha256 does not match the returned bytes", file=sys.stderr)
                return 1
            if pdf != cli_pdf.read_bytes():
                print("FAIL: MCP render bytes differ from the CLI render", file=sys.stderr)
                return 1
            print("MCP render bytes == CLI render bytes")
    print("MCP SMOKE PASSED")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="MCP smoke test (CORE-575)")
    ap.add_argument("--bin", required=True, help="path to the typeanvil binary")
    args = ap.parse_args()
    binary = str(Path(args.bin).resolve())
    with tempfile.TemporaryDirectory() as tmp:
        return asyncio.run(run(binary, Path(tmp)))


if __name__ == "__main__":
    sys.exit(main())
