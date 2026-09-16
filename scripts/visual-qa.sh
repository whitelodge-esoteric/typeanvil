#!/usr/bin/env bash
# visual-qa.sh — productized visual QA checks for both demo tracks (CORE-216).
#
# Renders each fixture's PDF(s), rasterizes every page, and runs four
# geometry-based checks via scripts/visual_qa.py:
#   1. ink-row overlap      — two text/element runs painting the same rows
#   2. page-box overflow    — ink outside the page content box (corpus) or
#                             MediaBox (showcase: full-bleed poster is legal)
#   3. text round-trip      — every visible source token in the PDF text layer
#   4. declared-fill        — a declared background-color actually paints
#
# Deterministic: re-run on an unchanged tree produces byte-identical JSON
# reports (no timestamps; fixture order from the manifest; sorted pages).
# Exits non-zero with per-fixture reasons when any check fails.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PY="${PY:-$REPO_ROOT/.venv/bin/python}"
if [ ! -x "$PY" ] && [ -x "$HOME/workspace/typeanvil/.venv/bin/python" ]; then
  PY="$HOME/workspace/typeanvil/.venv/bin/python"
fi
if [ ! -x "$PY" ]; then
  PY="$(command -v python3)"
fi

usage() {
  cat <<'EOF'
usage: visual-qa.sh --track corpus|showcase --report out.json [--keep-pdfs] [--check-pr] [--dry-run]

  --track       demo track to check: corpus (TypeAnvil vs Prince) or
                showcase (TypeAnvil only)
  --report      path for the machine-readable JSON report
  --keep-pdfs   keep the work dir (pdf/ + img/) instead of deleting at exit
  --check-pr    showcase: also render + check the Prince reference side
                (enabled by the CORE-217 overlay; corpus always checks PR)
  --dry-run     print the exact render commands without running them
  -h, --help    show this help
EOF
}

TRACK=""
REPORT=""
KEEP_PDFS=0
DRY_RUN=0
CHECK_PR=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --track) TRACK="$2"; shift 2 ;;
    --report) REPORT="$2"; shift 2 ;;
    --keep-pdfs) KEEP_PDFS=1; shift ;;
    --check-pr) CHECK_PR=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unrecognized option '$1'" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$TRACK" || -z "$REPORT" ]]; then
  echo "error: --track and --report are required" >&2
  usage >&2
  exit 2
fi

case "$TRACK" in
  corpus)
    MANIFEST="demo/corpus/manifest.json"
    FIXTURES_DIR="demo/corpus"
    GEOM_FLAGS="--page-width 5in --page-height 3in --margin-top 0.5in --margin-right 0.5in --margin-bottom 0.5in --margin-left 0.5in"
    DPI=96
    ;;
  showcase)
    MANIFEST="demo/showcase/manifest.json"
    FIXTURES_DIR="demo/showcase"
    GEOM_FLAGS="--page-width 8.5in --page-height 11in --margin-top 0.75in --margin-right 0.75in --margin-bottom 0.75in --margin-left 0.75in"
    DPI=300
    ;;
  *)
    echo "error: --track must be corpus or showcase" >&2
    exit 2
    ;;
esac

if [ ! -x engine/target/debug/typeanvil ]; then
  echo "error: engine binary not found (engine/target/debug/typeanvil). Build first: cargo build --manifest-path engine/Cargo.toml" >&2
  exit 2
fi
PRINCE_BIN="${PRINCE_BIN:-prince}"
if [[ "$TRACK" == "corpus" ]] && ! command -v "$PRINCE_BIN" >/dev/null 2>&1; then
  echo "error: Prince binary not found ('$PRINCE_BIN'). Install per demo/corpus/README.md." >&2
  exit 2
fi

# The work dir MUST live inside the worktree: the engine binary is a
# container shim that only maps worktree paths into /work (a /tmp path
# fails the shim's path guard). `out/` is gitignored for both tracks, so
# a leftover .qa dir can never be committed.
if [ "$KEEP_PDFS" = "1" ]; then
  WORK="$(mktemp -d "$REPO_ROOT/$FIXTURES_DIR/out/.qa-keep-XXXXXX")"
else
  WORK="$(mktemp -d "$REPO_ROOT/$FIXTURES_DIR/out/.qa-XXXXXX")"
fi
cleanup() {
  if [ "$KEEP_PDFS" = "1" ]; then
    echo "kept work dir: $WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT
mkdir -p "$WORK/pdf" "$WORK/img"

TA_VERSION="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
entries=()
while IFS=$'\t' read -r file name; do
  [ -n "$file" ] && entries+=("$file|$name")
done < <("$PY" scripts/demo_compare.py list-manifest --manifest "$MANIFEST")

if [ ${#entries[@]} -eq 0 ]; then
  echo "error: no $TRACK entries from manifest $MANIFEST" >&2
  exit 2
fi

echo "visual-qa $TRACK · typeanvil $TA_VERSION · ${#entries[@]} fixture(s) @ ${DPI} DPI"

FAILED=0
for entry in "${entries[@]}"; do
  file="${entry%%|*}"
  name="${entry##*|}"
  base="${file%.html}"

  engines=("ta")
  [[ "$TRACK" == "corpus" ]] && engines=("ta" "pr")
  # Showcase is TypeAnvil-only by default; the CORE-217 Prince reference
  # overlay adds a -pr side that must feed the same checks, so --check-pr
  # opts showcase into Prince rendering too.
  [[ "$TRACK" == "showcase" && "$CHECK_PR" = "1" ]] && engines+=("pr")

  for engine in "${engines[@]}"; do
    pdf="$WORK/pdf/$base-$engine.pdf"

    if [[ "$engine" == "ta" ]]; then
      if [ "$DRY_RUN" = "1" ]; then
        echo "[dry] engine/target/debug/typeanvil render $FIXTURES_DIR/$file $GEOM_FLAGS -o $pdf"
      elif (cd "$FIXTURES_DIR" && "$REPO_ROOT/engine/target/debug/typeanvil" render "$file" $GEOM_FLAGS -o "$pdf" 2>"$WORK/$base-ta.err"); then
        echo "TA  ok   $file"
      else
        echo "TA  FAIL $file :: $(head -1 "$WORK/$base-ta.err")" >&2
        FAILED=1
        continue
      fi
    else
      if [ "$DRY_RUN" = "1" ]; then
        echo "[dry] (cd $FIXTURES_DIR && scripts/render-prince.sh $file $GEOM_FLAGS -o $pdf)"
      elif (cd "$FIXTURES_DIR" && "$REPO_ROOT/scripts/render-prince.sh" "$file" $GEOM_FLAGS -o "$pdf" 2>"$WORK/$base-pr.err"); then
        echo "PR  ok   $file"
      else
        echo "PR  FAIL $file :: $(head -1 "$WORK/$base-pr.err")" >&2
        FAILED=1
        continue
      fi
    fi

    if [ "$DRY_RUN" = "1" ]; then
      echo "[dry] rasterize $pdf -> $WORK/img/$base-$engine/page-NNN.png"
      continue
    fi

    if ! "$PY" -c "
import sys
sys.path.insert(0, '$REPO_ROOT')
from harness.rasterize import rasterize_pdf
from pathlib import Path
imgs = rasterize_pdf(Path('$pdf'), dpi=$DPI)
assert imgs, 'zero pages rendered'
out = Path('$WORK/img/$base-$engine')
out.mkdir(parents=True, exist_ok=True)
for i, im in enumerate(imgs, 1):
    im.save(out / f'page-{i:03d}.png', 'PNG')
print(f'    {len(imgs)} page(s) -> $WORK/img/$base-$engine')
"; then
      echo "RASTER FAIL $file" >&2
      FAILED=1
      continue
    fi
  done
done

if [ "$DRY_RUN" = "1" ]; then
  echo "(dry run only — no checks run)"
  exit 0
fi

echo
set +e
"$PY" scripts/visual_qa.py run --track "$TRACK" --report "$REPORT" --work-dir "$WORK" --dpi "$DPI" $( [ "$CHECK_PR" = "1" ] && echo --check-pr )
RESULT=$?
set -e

if [ -f "$REPORT" ]; then
  echo
  echo "report: $REPORT"
fi

if [ "$FAILED" != "0" ]; then
  echo "render/raster failures occurred (see above) — report may be partial" >&2
  exit 1
fi
exit "$RESULT"