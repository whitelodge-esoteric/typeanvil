#!/usr/bin/env bash
# build-demo.sh — render the corpus through TypeAnvil + Prince, rasterize,
# diff, and emit the static gallery + scoreboard (CORE-71).
#
# Contract: docs/specifications/visual-comparison-demo.spec.md
#   §Behavior 1-11, §Interfaces, §Acceptance Criteria.
#
# Output: demo/out/index.html (gallery) + demo/out/scoreboard.json
# Deterministic: re-run on an unchanged tree is byte-identical except the
# scoreboard's "generated" field.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PY="${PY:-.venv/bin/python}"
PRINCE_BIN="${PRINCE_BIN:-prince}"
OUT="${OUT_DIR:-demo/out}"
WORK="$OUT/.work"
IMAGES="$OUT/images"
RESULTS="$OUT/.results"
GEOM_FLAGS="--page-width 5in --page-height 3in --margin-top 0.5in --margin-right 0.5in --margin-bottom 0.5in --margin-left 0.5in"
MANIFEST="demo/corpus/manifest.json"

usage() {
  cat <<'EOF'
usage: build-demo.sh [--dry-run] [--keep-work] [--validate] [--determinism]

  --dry-run        print the exact render commands without running them
  --keep-work      keep .work/.results dirs (default: cleaned at exit)
  --validate       validate demo/out/scoreboard.json after building
  --determinism    build twice and byte-compare demo/out (except generated)
EOF
}

DRY_RUN=0
KEEP_WORK=0
DO_VALIDATE=0
DO_DETERMINISM=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --keep-work) KEEP_WORK=1 ;;
    --validate) DO_VALIDATE=1 ;;
    --determinism) DO_DETERMINISM=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option $arg" >&2; usage; exit 2 ;;
  esac
done

if ! command -v "$PRINCE_BIN" >/dev/null 2>&1; then
  echo "error: Prince binary not found ('$PRINCE_BIN'). Install per demo/README.md (brew install --cask prince; ./install.sh /opt/homebrew)." >&2
  exit 2
fi

if [ ! -x engine/target/debug/typeanvil ]; then
  echo "error: engine binary not found (engine/target/debug/typeanvil). Build first: cargo build --manifest-path engine/Cargo.toml" >&2
  exit 2
fi

rm -rf "$RESULTS" "$WORK"
mkdir -p "$WORK" "$RESULTS" "$IMAGES"

TA_VERSION="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
PR_VERSION="$("$PRINCE_BIN" --version 2>&1 | head -1 || true)"

# mapfile is bash 4+; macOS bash 3.2 lacks it. Use a plain loop instead.
entries=()
while IFS=$'\t' read -r file name; do
  [ -n "$file" ] && entries+=("$file|$name")
done < <("$PY" scripts/demo_compare.py list-manifest --manifest "$MANIFEST")

if [ ${#entries[@]} -eq 0 ]; then
  echo "error: no corpus entries from manifest $MANIFEST" >&2
  exit 2
fi

echo "typeanvil $TA_VERSION · prince: $PR_VERSION"
echo "corpus: ${#entries[@]} doc(s)"

# Determinism mode: first pass to a side dir, compare against the second.
if [ "$DO_DETERMINISM" = "1" ]; then
  BASELINE="demo/out.baseline"
  rm -rf "$BASELINE"
  echo "== determinism pass 1 (baseline) =="
fi

FAILED=0

for entry in "${entries[@]}"; do
  file="${entry%%|*}"
  name="${entry##*|}"
  base="${file%.html}"
  html="demo/corpus/$file"

  ta_pdf="$WORK/$base-ta.pdf"
  pr_pdf="$WORK/$base-pr.pdf"
  result_json="$RESULTS/$base.json"

  # --dry-run: print the exact commands (acceptance criterion 3).
  if [ "$DRY_RUN" = "1" ]; then
    echo "[dry] engine/target/debug/typeanvil render $html $GEOM_FLAGS -o $ta_pdf"
    echo "[dry] scripts/render-prince.sh $html $GEOM_FLAGS -o $pr_pdf"
    continue
  fi

  # Render both engines with the identical flag set.
  if engine/target/debug/typeanvil render "$html" $GEOM_FLAGS -o "$ta_pdf" 2>"$WORK/$base-ta.err"; then
    echo "TA  ok   $file"
  else
    echo "TA  FAIL $file :: $(head -1 "$WORK/$base-ta.err")"
    "$PY" scripts/demo_compare.py error-entry --name "$name" --file "$file" \
      --message "typeanvil render failed: $(head -1 "$WORK/$base-ta.err")" \
      --out-json "$result_json"
    FAILED=1
    continue
  fi

  if scripts/render-prince.sh "$html" $GEOM_FLAGS -o "$pr_pdf" 2>"$WORK/$base-pr.err"; then
    echo "PR  ok   $file"
  else
    echo "PR  FAIL $file :: $(head -1 "$WORK/$base-pr.err")"
    "$PY" scripts/demo_compare.py error-entry --name "$name" --file "$file" \
      --message "prince render failed: $(head -1 "$WORK/$base-pr.err")" \
      --out-json "$result_json"
    FAILED=1
    continue
  fi

  # Rasterize + diff (per-page, WPT-consistent; page-count mismatch is data).
  "$PY" scripts/demo_compare.py compare \
    --name "$name" --file "$file" \
    --ta-pdf "$ta_pdf" --pr-pdf "$pr_pdf" \
    --images-dir "$IMAGES" \
    --out-json "$result_json"
done

if [ "$DRY_RUN" = "1" ]; then
  echo "(dry run only — nothing rendered)"
  exit 0
fi

# Assemble scoreboard + gallery.
"$PY" scripts/demo_compare.py assemble \
  --results "$RESULTS" \
  --manifest "$MANIFEST" \
  --out-dir "$OUT" \
  --typeanvil-version "$TA_VERSION" \
  --prince-version "$PR_VERSION"

echo
echo "gallery: $OUT/index.html"
echo "scoreboard: $OUT/scoreboard.json"

# Determinism mode: build into demo/out.baseline, then build into demo/out,
# then byte-compare the two trees (except scoreboard's "generated" field).
if [ "$DO_DETERMINISM" = "1" ]; then
  if [ -n "${BUILD_PASS:-}" ]; then
    echo "error: --determinism cannot be combined with BUILD_PASS" >&2
    exit 2
  fi
  BASELINE="demo/out.baseline"
  rm -rf "$BASELINE"
  echo "== determinism pass 1 (baseline) =="
  OUT_DIR="$BASELINE" BUILD_PASS=1 "$0" --keep-work
  echo "== determinism pass 2 (current) =="
  "$0" --keep-work
  "$PY" scripts/demo_compare.py check-determinism "$BASELINE" "$OUT" 2>&1 || {
    echo "determinism FAILED (see above)" >&2
    exit 1
  }
  rm -rf "$BASELINE" "$OUT.tmp"
  exit 0
fi

if [ "$DO_VALIDATE" = "1" ]; then
  "$PY" scripts/demo_compare.py validate-scoreboard "$OUT/scoreboard.json"
fi

if [ "$KEEP_WORK" = "0" ]; then
  rm -rf "$WORK"
fi

exit "$FAILED"
