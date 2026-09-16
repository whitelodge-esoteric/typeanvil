#!/usr/bin/env bash
# build-demo.sh — render the corpus through TypeAnvil + Prince, rasterize,
# diff, and emit the static gallery + scoreboard (CORE-71).
#
# Contract: docs/specifications/visual-comparison-demo.spec.md
#   §Behavior 1-11, §Interfaces, §Acceptance Criteria.
#
# Output: the gallery section of demo/corpus/README.md, plus the artifacts in
# demo/corpus/out/ (per-page images, scoreboard.json). Every corpus-track file
# lives under demo/corpus/, mirroring demo/showcase/.
# Deterministic: re-run on an unchanged tree is byte-identical except the
# scoreboard's "generated" field.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PY="${PY:-.venv/bin/python}"
PRINCE_BIN="${PRINCE_BIN:-prince}"
OUT="${OUT_DIR:-demo/corpus/out}"
WORK="$OUT/.work"
IMAGES="$OUT/images"
RESULTS="$OUT/.results"
GEOM_FLAGS="--page-width 5in --page-height 3in --margin-top 0.5in --margin-right 0.5in --margin-bottom 0.5in --margin-left 0.5in"
MANIFEST="demo/corpus/manifest.json"
BENCH_MANIFEST="demo/corpus/benchmark_manifest.json"
# Fixtures resolve relative asset URLs (img src, @font-face url()) against
# the process CWD on main (CORE-103 note: --base-url threading lands with
# CORE-140's link-CSS work). Render with the corpus dir as CWD so both
# engines resolve `assets/...` identically.
CORPUS_DIR="demo/corpus"

usage() {
  cat <<'EOF'
usage: build-demo.sh [--dry-run] [--keep-work] [--validate] [--determinism] [--guard]
  --keep-work      keep .work/.results dirs (default: cleaned at exit)
  --validate       validate demo/corpus/out/scoreboard.json after building
  --guard          check scoreboard movement against committed version
EOF
}

DRY_RUN=0
KEEP_WORK=0
DO_VALIDATE=0
DO_DETERMINISM=0
DO_GUARD=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --keep-work) KEEP_WORK=1 ;;
    --validate) DO_VALIDATE=1 ;;
    --determinism) DO_DETERMINISM=1 ;;
    --guard) DO_GUARD=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option $arg" >&2; usage; exit 2 ;;
  esac
done

if ! command -v "$PRINCE_BIN" >/dev/null 2>&1; then
  echo "error: Prince binary not found ('$PRINCE_BIN'). Install per demo/corpus/README.md (brew install --cask prince; ./install.sh /opt/homebrew)." >&2
  exit 2
fi

if [ ! -x engine/target/debug/typeanvil ]; then
  echo "error: engine binary not found (engine/target/debug/typeanvil). Build first: cargo build --manifest-path engine/Cargo.toml" >&2
  exit 2
fi

rm -rf "$RESULTS" "$WORK"
mkdir -p "$WORK" "$RESULTS" "$IMAGES"

# Guard mode: snapshot the committed scoreboard BEFORE the build overwrites
# it, so the post-assemble check compares fresh vs committed (CORE-215).
GUARD_SNAPSHOT=""
if [ "$DO_GUARD" = "1" ]; then
  GUARD_SNAPSHOT="$WORK/scoreboard.committed.json"
  if [ -f "$OUT/scoreboard.json" ]; then
    cp "$OUT/scoreboard.json" "$GUARD_SNAPSHOT"
    echo "guard: committed scoreboard snapshotted ($OUT/scoreboard.json)"
  else
    echo "warning: no committed scoreboard at $OUT/scoreboard.json — guard has nothing to compare against" >&2
    GUARD_SNAPSHOT=""
  fi
fi

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
  BASELINE="demo/corpus/out.baseline"
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

  # Render both engines with the identical flag set (from the corpus dir so
  # relative asset URLs resolve the same for both engines).
  if (cd "$CORPUS_DIR" && "$REPO_ROOT/engine/target/debug/typeanvil" render "$file" $GEOM_FLAGS -o "$REPO_ROOT/$ta_pdf" 2>"$REPO_ROOT/$WORK/$base-ta.err"); then
    echo "TA  ok   $file"
  else
    echo "TA  FAIL $file :: $(head -1 "$WORK/$base-ta.err")"
    "$PY" scripts/demo_compare.py error-entry --name "$name" --file "$file" \
      --message "typeanvil render failed: $(head -1 "$WORK/$base-ta.err")" \
      --out-json "$result_json"
    FAILED=1
    continue
  fi

  if (cd "$CORPUS_DIR" && "$REPO_ROOT/scripts/render-prince.sh" "$file" $GEOM_FLAGS -o "$REPO_ROOT/$pr_pdf" 2>"$REPO_ROOT/$WORK/$base-pr.err"); then
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

# ---- Benchmark layer (CORE-146): one fixture per open engine issue. ----
# Rendered with the CURRENT engine only (expected-vs-actual is the point;
# these show known gaps on purpose). Safe-to-render was verified per fixture
# before entering the manifest.
bench_entries=()
if [ -f "$BENCH_MANIFEST" ]; then
  while IFS=$'\t' read -r file name; do
    [ -n "$file" ] && bench_entries+=("$file")
  done < <("$PY" scripts/demo_compare.py list-manifest --manifest "$BENCH_MANIFEST")
fi

bench_failed=0
for file in "${bench_entries[@]:-}"; do
  [ -n "$file" ] || continue
  base="$(basename "${file%.html}")"
  ta_pdf="$WORK/bench-$base-ta.pdf"
  if (cd "$CORPUS_DIR" && "$REPO_ROOT/engine/target/debug/typeanvil" render "$file" $GEOM_FLAGS -o "$REPO_ROOT/$ta_pdf" 2>"$REPO_ROOT/$WORK/bench-$base-ta.err"); then
    echo "BENCH ok   $file"
  else
    echo "BENCH FAIL $file :: $(head -1 "$WORK/bench-$base-ta.err")"
    bench_failed=1
    continue
  fi
  # Rasterize the TA render into images/bench-<name>/ so the gallery section
  # can inline page 1.
  "$PY" -c "
import sys
sys.path.insert(0, '$REPO_ROOT')
from harness.rasterize import rasterize_pdf
from pathlib import Path
imgs = rasterize_pdf(Path('$ta_pdf'), dpi=96)
out = Path('$IMAGES/bench-$base')
out.mkdir(parents=True, exist_ok=True)
for i, im in enumerate(imgs, 1):
    im.save(out / f'page-{i:03d}-ta.png', 'PNG')
"
done

# Assemble the scoreboard and write the gallery into the corpus README.
# demo_compare.py owns the splice so both tracks behave identically: the
# hand-maintained preamble above the marker is never rewritten, and the
# generated body below it is replaced wholesale on every build.
"$PY" scripts/demo_compare.py assemble \
  --results "$RESULTS" \
  --manifest "$MANIFEST" \
  --benchmark-manifest "$BENCH_MANIFEST" \
  --out-dir "$OUT" \
  --readme demo/corpus/README.md \
  --image-prefix out/images \
  --typeanvil-version "$TA_VERSION" \
  --prince-version "$PR_VERSION"

echo

# Determinism mode: build into demo/corpus/out.baseline, then build into
# demo/corpus/out, then byte-compare the two trees (except scoreboard's
# "generated" field).
if [ "$DO_DETERMINISM" = "1" ]; then
  if [ -n "${BUILD_PASS:-}" ]; then
    echo "error: --determinism cannot be combined with BUILD_PASS" >&2
    exit 2
  fi
  BASELINE="demo/corpus/out.baseline"
  rm -rf "$BASELINE"
  echo "== determinism pass 1 (baseline) =="
  OUT_DIR="$BASELINE" BUILD_PASS=1 "$0" --keep-work
  # The generated gallery lives in demo/corpus/README.md, not inside out/, so
  # snapshot it here and byte-compare it after pass 2 as well.
  README_SNAP="$(mktemp -t typeanvil-readme)"
  cp demo/corpus/README.md "$README_SNAP"
  echo "== determinism pass 2 (current) =="
  "$0" --keep-work
  "$PY" scripts/demo_compare.py check-determinism "$BASELINE" "$OUT" 2>&1 || {
    echo "determinism FAILED (see above)" >&2
    exit 1
  }
  if ! cmp -s "$README_SNAP" demo/corpus/README.md; then
    echo "determinism FAILED (demo/corpus/README.md differs between passes)" >&2
    rm -f "$README_SNAP"
    exit 1
  fi
  rm -f "$README_SNAP"
  rm -rf "$BASELINE" "$OUT.tmp"
  exit 0
fi

if [ "$DO_VALIDATE" = "1" ]; then
  "$PY" scripts/demo_compare.py validate-scoreboard "$OUT/scoreboard.json"
fi

# Guard mode: compare the freshly assembled scoreboard against the committed
# snapshot (taken before the build). Per-doc movement > 3.0 pp or a page-count
# change fails unless the manifest entry carries `expected_change`.
if [ "$DO_GUARD" = "1" ]; then
  if [ -n "$GUARD_SNAPSHOT" ] && [ -f "$GUARD_SNAPSHOT" ]; then
    echo "guard: comparing fresh scoreboard against committed... "
    "$PY" scripts/demo_compare.py check-guard \
      --committed "$GUARD_SNAPSHOT" \
      --current "$OUT/scoreboard.json" \
      --manifest "$MANIFEST" || {
        echo "guard FAILED (see above)" >&2
        exit 1
      }
  else
    echo "guard: skipped — no committed scoreboard snapshot (first run?)" >&2
  fi
fi

if [ "$KEEP_WORK" = "0" ]; then
  rm -rf "$WORK"
fi

exit "$FAILED"
# A benchmark fixture render failure is a build failure too (safe-to-render
# was a precondition for entering the manifest). Unreachable when $FAILED is
# 1 (exited above); reports bench-only failures.
if [ "$bench_failed" != "0" ]; then
  exit 1
fi
