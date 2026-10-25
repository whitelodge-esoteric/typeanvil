#!/usr/bin/env python3
"""Check the docs site build emits llms.txt, llms-full.txt and a .md per page.

Usage: check_llms_txt.py [BUILD_DIR]   (default: docsite/build)

Published pages are found from the build's sitemap.xml. Every page URL must
have a Markdown copy, a link in llms.txt, and its text in llms-full.txt.
Exit 0 on success, 1 on failure.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parent.parent


def page_paths(build: Path) -> list[str]:
    sitemap = (build / "sitemap.xml").read_text(encoding="utf-8")
    paths = []
    for loc in re.findall(r"<loc>([^<]+)</loc>", sitemap):
        p = urlparse(loc).path.rstrip("/")
        # Docusaurus-generated index pages are not docs pages.
        if p in ("/blog", "/search") or p.startswith(("/tags/", "/blog/")) or p == "/tags":
            continue
        paths.append(p)
    return sorted(set(paths))


def md_name(path: str) -> str:
    return "/index.md" if path == "" else f"{path}.md"


def check(build: Path) -> list[str]:
    errors: list[str] = []
    for name in ("llms.txt", "llms-full.txt", "sitemap.xml"):
        if not (build / name).is_file():
            errors.append(f"missing {name}")
    if errors:
        return errors
    index = (build / "llms.txt").read_text(encoding="utf-8")
    full = (build / "llms-full.txt").read_text(encoding="utf-8")

    if not index.startswith("# TypeAnvil\n"):
        errors.append("llms.txt must start with the H1 '# TypeAnvil'")
    for needle in (
        "Use these docs",
        "not a fit",
        "/guides/getting-started.md",
        "/guides/agent-authoring.md",
    ):
        if needle not in index:
            errors.append(f"llms.txt lacks required text: {needle!r}")

    paths = page_paths(build)
    if not paths:
        errors.append("sitemap.xml lists no pages")
    for p in paths:
        md = build / md_name(p).lstrip("/")
        if not md.is_file():
            errors.append(f"no Markdown copy for {p or '/'}")
            continue
        if md_name(p) not in index:
            errors.append(f"llms.txt has no link to {md_name(p)}")
        text = md.read_text(encoding="utf-8")
        if not text.strip():
            errors.append(f"empty Markdown copy: {md_name(p)}")
        elif text.strip()[:200] not in full:
            errors.append(f"llms-full.txt lacks the text of {md_name(p)}")
        if text.startswith("---"):
            errors.append(f"frontmatter left in {md_name(p)}")
    return errors


def main(argv: list[str]) -> int:
    build = Path(argv[1]) if len(argv) > 1 else ROOT / "docsite" / "build"
    errors = check(build)
    for e in errors:
        print(f"FAIL {e}")
    if errors:
        return 1
    print(f"OK llms.txt, llms-full.txt and {len(page_paths(build))} .md pages")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
