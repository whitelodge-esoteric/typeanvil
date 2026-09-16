#!/usr/bin/env bash
# build-showcase.sh — render the showcase fixtures through TypeAnvil at
# realistic print geometry (US Letter @ 300 DPI) and assemble the markdown
# gallery written into demo/showcase/README.md (CORE-148).
#
# Contract: docs/specifications/showcase-render.spec.md §Behavior 1-6.
#
# Additive to build-demo.sh: the comparison pipeline is untouched.
# Deterministic: re-run on an unchanged tree is byte-identical (no
# timestamps anywhere in the showcase output).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PY="${PY:-$REPO_ROOT/.venv/bin/python}"
# Fresh worktrees have no .venv — fall back to MAIN's checkout venv (the
# issue-loop standard: harness runs use main's python), then bare python3.
if [ ! -x "$PY" ] && [ -x "$HOME/workspace/typeanvil/.venv/bin/python" ]; then
  PY="$HOME/workspace/typeanvil/.venv/bin/python"
fi
if [ ! -x "$PY" ]; then
  PY="$(command -v python3)"
fi
SHOWCASE_DIR="demo/showcase"
OUT="demo/showcase/out"
WORK="$OUT/.work"
IMAGES="$OUT/images"
MANIFEST="demo/showcase/manifest.json"
SHOWCASE_README="demo/showcase/README.md"
MARKER="<!-- BEGIN GENERATED SHOWCASE -->"
# Realistic print geometry. Fixture-internal @page rules (e.g. the poster's
# 0in margins) override these CLI defaults per @page cascade.
GEOM_FLAGS="--page-width 8.5in --page-height 11in --margin-top 0.75in --margin-right 0.75in --margin-bottom 0.75in --margin-left 0.75in"
DPI=300

usage() {
  cat <<'EOF'
usage: build-showcase.sh [--dry-run] [--keep-work] [--determinism]

  --dry-run        print the exact render commands without running them
  --keep-work      keep .work dir (default: cleaned at exit)
  --determinism    build twice and byte-compare demo/showcase/out
EOF
}

DRY_RUN=0
KEEP_WORK=0
DO_DETERMINISM=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --keep-work) KEEP_WORK=1 ;;
    --determinism) DO_DETERMINISM=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option $arg" >&2; usage; exit 2 ;;
  esac
done

if [ ! -x engine/target/debug/typeanvil ]; then
  echo "error: engine binary not found (engine/target/debug/typeanvil). Build first: cargo build --manifest-path engine/Cargo.toml" >&2
  exit 2
fi

rm -rf "$WORK"
mkdir -p "$WORK" "$IMAGES"

TA_VERSION="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

entries=()
while IFS=$'\t' read -r file name; do
  [ -n "$file" ] && entries+=("$file|$name")
done < <("$PY" scripts/demo_compare.py list-manifest --manifest "$MANIFEST")

if [ ${#entries[@]} -eq 0 ]; then
  echo "error: no showcase entries from manifest $MANIFEST" >&2
  exit 2
fi

echo "typeanvil $TA_VERSION · showcase: ${#entries[@]} doc(s) @ ${DPI} DPI Letter"

FAILED=0
for entry in "${entries[@]}"; do
  file="${entry%%|*}"
  name="${entry##*|}"
  base="${file%.html}"
  ta_pdf="$WORK/$base-ta.pdf"

  if [ "$DRY_RUN" = "1" ]; then
    echo "[dry] (cd $SHOWCASE_DIR && engine/target/debug/typeanvil render $file $GEOM_FLAGS -o $REPO_ROOT/$ta_pdf)"
    continue
  fi

  # Render from the showcase dir so relative url()/src resolve like the
  # comparison corpus (base-url threading is CORE-140).
  if (cd "$SHOWCASE_DIR" && "$REPO_ROOT/engine/target/debug/typeanvil" render "$file" $GEOM_FLAGS -o "$REPO_ROOT/$ta_pdf" 2>"$REPO_ROOT/$WORK/$base-ta.err"); then
    echo "TA  ok   $file"
  else
    echo "TA  FAIL $file :: $(head -1 "$WORK/$base-ta.err")"
    FAILED=1
    continue
  fi

  # Rasterize at print resolution (spec §Behavior 2) and fail on zero pages.
  if ! "$PY" -c "
import sys
sys.path.insert(0, '$REPO_ROOT')
from harness.rasterize import rasterize_pdf
from pathlib import Path
imgs = rasterize_pdf(Path('$ta_pdf'), dpi=$DPI)
assert imgs, 'zero pages rendered'
out = Path('$IMAGES/$base')
out.mkdir(parents=True, exist_ok=True)
for old in out.glob('page-*.png'):
    old.unlink()
for i, im in enumerate(imgs, 1):
    im.save(out / f'page-{i:03d}-ta.png', 'PNG')
print(f'    {len(imgs)} page(s) -> $IMAGES/$base')
"; then
    echo "RASTER FAIL $file"
    FAILED=1
    continue
  fi
done

if [ "$DRY_RUN" = "1" ]; then
  echo "(dry run only — nothing rendered)"
  exit 0
fi

# Assemble the markdown gallery (spec §Behavior 3) into its own README so
# showcase and comparison outputs stay separable. Skipped when any
# render/raster failed: never promote a partial gallery.
if [ "$FAILED" = "0" ]; then
# demo_compare.py owns the README splice so both tracks behave identically:
# the hand-maintained preamble above the marker is never rewritten, and the
# generated body below it is replaced wholesale. No intermediate index.md.
"$PY" scripts/demo_compare.py assemble-showcase \
  --manifest "$MANIFEST" \
  --images-dir "$IMAGES" \
  --readme "$SHOWCASE_README" \
  --marker "$MARKER" \
  --image-prefix out/images \
  --typeanvil-version "$TA_VERSION"
fi

# Determinism mode: byte-compare two consecutive builds (spec §Behavior 5).
if [ "$DO_DETERMINISM" = "1" ]; then
  BASELINE="demo/showcase/out.baseline"
  rm -rf "$BASELINE"
  cp -R "$OUT" "$BASELINE"
  # The generated gallery lives in demo/showcase/README.md, not inside out/,
  # so snapshot it here and byte-compare it after the rebuild as well.
  README_SNAP="$(mktemp -t typeanvil-showcase-readme)"
  cp "$SHOWCASE_README" "$README_SNAP"
  rm -rf "$OUT"
  "$0" --keep-work
  "$PY" scripts/demo_compare.py check-determinism "$BASELINE" "$OUT" 2>&1 || {
    echo "showcase determinism FAILED (see above)" >&2
    exit 1
  }
  if ! cmp -s "$README_SNAP" "$SHOWCASE_README"; then
    echo "showcase determinism FAILED ($SHOWCASE_README differs between passes)" >&2
    rm -f "$README_SNAP"
    exit 1
  fi
  rm -f "$README_SNAP"
  rm -rf "$BASELINE"
fi

if [ "$KEEP_WORK" = "0" ]; then
  rm -rf "$WORK"
fi

exit "$FAILED"
