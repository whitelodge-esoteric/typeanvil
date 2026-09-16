"""Release gate: compare two captures, apply dispositions, assert direct evidence.

The gate has three evidence layers:

1. **WPT pair score** -- recorded per test in each capture; the existing rule.
2. **Per-document output comparison** -- each document's page count, page size,
   and per-page fingerprints are compared against its own earlier output.
3. **Direct PDF assertions** -- the reviewed direct-check manifest, run against
   the emitted PDFs.

The gate passes only when all three layers pass and every output change carries a
recorded disposition bound to the exact baseline and candidate fingerprints.

Environment identity (page geometry, capture DPI, engine kind, WPT revision,
rasterizer identity, font identity) must match between baseline and candidate;
source commit, binary, and label are expected to differ and are recorded but
never treated as an incompatibility.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path

from .capture import CAPTURE_SCHEMA, Capture, CaptureError, load_capture
from .direct import CheckResult, DirectError, load_manifest, run_manifest

REVIEW_SCHEMA = "typeanvil.harness.reviews/1"


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Change:
    doc_id: str
    property: str  # "page_count" | "page_size" | "rendered_image"
    baseline: str
    candidate: str
    change_id: str


@dataclass(frozen=True)
class Disposition:
    doc_id: str
    change_id: str
    kind: str  # "correction" | "regression" | "variation"
    reason: str
    provenance: str


@dataclass(frozen=True)
class Condition:
    name: str  # stable slug, e.g. "missing_baseline"
    detail: str


@dataclass
class GateVerdict:
    ok: bool
    conditions: list[Condition]
    changes: list[Change]
    direct: list[CheckResult]
    report_path: Path | None


# ---------------------------------------------------------------------------
# Change computation
# ---------------------------------------------------------------------------


def change_id(doc_id: str, baseline, candidate) -> str:
    """Hash the document id and both sides' page fingerprints.

    ``baseline`` and ``candidate`` are :class:`DocumentCapture` objects. Any
    change to either side's output (page count, page size, or pixels) changes the
    fingerprint sequence and therefore the id.
    """
    payload = {
        "doc_id": doc_id,
        "baseline": [p.fingerprint for p in baseline.pages],
        "candidate": [p.fingerprint for p in candidate.pages],
    }
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def _page_sizes(doc) -> tuple[tuple[float, float], ...]:
    return tuple((p.size_pt[0], p.size_pt[1]) for p in doc.pages)


def _fingerprints(doc) -> tuple[str, ...]:
    return tuple(p.fingerprint for p in doc.pages)


def _fmt_sizes(doc) -> str:
    return json.dumps([list(p.size_pt) for p in doc.pages])


def _fmt_fingerprints(doc) -> str:
    return json.dumps([p.fingerprint for p in doc.pages])


def compare(baseline: Capture, candidate: Capture) -> list[Change]:
    """Report one :class:`Change` per changed property per document.

    Page fingerprint differences produce a ``rendered_image`` change; a page-count
    change produces a ``page_count`` change (and, since the per-page size list
    differs in length, a ``page_size`` change). Documents present on only one side
    are a coverage problem, handled by :func:`conditions`, not by this function.
    """
    bdocs = {d.doc_id: d for d in baseline.documents}
    cdocs = {d.doc_id: d for d in candidate.documents}
    changes: list[Change] = []
    for doc_id in sorted(set(bdocs) & set(cdocs)):
        b = bdocs[doc_id]
        c = cdocs[doc_id]
        cid = change_id(doc_id, b, c)
        if b.page_count != c.page_count:
            changes.append(
                Change(doc_id, "page_count", str(b.page_count), str(c.page_count), cid)
            )
        if _page_sizes(b) != _page_sizes(c):
            changes.append(Change(doc_id, "page_size", _fmt_sizes(b), _fmt_sizes(c), cid))
        if _fingerprints(b) != _fingerprints(c):
            changes.append(
                Change(doc_id, "rendered_image", _fmt_fingerprints(b), _fmt_fingerprints(c), cid)
            )
    return changes


# ---------------------------------------------------------------------------
# Policy / reviews loading
# ---------------------------------------------------------------------------


def load_policy(path: Path | None) -> dict:
    """Load the optional policy file; a missing file yields an empty policy."""
    if path is None:
        return {}
    p = Path(path)
    if not p.exists():
        return {}
    data = json.loads(p.read_text())
    if not isinstance(data, dict):
        raise ValueError(f"policy file {p} is not a JSON object")
    return {
        "acknowledged_unknown": list(data.get("acknowledged_unknown", [])),
        "allowed_errors": list(data.get("allowed_errors", [])),
        "allowed_skips": list(data.get("allowed_skips", [])),
        "allowed_render_failures": list(data.get("allowed_render_failures", [])),
    }


def load_reviews(path: Path | None) -> list[Disposition]:
    """Load review dispositions.

    ``path`` is a review record file or a directory of them — the documented
    layout is ``gate/reviews/<candidate-label>.json``, so a directory is the
    normal case. A missing path or an empty directory yields an empty list, and
    unreviewed changes then stay unreviewed.
    """
    if path is None:
        return []
    p = Path(path)
    if not p.exists():
        return []
    files = sorted(p.glob("*.json")) if p.is_dir() else [p]
    out: list[Disposition] = []
    for f in files:
        out.extend(_load_review_file(f))
    return out


def _load_review_file(p: Path) -> list[Disposition]:
    data = json.loads(p.read_text())
    if not isinstance(data, dict):
        raise ValueError(f"review file {p} is not a JSON object")
    if data.get("schema") != REVIEW_SCHEMA:
        raise ValueError(
            f"unsupported review schema {data.get('schema')!r} (expected {REVIEW_SCHEMA!r})"
        )
    out: list[Disposition] = []
    for d in data.get("dispositions", []):
        out.append(
            Disposition(
                doc_id=d["doc_id"],
                change_id=d["change_id"],
                kind=d["kind"],
                reason=d.get("reason", ""),
                provenance=d.get("provenance", ""),
            )
        )
    return out


# ---------------------------------------------------------------------------
# Identity conditions
# ---------------------------------------------------------------------------

#: Environment identity fields compared between baseline and candidate. Source
#: commit, binary, and label are intentionally excluded: they are expected to
#: differ between a real baseline and candidate.
_COMPARED_FIELDS = (
    "engine_kind",
    "page_spec",
    "dpi",
    "wpt_revision",
    "rasterizer",
    "fonts",
)


def _identity_fields(identity) -> dict:
    fonts = identity.fonts or {}
    return {
        "engine_kind": identity.engine_kind,
        "page_spec": identity.page_spec,
        "dpi": identity.dpi,
        "wpt_revision": identity.wpt_revision,
        "rasterizer": identity.rasterizer,
        "fonts": fonts.get("identity"),
    }


def conditions(baseline: Capture, candidate: Capture, policy: dict) -> list[Condition]:
    """Return the blocking conditions derivable from baseline, candidate, policy.

    Review (unreviewed_change, regression_disposition, stale_review,
    provenance_missing) and direct-evidence (missing_direct_evidence,
    direct_check_failed) conditions are computed by :func:`evaluate`, which needs
    the review file and direct manifest those conditions depend on.
    """
    out: list[Condition] = []

    # Empty selection.
    if not baseline.selection or not candidate.selection:
        out.append(
            Condition(
                "empty_selection",
                f"baseline selection={len(baseline.selection)} "
                f"candidate selection={len(candidate.selection)}",
            )
        )

    # Duplicate document identity within a capture.
    for side, cap in (("baseline", baseline), ("candidate", candidate)):
        seen: set[str] = set()
        dups: set[str] = set()
        for d in cap.documents:
            if d.doc_id in seen:
                dups.add(d.doc_id)
            seen.add(d.doc_id)
        if dups:
            out.append(
                Condition(
                    "duplicate_document_identity",
                    f"{side}: duplicate doc_id {sorted(dups)!r}",
                )
            )

    # Coverage: selection and document sets must match.
    cov_details = []
    if set(baseline.selection) != set(candidate.selection):
        diff = sorted(set(baseline.selection) ^ set(candidate.selection))
        cov_details.append(f"selection differs: {diff}")
    bdocs = {d.doc_id for d in baseline.documents}
    cdocs = {d.doc_id for d in candidate.documents}
    if bdocs != cdocs:
        cov_details.append(f"documents differ: {sorted(bdocs ^ cdocs)}")
    if cov_details:
        out.append(Condition("coverage_mismatch", "; ".join(cov_details)))

    # Incomplete capture.
    for side, cap in (("baseline", baseline), ("candidate", candidate)):
        if not cap.complete:
            out.append(Condition("incomplete_capture", f"{side} capture is incomplete"))

    # Unapproved ERROR / SKIP results.
    allowed_errors = set(policy.get("allowed_errors", []))
    allowed_skips = set(policy.get("allowed_skips", []))
    for side, cap in (("baseline", baseline), ("candidate", candidate)):
        errors = [r["id"] for r in cap.results if r.get("status") == "ERROR"]
        unapproved = sorted(set(errors) - allowed_errors)
        if unapproved:
            out.append(
                Condition("unapproved_error", f"{side}: ERROR for {unapproved}")
            )
        skips = [r["id"] for r in cap.results if r.get("status") == "SKIP"]
        unapproved_skips = sorted(set(skips) - allowed_skips)
        if unapproved_skips:
            out.append(
                Condition("unapproved_skip", f"{side}: SKIP for {unapproved_skips}")
            )

    # Render failures not covered by policy.
    allowed_failures = set(policy.get("allowed_render_failures", []))
    for side, cap in (("baseline", baseline), ("candidate", candidate)):
        for d in cap.documents:
            if d.render_failed and d.doc_id not in allowed_failures:
                out.append(
                    Condition("render_failure", f"{side}: {d.doc_id} render failed: {d.message}")
                )

    # Environment compatibility.
    bf = _identity_fields(baseline.identity)
    cf = _identity_fields(candidate.identity)
    diffs = [name for name in _COMPARED_FIELDS if bf[name] != cf[name]]
    if diffs:
        out.append(
            Condition(
                "incompatible_environment",
                f"fields differ: {diffs} (baseline={ {f: bf[f] for f in diffs}!r} "
                f"candidate={ {f: cf[f] for f in diffs}!r})",
            )
        )

    # Unknown identity fields must be acknowledged.
    acknowledged = set(policy.get("acknowledged_unknown", []))
    for name in _COMPARED_FIELDS:
        if bf[name] == "unknown" or cf[name] == "unknown":
            if name not in acknowledged:
                out.append(
                    Condition(
                        "unknown_identity_unacknowledged",
                        f"{name} is 'unknown' on at least one side and is not listed "
                        f"under acknowledged_unknown",
                    )
                )

    return out


# ---------------------------------------------------------------------------
# Review conditions
# ---------------------------------------------------------------------------


def _review_conditions(
    changes: list[Change], dispositions: list[Disposition]
) -> list[Condition]:
    out: list[Condition] = []
    change_keys = {(c.doc_id, c.change_id) for c in changes}
    by_doc: dict[str, list[Change]] = {}
    for c in changes:
        by_doc.setdefault(c.doc_id, []).append(c)

    disp_by_key: dict[tuple[str, str], list[Disposition]] = {}
    for d in dispositions:
        disp_by_key.setdefault((d.doc_id, d.change_id), []).append(d)

    for d in dispositions:
        if (d.doc_id, d.change_id) not in change_keys:
            out.append(
                Condition(
                    "stale_review",
                    f"{d.doc_id}: change_id={d.change_id} no longer matches any change",
                )
            )

    for doc_id in sorted(by_doc):
        doc_changes = by_doc[doc_id]
        cid = doc_changes[0].change_id
        disps = disp_by_key.get((doc_id, cid), [])
        if not disps:
            props = sorted({c.property for c in doc_changes})
            out.append(
                Condition(
                    "unreviewed_change",
                    f"{doc_id}: change_id={cid} properties={props} has no disposition",
                )
            )
            continue
        for d in disps:
            if d.kind == "regression":
                out.append(
                    Condition(
                        "regression_disposition",
                        f"{doc_id}: regression disposition ({d.reason or 'no reason'})",
                    )
                )
            elif d.kind == "variation" and not (d.provenance or "").strip():
                out.append(
                    Condition(
                        "provenance_missing",
                        f"{doc_id}: variation disposition has no provenance",
                    )
                )
    return out


# ---------------------------------------------------------------------------
# Direct-evidence conditions
# ---------------------------------------------------------------------------


def _direct_conditions(
    manifest_path,
    *,
    wpt_root,
    corpus_root,
    fixture_root,
    engine_cfg,
    dpi,
    spec,
) -> tuple[list[Condition], list[CheckResult]]:
    out: list[Condition] = []
    direct: list[CheckResult] = []
    if manifest_path is None:
        out.append(Condition("missing_direct_evidence", "no direct manifest provided"))
        return out, direct
    mp = Path(manifest_path)
    if not mp.exists():
        out.append(
            Condition("missing_direct_evidence", f"direct manifest not found: {mp}")
        )
        return out, direct
    try:
        checks = load_manifest(mp)
    except DirectError as exc:
        if "empty checks" in str(exc):
            out.append(Condition("missing_direct_evidence", str(exc)))
        else:
            out.append(Condition("direct_check_failed", str(exc)))
        return out, direct
    if not checks:
        out.append(Condition("missing_direct_evidence", f"direct manifest is empty: {mp}"))
        return out, direct
    if engine_cfg is None:
        out.append(
            Condition("direct_check_failed", "direct manifest present but no engine configured")
        )
        return out, direct
    try:
        direct = run_manifest(
            mp,
            wpt_root=wpt_root,
            corpus_root=corpus_root,
            fixture_root=fixture_root,
            engine_cfg=engine_cfg,
            spec=spec,
            dpi=dpi,
        )
    except Exception as exc:  # noqa: BLE001 -- a broken engine must not crash the gate
        out.append(
            Condition(
                "direct_check_failed",
                f"direct checks could not run: {type(exc).__name__}: {exc}",
            )
        )
        return out, direct
    for r in direct:
        if not r.passed:
            out.append(Condition("direct_check_failed", r.detail))
    return out, direct


# ---------------------------------------------------------------------------
# Capture resolution
# ---------------------------------------------------------------------------


def _resolve_capture(ref, out_dir: Path) -> tuple[Capture | None, str | None]:
    """Resolve a baseline/candidate reference to a :class:`Capture`.

    Returns ``(capture, error)`` where ``error`` is ``None``, ``"missing"``, or
    ``"schema"``. Accepts a :class:`Capture`, a path, or a bare label (resolved
    against ``out_dir/../captures``).
    """
    if isinstance(ref, Capture):
        return ref, None
    if isinstance(ref, Path):
        path = ref
    elif isinstance(ref, str):
        p = Path(ref)
        if p.exists() or p.is_absolute():
            path = p
        else:
            path = out_dir.parent / "captures" / f"{ref}.json"
    else:
        raise TypeError(f"baseline/candidate must be a Capture, path, or label: {ref!r}")
    if not path.exists():
        return None, "missing"
    try:
        return load_capture(path), None
    except CaptureError as exc:
        # Carry the reason so the gate can report why the record is unusable
        # instead of a bare "unsupported schema".
        return None, f"schema: {exc}"


def _ref_label(ref) -> str:
    if isinstance(ref, Capture):
        return ref.label
    if isinstance(ref, Path):
        return ref.name
    return str(ref)


def _artifact_label(ref) -> str:
    """A safe file-name stem for a report, derived from the reference.

    A bare label is used as given. A path is reduced to its base name with any
    ``.json`` suffix removed, so ``--out`` always receives the report: using the
    raw path as a file name would make ``out_dir / "<absolute path>-gate.json"``
    an absolute path and write outside the requested directory.
    """
    name = Path(_ref_label(ref)).name
    if name.endswith(".json"):
        name = name[: -len(".json")]
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", name) or "candidate"


# ---------------------------------------------------------------------------
# Report / review artifacts
# ---------------------------------------------------------------------------


def _slug(doc_id: str, property_: str) -> str:
    s = re.sub(r"[^A-Za-z0-9_.-]+", "_", doc_id)
    return f"{s}-{property_}"


def _identity_dict(identity) -> dict | None:
    if identity is None:
        return None
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


def _write_report(
    out_dir: Path,
    candidate_label: str,
    conditions: list[Condition],
    changes: list[Change],
    direct: list[CheckResult],
    baseline_identity,
    candidate_identity,
) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "candidate_label": candidate_label,
        "conditions": [{"name": c.name, "detail": c.detail} for c in conditions],
        "changes": [
            {
                "doc_id": c.doc_id,
                "property": c.property,
                "baseline": c.baseline,
                "candidate": c.candidate,
                "change_id": c.change_id,
            }
            for c in changes
        ],
        "direct": [
            {"check_id": r.check_id, "input": r.input, "passed": r.passed, "detail": r.detail}
            for r in direct
        ],
        "baseline_identity": _identity_dict(baseline_identity),
        "candidate_identity": _identity_dict(candidate_identity),
    }
    path = out_dir / f"{candidate_label}-gate.json"
    path.write_text(json.dumps(report, indent=2))
    return path


def _decode_thumbnail(encoded: str):
    """Decode a capture thumbnail (base64 grey PNG) to a PIL image, or ``None``."""
    if not encoded:
        return None
    import base64
    import io

    from PIL import Image

    try:
        return Image.open(io.BytesIO(base64.b64decode(encoded))).convert("L")
    except Exception:  # noqa: BLE001 -- a bad thumbnail is a missing review aid
        return None


def _compose_side_by_side(baseline_img, candidate_img, *, gap: int = 4):
    """Compose ``baseline | candidate`` on a white canvas, padding the shorter one."""
    from PIL import Image

    present = [img for img in (baseline_img, candidate_img) if img is not None]
    height = max(img.height for img in present)
    width = sum(img.width for img in present) + gap * (len(present) - 1)
    canvas = Image.new("L", (width, height), 255)
    x = 0
    for img in (baseline_img, candidate_img):
        if img is None:
            continue
        canvas.paste(img, (x, 0))
        x += img.width + gap
    return canvas


def _live_page_images(doc_id: str, engine, wpt_root: Path, spec, dpi: int) -> list:
    """Render ``doc_id`` and return downscaled page images (engine fallback)."""
    from .capture import _thumbnail_png
    from .rasterize import rasterize_pdf

    html = Path(wpt_root) / doc_id
    if not html.exists():
        return []
    pdf = engine.render_pdf(html, spec)
    return [
        _decode_thumbnail(_thumbnail_png(img))
        for img in rasterize_pdf(pdf, dpi=dpi)
    ]


def _write_change_pngs(
    changes: list[Change],
    out_dir: Path,
    baseline: Capture | None,
    candidate: Capture | None,
    wpt_root: Path,
    engine_cfg,
    spec,
    dpi: int,
) -> None:
    """Write one ``baseline | candidate`` review PNG per image change.

    Pages come from the thumbnails stored in each capture, so a reviewer sees the
    baseline the candidate was compared against. When a side has no thumbnail
    (an older capture, or thumbnails disabled), that side falls back to a live
    render of the document. Every failure here is ignored: a review aid must
    never change the verdict.
    """
    doc_ids = sorted({c.doc_id for c in changes if c.property == "rendered_image"})
    if not doc_ids:
        return
    base_docs = {d.doc_id: d for d in (baseline.documents if baseline else ())}
    cand_docs = {d.doc_id: d for d in (candidate.documents if candidate else ())}
    engine = None
    try:
        for doc_id in doc_ids:
            b = base_docs.get(doc_id)
            c = cand_docs.get(doc_id)
            base_imgs = [_decode_thumbnail(p.thumbnail) for p in (b.pages if b else ())]
            cand_imgs = [_decode_thumbnail(p.thumbnail) for p in (c.pages if c else ())]
            needs_live = not any(base_imgs) or not any(cand_imgs)
            if needs_live and engine_cfg is not None and engine is None:
                try:
                    engine = engine_cfg.build()
                except Exception:  # noqa: BLE001
                    engine = None
            if needs_live and engine is not None:
                try:
                    live = _live_page_images(doc_id, engine, Path(wpt_root), spec, dpi)
                except Exception:  # noqa: BLE001
                    live = []
                if not any(base_imgs):
                    base_imgs = base_imgs or live
                if not any(cand_imgs):
                    cand_imgs = cand_imgs or live

            slug = _slug(doc_id, "rendered_image")
            changes_dir = out_dir / "changes"
            for i in range(max(len(base_imgs), len(cand_imgs))):
                bi = base_imgs[i] if i < len(base_imgs) else None
                ci = cand_imgs[i] if i < len(cand_imgs) else None
                if bi is None and ci is None:
                    continue
                try:
                    composed = _compose_side_by_side(bi, ci)
                except Exception:  # noqa: BLE001
                    continue
                changes_dir.mkdir(parents=True, exist_ok=True)
                name = f"{slug}.png" if i == 0 else f"{slug}-p{i}.png"
                composed.save(changes_dir / name)
    finally:
        close = getattr(engine, "close", None)
        if close:
            close()


# ---------------------------------------------------------------------------
# Evaluation
# ---------------------------------------------------------------------------


def evaluate(
    *,
    baseline,
    candidate,
    manifest_path,
    reviews_path,
    policy_path,
    out_dir,
    wpt_root,
    corpus_root,
    fixture_root,
    engine_cfg,
    dpi,
    spec,
) -> GateVerdict:
    """Run the full gate and return a :class:`GateVerdict`.

    ``baseline``/``candidate`` may each be a :class:`Capture`, a path, or a bare
    label (resolved against ``out_dir/../captures``). A failing verdict writes a
    report (``<out_dir>/<candidate-label>-gate.json``) and best-effort review
    PNGs; a passing verdict writes no new state.
    """
    out_dir = Path(out_dir)
    policy = load_policy(policy_path)

    conditions_list: list[Condition] = []
    changes: list[Change] = []
    direct: list[CheckResult] = []
    report_path: Path | None = None

    base_cap, base_err = _resolve_capture(baseline, out_dir)
    cand_cap, cand_err = _resolve_capture(candidate, out_dir)

    if base_err == "missing":
        conditions_list.append(
            Condition("missing_baseline", f"baseline not found: {_ref_label(baseline)}")
        )
    elif base_err and base_err.startswith("schema"):
        conditions_list.append(
            Condition(
                "unsupported_schema",
                f"baseline {_ref_label(baseline)}: {base_err.split(': ', 1)[-1]}",
            )
        )
    if cand_err == "missing":
        conditions_list.append(
            Condition("missing_candidate", f"candidate not found: {_ref_label(candidate)}")
        )
    elif cand_err and cand_err.startswith("schema"):
        conditions_list.append(
            Condition(
                "unsupported_schema",
                f"candidate {_ref_label(candidate)}: {cand_err.split(': ', 1)[-1]}",
            )
        )

    if base_cap is not None and cand_cap is not None:
        conditions_list += conditions(base_cap, cand_cap, policy)
        changes = compare(base_cap, cand_cap)
        dispositions = load_reviews(reviews_path)
        conditions_list += _review_conditions(changes, dispositions)
        direct_conds, direct = _direct_conditions(
            manifest_path,
            wpt_root=wpt_root,
            corpus_root=corpus_root,
            fixture_root=fixture_root,
            engine_cfg=engine_cfg,
            dpi=dpi,
            spec=spec,
        )
        conditions_list += direct_conds
    else:
        # Direct evidence is still assessed against the manifest alone when a
        # capture is missing, so a bare `gate` still names the missing manifest.
        direct_conds, direct = _direct_conditions(
            manifest_path,
            wpt_root=wpt_root,
            corpus_root=corpus_root,
            fixture_root=fixture_root,
            engine_cfg=engine_cfg,
            dpi=dpi,
            spec=spec,
        )
        conditions_list += direct_conds

    ok = not conditions_list

    if not ok:
        base_identity = base_cap.identity if base_cap is not None else None
        cand_identity = cand_cap.identity if cand_cap is not None else None
        report_path = _write_report(
            out_dir,
            _artifact_label(candidate),
            conditions_list,
            changes,
            direct,
            base_identity,
            cand_identity,
        )
        _write_change_pngs(
            changes,
            out_dir,
            base_cap,
            cand_cap,
            Path(wpt_root),
            engine_cfg,
            spec,
            dpi,
        )

    return GateVerdict(
        ok=ok,
        conditions=conditions_list,
        changes=changes,
        direct=direct,
        report_path=report_path,
    )
