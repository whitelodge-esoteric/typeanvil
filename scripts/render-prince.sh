#!/usr/bin/env bash
# render-prince.sh — render an HTML file to PDF with Prince, honoring the
# `typeanvil render` CLI contract so the demo pipeline swaps engines
# transparently (visual-comparison-demo spec §Behavior 3, CORE-69).
#
# Prince's CLI uses `--page-size` + `--page-margin` (no per-side flags), so
# this wrapper translates the engine's per-side geometry flags into Prince's.
#
# Usage:
#   scripts/render-prince.sh <input.html> \
#       --page-width W --page-height H \
#       --margin-top MT --margin-right MR --margin-bottom MB --margin-left ML \
#       [--base-url URL] \
#       -o <output.pdf>
#
# Prince version is printed to stderr for the scoreboard (spec §Behavior 6).
set -euo pipefail

PRINCE_BIN="${PRINCE_BIN:-prince}"

if ! command -v "$PRINCE_BIN" >/dev/null 2>&1; then
  echo "error: Prince binary not found ('$PRINCE_BIN'). Install per demo/README.md (brew install --cask prince, then run its install.sh)." >&2
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

if [[ -z "$INPUT" || -z "$OUTPUT" || -z "$PAGE_W" || -z "$PAGE_H" ]]; then
  echo "error: input, output, --page-width and --page-height are required" >&2
  exit 2
fi

# Page size: Prince wants `"W H"` (space-separated) — `5inx3in` and `5in` are
# rejected/treated as square. Verified 2026-08-17: "360pt 216pt" → 360×216.
PAGE_SIZE="${PAGE_W} ${PAGE_H}"
# Margins: Prince takes one value or up to four (top right bottom left),
# matching our per-side contract order exactly.
PAGE_MARGIN="${MARGIN_T:-0in} ${MARGIN_R:-0in} ${MARGIN_B:-0in} ${MARGIN_L:-0in}"

ARGS=(--page-size="$PAGE_SIZE" --page-margin="$PAGE_MARGIN" --media=print)
if [[ -n "$BASE_URL" ]]; then
  ARGS+=(--baseurl="$BASE_URL")
fi

# Record version for the scoreboard (stderr, captured by the pipeline).
echo "prince $( "$PRINCE_BIN" --version 2>&1 | head -1 )" >&2

"$PRINCE_BIN" "${ARGS[@]}" "$INPUT" -o "$OUTPUT"
echo "rendered $OUTPUT via Prince" >&2
