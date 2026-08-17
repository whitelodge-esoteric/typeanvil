#!/bin/bash
# Render the typography demo fixture through the CLI and rasterize page 1.
#
# Usage: scripts/render-typography-demo.sh
# Prints the PNG output path. Requires `sips` (macOS).
set -euo pipefail

cd "$(dirname "$0")/.."

FIXTURE="engine/tests/fixtures/typography-demo.html"
OUT_PDF="/tmp/typeanvil-typography-demo.pdf"
OUT_PNG="/tmp/typeanvil-typography-demo-page1.png"

cargo run --quiet --manifest-path engine/Cargo.toml -- render "$FIXTURE" \
    --page-width 8.27in --page-height 11.69in \
    --margin-top 1in --margin-right 1in --margin-bottom 1in --margin-left 1in \
    -o "$OUT_PDF"

sips -s format png "$OUT_PDF" --out "$OUT_PNG" >/dev/null
echo "$OUT_PNG"
