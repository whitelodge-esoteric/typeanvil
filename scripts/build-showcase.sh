#!/usr/bin/env bash
# build-showcase.sh — render the showcase fixtures through TypeAnvil at
# realistic print geometry (US Letter @ 300 DPI) and assemble the markdown
# gallery written into demo/showcase/README.md (CORE-148).
#
# CORE-217 adds an ADDITIVE, unstressed Prince reference overlay: every
# fixture is also rendered through host Prince at the identical geometry and
# rasterized into page-NNN-pr.png, plus demo/showcase/out/inspect.md — a
# per-page TypeAnvil | Prince side-by-side that is NOT scored and NOT part of
# the gallery. A Prince failure never aborts the build.
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
# Host Prince (CORE-217): the overlay renders with the host binary, never the
# container shim. An explicit PRINCE_BIN wins (failure-isolation tests point it
# at a bogus binary); otherwise fall back to PATH, then known install roots.
if [ -z "${PRINCE_BIN:-}" ]; then
  PRINCE_BIN="prince"
  if ! command -v "$PRINCE_BIN" >/dev/null 2>&1; then
    for candidate in /opt/homebrew/bin/prince /usr/local/bin/prince; do
      if [ -x "$candidate" ]; then
        PRINCE_BIN="$candidate"
        break
      fi
    done
  fi
fi
# render-prince.sh reads PRINCE_BIN from the environment.
export PRINCE_BIN
SHOWCASE_DIR="demo/showcase"
OUT="demo/showcase/out"
WORK="$OUT/.work"
IMAGES="$OUT/images"
MANIFEST="demo/showcase/manifest.json"
SHOWCASE_README="demo/showcase/README.md"
MARKER="<!-- BEGIN GENERATED SHOWCASE -->"
# Unstressed Prince reference overlay (CORE-217). Lives inside out/ so the
# existing check-determinism walk covers it; never spliced into the README.
INSPECT="$OUT/inspect.md"
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

Each fixture renders through TypeAnvil (the gallery) and, additively, through
host Prince (the unstressed reference overlay written as page-NNN-pr.png plus
demo/showcase/out/inspect.md). A Prince failure is recorded, never fatal.
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
PRINCE_VERSION="$("$PRINCE_BIN" --version 2>&1 | head -1 || true)"

entries=()
while IFS=$'\t' read -r file name; do
  [ -n "$file" ] && entries+=("$file|$name")
done < <("$PY" scripts/demo_compare.py list-manifest --manifest "$MANIFEST")

if [ ${#entries[@]} -eq 0 ]; then
  echo "error: no showcase entries from manifest $MANIFEST" >&2
  exit 2
fi

echo "typeanvil $TA_VERSION · prince: ${PRINCE_VERSION:-unavailable} · showcase: ${#entries[@]} doc(s) @ ${DPI} DPI Letter"

FAILED=0
# Prince failures are recorded and isolated (an overlay must never break the
# gallery); they do not set FAILED.
PRINCE_FAILED=0
for entry in "${entries[@]}"; do
  file="${entry%%|*}"
  name="${entry##*|}"
  base="${file%.html}"
  ta_pdf="$WORK/$base-ta.pdf"

  if [ "$DRY_RUN" = "1" ]; then
    echo "[dry] (cd $SHOWCASE_DIR && engine/target/debug/typeanvil render $file $GEOM_FLAGS -o $REPO_ROOT/$ta_pdf)"
    echo "[dry] (cd $SHOWCASE_DIR && scripts/render-prince.sh $file $GEOM_FLAGS -o $REPO_ROOT/$WORK/$base-pr.pdf)  # reference overlay, not scored"
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

# ---- Prince reference overlay (CORE-217) ----------------------------------
# ADDITIVE and unstressed: rendered at the identical geometry, rasterized at
# the identical DPI, and never scored. A Prince failure is recorded and the
# fixture is skipped; it never aborts the build and never touches the gallery.
PRINCE_AVAILABLE=1
if ! command -v "$PRINCE_BIN" >/dev/null 2>&1 && [ ! -x "$PRINCE_BIN" ]; then
  echo "PR  SKIP Prince binary not found ('$PRINCE_BIN') — reference overlay unavailable, gallery unaffected" >&2
  PRINCE_AVAILABLE=0
  PRINCE_FAILED=1
  for entry in "${entries[@]}"; do
    base="${entry%%|*}"
    base="${base%.html}"
    rm -f "$IMAGES/$base"/page-*-pr.png
  done
fi

if [ "$PRINCE_AVAILABLE" = "1" ]; then
for entry in "${entries[@]}"; do
  file="${entry%%|*}"
  base="${file%.html}"
  pr_pdf="$WORK/$base-pr.pdf"
  pr_dir="$IMAGES/$base"

  # Drop stale references first: a skipped fixture must show no Prince pages
  # rather than a previous build's pages.
  rm -f "$pr_dir"/page-*-pr.png

  if (cd "$SHOWCASE_DIR" && "$REPO_ROOT/scripts/render-prince.sh" "$file" $GEOM_FLAGS -o "$REPO_ROOT/$pr_pdf" 2>"$REPO_ROOT/$WORK/$base-pr.err"); then
    echo "PR  ok   $file"
  else
    echo "PR  FAIL $file :: $(head -1 "$WORK/$base-pr.err")" >&2
    PRINCE_FAILED=1
    continue
  fi

  # Rasterize at the same 300 DPI so both sides are directly comparable.
  if ! "$PY" -c "
import sys
sys.path.insert(0, '$REPO_ROOT')
from harness.rasterize import rasterize_pdf
from pathlib import Path
imgs = rasterize_pdf(Path('$pr_pdf'), dpi=$DPI)
if not imgs:
    sys.exit('zero pages rendered')
out = Path('$pr_dir')
out.mkdir(parents=True, exist_ok=True)
for i, im in enumerate(imgs, 1):
    im.save(out / f'page-{i:03d}-pr.png', 'PNG')
print(f'    {len(imgs)} page(s) -> $pr_dir')
"; then
    echo "PR  RASTER FAIL $file — reference skipped" >&2
    rm -f "$pr_dir"/page-*-pr.png
    PRINCE_FAILED=1
    continue
  fi
done
fi

if [ "$PRINCE_FAILED" != "0" ]; then
  echo "note: the Prince reference overlay is incomplete (unstressed — not a build failure)" >&2
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

# Inspection page (CORE-217): TypeAnvil | Prince per-page side-by-side. Lives
# INSIDE out/ so the check-determinism walk byte-compares it; it is never
# spliced into the README and carries no scores, buckets, or failure states.
"$PY" scripts/demo_compare.py assemble-showcase-inspect \
  --manifest "$MANIFEST" \
  --images-dir "$IMAGES" \
  --out "$INSPECT" \
  --image-prefix images

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
