"""Engine adapters: the contract between the harness and an HTML/CSS -> PDF engine.

The harness renders both the test and its reference through the *same* engine and
compares the resulting PDFs page-by-page (research brief section 5, item 6, mode (a):
"strict WPT semantics -- Typeanvil(test) vs Typeanvil(ref)"). The print geometry is
fixed by the WPT print-reftest spec: a **5in x 3in page with 0.5in margins on all
sides** (brief section 1).

Two adapters implement :class:`EngineAdapter`:

* :class:`ChromiumEngine` -- the oracle. Uses Playwright's bundled Chromium via the
  *sync* API. One browser instance is reused across all tests (launch cost dominates
  otherwise). WPT tests reference support files by absolute path (``/fonts/ahem.css``,
  ``/css/support/...``), so the checkout is served over ``http://127.0.0.1:PORT/``
  rooted at ``.wpt/`` via a stdlib ``http.server`` in a background thread -- NOT
  ``file://`` URLs, which cannot resolve those absolute paths.

* :class:`CliEngine` -- invokes an external ``typeanvil render``-style CLI. This
  defines the future engine contract (documented on the class): the CLI receives the
  input HTML path, page geometry flags, and an output PDF path, and MUST write a
  deterministic, offline-rendered PDF. This is the public API v0 (brief section 5,
  item 4).

Playwright is imported lazily inside :class:`ChromiumEngine` so the rest of the harness
(manifest, compare, unit tests) stays importable without Playwright installed.
"""

from __future__ import annotations

import shlex
import subprocess
import tempfile
import threading
from dataclasses import dataclass
from functools import cached_property
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Protocol


@dataclass(frozen=True)
class PageSpec:
    """Print page geometry in inches. WPT print-reftests fix this to 5x3in / 0.5in."""

    width_in: float = 5.0
    height_in: float = 3.0
    margin_top_in: float = 0.5
    margin_right_in: float = 0.5
    margin_bottom_in: float = 0.5
    margin_left_in: float = 0.5

    @classmethod
    def wpt_default(cls) -> "PageSpec":
        return cls()

    def width_css(self) -> str:
        return f"{self.width_in}in"

    def height_css(self) -> str:
        return f"{self.height_in}in"

    def margin_css(self) -> dict[str, str]:
        return {
            "top": f"{self.margin_top_in}in",
            "right": f"{self.margin_right_in}in",
            "bottom": f"{self.margin_bottom_in}in",
            "left": f"{self.margin_left_in}in",
        }


class EngineAdapter(Protocol):
    """Renders an HTML file to a paginated PDF at the given page geometry."""

    def render_pdf(self, html_path: Path, page: PageSpec) -> bytes: ...


# ---------------------------------------------------------------------------
# Local HTTP server rooted at the WPT checkout
# ---------------------------------------------------------------------------


class WptServer:
    """Serve the WPT checkout over localhost so absolute paths (``/fonts/...``) work."""

    def __init__(self, root: Path) -> None:
        self._root = root.resolve()
        self._httpd: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> str:
        if self._httpd is not None:
            return self.base_url
        root = self._root

        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *a, **kw):
                super().__init__(*a, directory=str(root), **kw)

            def log_message(self, *args):  # silence request logging
                pass

        self._httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self._thread = threading.Thread(target=self._httpd.serve_forever, daemon=True)
        self._thread.start()
        return self.base_url

    @property
    def port(self) -> int:
        assert self._httpd is not None, "server not started"
        return self._httpd.server_address[1]

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def url_for(self, path: Path) -> str:
        """Return the served URL for a path inside the checkout root."""
        rel = path.resolve().relative_to(self._root).as_posix()
        return f"{self.base_url}/{rel}"

    def stop(self) -> None:
        if self._httpd is not None:
            self._httpd.shutdown()
            self._httpd.server_close()
            self._httpd = None
            self._thread = None


# ---------------------------------------------------------------------------
# Chromium engine (the oracle)
# ---------------------------------------------------------------------------


class ChromiumEngine:
    """Render HTML -> PDF via Playwright's bundled Chromium (sync API).

    One browser instance is reused for the lifetime of the engine. Use as a context
    manager, or call :meth:`close` when done. Not thread-safe: the sync Playwright API
    must be driven from a single thread. For parallelism, use one engine (hence one
    browser) per worker *process* (see :mod:`harness.runner`).
    """

    def __init__(self, wpt_root: Path, *, timeout_ms: int = 30_000) -> None:
        self._wpt_root = wpt_root.resolve()
        self._timeout_ms = timeout_ms
        self._server = WptServer(self._wpt_root)
        self._pw = None
        self._browser = None
        self._started = False

    def __enter__(self) -> "ChromiumEngine":
        self.start()
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def start(self) -> None:
        if self._started:
            return
        from playwright.sync_api import sync_playwright

        self._server.start()
        self._pw = sync_playwright().start()
        self._browser = self._pw.chromium.launch(args=["--font-render-hinting=none"])
        self._started = True

    def close(self) -> None:
        if self._browser is not None:
            self._browser.close()
            self._browser = None
        if self._pw is not None:
            self._pw.stop()
            self._pw = None
        self._server.stop()
        self._started = False

    def render_pdf(self, html_path: Path, page: PageSpec) -> bytes:
        if not self._started:
            self.start()
        assert self._browser is not None

        url = self._server.url_for(html_path)
        ctx = self._browser.new_context()
        try:
            pg = ctx.new_page()
            pg.goto(url, wait_until="load", timeout=self._timeout_ms)
            # Ensure webfonts (Ahem) are ready before printing.
            pg.evaluate("() => document.fonts.ready")
            return pg.pdf(
                width=page.width_css(),
                height=page.height_css(),
                margin=page.margin_css(),
                print_background=True,
                prefer_css_page_size=False,
            )
        finally:
            ctx.close()


# ---------------------------------------------------------------------------
# CLI engine (the future `typeanvil render` contract)
# ---------------------------------------------------------------------------


class CliEngine:
    """Render HTML -> PDF by invoking an external CLI (the Typeanvil engine contract).

    The CLI contract this adapter targets -- and which ``typeanvil render`` MUST
    satisfy -- is::

        typeanvil render <input.html> \\
            --page-width 5in --page-height 3in \\
            --margin-top 0.5in --margin-right 0.5in \\
            --margin-bottom 0.5in --margin-left 0.5in \\
            --base-url http://127.0.0.1:PORT/ \\
            -o <output.pdf>

    Requirements on the engine:

    * Deterministic and offline: no network fetches except via ``--base-url`` (the
      harness serves the WPT checkout locally); identical input -> identical PDF bytes.
    * Fixed geometry: honour the page-size and margin flags exactly (5x3in / 0.5in for
      WPT print-reftests).
    * Paginated output: emit one PDF page per laid-out page so page-by-page comparison
      is meaningful.

    The command is built from a template list where these placeholders are substituted:
    ``{input}``, ``{output}``, ``{width}``, ``{height}``, ``{margin_top}``,
    ``{margin_right}``, ``{margin_bottom}``, ``{margin_left}``, ``{base_url}``.
    """

    #: Default template matching the documented contract above.
    DEFAULT_TEMPLATE: tuple[str, ...] = (
        "{input}",
        "--page-width",
        "{width}",
        "--page-height",
        "{height}",
        "--margin-top",
        "{margin_top}",
        "--margin-right",
        "{margin_right}",
        "--margin-bottom",
        "{margin_bottom}",
        "--margin-left",
        "{margin_left}",
        "-o",
        "{output}",
    )

    def __init__(
        self,
        cmd: str | list[str],
        *,
        template: tuple[str, ...] | None = None,
        base_url: str | None = None,
        timeout_s: float = 60.0,
    ) -> None:
        self._cmd = shlex.split(cmd) if isinstance(cmd, str) else list(cmd)
        self._template = template if template is not None else self.DEFAULT_TEMPLATE
        self._base_url = base_url
        self._timeout_s = timeout_s

    def _subst(self, arg: str, *, html_path: Path, out_path: Path, page: PageSpec) -> str:
        return arg.format(
            input=str(html_path),
            output=str(out_path),
            width=page.width_css(),
            height=page.height_css(),
            margin_top=f"{page.margin_top_in}in",
            margin_right=f"{page.margin_right_in}in",
            margin_bottom=f"{page.margin_bottom_in}in",
            margin_left=f"{page.margin_left_in}in",
            base_url=self._base_url or "",
        )

    def render_pdf(self, html_path: Path, page: PageSpec) -> bytes:
        with tempfile.TemporaryDirectory() as td:
            out_path = Path(td) / "out.pdf"
            args = list(self._cmd) + [
                self._subst(a, html_path=html_path, out_path=out_path, page=page)
                for a in self._template
            ]
            if self._base_url and "{base_url}" not in "".join(self._template):
                args += ["--base-url", self._base_url]
            subprocess.run(
                args,
                check=True,
                timeout=self._timeout_s,
                capture_output=True,
            )
            if not out_path.exists():
                raise RuntimeError(f"engine produced no output PDF: {' '.join(args)}")
            return out_path.read_bytes()
