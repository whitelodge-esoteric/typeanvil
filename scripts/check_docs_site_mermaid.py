#!/usr/bin/env python3
"""Verify the built docs site renders Mermaid diagrams (client-side hydration).

Loads the home page in real Chromium and counts rendered mermaid SVGs.
"""
from playwright.sync_api import sync_playwright

BASE = "http://localhost:3999"

with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page()
    page.goto(BASE, wait_until="load")
    page.wait_for_timeout(8000)  # mermaid renders after hydration
    containers = page.evaluate(
        "document.querySelectorAll('.docusaurus-mermaid-container').length"
    )
    svgs = page.evaluate(
        "document.querySelectorAll('.docusaurus-mermaid-container svg').length"
    )
    title = page.title()
    print(f"title: {title}")
    print(f"mermaid containers: {containers}, rendered svgs: {svgs}")
    # Walk the whole site: every page should have the docs sidebar and render
    browser.close()
