#!/usr/bin/env python3
"""benchmark.py — TypeAnvil vs PrinceXML vs WeasyPrint vs Headless Chrome
(CORE-123, CORE-318, CORE-403).

Renders every demo-corpus fixture through all engines at the standard demo
geometry and records, per fixture and engine:

  * wall-clock time (median of N timed runs, after warmup runs)
  * peak resident set size (macOS /usr/bin/time -l, bytes -> MiB)
  * process-tree memory: summed RSS and summed physical footprint over the
    whole process group (multi-process engines such as Chrome)
  * output page count (sanity check)

On-demand only — this is NOT part of CI or pre-commit.

Usage:
    python3 scripts/benchmark.py                     # full corpus, all engines
    python3 scripts/benchmark.py --runs 3            # fewer timed runs
    python3 scripts/benchmark.py --engine ta         # only TypeAnvil
    python3 scripts/benchmark.py --engine weasy      # only WeasyPrint
    python3 scripts/benchmark.py --engine chrome     # only Headless Chrome
    python3 scripts/benchmark.py --engine ta,chrome  # explicit engine list
    python3 scripts/benchmark.py --engine both       # TypeAnvil + Prince (legacy)
    python3 scripts/benchmark.py --filter invoice    # substring match
    python3 scripts/benchmark.py --role perf --size small   # manifest tag filters

Outputs (written next to this script's repo root):
    benchmarks/results.json   machine-readable
    benchmarks/RESULTS.md     human-readable summary

Stdlib only; pypdfium2 is used for page counts when importable (regex
fallback otherwise).
"""

import argparse
import ctypes
import ctypes.util
import json
import os
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Identical geometry to scripts/build-demo.sh so numbers map onto the demo
# corpus exactly (5in x 3in pages, 0.5in margins).
DEFAULT_GEOMETRY = {"width": "5in", "height": "3in", "margin": "0.5in"}
# Page size of the generated scaling-series book (CORE-493): a trade paperback.
SCALING_GEOMETRY = {"width": "6in", "height": "9in", "margin": "0.75in"}
# Series lengths in pages (targets; the report shows the pages each engine made).
SCALING_SIZES = (10, 50, 150, 300)
# Heavy documents (hundreds of pages in WeasyPrint/Chrome) need more than the
# 60 s a small fixture gets.
PERF_TIMEOUT_S = 300.0


def geom_ta(g: dict) -> list[str]:
    """TypeAnvil / render-weasyprint.sh per-side geometry flags."""
    m = g["margin"]
    return ["--page-width", g["width"], "--page-height", g["height"],
            "--margin-top", m, "--margin-right", m,
            "--margin-bottom", m, "--margin-left", m]


def geom_prince(g: dict) -> tuple[str, str]:
    return f"{g['width']} {g['height']}", g["margin"]


GEOM_TA = geom_ta(DEFAULT_GEOMETRY)
# WeasyPrint's CLI has no page-size flags; geometry comes from an @page rule
# built by scripts/render-weasyprint.sh. The wrapper also (re)exports
# DYLD_FALLBACK_LIBRARY_PATH for Homebrew Pango/GLib: this interpreter's
# posix_spawn strips DYLD_* vars from children, so they cannot be inherited.
WEASY_WRAPPER = REPO_ROOT / "scripts" / "render-weasyprint.sh"
# Chrome (headless shell) has no page-size CLI flags either; geometry comes
# from an @page rule appended to a prepared copy of each fixture (see
# prepare_chrome_input). Keep these values identical to GEOM_TA.
# Default install location for Chrome-for-Testing's chrome-headless-shell.
CHROME_CACHE = Path("~/.cache/puppeteer/chrome-headless-shell")
# Any single engine run longer than this is killed (whole process group) and
# counted as a failure, so a hung Chrome cannot hang the benchmark.
ENGINE_TIMEOUT_S = 60.0


# WeasyPrint 70.0 raises an AssertionError in its `float_layout` on this fixture
# (a drop-cap `::first-letter` float interacting with `float: footnote`). Prince
# and TypeAnvil render it. The bug is upstream, so the WeasyPrint leg skips the
# fixture instead of recording a per-run failure, and the skip is printed in
# both outputs. Re-test against a newer WeasyPrint before removing an entry.
# See docs/operations/benchmark-engines.md.
WEASY_KNOWN_CRASHES = {
    "Book Chapter — The Cartography of Rivers":
        "WeasyPrint 70.0 AssertionError in float_layout (upstream)",
}


def skip_reason(engine: str, name: str) -> str | None:
    """Why this engine's leg must not render this fixture, else None."""
    if engine == "weasy":
        return WEASY_KNOWN_CRASHES.get(name)
    return None


def weasy_env(lib_dir: str = "/opt/homebrew/lib") -> dict:
    """Subprocess environment for WeasyPrint.

    Mirrors the export in scripts/render-weasyprint.sh: Homebrew's Pango/GLib
    live in /opt/homebrew/lib, which is not on the default dyld search path, so
    the CLI cannot import them without the fallback. Without this the version
    probe reads back as blank instead of the version string.
    """
    env = dict(os.environ)
    if not env.get("DYLD_FALLBACK_LIBRARY_PATH") and Path(lib_dir).is_dir():
        env["DYLD_FALLBACK_LIBRARY_PATH"] = lib_dir
    return env


# Process-tree memory sampling (macOS libproc). The figure is summed over every
# process in the run's process group, so multi-process engines (Chrome spawns
# browser, renderer, GPU and utility processes) are not under-reported the way
# /usr/bin/time -l's single-process max RSS under-reports them. If libproc is
# unavailable (non-macOS), group_mem returns None and the tree figures are
# omitted while everything else keeps working.
_proc_lib = ctypes.util.find_library("proc")
_libproc = ctypes.CDLL(_proc_lib) if _proc_lib else None
RUSAGE_INFO_V4 = 4


class _RUsageInfoV4(ctypes.Structure):
    # struct rusage_info_v4: a 16-byte uuid followed by 35 uint64 fields in
    # kernel-declared order (sys/resource.h). ri_lifetime_max_phys_footprint
    # is the process's peak private footprint over its whole life, so even a
    # late sample reports the peak of a render that has already gone.
    _fields_ = [("ri_uuid", ctypes.c_uint8 * 16)] + [(n, ctypes.c_uint64) for n in (
        "ri_user_time", "ri_system_time", "ri_pkg_idle_wkups", "ri_interrupt_wkups",
        "ri_pageins", "ri_wired_size", "ri_resident_size", "ri_phys_footprint",
        "ri_proc_start_abstime", "ri_proc_exit_abstime", "ri_child_user_time",
        "ri_child_system_time", "ri_child_pkg_idle_wkups", "ri_child_interrupt_wkups",
        "ri_child_pageins", "ri_child_elapsed_abstime", "ri_diskio_bytesread",
        "ri_diskio_byteswritten", "ri_cpu_time_qos_default",
        "ri_cpu_time_qos_maintenance", "ri_cpu_time_qos_background",
        "ri_cpu_time_qos_utility", "ri_cpu_time_qos_legacy",
        "ri_cpu_time_qos_user_initiated", "ri_cpu_time_qos_user_interactive",
        "ri_billed_system_time", "ri_serviced_system_time", "ri_logical_writes",
        "ri_lifetime_max_phys_footprint", "ri_instructions", "ri_cycles",
        "ri_billed_energy", "ri_serviced_energy",
        "ri_interval_max_phys_footprint", "ri_runnable_time")]


def group_mem(pgid: int, fp_by_pid: dict, rss_by_pid: dict):
    """Sample a process group; fold the result into per-pid peak dicts.

    fp_by_pid and rss_by_pid persist across a run's samples. Each maps pid to
    the largest value seen. The footprint figure is the sum over every pid ever
    seen of its lifetime peak private footprint: a process that exits between
    polls keeps its last (peak) value, so a ~15 ms render is not forgotten. The
    RSS figure follows the original behaviour — the sum of resident sizes over
    the processes currently in the group, sampled per poll — which counts shared
    pages once per process and so over-counts multi-process engines. The group
    leader (the `/usr/bin/time` launcher itself, always pid == pgid here) is
    excluded: it is our instrumentation, not the render. Returns
    (live processes, summed resident bytes, summed peak footprint bytes) or None
    when libproc is unavailable.
    """
    if _libproc is None or not hasattr(_libproc, "proc_listpgrppids"):
        return None
    buf = (ctypes.c_int * 1024)()
    n = _libproc.proc_listpgrppids(pgid, buf, ctypes.sizeof(buf))  # a COUNT of pids
    rss = 0
    procs = 0
    for pid in buf[:max(n, 0)]:
        if pid <= 0 or pid == pgid:
            continue
        ri = _RUsageInfoV4()
        if _libproc.proc_pid_rusage(pid, RUSAGE_INFO_V4, ctypes.byref(ri)) == 0:
            fp_by_pid[pid] = max(fp_by_pid.get(pid, 0),
                                 ri.ri_lifetime_max_phys_footprint)
            rss_by_pid[pid] = max(rss_by_pid.get(pid, 0), ri.ri_resident_size)
            rss += rss_by_pid[pid]
            procs += 1
    return procs, rss, sum(fp_by_pid.values())


def _fold_sample(pgid: int, out: dict) -> None:
    """Sample the group once and keep the maxima in the accumulator dict."""
    res = group_mem(pgid, out["fp_by_pid"], out["rss_by_pid"])
    if res is None:
        return
    n, rss, fp = res
    out["seen"] = True
    out["procs"] = max(out["procs"], n)
    out["rss"] = max(out["rss"], rss)
    out["footprint"] = max(out["footprint"], fp)


def _sample_memory(pgid: int, out: dict, stop: threading.Event) -> None:
    """Poll a process group's summed memory until stop is set (sampler thread)."""
    while not stop.is_set():
        _fold_sample(pgid, out)
        stop.wait(0.002)


CORPUS = REPO_ROOT / "corpus"
MANIFEST = CORPUS / "manifest.json"
OUT_DIR = REPO_ROOT / "benchmarks"


def load_corpus(role=None, size=None, oracle=None, category=None):
    """(name, file) pairs from corpus/manifest.json passing the tag filters.

    `file` is `<category>/<doc>.html` relative to corpus/. Each document is
    rendered at the geometry its manifest entry declares (see
    `load_geometry`), so heavy and visual documents run at real page size.
    """
    def keep(e):
        return ((not role or role in (e.get("roles") or []))
                and (not size or e.get("size") == size)
                and (not oracle or e.get("oracle") == oracle)
                and (not category or e.get("category") == category))
    entries = json.loads(MANIFEST.read_text())
    return [(e["name"], e["file"]) for e in entries if keep(e)]


def perf_corpus(role=None, size=None, oracle=None, category=None):
    """load_corpus plus, for an unsized `perf` run, the visual documents.

    The visual documents (poster, ticket, flyer) carry the showcase role in
    the manifest, not perf, but the release-time run measures them too.
    """
    corpus = load_corpus(role, size, oracle, category)
    if role == "perf" and not size:
        have = {n for n, _ in corpus}
        corpus += [(n, f) for n, f in load_corpus(
            "showcase", "visual", oracle, category) if n not in have]
    return corpus


def load_geometry() -> dict:
    """name -> {width, height, margin} from corpus/manifest.json (CORE-493).

    An entry without a geometry falls back to the 5in x 3in demo geometry.
    """
    out = {}
    for e in json.loads(MANIFEST.read_text()):
        g = dict(DEFAULT_GEOMETRY)
        g.update(e.get("geometry") or {})
        out[e["name"]] = g
    return out


def engine_version_ta(binary: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"], cwd=REPO_ROOT,
        capture_output=True, text=True,
    ).stdout.strip() or "unknown"


def engine_version_prince() -> str:
    r = subprocess.run(["prince", "--version"], capture_output=True, text=True)
    return (r.stdout or r.stderr).strip().splitlines()[0] if (r.stdout or r.stderr) else "unknown"


def engine_version_weasy(binary: str = "weasyprint") -> str:
    r = subprocess.run([binary, "--version"], capture_output=True, text=True,
                       env=weasy_env())
    out = (r.stdout or r.stderr).strip()
    return out.splitlines()[0] if out else "unknown"


def engine_version_chrome(binary: Path) -> str:
    r = subprocess.run([str(binary), "--version"], capture_output=True, text=True)
    out = (r.stdout or r.stderr).strip()
    return out.splitlines()[0] if out else "unknown"


def _chrome_version_key(path: Path) -> tuple:
    # Version directory name looks like "chrome-headless-shell-154.0.8037.57".
    m = re.search(r"(\d+(?:\.\d+)*)$", path.parent.name)
    return tuple(int(x) for x in m.group(1).split(".")) if m else (0,)


def resolve_chrome_binary(explicit: str | None) -> Path | None:
    """Resolve the chrome-headless-shell binary, or None when not installed.

    An explicit path (--chrome-binary / $CHROME_BIN) wins. Otherwise glob the
    Puppeteer cache and pick the highest installed version directory.
    """
    if explicit:
        p = Path(explicit).expanduser()
        return p if p.exists() else None
    cands = [p for p in CHROME_CACHE.expanduser().glob(
        "*/chrome-headless-shell-*/chrome-headless-shell") if p.exists()]
    return max(cands, key=_chrome_version_key) if cands else None


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


def timed_run(argv: list[str], cwd: Path,
              timeout_s: float = ENGINE_TIMEOUT_S) -> dict:
    """Run argv once under /usr/bin/time -l.

    Return elapsed seconds, peak RSS and peak physical footprint bytes (largest
    single process) and process-tree memory sampled over the whole process
    group. A run that exceeds timeout_s is killed (whole process group) and
    reported as failed.
    """
    t0 = time.perf_counter()
    # start_new_session makes the child a session/process-group leader, so the
    # Popen pid is the pgid the memory sampler walks.
    proc = subprocess.Popen(
        ["/usr/bin/time", "-l"] + argv,
        cwd=str(cwd), stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        start_new_session=True,
    )
    sample = {"rss": 0, "footprint": 0, "procs": 0, "seen": False,
              "fp_by_pid": {}, "rss_by_pid": {}}
    # First sample before the sampler thread even starts (and before any
    # sleep): a ~15 ms render can be gone before a 2 ms poll would land.
    _fold_sample(proc.pid, sample)
    stop = threading.Event()
    sampler = threading.Thread(target=_sample_memory,
                               args=(proc.pid, sample, stop), daemon=True)
    sampler.start()
    timed_out = False
    try:
        _, err_bytes = proc.communicate(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        _, err_bytes = proc.communicate()
    elapsed = time.perf_counter() - t0
    stop.set()
    sampler.join(timeout=1.0)

    err = err_bytes.decode("utf-8", errors="replace")
    rss = None
    m = re.search(r"^\s*(\d+)\s+maximum resident set size", err, re.M)
    if not m:
        # Linux GNU time style, in KB, just in case.
        m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", err)
        if m:
            rss = int(m.group(1)) * 1024
    else:
        rss = int(m.group(1))
    mf = re.search(r"^\s*(\d+)\s+peak memory footprint", err, re.M)
    peak_footprint = int(mf.group(1)) if mf else None

    ok = proc.returncode == 0 and not timed_out
    # The sampled sums are None when the sampler never saw the group (libproc
    # unavailable, or every process was gone before the first sample). The
    # exact /usr/bin/time figures for the directly-launched process are a
    # floor: the tree can never be smaller than its own leader.
    tree_rss = sample["rss"] if sample["seen"] else None
    tree_fp = sample["footprint"] if sample["seen"] else None
    tree_procs = sample["procs"] if sample["seen"] else None
    if rss is not None:
        tree_rss = max(tree_rss or 0, rss)
    if peak_footprint is not None:
        tree_fp = max(tree_fp or 0, peak_footprint)
    if ok and _libproc is not None:
        tree_procs = max(tree_procs or 0, 1)
    return {
        "ok": ok,
        "elapsed_s": elapsed,
        "peak_rss_bytes": rss,
        "peak_footprint_bytes": peak_footprint,
        "tree_rss_bytes": tree_rss,
        "tree_footprint_bytes": tree_fp,
        "tree_procs": tree_procs,
        "timed_out": timed_out,
        "rc": proc.returncode,
        "err_tail": err.strip().splitlines()[0][:200] if err.strip() else "",
    }


def prepare_chrome_input(src: Path, dst: Path, base_dir: Path,
                         geom: dict | None = None) -> None:
    """Write a Chrome-ready copy of a fixture.

    Chrome's CLI has no page-size flags, so geometry comes from an @page rule
    appended at the very end of the document (it wins the cascade, like the
    WeasyPrint wrapper's appended stylesheet). A <base href> pointing at the
    corpus directory resolves the fixture's relative images, fonts and CSS.
    """
    text = src.read_text(encoding="utf-8")
    base_tag = f'<base href="file://{base_dir}/">'
    head = re.search(r"<head\b[^>]*>", text, flags=re.IGNORECASE)
    if head:
        text = text[:head.end()] + base_tag + text[head.end():]
    else:
        text = base_tag + text
    g = geom or DEFAULT_GEOMETRY
    style = (f"<style>@page {{ size: {g['width']} {g['height']} !important; "
             f"margin: {g['margin']} !important; }}</style>")
    dst.write_text(text + style, encoding="utf-8")


def render_argv(engine: str, html: Path, out_pdf: Path, ta_bin: Path,
                chrome_bin: Path | None,
                geom: dict | None = None) -> tuple[list[str], Path | None]:
    """Command for one render; returns (argv, temp dir to clean up or None)."""
    g = geom or DEFAULT_GEOMETRY
    if engine == "ta":
        return [str(ta_bin), "render", str(html)] + geom_ta(g) + ["-o", str(out_pdf)], None
    if engine == "weasy":
        # Geometry via scripts/render-weasyprint.sh, which translates the
        # per-side flags into an @page stylesheet AND (re)exports
        # DYLD_FALLBACK_LIBRARY_PATH so Homebrew Pango/GLib load. The wrapper
        # is needed because this interpreter strips DYLD_* vars from spawned
        # children; its bash startup (~10 ms) is negligible against WeasyPrint
        # render times.
        return [str(WEASY_WRAPPER), str(html)] + geom_ta(g) + [
            "-o", str(out_pdf)], None
    if engine == "chrome":
        # One process spawn per render (~0.14 s on the invoice fixture). A
        # fresh user-data-dir per run avoids profile lock contention; the
        # keychain flags are required or Chrome blocks on a keychain prompt.
        user_data_dir = Path(tempfile.mkdtemp(prefix="bench-chrome-"))
        return [
            str(chrome_bin),
            "--headless", "--disable-gpu",
            "--no-first-run", "--no-default-browser-check",
            "--no-pdf-header-footer",
            "--use-mock-keychain", "--password-store=basic",
            "--disable-extensions", "--disable-background-networking",
            f"--user-data-dir={user_data_dir.resolve()}",
            f"--print-to-pdf={out_pdf.resolve()}",
            f"file://{html.resolve()}",
        ], user_data_dir
    # Prince directly (same translation scripts/render-prince.sh does),
    # so no wrapper-script overhead lands in the timing.
    size, margin = geom_prince(g)
    return [
        "prince",
        f"--page-size={size}",
        f"--page-margin={margin}",
        "--media=print",
        str(html), "-o", str(out_pdf),
    ], None


def bench_engine(engine: str, corpus, workdir: Path, ta_bin: Path,
                 chrome_bin: Path | None, runs: int, warmup: int,
                 geoms: dict | None = None,
                 timeout_s: float = ENGINE_TIMEOUT_S) -> dict:
    """Time every (name, file) of `corpus`; `file` may be an absolute path.

    `geoms` maps name -> geometry dict (default: the 5in x 3in demo page).
    """
    results = {}
    geoms = geoms or {}
    for name, fname in corpus:
        geom = geoms.get(name, DEFAULT_GEOMETRY)
        reason = skip_reason(engine, name)
        if reason:
            results[name] = {"runs": 0, "failures": [], "skipped": reason,
                             "pages": None}
            continue
        html = CORPUS / fname
        base = Path(fname).stem
        out_pdf = workdir / f"{base}-{engine}.pdf"
        render_html = html
        if engine == "chrome":
            # Prepared once per fixture, outside the timed region.
            render_html = workdir / f"{base}-chrome-input.html"
            prepare_chrome_input(html, render_html, html.parent, geom)
        samples = []
        failures = []
        total_iters = warmup + runs
        for i in range(total_iters):
            argv, tmpdir = render_argv(engine, render_html, out_pdf, ta_bin,
                                       chrome_bin, geom)
            try:
                r = timed_run(argv, REPO_ROOT, timeout_s)
            finally:
                if tmpdir is not None:
                    shutil.rmtree(tmpdir, ignore_errors=True)
            if i < warmup:
                continue  # warm cache, discard timing
            if not r["ok"]:
                if r["timed_out"]:
                    failures.append(f"run {i}: timeout after {timeout_s:.0f}s")
                else:
                    failures.append(f"run {i}: rc={r['rc']} {r.get('err_tail', '')}".rstrip())
                continue
            samples.append(r)
        entry = {
            "runs": len(samples),
            "failures": failures,
        }
        if samples:
            times = [s["elapsed_s"] for s in samples]
            rss_vals = [s["peak_rss_bytes"] for s in samples]
            fp_vals = [s["peak_footprint_bytes"] for s in samples
                       if s["peak_footprint_bytes"] is not None]
            entry.update({
                "median_s": round(statistics.median(times), 4),
                "min_s": round(min(times), 4),
                "max_s": round(max(times), 4),
                "peak_rss_mib": round(max(rss_vals) / (1024 * 1024), 1),
                "peak_rss_median_mib": round(statistics.median(rss_vals) / (1024 * 1024), 1),
            })
            if fp_vals:
                entry["peak_footprint_mib"] = round(max(fp_vals) / (1024 * 1024), 1)
                entry["peak_footprint_median_mib"] = round(
                    statistics.median(fp_vals) / (1024 * 1024), 1)
            tree_rss = [s["tree_rss_bytes"] for s in samples
                        if s["tree_rss_bytes"] is not None]
            tree_fp = [s["tree_footprint_bytes"] for s in samples
                       if s["tree_footprint_bytes"] is not None]
            procs = [s["tree_procs"] for s in samples if s["tree_procs"] is not None]
            if tree_rss:
                entry["tree_rss_mib"] = round(max(tree_rss) / (1024 * 1024), 1)
            if tree_fp:
                entry["tree_footprint_mib"] = round(max(tree_fp) / (1024 * 1024), 1)
                entry["tree_footprint_median_mib"] = round(
                    statistics.median(tree_fp) / (1024 * 1024), 1)
            if procs:
                entry["max_procs"] = max(procs)
        if out_pdf.exists():
            entry["pages"] = count_pages(out_pdf)
            entry["pdf_kib"] = round(out_pdf.stat().st_size / 1024, 1)
        else:
            entry["pages"] = None
        results[name] = entry
    return results


_WORDS = ("river sediment channel flow margin print engine page break "
          "column table figure layout rule stream gauge basin delta "
          "survey record method result model field value series").split()
PARAS_PER_PAGE = 6   # calibrated for SCALING_GEOMETRY at 10pt / 1.4 line height
PARAS_PER_CHAPTER = 60


def _paragraph(i: int) -> str:
    """Deterministic ~60-word paragraph; no randomness, no clock."""
    words = [_WORDS[(i * 7 + k * 13 + (k * k) % 5) % len(_WORDS)]
             for k in range(60)]
    words[0] = words[0].capitalize()
    return " ".join(words) + "."


def scaling_html(pages: int) -> str:
    """Self-contained book of roughly `pages` pages at SCALING_GEOMETRY.

    Plain flow text with chapter headings: the same constructs at every
    size, so only length changes between series members.
    """
    n = max(1, pages * PARAS_PER_PAGE)
    parts = ["<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">"
             "<title>Scaling series</title><style>"
             "body{font-family:serif;font-size:10pt;line-height:1.4}"
             "p{margin:0 0 10pt;text-align:justify}"
             "h2{font-size:14pt;margin:0 0 12pt;break-before:page}"
             "</style></head><body>"]
    for i in range(n):
        if i % PARAS_PER_CHAPTER == 0:
            parts.append(f"<h2>Chapter {i // PARAS_PER_CHAPTER + 1}</h2>")
        parts.append(f"<p>{_paragraph(i)}</p>")
    parts.append("</body></html>\n")
    return "\n".join(parts)


EMPTY_HTML = ("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">"
              "<title>Empty</title></head><body><p>.</p></body></html>\n")


def scaling_corpus(workdir: Path) -> tuple[list, dict]:
    """Write the startup + scaling inputs; return (corpus, geometries)."""
    sdir = workdir.resolve() / "scaling"  # absolute: bench_engine joins onto corpus/
    sdir.mkdir(parents=True, exist_ok=True)
    corpus = []
    empty = sdir / "empty.html"
    empty.write_text(EMPTY_HTML, encoding="utf-8")
    corpus.append((STARTUP_NAME, str(empty)))
    for n in SCALING_SIZES:
        f = sdir / f"book-{n}.html"
        f.write_text(scaling_html(n), encoding="utf-8")
        corpus.append((scaling_name(n), str(f)))
    return corpus, {name: SCALING_GEOMETRY for name, _ in corpus}


STARTUP_NAME = "Empty document (startup)"


def scaling_name(n: int) -> str:
    return f"Book ~{n} pp"


def per_page_ms(entry: dict, startup_s: float | None) -> float | None:
    """ms per page net of the engine's startup cost, or None when unknown."""
    med, pages = entry.get("median_s"), entry.get("pages")
    if med is None or not pages:
        return None
    net = med - (startup_s or 0.0)
    return max(net, 0.0) * 1000.0 / pages


def growth_exponent(points: list[tuple[int, float]],
                    min_pages: int = 50) -> float | None:
    """Least-squares slope of log(net seconds) vs log(pages).

    ~1.0 means linear growth; clearly above 1.0 is worse than linear.
    Points under `min_pages` are ignored: after subtracting startup their net
    time is within timer noise (Chrome at 10 pages nets ~0 s), which skews a
    log fit. Needs >= 2 remaining points with positive time.
    """
    import math
    pts = [(math.log(p), math.log(t)) for p, t in points
           if p >= min_pages and t > 0]
    if len(pts) < 2:
        return None
    mx = sum(x for x, _ in pts) / len(pts)
    my = sum(y for _, y in pts) / len(pts)
    den = sum((x - mx) ** 2 for x, _ in pts)
    if den == 0:
        return None
    return sum((x - mx) * (y - my) for x, y in pts) / den


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
    names = {"ta": "TypeAnvil", "prince": "PrinceXML", "weasy": "WeasyPrint",
             "chrome": "Headless Chrome"}

    def table(key: str, fmt: str) -> list:
        out = ["| Fixture | " + " | ".join(data["engines"]) + " |",
               "|---|" + "---|" * len(data["engines"])]
        for name, _f in data["corpus"]:
            cells = []
            for eng in data["engines"]:
                v = data["results"][eng].get(name, {}).get(key)
                cells.append("—" if v is None else format(v, fmt))
            out.append(f"| {name} | " + " | ".join(cells) + " |")
        return out

    engines = [names.get(e, str(e)) for e in data["engines"]]
    lines.append("# Engine benchmark — " + " vs ".join(engines))
    lines.append("")
    versions = []
    if "ta" in data["engines"]:
        versions.append(f"TypeAnvil `{data['typeanvil_version']}` "
                        f"({data['typeanvil_profile']} profile)")
    if "prince" in data["engines"]:
        versions.append(str(data["prince_version"]))
    if "weasy" in data["engines"]:
        versions.append(str(data["weasy_version"]))
    if "chrome" in data["engines"]:
        versions.append(str(data["chrome_version"]))
    lines.append(f"Generated {env['date_utc']} · " + " · ".join(versions))
    lines.append("")
    lines.append(f"Environment: {env.get('cpu', env['platform'])}, "
                 f"{round(env.get('ram_bytes', 0) / (1024**3))} GB RAM, "
                 f"macOS ({env['platform'].split()[0]} {env['platform'].split()[1] if len(env['platform'].split()) > 1 else ''})")
    lines.append("")
    lines.append("Method: each fixture rendered at the geometry its "
                 "`corpus/manifest.json` entry declares (the small corpus "
                 "uses 5in x 3in pages, 0.5in margins; heavy and visual "
                 "documents use their real page size). Release-time "
                 "measurement: same host, TypeAnvil release profile. "
                 "WeasyPrint uses a matching "
                 "@page stylesheet; Headless Chrome receives a prepared copy of "
                 "the fixture with a `<base href>` for relative assets and the "
                 "same @page rule appended. 1 warmup run discarded, then "
                 f"{data['config']['timed_runs']} timed runs; wall-clock is the median, "
                 "single-process memory is `/usr/bin/time -l`, process-tree "
                 "memory is sampled over the whole process group. Process spawn "
                 "cost included for all engines.")
    lines.append("")
    skipped = [(eng, name, r["skipped"])
               for eng in data["engines"]
               for name, _f in data["corpus"]
               if (r := data["results"][eng].get(name, {})).get("skipped")]
    if skipped:
        lines.append("Skipped: " + "; ".join(
            f"{names.get(eng, eng)} / {name} — {why}" for eng, name, why in skipped))
        lines.append("")
    lines.append("## Render speed (median wall-clock) (s)")
    lines.append("")
    lines += table("median_s", ".3f")
    lines.append("")
    lines.append("## Peak memory (MiB)")
    lines.append("")
    lines.append("Largest single process. Under-reports multi-process engines "
                 "(Chrome) — see the process-tree tables.")
    lines.append("")
    lines += table("peak_rss_mib", ".1f")
    lines.append("")
    lines.append("## Peak memory, whole process tree — summed RSS (MiB)")
    lines.append("")
    lines.append("Sum of resident memory across every process the render "
                 "spawned; shared pages are counted once per process, so this "
                 "over-counts multi-process engines.")
    lines.append("")
    lines += table("tree_rss_mib", ".1f")
    lines.append("")
    lines.append("## Peak memory, whole process tree — physical footprint (MiB)")
    lines.append("")
    lines.append("Sum of each process's private physical footprint (the figure "
                 "Activity Monitor shows as Memory); excludes shared libraries. "
                 "The fairest single cross-engine memory comparison.")
    lines.append("")
    lines += table("tree_footprint_mib", ".1f")
    lines.append("")
    procs = []
    for eng in data["engines"]:
        vals = [data["results"][eng].get(name, {}).get("max_procs")
                for name, _f in data["corpus"]]
        vals = [v for v in vals if v is not None]
        if vals:
            procs.append(f"{eng} {max(vals)}")
    if procs:
        lines.append("Processes per render: " + ", ".join(procs) + ".")
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
    lines += perf_sections(data, names)
    path.write_text("\n".join(lines) + "\n")


def perf_record(data: dict) -> dict:
    """Compact per-engine perf numbers for the typeanvil-showcase perf history.

    Per engine: startup seconds, ms/page for each heavy document and each
    scaling-series member (startup subtracted), and the growth exponent.
    """
    rec = {"commit": data["typeanvil_version"], "date_utc": data["environment"]["date_utc"],
           "profile": data["typeanvil_profile"], "engines": {}}
    for eng in data["engines"]:
        st = data["startup"].get(eng, {}).get(STARTUP_NAME, {}).get("median_s")
        docs = {n: per_page_ms(data["results"][eng].get(n, {}), st)
                for n in data.get("heavy_names", []) if n in data["results"][eng]}
        series = {scaling_name(n): per_page_ms(data["scaling"][eng].get(scaling_name(n), {}), st)
                  for n in SCALING_SIZES}
        pts = [(e["pages"], e["median_s"] - (st or 0.0))
               for e in data["scaling"][eng].values()
               if e.get("median_s") is not None and e.get("pages")]
        rec["engines"][eng] = {
            "startup_s": st,
            "heavy_ms_per_page": {k: v for k, v in docs.items() if v is not None},
            "scaling_ms_per_page": {k: v for k, v in series.items() if v is not None},
            "growth_exponent": growth_exponent(pts),
        }
    return rec


def perf_sections(data: dict, names: dict) -> list:
    """Startup, heavy-document and scaling-series sections (CORE-493)."""
    if "startup" not in data:
        return []
    engs = data["engines"]
    startup = data["startup"]
    out = ["## Startup cost (empty document)", "",
           "One-paragraph document, same process spawn as every other run. "
           "Per-page figures below subtract this per engine.", "",
           "| Engine | Median wall-clock (ms) | Peak RSS (MiB) |", "|---|---|---|"]
    start_s = {}
    for eng in engs:
        e = startup.get(eng, {}).get(STARTUP_NAME, {})
        if e.get("median_s") is not None:
            start_s[eng] = e["median_s"]
        out.append(f"| {names.get(eng, eng)} | "
                   + (f"{e['median_s'] * 1000:.0f}" if "median_s" in e else "—")
                   + " | " + (f"{e['peak_rss_mib']:.1f}" if "peak_rss_mib" in e else "—")
                   + " |")
    out.append("")

    heavy = [(n, f) for n, f in data["corpus"] if n in data.get("heavy_names", [])]
    out += ["## Heavy and visual documents at real geometry", "",
            "Each document renders at the page size its manifest entry "
            "declares. Cells: median wall-clock seconds / pages / ms per page "
            "(startup subtracted) / peak footprint MiB (whole process tree).", "",
            "| Document | Geometry | " + " | ".join(names.get(e, e) for e in engs) + " |",
            "|---|---|" + "---|" * len(engs)]
    geoms = data.get("geometries", {})
    for name, _f in heavy:
        g = geoms.get(name, DEFAULT_GEOMETRY)
        cells = []
        for eng in engs:
            e = data["results"][eng].get(name, {})
            if e.get("skipped"):
                cells.append("skipped")
            elif "median_s" not in e:
                cells.append("—")
            else:
                ms = per_page_ms(e, start_s.get(eng))
                fp = e.get("tree_footprint_mib")
                cells.append(f"{e['median_s']:.2f} s / {e.get('pages')} pp / "
                             + (f"{ms:.1f} ms/pp" if ms is not None else "—")
                             + (f" / {fp:.0f} MiB" if fp is not None else ""))
        out.append(f"| {name} | {g['width']}×{g['height']}, {g['margin']} | "
                   + " | ".join(cells) + " |")
    out.append("")

    scaling = data.get("scaling", {})
    out += ["## Scaling series", "",
            "Generated plain-flow book (justified text, a chapter heading "
            f"every {PARAS_PER_CHAPTER // PARAS_PER_PAGE} pages) at "
            f"{SCALING_GEOMETRY['width']}×{SCALING_GEOMETRY['height']}, "
            f"{SCALING_GEOMETRY['margin']} margins, built at roughly "
            f"{', '.join(map(str, SCALING_SIZES))} pages. Only length changes "
            "between rows. Cells: median seconds / actual pages / ms per page "
            "(startup subtracted).", "",
            "| Target | " + " | ".join(names.get(e, e) for e in engs) + " |",
            "|---|" + "---|" * len(engs)]
    for n in SCALING_SIZES:
        cells = []
        for eng in engs:
            e = scaling.get(eng, {}).get(scaling_name(n), {})
            if "median_s" not in e:
                cells.append("—")
                continue
            ms = per_page_ms(e, start_s.get(eng))
            cells.append(f"{e['median_s']:.2f} s / {e.get('pages')} pp / "
                         + (f"{ms:.1f} ms/pp" if ms is not None else "—"))
        out.append(f"| ~{n} pp | " + " | ".join(cells) + " |")
    out.append("")
    exps = []
    for eng in engs:
        pts = []
        for n in SCALING_SIZES:
            e = scaling.get(eng, {}).get(scaling_name(n), {})
            if "median_s" in e and e.get("pages"):
                pts.append((e["pages"], e["median_s"] - start_s.get(eng, 0.0)))
        k = growth_exponent(pts)
        if k is not None:
            exps.append(f"{names.get(eng, eng)} {k:.2f}")
    if exps:
        out.append("Growth exponent (slope of log time vs log pages over the "
                   "rows of 50+ pages, startup subtracted; 1.00 = linear, "
                   "above 1.00 = worse than linear): "
                   + ", ".join(exps) + ".")
        out.append("")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--runs", type=int, default=5, help="timed runs per fixture/engine")
    ap.add_argument("--warmup", type=int, default=1)
    ap.add_argument("--ta-binary", default="engine/target/release/typeanvil")
    ap.add_argument("--engine", default="all",
                    help="comma list or alias: ta,prince,weasy,chrome | all | both "
                         "(default: all; 'both' = TypeAnvil + Prince)")
    ap.add_argument("--chrome-binary", default=None,
                    help="chrome-headless-shell binary (default: newest in the "
                         "Puppeteer cache; $CHROME_BIN also honored)")
    ap.add_argument("--filter", default="", help="substring match on fixture name")
    ap.add_argument("--role", default="perf",
                    help="corpus/manifest.json role tag (default: perf; '' = all)")
    ap.add_argument("--size", default=None, help="size tag: small | heavy | visual")
    ap.add_argument("--oracle", default=None, help="oracle tag: prince | weasyprint | chromium")
    ap.add_argument("--category", default=None, help="category directory, e.g. financial")
    ap.add_argument("--no-scaling", action="store_true",
                    help="skip the startup measurement and the scaling series")
    ap.add_argument("--out-dir", default=str(OUT_DIR))
    ap.add_argument("--perf-json", default=None,
                    help="also write a compact perf record (for the "
                         "typeanvil-showcase perf history) to this path")
    args = ap.parse_args()

    engine_sel = args.engine
    if engine_sel == "both":
        engine_sel = "ta,prince"
    elif engine_sel == "all":
        engine_sel = "ta,prince,weasy,chrome"
    wanted = [e.strip() for e in engine_sel.split(",") if e.strip()]
    unknown = [e for e in wanted if e not in ("ta", "prince", "weasy", "chrome")]
    if unknown:
        print(f"error: unknown engine(s): {', '.join(unknown)} "
              f"(expected ta, prince, weasy, chrome, all, both)", file=sys.stderr)
        return 2

    ta_bin = (REPO_ROOT / args.ta_binary).resolve()
    if "ta" in wanted and not ta_bin.exists():
        print(f"error: TypeAnvil binary not found at {ta_bin}. "
              f"Build it: cargo build --release --manifest-path engine/Cargo.toml", file=sys.stderr)
        return 2

    chrome_bin = None
    if "chrome" in wanted:
        chrome_bin = resolve_chrome_binary(args.chrome_binary or os.environ.get("CHROME_BIN"))
        if chrome_bin is None:
            print("error: chrome-headless-shell not found. Install it: "
                  "npx @puppeteer/browsers install chrome-headless-shell@stable "
                  "(or pass --chrome-binary PATH / set $CHROME_BIN).", file=sys.stderr)
            return 2

    corpus = perf_corpus(args.role, args.size, args.oracle, args.category)
    if args.filter:
        corpus = [(n, f) for n, f in corpus if args.filter.lower() in n.lower()]
    if not corpus:
        print("error: no fixtures matched", file=sys.stderr)
        return 2

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    workdir = out_dir / ".work"
    workdir.mkdir(exist_ok=True)

    engines: dict[str, str] = {}
    if "ta" in wanted:
        engines["ta"] = "typeanvil"
    if "prince" in wanted:
        if not shutil.which("prince"):
            print("error: prince binary not found on PATH", file=sys.stderr)
            return 2
        engines["prince"] = "prince"
    if "weasy" in wanted:
        if not shutil.which("weasyprint"):
            print("error: weasyprint binary not found on PATH "
                  "(install: uv tool install weasyprint)", file=sys.stderr)
            return 2
        engines["weasy"] = "weasyprint"
    if "chrome" in wanted:
        engines["chrome"] = str(chrome_bin)

    print(f"corpus: {len(corpus)} fixture(s) · engines: {'+'.join(engines)} · "
          f"{args.warmup} warmup + {args.runs} timed runs each")

    data = {
        "schema": 3,
        "issue": "CORE-123, CORE-318, CORE-403, CORE-493",
        "environment": machine_info(),
        "typeanvil_version": engine_version_ta(ta_bin) if "ta" in engines else "not run",
        "prince_version": engine_version_prince() if "prince" in engines else "not run",
        "weasy_version": engine_version_weasy() if "weasy" in engines else "not run",
        "chrome_version": engine_version_chrome(chrome_bin) if "chrome" in engines else "not run",
        "chrome_binary": str(chrome_bin) if "chrome" in engines else "not run",
        "typeanvil_profile": "debug" if "/debug/" in str(ta_bin) else "release",
        "config": {"warmup_runs": args.warmup, "timed_runs": args.runs},
        "corpus": corpus,
        "engines": list(engines.keys()),
        "results": {},
    }

    geoms = load_geometry()
    data["geometries"] = {n: geoms.get(n, DEFAULT_GEOMETRY) for n, _ in corpus}
    data["heavy_names"] = [e["name"] for e in json.loads(MANIFEST.read_text())
                           if e.get("size") in ("heavy", "visual")]
    perf_mode = args.role == "perf" and not args.no_scaling and not args.filter
    rc = 0
    for eng in engines:
        data["results"][eng] = bench_engine(
            eng, corpus, workdir, ta_bin, chrome_bin, args.runs, args.warmup,
            geoms=geoms, timeout_s=PERF_TIMEOUT_S)
    if perf_mode:
        sc_corpus, sc_geoms = scaling_corpus(workdir)
        data["startup"], data["scaling"] = {}, {}
        for eng in engines:
            res = bench_engine(eng, sc_corpus, workdir, ta_bin, chrome_bin,
                               args.runs, args.warmup, geoms=sc_geoms,
                               timeout_s=PERF_TIMEOUT_S)
            data["startup"][eng] = {STARTUP_NAME: res.pop(STARTUP_NAME)}
            data["scaling"][eng] = res
            for n, e in res.items():
                if e["failures"]:
                    print(f"{eng:6s} {n}: FAILURES: {'; '.join(e['failures'])}")
                    rc = 1

    # Report + fail loudly on render failures (a failed render must not
    # silently become a missing number).
    for eng, per in data["results"].items():
        for name, e in per.items():
            if e.get("skipped"):
                print(f"{eng:6s} {name[:40]:40s} SKIPPED  {e['skipped']}")
                continue
            status = "ok" if not e["failures"] else f"FAILURES: {'; '.join(e['failures'])}"
            med = e.get("median_s", "—")
            mem = e.get("peak_rss_mib", "—")
            tree = e.get("tree_footprint_mib", "—")
            pages = e.get("pages", "—")
            print(f"{eng:6s} {name[:40]:40s} median {med}s  peak {mem} MiB  "
                  f"tree {tree} MiB  pages {pages}  {status}")
            if e["failures"]:
                rc = 1

    (out_dir / "results.json").write_text(json.dumps(data, indent=2) + "\n")
    write_results_md(data, out_dir / "RESULTS.md")
    if args.perf_json and "startup" in data:
        Path(args.perf_json).write_text(
            json.dumps(perf_record(data), indent=2, sort_keys=True) + "\n")
    print(f"\nwrote {out_dir/'results.json'} and {out_dir/'RESULTS.md'}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
