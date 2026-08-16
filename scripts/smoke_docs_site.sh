#!/bin/bash
# Smoke-test the built Docusaurus site on :3999
set -u
base="http://localhost:3999"
check() {
  local label="$1" path="$2" want="$3"
  code=$(curl -s -o /dev/null -w '%{http_code}' "$base$path")
  if [ "$code" = "$want" ]; then
    echo "OK   $label ($path -> $code)"
  else
    echo "FAIL $label ($path -> $code, wanted $want)"
  fi
}
check "home"        "/"                                    200
check "spec"        "/specifications/wpt-conformance-harness" 200
check "conventions" "/conventions/doc-conventions"         200
check "runbook"     "/operations/docs-site"                200
check "research"    "/research/wpt-harness/typeanvil-wpt-harness-brief" 200
title=$(curl -s "$base/" | grep -o '<title[^>]*>[^<]*</title>' | head -1)
echo "title: ${title:-MISSING}"
# Mermaid diagrams render client-side after hydration; verified separately with
# scripts/check_docs_site_mermaid.py (real Chromium).
