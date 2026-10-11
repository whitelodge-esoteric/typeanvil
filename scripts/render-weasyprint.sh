#!/usr/bin/env bash
# render-weasyprint.sh — render an HTML file to PDF with WeasyPrint, honoring
# the `typeanvil render` CLI contract so benchmark/triage legs swap engines
# transparently (CORE-318).
#
# WeasyPrint's CLI has no per-side page-size flags; page geometry comes from
# CSS `@page`. This wrapper translates the engine's per-side geometry flags
# into a small `@page` stylesheet passed with `-s` (appended after the
# document's own stylesheets, so it wins the cascade for equal specificity).
#
# Usage:
#   scripts/render-weasyprint.sh <input.html> \
#       --page-width W --page-height H \
#       --margin-top MT --margin-right MR --margin-bottom MB --margin-left ML \
#       [--base-url URL] \
#       -o <output.pdf>
#
# WeasyPrint version is printed to stderr so the pipeline can record it.
set -euo pipefail

WEASY_BIN="${WEASY_BIN:-weasyprint}"

# macOS: Homebrew's Pango/GLib live in /opt/homebrew/lib, which is not on the
# default dyld search path; WeasyPrint loads them with cffi/dlopen at import.
# Set the documented fallback when it is not already set and brew libs exist.
if [[ -z "${DYLD_FALLBACK_LIBRARY_PATH:-}" ]] && [[ -d "/opt/homebrew/lib" ]]; then
  export DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/lib"
fi

if ! command -v "$WEASY_BIN" >/dev/null 2>&1; then
  echo "error: WeasyPrint binary not found ('$WEASY_BIN'). Install it: uv tool install weasyprint (needs brew install pango)." >&2
  exit 2
fi

INPUT=""
OUTPUT=""
PAGE_W=""
PAGE_H=""
MARGIN_T=""
MARGIN_R=""
MARGIN_B=""
MARGIN_L=""
BASE_URL=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --page-width)    PAGE_W="$2";    shift 2 ;;
    --page-height)   PAGE_H="$2";    shift 2 ;;
    --margin-top)    MARGIN_T="$2";  shift 2 ;;
    --margin-right)  MARGIN_R="$2";  shift 2 ;;
    --margin-bottom) MARGIN_B="$2";  shift 2 ;;
    --margin-left)   MARGIN_L="$2";  shift 2 ;;
    --base-url)      BASE_URL="$2";  shift 2 ;;
    -o|--output)     OUTPUT="$2";    shift 2 ;;
    -h|--help)
      echo "usage: $0 <input.html> --page-width W --page-height H --margin-top MT --margin-right MR --margin-bottom MB --margin-left ML [--base-url URL] -o out.pdf"
      exit 0
      ;;
    -*)
      echo "error: unrecognized option '$1' (use --page-width/--page-height/--margin-*/--base-url/-o)" >&2
      exit 2
      ;;
    *)
      INPUT="$1"
      shift
      ;;
  esac
done

if [[ -z "$INPUT" || -z "$OUTPUT" ]]; then
  echo "error: input and -o output are required" >&2
  exit 2
fi

ARGS=()
if [[ -n "$PAGE_W" && -n "$PAGE_H" ]]; then
  GEOM_CSS="$(mktemp "${TMPDIR:-/tmp}/weasy-geom-XXXXXX.css")"
  trap 'rm -f "$GEOM_CSS"' EXIT
  cat > "$GEOM_CSS" <<EOF
@page {
  size: ${PAGE_W} ${PAGE_H};
  margin: ${MARGIN_T:-0in} ${MARGIN_R:-0in} ${MARGIN_B:-0in} ${MARGIN_L:-0in};
}
EOF
  ARGS+=(-s "$GEOM_CSS")
fi
if [[ -n "$BASE_URL" ]]; then
  ARGS+=(-u "$BASE_URL")
fi

# Record version for any scoreboard (stderr, captured by the pipeline).
echo "weasyprint $( "$WEASY_BIN" --version 2>&1 | head -1 )" >&2

"$WEASY_BIN" ${ARGS[@]+"${ARGS[@]}"} "$INPUT" "$OUTPUT"
echo "rendered $OUTPUT via WeasyPrint" >&2
