#!/usr/bin/env python3
"""benchmark.py — TypeAnvil vs PrinceXML performance benchmark (CORE-123).

Renders every demo-corpus fixture through both engines at the standard demo
geometry and records, per fixture and engine:

  * wall-clock time (median of N timed runs, after warmup runs)
  * peak resident set size (macOS /usr/bin/time -l, bytes -> MiB)
  * output page count (sanity check)

On-demand only — this is NOT part of CI or pre-commit.

Usage:
    python3 scripts/benchmark.py                     # full corpus, defaults
    python3 scripts/benchmark.py --runs 3            # fewer timed runs
    python3 scripts/benchmark.py --engine ta         # only TypeAnvil
    python3 scripts/benchmark.py --filter invoice    # substring match

Outputs (written next to this script's repo root):
    benchmarks/results.json   machine-readable
    benchmarks/RESULTS.md     human-readable summary

Stdlib only; pypdfium2 is used for page counts when importable (regex
fallback otherwise).
"""

import argparse
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Identical geometry to scripts/build-demo.sh so numbers map onto the demo
# corpus exactly (5in x 3in pages, 0.5in margins).
GEOM_TA = [
    "--page-width", "5in", "--page-height", "3in",
    "--margin-top", "0.5in", "--margin-right", "0.5in",
    "--margin-bottom", "0.5in", "--margin-left", "0.5in",
]
PAGE_SIZE_PRINCE = "360pt 216pt"
PAGE_MARGIN_PRINCE = "36pt 36pt 36pt 36pt"

MANIFEST = REPO_ROOT / "demo/corpus/manifest.json"
OUT_DIR = REPO_ROOT / "benchmarks"


def load_corpus():
    entries = json.loads(MANIFEST.read_text())
    return [(e["name"], e["file"]) for e in entries]


def engine_version_ta(binary: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"], cwd=REPO_ROOT,
        capture_output=True, text=True,
    ).stdout.strip() or "unknown"


def engine_version_prince() -> str:
    r = subprocess.run(["prince", "--version"], capture_output=True, text=True)
    return (r.stdout or r.stderr).strip().splitlines()[0] if (r.stdout or r.stderr) else "unknown"


def count_pages(pdf_path: Path) -> int | None:
    data = pdf_path.read_bytes()
    # Krilla emits a plain /Type/Pages/Count N; Prince may put /Count inside
    # an object stream. Try plain regex first, then pypdfium2.
    m = re.findall(rb"/Count\s+(\d+)", data)
    if m:
        return max(int(x) for x in m)
    try:
        import pypdfium2 as pdfium  # type: ignore
        doc = pdfium.PdfDocument(str(pdf_path))
        n = len(doc)
        doc.close()
        return n
    except Exception:
        return None


def timed_run(argv: list[str], cwd: Path) -> dict:
    """Run argv once under /usr/bin/time -l; return elapsed s + peak RSS bytes."""
    t0 = time.perf_counter()
    proc = subprocess.run(
        ["/usr/bin/time", "-l"] + argv,
        cwd=str(cwd), capture_output=True, text=False,
    )
    elapsed = time.perf_counter() - t0
    ok = proc.returncode == 0
    rss = None
    err = proc.stderr.decode("utf-8", errors="replace")
    m = re.search(r"^\s*(\d+)\s+maximum resident set size", err, re.M)
    if not m:
        # Linux GNU time style, in KB, just in case.
        m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", err)
        if m:
            rss = int(m.group(1)) * 1024
    else:
        rss = int(m.group(1))
    return {"ok": ok, "elapsed_s": elapsed, "peak_rss_bytes": rss,
            "rc": proc.returncode}


def render_once(engine: str, html: Path, out_pdf: Path, ta_bin: Path) -> list[str]:
    if engine == "ta":
        return [str(ta_bin), "render", str(html)] + GEOM_TA + ["-o", str(out_pdf)]
    # Prince directly (same translation scripts/render-prince.sh does),
    # so no wrapper-script overhead lands in the timing.
    return [
        "prince",
        f"--page-size={PAGE_SIZE_PRINCE}",
        f"--page-margin={PAGE_MARGIN_PRINCE}",
        "--media=print",
        str(html), "-o", str(out_pdf),
    ]


def bench_engine(engine: str, corpus, workdir: Path, ta_bin: Path,
                 runs: int, warmup: int) -> dict:
    results = {}
    for name, fname in corpus:
        html = REPO_ROOT / "demo/corpus" / fname
        base = fname[:-len(".html")]
        out_pdf = workdir / f"{base}-{engine}.pdf"
        samples = []
        failures = []
        total_iters = warmup + runs
        for i in range(total_iters):
            argv = render_once(engine, html, out_pdf, ta_bin)
            r = timed_run(argv, REPO_ROOT)
            if i < warmup:
                continue  # warm cache, discard timing
            if not r["ok"]:
                failures.append(f"run {i}: rc={r['rc']}")
                continue
            samples.append(r)
        entry = {
            "runs": len(samples),
            "failures": failures,
        }
        if samples:
            times = [s["elapsed_s"] for s in samples]
            rss_vals = [s["peak_rss_bytes"] for s in samples]
            entry.update({
                "median_s": round(statistics.median(times), 4),
                "min_s": round(min(times), 4),
                "max_s": round(max(times), 4),
                "peak_rss_mib": round(max(rss_vals) / (1024 * 1024), 1),
                "peak_rss_median_mib": round(statistics.median(rss_vals) / (1024 * 1024), 1),
            })
        if out_pdf.exists():
            entry["pages"] = count_pages(out_pdf)
            entry["pdf_kib"] = round(out_pdf.stat().st_size / 1024, 1)
        else:
            entry["pages"] = None
        results[name] = entry
    return results


def machine_info() -> dict:
    info = {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "date_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    if sys.platform == "darwin":
        try:
            chip = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"],
                                  capture_output=True, text=True).stdout.strip()
            ram = subprocess.run(["sysctl", "-n", "hw.memsize"],
                                 capture_output=True, text=True).stdout.strip()
            info["cpu"] = chip
            info["ram_bytes"] = int(ram)
        except Exception:
            pass
    return info


def write_results_md(data: dict, path: Path) -> None:
    lines = []
    env = data["environment"]
    lines.append("# Engine benchmark — TypeAnvil vs PrinceXML")
    lines.append("")
    lines.append(f"Generated {env['date_utc']} · TypeAnvil `{data['typeanvil_version']}` "
                 f"(release profile) · {data['prince_version']}")
    lines.append("")
    lines.append(f"Environment: {env.get('cpu', env['platform'])}, "
                 f"{round(env.get('ram_bytes', 0) / (1024**3))} GB RAM, "
                 f"macOS ({env['platform'].split()[0]} {env['platform'].split()[1] if len(env['platform'].split()) > 1 else ''})")
    lines.append("")
    lines.append("Method: each fixture rendered at the standard demo geometry "
                 "(5in x 3in pages, 0.5in margins); 1 warmup run discarded, then "
                 f"{data['config']['timed_runs']} timed runs; wall-clock is the median, "
                 "memory is the max peak RSS across timed runs (`/usr/bin/time -l`). "
                 "Process spawn cost included for both engines.")
    lines.append("")
    for metric, unit, key, fmt in (
        ("Render speed (median wall-clock)", "s", "median_s", ".3f"),
        ("Peak memory", "MiB", "peak_rss_mib", ".1f"),
    ):
        lines.append(f"## {metric} ({unit})")
        lines.append("")
        header = "| Fixture | " + " | ".join(data["engines"]) + " |"
        sep = "|---|" + "---|" * len(data["engines"])
        lines += [header, sep]
        rows = []
        for name, _f in data["corpus"]:
            cells = []
            vals = []
            for eng in data["engines"]:
                v = data["results"][eng].get(name, {}).get(key)
                if v is None:
                    cells.append("—")
                else:
                    cells.append(format(v, fmt))
                    vals.append(v)
            ratio_txt = ""
            if len(vals) == 2 and vals[1] > 0:
                ratio_txt = f" (×{vals[0]/vals[1]:.2f})"
            rows.append(f"| {name} | " + " | ".join(cells) + f" |{ratio_txt}")
        lines += rows
        lines.append("")
    lines.append("## Page counts (sanity)")
    lines.append("")
    lines.append("| Fixture | " + " | ".join(data["engines"]) + " |")
    lines.append("|---|" + "---|" * len(data["engines"]))
    for name, _f in data["corpus"]:
        cells = []
        for eng in data["engines"]:
            p = data["results"][eng].get(name, {}).get("pages")
            cells.append(str(p) if p is not None else "—")
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    lines.append("")
    path.write_text("\n".join(lines) + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--runs", type=int, default=5, help="timed runs per fixture/engine")
    ap.add_argument("--warmup", type=int, default=1)
    ap.add_argument("--ta-binary", default="engine/target/release/typeanvil")
    ap.add_argument("--engine", choices=["ta", "prince", "both"], default="both")
    ap.add_argument("--filter", default="", help="substring match on fixture name")
    ap.add_argument("--out-dir", default=str(OUT_DIR))
    args = ap.parse_args()

    ta_bin = (REPO_ROOT / args.ta_binary).resolve()
    if args.engine in ("ta", "both") and not ta_bin.exists():
        print(f"error: TypeAnvil binary not found at {ta_bin}. "
              f"Build it: cargo build --release --manifest-path engine/Cargo.toml", file=sys.stderr)
        return 2

    corpus = load_corpus()
    if args.filter:
        corpus = [(n, f) for n, f in corpus if args.filter.lower() in n.lower()]
    if not corpus:
        print("error: no fixtures matched", file=sys.stderr)
        return 2

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    workdir = out_dir / ".work"
    workdir.mkdir(exist_ok=True)

    engines = {"ta": "typeanvil"}
    if args.engine in ("prince", "both"):
        if not shutil.which("prince"):
            print("error: prince binary not found on PATH", file=sys.stderr)
            return 2
        engines["prince"] = "prince"

    print(f"corpus: {len(corpus)} fixture(s) · engines: {'+'.join(engines)} · "
          f"{args.warmup} warmup + {args.runs} timed runs each")

    data = {
        "schema": 1,
        "issue": "CORE-123",
        "environment": machine_info(),
        "typeanvil_version": engine_version_ta(ta_bin),
        "prince_version": engine_version_prince(),
        "typeanvil_profile": "debug" if "/debug/" in str(ta_bin) else "release",
        "config": {"warmup_runs": args.warmup, "timed_runs": args.runs},
        "corpus": corpus,
        "engines": list(engines.keys()),
        "results": {},
    }

    rc = 0
    for eng in engines:
        data["results"][eng] = bench_engine(eng, corpus, workdir, ta_bin,
                                            args.runs, args.warmup)

    # Report + fail loudly on render failures (a failed render must not
    # silently become a missing number).
    for eng, per in data["results"].items():
        for name, e in per.items():
            status = "ok" if not e["failures"] else f"FAILURES: {'; '.join(e['failures'])}"
            med = e.get("median_s", "—")
            mem = e.get("peak_rss_mib", "—")
            pages = e.get("pages", "—")
            print(f"{eng:6s} {name[:40]:40s} median {med}s  peak {mem} MiB  pages {pages}  {status}")
            if e["failures"]:
                rc = 1

    (out_dir / "results.json").write_text(json.dumps(data, indent=2) + "\n")
    write_results_md(data, out_dir / "RESULTS.md")
    print(f"\nwrote {out_dir/'results.json'} and {out_dir/'RESULTS.md'}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
