"""Enumerate WPT print-reftests by scanning the sparse checkout.

Rather than parsing WPT's generated ``MANIFEST.json`` (which requires the wpt tooling),
this module scans the sparse checkout directly for print-reftests and parses the test
HTML head for the reftest metadata that drives comparison.

A print-reftest (research brief section 1) is a reftest rendered *paginated* and
compared page-by-page. We identify candidates as HTML files that either:

* end in ``-print.html`` (the WPT ``-print`` suffix), or
* live under a ``print/`` directory, or
* declare a ``<link rel="match">`` / ``<link rel="mismatch">`` whose target exists.

For each test we parse:

* ``<link rel="match">`` / ``<link rel="mismatch">`` references (relative paths
  resolved against the test file, absolute ``/...`` paths resolved against the WPT
  root).
* ``<meta name="fuzzy">`` -- allowed per-channel ``maxDifference`` and
  ``totalPixels`` ranges, both ``a-b`` and bare ``n`` forms, with an optional
  per-reference ``ref.html:...`` prefix (WPT reftest fuzzy syntax, brief section 2).
* ``<meta name="reftest-pages">`` -- which pages to compare (list/ranges like
  ``-2,4,6-``).

Tests whose source contains ``<script`` are excluded (they need a JS runtime; WPT
layout reftests are overwhelmingly script-free -- brief section 5, item 2).

References chain: a reference file may itself carry a ``rel=match``/``rel=mismatch``
link. We follow one level (``TestCase.refs`` holds the immediate refs; each ``Ref``
carries its own resolved chained ref if present).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class FuzzyRange:
    """A closed inclusive integer range ``[lo, hi]`` (``lo == hi`` for bare forms)."""

    lo: int
    hi: int

    def contains(self, value: int) -> bool:
        return self.lo <= value <= self.hi


@dataclass(frozen=True)
class Fuzzy:
    """Fuzzy tolerance: allowed per-channel max difference and differing-pixel count.

    ``max_difference`` bounds the maximum per-channel absolute delta of any single
    differing pixel; ``total_pixels`` bounds how many pixels may differ at all.
    """

    max_difference: FuzzyRange
    total_pixels: FuzzyRange


@dataclass
class Ref:
    """A reference link on a test (or on another reference, for chaining)."""

    path: Path
    # "==" for rel=match, "!=" for rel=mismatch.
    relation: str
    # Chained reference (one level), if the ref file itself links a ref.
    chained: "Ref | None" = None


@dataclass
class TestCase:
    """A single print-reftest.

    ``path``: absolute path to the test HTML.
    ``refs``: immediate references (usually one).
    ``fuzzy``: global fuzzy (keyed under ``None``) and per-ref fuzzy (keyed by the
        referenced file's basename) -- see :func:`parse_fuzzy`.
    ``pages``: parsed ``reftest-pages`` selection, or ``None`` for "all pages".
    ``mismatch``: True if the primary relation is ``rel=mismatch``.
    """

    path: Path
    refs: list[Ref]
    fuzzy: dict[str | None, Fuzzy] = field(default_factory=dict)
    pages: "list[int] | None" = None
    mismatch: bool = False

    @property
    def id(self) -> str:
        """Stable test id: path relative to the WPT root, POSIX-style."""
        return self._rel

    _rel: str = ""


# ---------------------------------------------------------------------------
# HTML head parsing (regex-based; reftests are static, well-formed HTML)
# ---------------------------------------------------------------------------

_LINK_RE = re.compile(
    r"""<link\b[^>]*?\brel\s*=\s*["']?(match|mismatch)["']?[^>]*?>""",
    re.IGNORECASE | re.DOTALL,
)
_HREF_RE = re.compile(r"""\bhref\s*=\s*["']([^"']+)["']""", re.IGNORECASE)
_META_RE = re.compile(
    r"""<meta\b[^>]*?\bname\s*=\s*["']?(fuzzy|reftest-pages)["']?[^>]*?>""",
    re.IGNORECASE | re.DOTALL,
)
_CONTENT_RE = re.compile(r"""\bcontent\s*=\s*["']([^"']*)["']""", re.IGNORECASE)
_SCRIPT_RE = re.compile(r"<script\b", re.IGNORECASE)


def _read_head(path: Path) -> str:
    """Read enough of the file to cover the head; reftests keep metadata early."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""
    lower = text.lower()
    end = lower.find("</head>")
    if end != -1:
        return text[: end + len("</head>")]
    # No head close found; the body may still carry links -- return whole file capped.
    return text[:65536]


def has_script(path: Path) -> bool:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    return _SCRIPT_RE.search(text) is not None


# ---------------------------------------------------------------------------
# Fuzzy parsing
# ---------------------------------------------------------------------------


def _parse_range(token: str) -> FuzzyRange:
    """Parse ``"a-b"`` or bare ``"n"`` into a :class:`FuzzyRange`."""
    token = token.strip()
    if "-" in token:
        lo_s, hi_s = token.split("-", 1)
        return FuzzyRange(int(lo_s.strip()), int(hi_s.strip()))
    n = int(token)
    return FuzzyRange(n, n)


def _parse_fuzzy_value(value: str) -> Fuzzy:
    """Parse the value part ``maxDifference=a-b;totalPixels=c-d`` (keys optional)."""
    max_diff: FuzzyRange | None = None
    total: FuzzyRange | None = None

    parts = [p for p in value.split(";") if p.strip()]
    for i, part in enumerate(parts):
        part = part.strip()
        if "=" in part:
            key, _, rng = part.partition("=")
            key = key.strip().lower()
            rng_val = _parse_range(rng)
            if key in ("maxdifference", "max-difference"):
                max_diff = rng_val
            elif key in ("totalpixels", "total-pixels"):
                total = rng_val
        else:
            # Positional: first is maxDifference, second is totalPixels.
            if i == 0:
                max_diff = _parse_range(part)
            else:
                total = _parse_range(part)

    if max_diff is None:
        max_diff = FuzzyRange(0, 0)
    if total is None:
        total = FuzzyRange(0, 0)
    return Fuzzy(max_diff, total)


def parse_fuzzy(content: str) -> dict[str | None, Fuzzy]:
    """Parse a ``<meta name=fuzzy>`` content string.

    Supports an optional per-reference prefix, e.g.::

        ref.html:maxDifference=1-5;totalPixels=200-300
        maxDifference=15;totalPixels=300

    Multiple comma-separated entries are allowed. The global (unprefixed) entry is
    keyed under ``None``; per-ref entries are keyed by the referenced basename.
    """
    result: dict[str | None, Fuzzy] = {}
    if not content:
        return result

    for entry in content.split(","):
        entry = entry.strip()
        if not entry:
            continue
        key: str | None = None
        value = entry
        # A per-ref prefix looks like "name.html:...". Distinguish from a bare
        # "maxDifference=..." by requiring the pre-colon token to not contain '='.
        if ":" in entry:
            head, _, tail = entry.partition(":")
            if "=" not in head and (";" not in head):
                key = head.strip()
                value = tail
        result[key] = _parse_fuzzy_value(value)
    return result


# ---------------------------------------------------------------------------
# reftest-pages parsing
# ---------------------------------------------------------------------------


def parse_reftest_pages(content: str, *, max_pages: int = 512) -> list[int] | None:
    """Parse a ``reftest-pages`` selection into a sorted list of 1-based page numbers.

    Supports comma-separated single pages and ranges: ``"2"``, ``"1,3,5"``,
    ``"-2,4,6-"`` where a leading ``-N`` means "pages 1..N" and a trailing ``N-``
    means "pages N..end". ``max_pages`` bounds open-ended tails. Returns ``None`` for
    empty content.
    """
    content = content.strip()
    if not content:
        return None

    pages: set[int] = set()
    for tok in content.split(","):
        tok = tok.strip()
        if not tok:
            continue
        if tok == "-":
            continue
        if tok.startswith("-"):
            # -N  => 1..N
            hi = int(tok[1:])
            pages.update(range(1, hi + 1))
        elif tok.endswith("-"):
            # N-  => N..max
            lo = int(tok[:-1])
            pages.update(range(lo, max_pages + 1))
        elif "-" in tok:
            lo_s, hi_s = tok.split("-", 1)
            pages.update(range(int(lo_s), int(hi_s) + 1))
        else:
            pages.add(int(tok))
    return sorted(pages)


# ---------------------------------------------------------------------------
# Reference resolution
# ---------------------------------------------------------------------------


def resolve_href(href: str, base_file: Path, wpt_root: Path) -> Path:
    """Resolve an href relative to the test file or (for ``/...``) the WPT root."""
    href = href.split("#", 1)[0].split("?", 1)[0]
    if href.startswith("/"):
        return (wpt_root / href.lstrip("/")).resolve()
    return (base_file.parent / href).resolve()


def _extract_refs(path: Path, wpt_root: Path) -> list[tuple[str, Path]]:
    """Return ``(relation, resolved_path)`` for each rel=match/mismatch link."""
    head = _read_head(path)
    out: list[tuple[str, Path]] = []
    for m in _LINK_RE.finditer(head):
        relation = "!=" if m.group(1).lower() == "mismatch" else "=="
        href_m = _HREF_RE.search(m.group(0))
        if not href_m:
            continue
        out.append((relation, resolve_href(href_m.group(1), path, wpt_root)))
    return out


def _build_ref(relation: str, ref_path: Path, wpt_root: Path, depth: int) -> Ref:
    """Build a :class:`Ref`, following one level of chaining."""
    ref = Ref(path=ref_path, relation=relation)
    if depth > 0 and ref_path.exists():
        chained = _extract_refs(ref_path, wpt_root)
        if chained:
            crel, cpath = chained[0]
            ref.chained = _build_ref(crel, cpath, wpt_root, depth - 1)
    return ref


# ---------------------------------------------------------------------------
# Test parsing / enumeration
# ---------------------------------------------------------------------------


def _is_reference_file(path: Path) -> bool:
    """Heuristic: reference files are named ``*-ref.html`` / ``*-notref.html`` etc."""
    stem = path.stem.lower()
    return stem.endswith("-ref") or stem.endswith("-notref") or "-ref-" in stem


def parse_test(path: Path, wpt_root: Path) -> TestCase | None:
    """Parse a single HTML file into a :class:`TestCase`, or ``None`` if not a test.

    Returns ``None`` for files with no reference link or containing ``<script>``.
    """
    if has_script(path):
        return None

    raw_refs = _extract_refs(path, wpt_root)
    if not raw_refs:
        return None

    refs = [_build_ref(rel, rp, wpt_root, depth=1) for rel, rp in raw_refs]
    mismatch = refs[0].relation == "!="

    head = _read_head(path)
    fuzzy: dict[str | None, Fuzzy] = {}
    pages: list[int] | None = None
    for m in _META_RE.finditer(head):
        name = m.group(1).lower()
        content_m = _CONTENT_RE.search(m.group(0))
        content = content_m.group(1) if content_m else ""
        if name == "fuzzy":
            fuzzy.update(parse_fuzzy(content))
        elif name == "reftest-pages":
            pages = parse_reftest_pages(content)

    rel = path.resolve().relative_to(wpt_root.resolve()).as_posix()
    tc = TestCase(path=path.resolve(), refs=refs, fuzzy=fuzzy, pages=pages, mismatch=mismatch)
    tc._rel = rel
    return tc


def _looks_like_print_test(path: Path) -> bool:
    """True for the ``-print`` suffix or a ``print/`` directory location."""
    if path.stem.endswith("-print"):
        return True
    return any(part == "print" for part in path.parts)


def enumerate_tests(
    wpt_root: Path,
    *,
    dirs: "tuple[str, ...] | None" = None,
    print_only: bool = True,
) -> list[TestCase]:
    """Scan the checkout for print-reftests.

    ``dirs``: subset of test directories (defaults to the three paged-media dirs).
    ``print_only``: if True, only include tests classified as print-reftests
        (``-print`` suffix or ``print/`` directory). If False, include any reftest
        with a resolvable reference (used for broader runs).
    """
    from .wpt_fetch import TEST_PATHS

    dirs = dirs or TEST_PATHS
    tests: list[TestCase] = []
    seen: set[Path] = set()

    for d in dirs:
        base = wpt_root / d
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.html")):
            if path in seen:
                continue
            if _is_reference_file(path):
                continue
            if print_only and not _looks_like_print_test(path):
                continue
            tc = parse_test(path, wpt_root)
            if tc is None:
                continue
            # A referenced target must exist for at least one ref.
            if not any(r.path.exists() for r in tc.refs):
                continue
            seen.add(path)
            tests.append(tc)

    return tests
