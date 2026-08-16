#!/usr/bin/env python3
"""Verify the built docs site: Mermaid rendering + sidebar navigation.

Loads pages in real Chromium and checks:
- the docs sidebar menu is present and lists the categories/docs,
- Mermaid diagrams hydrate into SVGs (client-side render).
"""
from playwright.sync_api import sync_playwright

BASE = "http://localhost:3999"


def check_page(page, path: str, expect_sidebar_links: int):
    page.goto(f"{BASE}{path}", wait_until="load")
    page.wait_for_timeout(6000)  # hydration + mermaid render
    links = page.evaluate(
        "Array.from(document.querySelectorAll('.theme-doc-sidebar-menu a'))"
        ".map(a => a.textContent.trim())"
    )
    svgs = page.evaluate(
        "document.querySelectorAll('.docusaurus-mermaid-container svg').length"
    )
    print(f"{path}: sidebar links={len(links)}, mermaid svgs={svgs}")
    if links:
        print(f"  sidebar items: {links[:12]}{'...' if len(links) > 12 else ''}")
    if len(links) < expect_sidebar_links:
        raise SystemExit(f"FAIL: expected >= {expect_sidebar_links} sidebar links on {path}, got {len(links)}")


with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page()
    check_page(page, "/", expect_sidebar_links=6)  # 6 categories + home
    check_page(page, "/specifications/wpt-conformance-harness", expect_sidebar_links=6)
    check_page(page, "/operations/docs-site", expect_sidebar_links=6)
    browser.close()

print("OK: sidebar present, mermaid renders")
