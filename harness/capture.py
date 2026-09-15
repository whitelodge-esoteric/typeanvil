"""Capture layer: render a selected test set and record per-document evidence.

A :class:`Capture` is the durable evidence unit for the release gate. It records,
for every unique document in the selected set (test documents, immediate
references, and chained references), the document's content hash, page count,
each page's size in PDF points, and a per-page fingerprint of the rasterized
page at the capture DPI. It also records the WPT pair status per test (the
existing conformance measure) and an identity block naming the engine binary,
source commit, WPT revision, page geometry, capture DPI, rasterizer, and font
identity.

Two captures with matching document identities can then be compared per
document (``harness.release_gate``), catching a change that moves both the test
and its reference the same way -- the gap the pair score alone cannot see.

pypdfium2 is imported lazily so ``fetch``/``score``/``history`` stay importable
without it installed.
"""

from __future__ import annotations

import hashlib
import json
import shlex
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

from PIL import Image

from .rasterize import DEFAULT_DPI

CAPTURE_SCHEMA = "typeanvil.harness.capture/1"

_BASE_DPI = 72.0  # PDF user-space unit is 1/72 inch.


class CaptureError(Exception):
    """A capture file could not be loaded or validated."""


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class PageCapture:
    index: int
    size_pt: tuple[float, float]  # PDF page size in points (1/72 inch)
    fingerprint: str  # sha256 hex of the rasterized RGB page
    # Base64 PNG of a downscaled greyscale page, for review only. The gate
    # composes baseline|candidate review images from these; they are never
    # compared, and an empty string means "not recorded".
    thumbnail: str = ""


@dataclass(frozen=True)
class DocumentCapture:
    doc_id: str  # WPT-root-relative POSIX path
    role: tuple[str, ...]  # "test" and/or "reference"
    content_sha256: str  # sha256 hex of the document bytes
    page_count: int
    pages: tuple[PageCapture, ...]
    render_failed: bool = False
    message: str = ""


@dataclass(frozen=True)
class CaptureIdentity:
    engine_kind: str  # "cli" | "chromium"
    cli_cmd: str | None
    source_commit: str  # "unknown" when undeterminable
    binary: dict  # {"path", "sha256", "version"}
    wpt_revision: str  # "unknown" when .wpt is not a git checkout
    page_spec: dict  # the PageSpec fields as a dict
    dpi: int
    rasterizer: str  # e.g. "pypdfium2 4.30.0"
    fonts: dict  # {"identity": ..., "source": ...}


@dataclass(frozen=True)
class Capture:
    label: str
    complete: bool
    identity: CaptureIdentity
    selection: tuple[str, ...]  # test ids, sorted
    documents: tuple[DocumentCapture, ...]
    results: tuple[dict, ...]  # [{"id": ..., "status": ...}], WPT statuses


# ---------------------------------------------------------------------------
# Fingerprinting (single source of truth for capture and comparison)
# ---------------------------------------------------------------------------


def _fingerprint_image(img: Image.Image) -> str:
    """sha256 of the raw RGB pixel bytes plus width, height, and mode.

    Width/height/mode are folded in so a page-size change (which changes the
    raster dimensions at a fixed DPI) produces a different fingerprint even when
    the underlying bytes would otherwise collide.
    """
    header = f"{img.width}x{img.height}x{img.mode}".encode("ascii")
    return hashlib.sha256(header + b"\0" + img.tobytes()).hexdigest()


# ---------------------------------------------------------------------------
# PDF capture
# ---------------------------------------------------------------------------


#: Review thumbnails are downscaled to this width in pixels.
THUMBNAIL_WIDTH = 160


def _thumbnail_png(img: Image.Image, *, width: int = THUMBNAIL_WIDTH) -> str:
    """Base64 PNG of a downscaled greyscale copy of ``img``, for review only."""
    import base64
    import io

    grey = img.convert("L")
    if grey.width > width:
        height = max(1, round(grey.height * width / grey.width))
        grey = grey.resize((width, height), Image.Resampling.LANCZOS)
    buf = io.BytesIO()
    grey.save(buf, format="PNG", optimize=True)
    return base64.b64encode(buf.getvalue()).decode("ascii")


def capture_pdf(
    pdf: bytes, *, dpi: int, thumbnails: bool = True
) -> tuple[int, list[PageCapture]]:
    """Open ``pdf`` once, recording page count, per-page size, and fingerprints.

    Renders each page at ``dpi`` (scale = dpi / 72) and fingerprints the RGB
    pixel bytes. Returns ``(page_count, pages)``. When ``thumbnails`` is set, each
    page also carries a base64 PNG review thumbnail.
    """
    import pypdfium2 as pdfium

    scale = dpi / _BASE_DPI
    doc = pdfium.PdfDocument(pdf)
    pages: list[PageCapture] = []
    try:
        for index, page in enumerate(doc):
            width, height = page.get_size()
            bitmap = page.render(scale=scale)
            try:
                img = bitmap.to_pil().convert("RGB")
            finally:
                bitmap.close()
            pages.append(
                PageCapture(
                    index=index,
                    size_pt=(float(width), float(height)),
                    fingerprint=_fingerprint_image(img),
                    thumbnail=_thumbnail_png(img) if thumbnails else "",
                )
            )
            page.close()
    finally:
        doc.close()
    return len(pages), pages


# ---------------------------------------------------------------------------
# Document enumeration
# ---------------------------------------------------------------------------


def _iter_documents(tests: list) -> list[tuple[Path, str]]:
    """Yield ``(path, role)`` for the test document and every reference document."""
    out: list[tuple[Path, str]] = []
    for tc in tests:
        out.append((tc.path, "test"))
        for ref in tc.refs:
            out.append((ref.path, "reference"))
            chained = ref.chained
            while chained is not None:
                out.append((chained.path, "reference"))
                chained = chained.chained
    return out


def documents_for(tests: list, wpt_root: Path) -> list[tuple[str, set[str]]]:
    """Return each unique document once, ordered by ``doc_id``, with its roles.

    A document shared by several tests (or referenced by several tests) is
    reported once. The returned ``doc_id`` is the WPT-root-relative POSIX path;
    the role set carries ``"test"`` and/or ``"reference"``.
    """
    root = wpt_root.resolve()
    docs: dict[str, set[str]] = {}
    for path, role in _iter_documents(tests):
        doc_id = path.resolve().relative_to(root).as_posix()
        docs.setdefault(doc_id, set()).add(role)
    return [(doc_id, docs[doc_id]) for doc_id in sorted(docs)]


# ---------------------------------------------------------------------------
# Capture construction
# ---------------------------------------------------------------------------


def repo_root_default() -> Path:
    """The repository root that owns this package.

    Derived from the module location rather than the working directory, so a
    capture records the right source commit no matter where it is invoked from.
    """
    return Path(__file__).resolve().parents[1]


def build_capture(
    *,
    tests,
    wpt_root,
    engine_cfg,
    spec,
    label,
    dpi=DEFAULT_DPI,
    repo_root: Path | None = None,
    source_commit_override: str | None = None,
) -> Capture:
    """Render the selected set once and record the per-document evidence.

    Each unique document is rendered once, sequentially (determinism matters more
    than speed here). A document whose render raises is recorded with
    ``render_failed=True`` and the capture is marked ``complete=False``. The WPT
    pair status per test is recorded separately via :func:`harness.runner.run_one`.

    ``repo_root`` names the checkout whose commit identifies the capture; it
    defaults to the repository that contains this package, never the working
    directory. ``source_commit_override`` supplies the commit directly, which is
    required inside the dev container: a linked worktree's ``.git`` is a pointer
    file to a host path, so ``git rev-parse`` cannot resolve there.
    """
    from .runner import run_one

    root = wpt_root.resolve()
    engine = engine_cfg.build()
    complete = True
    documents: list[DocumentCapture] = []
    try:
        for doc_id, roles in documents_for(tests, wpt_root):
            path = root / doc_id
            try:
                content_sha256 = hashlib.sha256(path.read_bytes()).hexdigest()
            except OSError as exc:
                content_sha256 = "unknown"
            try:
                pdf = engine.render_pdf(path, spec)
                page_count, pages = capture_pdf(pdf, dpi=dpi)
                documents.append(
                    DocumentCapture(
                        doc_id=doc_id,
                        role=tuple(sorted(roles)),
                        content_sha256=content_sha256,
                        page_count=page_count,
                        pages=tuple(pages),
                    )
                )
            except Exception as exc:  # noqa: BLE001 -- a render failure is evidence, not a crash
                complete = False
                documents.append(
                    DocumentCapture(
                        doc_id=doc_id,
                        role=tuple(sorted(roles)),
                        content_sha256=content_sha256,
                        page_count=0,
                        pages=(),
                        render_failed=True,
                        message=f"{type(exc).__name__}: {exc}",
                    )
                )

        results = []
        for tc in tests:
            res = run_one(engine, tc, spec, None)
            results.append({"id": tc.id, "status": res.status})
    finally:
        close = getattr(engine, "close", None)
        if close:
            close()

    identity = CaptureIdentity(
        engine_kind=engine_cfg.kind,
        cli_cmd=engine_cfg.cli_cmd,
        source_commit=(
            source_commit_override
            or source_commit(Path(repo_root) if repo_root else repo_root_default())
        ),
        binary=binary_identity(engine_cfg.cli_cmd),
        wpt_revision=wpt_revision(wpt_root),
        page_spec=_page_spec_dict(spec),
        dpi=dpi,
        rasterizer=rasterizer_identity(),
        fonts={"identity": "unknown", "source": "unavailable"},
    )
    return Capture(
        label=label,
        complete=complete,
        identity=identity,
        selection=tuple(sorted(tc.id for tc in tests)),
        documents=tuple(documents),
        results=tuple(results),
    )


# ---------------------------------------------------------------------------
# Identity helpers
# ---------------------------------------------------------------------------


def _page_spec_dict(spec) -> dict:
    return {
        "width_in": spec.width_in,
        "height_in": spec.height_in,
        "margin_top_in": spec.margin_top_in,
        "margin_right_in": spec.margin_right_in,
        "margin_bottom_in": spec.margin_bottom_in,
        "margin_left_in": spec.margin_left_in,
    }


def binary_identity(cli_cmd: str | None) -> dict:
    """Identify the engine binary by resolved path, sha256, and ``--version``.

    ``cli_cmd`` is the full command prefix (e.g. ``engine/target/debug/typeanvil
    render``); its first token names the binary. A ``None`` or empty command
    yields the all-``"unknown"`` dict.
    """
    unknown = {"path": "unknown", "sha256": "unknown", "version": "unknown"}
    if cli_cmd is None:
        return dict(unknown)
    argv = shlex.split(cli_cmd)
    if not argv:
        return dict(unknown)

    name = argv[0]
    resolved = shutil.which(name)
    if resolved is not None:
        path = str(Path(resolved).resolve())
    else:
        candidate = Path(name).expanduser()
        path = str(candidate.resolve()) if candidate.exists() else name

    sha256 = "unknown"
    try:
        sha256 = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    except OSError:
        pass

    version = "unknown"
    try:
        proc = subprocess.run(
            [path, "--version"], capture_output=True, text=True, timeout=5.0
        )
        out = (proc.stdout or proc.stderr or "").strip()
        if out:
            version = out.splitlines()[0]
    except Exception:  # noqa: BLE001 -- a failed probe yields "unknown"
        pass

    return {"path": path, "sha256": sha256, "version": version}


def _git_head(repo_root: Path) -> str:
    try:
        proc = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=10.0,
        )
        head = (proc.stdout or "").strip()
        if proc.returncode == 0 and head:
            return head
    except Exception:  # noqa: BLE001
        pass
    return "unknown"


def source_commit(repo_root: Path) -> str:
    """Return ``git rev-parse HEAD`` in ``repo_root``, or ``"unknown"``."""
    return _git_head(repo_root)


def wpt_revision(wpt_root: Path) -> str:
    """Return the WPT checkout revision, or ``"unknown"`` when not a git repo."""
    return _git_head(wpt_root)


def rasterizer_identity() -> str:
    """Return the rasterizer name and version, e.g. ``"pypdfium2 4.30.0"``."""
    import importlib.metadata

    try:
        version = importlib.metadata.version("pypdfium2")
    except importlib.metadata.PackageNotFoundError:
        version = "unknown"
    return f"pypdfium2 {version}"


# ---------------------------------------------------------------------------
# Serialization
# ---------------------------------------------------------------------------


def _identity_to_dict(identity: CaptureIdentity) -> dict:
    return {
        "engine_kind": identity.engine_kind,
        "cli_cmd": identity.cli_cmd,
        "source_commit": identity.source_commit,
        "binary": dict(identity.binary),
        "wpt_revision": identity.wpt_revision,
        "page_spec": dict(identity.page_spec),
        "dpi": identity.dpi,
        "rasterizer": identity.rasterizer,
        "fonts": dict(identity.fonts),
    }


def _document_to_dict(doc: DocumentCapture) -> dict:
    return {
        "doc_id": doc.doc_id,
        "role": list(doc.role),
        "content_sha256": doc.content_sha256,
        "page_count": doc.page_count,
        "pages": [
            {
                "index": p.index,
                "size_pt": list(p.size_pt),
                "fingerprint": p.fingerprint,
                "thumbnail": p.thumbnail,
            }
            for p in doc.pages
        ],
        "render_failed": doc.render_failed,
        "message": doc.message,
    }


def capture_to_dict(capture: Capture) -> dict:
    return {
        "schema": CAPTURE_SCHEMA,
        "label": capture.label,
        "complete": capture.complete,
        "identity": _identity_to_dict(capture.identity),
        "selection": list(capture.selection),
        "documents": [_document_to_dict(d) for d in capture.documents],
        "results": [dict(r) for r in capture.results],
    }


def write_capture(path: Path, capture: Capture) -> Path:
    """Write ``capture`` to ``path`` as JSON and return ``path``."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(capture_to_dict(capture), indent=2))
    return path


def load_capture(path: Path) -> Capture:
    """Load and validate a capture file; raise :class:`CaptureError` on error."""
    path = Path(path)
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise CaptureError(f"cannot load capture {path}: {exc}") from exc

    if not isinstance(data, dict):
        raise CaptureError(f"malformed capture {path}: not a JSON object")
    if data.get("schema") != CAPTURE_SCHEMA:
        raise CaptureError(
            f"unsupported capture schema {data.get('schema')!r} (expected {CAPTURE_SCHEMA!r})"
        )

    try:
        identity = data["identity"]
        identity_obj = CaptureIdentity(
            engine_kind=identity["engine_kind"],
            cli_cmd=identity.get("cli_cmd"),
            source_commit=identity["source_commit"],
            binary=dict(identity["binary"]),
            wpt_revision=identity["wpt_revision"],
            page_spec=dict(identity["page_spec"]),
            dpi=int(identity["dpi"]),
            rasterizer=identity["rasterizer"],
            fonts=dict(identity["fonts"]),
        )
        documents = tuple(
            DocumentCapture(
                doc_id=d["doc_id"],
                role=tuple(d["role"]),
                content_sha256=d["content_sha256"],
                page_count=int(d["page_count"]),
                pages=tuple(
                    PageCapture(
                        index=int(p["index"]),
                        size_pt=(float(p["size_pt"][0]), float(p["size_pt"][1])),
                        fingerprint=p["fingerprint"],
                        thumbnail=p.get("thumbnail", ""),
                    )
                    for p in d["pages"]
                ),
                render_failed=bool(d.get("render_failed", False)),
                message=d.get("message", ""),
            )
            for d in data["documents"]
        )
        return Capture(
            label=data["label"],
            complete=bool(data["complete"]),
            identity=identity_obj,
            selection=tuple(data["selection"]),
            documents=documents,
            results=tuple(dict(r) for r in data["results"]),
        )
    except (KeyError, TypeError, ValueError, IndexError) as exc:
        raise CaptureError(f"malformed capture {path}: {exc}") from exc
