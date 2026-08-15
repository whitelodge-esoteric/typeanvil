"""Orchestrate a conformance run: enumerate -> render test+ref -> compare -> record.

For each test the runner renders both the test HTML and its reference through the *same*
engine (research brief section 5, item 6, mode (a)), rasterizes each PDF at 96 DPI, and
compares page-by-page with fuzzy semantics. Chained references are followed one level:
the immediate ref is compared against the test, and if that ref itself carries a
reference the deeper comparison is evaluated too (WPT AND-chains for ``rel=match``).

Engine crashes, timeouts, or any exception during a single test are captured as an
``ERROR`` status and never abort the run (brief: "Crashes/timeouts in the engine =
ERROR, never abort the run").

Parallelism: Playwright's sync API is not thread-safe, so workers are *processes*
(``ProcessPoolExecutor``), each owning one browser instance for its lifetime. Default
pool size is 4.
"""

from __future__ import annotations

import time
import traceback
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

from .compare import CompareResult, compare
from .engine import ChromiumEngine, CliEngine, EngineAdapter, PageSpec
from .manifest import Ref, TestCase, enumerate_tests
from .rasterize import rasterize_pdf

Status = Literal["PASS", "FAIL", "ERROR", "SKIP"]


@dataclass
class TestResult:
    """Outcome of running a single test."""

    __test__ = False  # not a pytest test class despite the name

    id: str
    status: Status
    time: float
    diff_stats: dict = field(default_factory=dict)
    message: str = ""


# ---------------------------------------------------------------------------
# Engine construction (per worker process)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class EngineConfig:
    """Serializable description of the engine to build inside each worker."""

    kind: Literal["chromium", "cli"]
    wpt_root: Path
    cli_cmd: str | None = None
    timeout_ms: int = 30_000

    def build(self) -> EngineAdapter:
        if self.kind == "chromium":
            eng = ChromiumEngine(self.wpt_root, timeout_ms=self.timeout_ms)
            eng.start()
            return eng
        if self.kind == "cli":
            assert self.cli_cmd, "cli engine requires cli_cmd"
            return CliEngine(self.cli_cmd)
        raise ValueError(f"unknown engine kind: {self.kind}")


# ---------------------------------------------------------------------------
# Per-test evaluation
# ---------------------------------------------------------------------------


def _compare_ref(
    engine: EngineAdapter,
    test_images: list,
    ref: Ref,
    tc: TestCase,
    spec: PageSpec,
    artifact_dir: Path | None,
) -> CompareResult:
    """Compare the test against a single reference, following one chain level."""
    ref_pdf = engine.render_pdf(ref.path, spec)
    ref_images = rasterize_pdf(ref_pdf)
    mismatch = ref.relation == "!="
    result = compare(
        test_images,
        ref_images,
        fuzzy=tc.fuzzy.get(ref.path.name) or tc.fuzzy.get(None),
        pages=tc.pages,
        mismatch=mismatch,
        artifact_dir=artifact_dir,
    )
    # Chained reference: for rel=match, the ref must itself match its own ref (AND).
    if ref.chained is not None and not mismatch:
        chained = _compare_ref(engine, ref_images, ref.chained, tc, spec, None)
        if not chained.passed:
            result.passed = False
            if not result.reason:
                result.reason = f"chained ref failed: {chained.reason}"
    return result


def run_one(
    engine: EngineAdapter,
    tc: TestCase,
    spec: PageSpec,
    artifacts_root: Path | None,
) -> TestResult:
    """Render + compare a single test. Never raises; errors become ERROR results."""
    start = time.monotonic()
    try:
        test_pdf = engine.render_pdf(tc.path, spec)
        test_images = rasterize_pdf(test_pdf)

        artifact_dir = (artifacts_root / tc.id) if artifacts_root else None

        # A test may declare multiple refs. rel=match with several refs => any-of (OR);
        # a single ref is the common case. We PASS if any match-ref passes; for a lone
        # mismatch-ref we honour its result directly.
        best: CompareResult | None = None
        for ref in tc.refs:
            res = _compare_ref(engine, test_images, ref, tc, spec, artifact_dir)
            if res.passed:
                best = res
                break
            if best is None:
                best = res

        assert best is not None
        status: Status = "PASS" if best.passed else "FAIL"
        stats = {
            "max_difference": best.max_difference,
            "total_pixels": best.total_pixels,
            "pages": [
                {
                    "page": p.page_index,
                    "max_difference": p.max_difference,
                    "total_pixels": p.total_pixels,
                    "passed": p.passed,
                }
                for p in best.pages
            ],
        }
        return TestResult(
            id=tc.id,
            status=status,
            time=time.monotonic() - start,
            diff_stats=stats,
            message=best.reason,
        )
    except Exception as exc:  # noqa: BLE001 -- engine crashes must not abort the run
        return TestResult(
            id=tc.id,
            status="ERROR",
            time=time.monotonic() - start,
            message=f"{type(exc).__name__}: {exc}\n{traceback.format_exc(limit=3)}",
        )


# ---------------------------------------------------------------------------
# Worker-process entry point
# ---------------------------------------------------------------------------

_WORKER_ENGINE: EngineAdapter | None = None
_WORKER_CONFIG: EngineConfig | None = None


def _worker_init(config: EngineConfig) -> None:
    global _WORKER_ENGINE, _WORKER_CONFIG
    _WORKER_CONFIG = config
    _WORKER_ENGINE = config.build()


def _worker_run(args: tuple[TestCase, PageSpec, Path | None]) -> TestResult:
    tc, spec, artifacts_root = args
    assert _WORKER_ENGINE is not None, "worker engine not initialized"
    return run_one(_WORKER_ENGINE, tc, spec, artifacts_root)


# ---------------------------------------------------------------------------
# Top-level run
# ---------------------------------------------------------------------------


@dataclass
class RunConfig:
    wpt_root: Path
    engine: EngineConfig
    spec: PageSpec = field(default_factory=PageSpec.wpt_default)
    filter_substr: str | None = None
    limit: int | None = None
    workers: int = 4
    artifacts_root: Path | None = None


def select_tests(cfg: RunConfig) -> list[TestCase]:
    """Enumerate + filter + limit the tests to run."""
    tests = enumerate_tests(cfg.wpt_root)
    if cfg.filter_substr:
        tests = [t for t in tests if cfg.filter_substr in t.id]
    if cfg.limit is not None:
        tests = tests[: cfg.limit]
    return tests


def run(cfg: RunConfig, *, on_result=None) -> list[TestResult]:
    """Run all selected tests, returning a list of :class:`TestResult`.

    ``on_result``: optional callback invoked with each :class:`TestResult` as it
    completes (for live progress). Runs sequentially when ``workers <= 1`` (simpler,
    used by tests); otherwise a process pool with one browser per worker.
    """
    tests = select_tests(cfg)
    if cfg.artifacts_root:
        cfg.artifacts_root.mkdir(parents=True, exist_ok=True)

    results: list[TestResult] = []

    if cfg.workers <= 1:
        engine = cfg.engine.build()
        try:
            for tc in tests:
                res = run_one(engine, tc, cfg.spec, cfg.artifacts_root)
                results.append(res)
                if on_result:
                    on_result(res)
        finally:
            close = getattr(engine, "close", None)
            if close:
                close()
        return results

    with ProcessPoolExecutor(
        max_workers=cfg.workers,
        initializer=_worker_init,
        initargs=(cfg.engine,),
    ) as pool:
        futures = {
            pool.submit(_worker_run, (tc, cfg.spec, cfg.artifacts_root)): tc
            for tc in tests
        }
        for fut in as_completed(futures):
            tc = futures[fut]
            try:
                res = fut.result()
            except Exception as exc:  # noqa: BLE001 -- worker died
                res = TestResult(
                    id=tc.id, status="ERROR", time=0.0, message=f"worker crash: {exc}"
                )
            results.append(res)
            if on_result:
                on_result(res)

    # Stable ordering by test id for reproducible reports.
    results.sort(key=lambda r: r.id)
    return results
