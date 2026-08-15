"""Sparse checkout of the Web Platform Tests repository into ``.wpt/``.

The harness does not vendor the WPT repo (it is enormous). Instead we perform a
depth-1, sparse-checkout clone limited to the paged-media test directories the harness
cares about, plus the repo-root support files (fonts, shared CSS) those tests reference
via absolute paths like ``/fonts/ahem.css`` (see research brief section 5, item 3:
"honor WPT's /fonts/ahem.css -- install Ahem").

The operation is idempotent: if ``.wpt/`` already contains a checkout it is left
untouched unless ``force`` is given.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

WPT_REMOTE = "https://github.com/web-platform-tests/wpt"

# Test directories we enumerate print-reftests from.
TEST_PATHS: tuple[str, ...] = (
    "css/css-page",
    "css/css-break",
    "css/css-multicol",
)

# Repo-root support paths those tests reference by absolute URL (fonts, shared CSS,
# images, common helpers). Cone-mode sparse-checkout resolves these at directory
# granularity, so e.g. ``fonts`` brings in ``/fonts/ahem.css`` and ``Ahem.ttf`` (the
# metrics-exact test font virtually all layout reftests depend on -- brief section 5,
# item 3). ``resources`` supplies ``/resources/testharness*.js`` referenced by the
# non-reftest files that live alongside the reftests (harmless to include).
SUPPORT_PATHS: tuple[str, ...] = (
    "fonts",
    "css/support",
    "images",
    "common",
    "resources",
)

SPARSE_PATHS: tuple[str, ...] = TEST_PATHS + SUPPORT_PATHS


def default_wpt_dir(root: Path | None = None) -> Path:
    """Return the canonical ``.wpt`` directory relative to the repo root."""
    base = root if root is not None else Path.cwd()
    return base / ".wpt"


def is_checked_out(wpt_dir: Path) -> bool:
    """Return True if ``wpt_dir`` looks like a populated WPT sparse checkout."""
    if not (wpt_dir / ".git").exists():
        return False
    # At least one of the test directories must have materialised.
    return any((wpt_dir / p).is_dir() for p in TEST_PATHS)


def _run(args: list[str], cwd: Path | None = None) -> None:
    subprocess.run(args, cwd=cwd, check=True)


def fetch(wpt_dir: Path | None = None, *, force: bool = False) -> Path:
    """Sparse-clone the WPT paged-media directories into ``wpt_dir``.

    Idempotent: returns immediately if a checkout already exists (unless ``force``).
    """
    wpt_dir = wpt_dir or default_wpt_dir()

    if is_checked_out(wpt_dir) and not force:
        return wpt_dir

    wpt_dir.mkdir(parents=True, exist_ok=True)

    if not (wpt_dir / ".git").exists():
        # Bare-ish init: clone with no checkout, blobless, depth 1.
        _run(
            [
                "git",
                "clone",
                "--depth",
                "1",
                "--filter=blob:none",
                "--sparse",
                "--no-checkout",
                WPT_REMOTE,
                str(wpt_dir),
            ]
        )

    # Cone-mode sparse checkout of just our paths.
    _run(["git", "-C", str(wpt_dir), "sparse-checkout", "init", "--cone"])
    _run(["git", "-C", str(wpt_dir), "sparse-checkout", "set", *SPARSE_PATHS])
    _run(["git", "-C", str(wpt_dir), "checkout"])

    return wpt_dir
