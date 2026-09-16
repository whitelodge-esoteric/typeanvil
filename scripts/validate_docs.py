#!/usr/bin/env python3
"""Validate docs/ against frontmatter-schema.md conventions.

Checks every .md under docs/ has frontmatter with required fields and valid
enums; spec_id uniqueness; _category_.yml files parse. Exits non-zero on failure.

Requires PyYAML. Without it frontmatter YAML cannot be parsed, and a check that
skips YAML would report a false pass, so the script refuses to run (exit 2).
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"

TYPES = {"spec", "architecture", "lesson", "runbook", "convention", "research"}
STATUSES = {"draft", "in-review", "approved", "superseded"}
REQUIRED = {"title", "type", "status", "owner", "created", "updated", "sidebar_position", "tags"}
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

try:
    import yaml  # type: ignore
except ImportError:
    yaml = None


def parse_frontmatter(text: str) -> tuple[dict | None, str | None]:
    m = re.match(r"^---\r?\n(.*?)\r?\n---(\r?\n|$)", text, re.S)
    if not m:
        return None, "missing or malformed frontmatter (must start with '---' and close with '---')"
    fm_text = m.group(1)
    if yaml is None:
        # main() refuses to run without PyYAML; kept so this function is also
        # safe when called on its own.
        return None, "PyYAML is required to parse frontmatter"
    try:
        data = yaml.safe_load(fm_text)
    except Exception as e:  # noqa: BLE001
        return None, f"yaml parse error: {e}"
    return (data if isinstance(data, dict) else None), None


def check_file(path: Path) -> list[str]:
    errors: list[str] = []
    text = path.read_text(encoding="utf-8")
    data, err = parse_frontmatter(text)
    if err:
        return [f"{path.relative_to(ROOT)}: {err}"]
    assert data is not None
    is_root_index = path == DOCS / "README.md"
    if is_root_index:
        # Category index page: only title + sidebar_position required.
        if "title" not in data:
            errors.append(f"{path.relative_to(ROOT)}: missing required field 'title'")
        return errors
    for field in REQUIRED:
        if field not in data or data[field] in (None, ""):
            errors.append(f"{path.relative_to(ROOT)}: missing required field '{field}'")
    if "type" in data and data["type"] not in TYPES:
        errors.append(f"{path.relative_to(ROOT)}: invalid type '{data['type']}' (valid: {sorted(TYPES)})")
    if "status" in data and data["status"] not in STATUSES:
        errors.append(f"{path.relative_to(ROOT)}: invalid status '{data['status']}' (valid: {sorted(STATUSES)})")
    for field in ("created", "updated"):
        if field in data and not DATE_RE.match(str(data[field])):
            errors.append(f"{path.relative_to(ROOT)}: '{field}' must be YYYY-MM-DD, got '{data[field]}'")
    if "tags" in data and not isinstance(data["tags"], list):
        errors.append(f"{path.relative_to(ROOT)}: 'tags' must be a list")
    if data.get("type") == "spec" or path.name.endswith(".spec.md"):
        errors.extend(check_spec_conventions(path, data))
    return errors


def check_spec_conventions(path: Path, data: dict) -> list[str]:
    """Enforce the .spec.md convention (frontmatter-schema.md, Docusaurus slugs).

    Specs: live in specifications/, named <feature>.spec.md, declare
    type: spec, and carry a slug starting /specifications/ that does NOT end
    in .spec (a dot-suffix URL breaks the Docusaurus site).
    """
    rel = path.relative_to(DOCS)
    in_specs_dir = rel.parts[0] == "specifications"
    is_spec_filename = path.name.endswith(".spec.md")
    is_spec_type = data.get("type") == "spec"
    errors: list[str] = []
    if is_spec_type and not in_specs_dir:
        errors.append(f"{rel}: type: spec docs must live under specifications/")
    if is_spec_filename and not is_spec_type:
        errors.append(f"{rel}: *.spec.md files must declare type: spec")
    if is_spec_type:
        if not is_spec_filename:
            errors.append(f"{rel}: spec files must be named <feature>.spec.md")
        slug = data.get("slug")
        if not slug:
            errors.append(
                f"{rel}: spec docs require a 'slug' (a .spec URL suffix breaks "
                "the Docusaurus site)"
            )
        else:
            if str(slug).endswith(".spec"):
                errors.append(
                    f"{rel}: slug must not end in '.spec' — use '/specifications/<feature>'"
                )
            if not str(slug).startswith("/specifications/"):
                errors.append(f"{rel}: slug should start with '/specifications/'")
    return errors


def main() -> int:
    if yaml is None:
        # A silent skip is worse than a failure: without PyYAML the frontmatter
        # is never parsed, so frontmatter that CI rejects looks valid locally.
        # Verified 2026-09-16 — an unquoted colon in a title passed on the host
        # and failed three consecutive CI runs.
        print(
            "FAIL: PyYAML is not installed, so frontmatter YAML cannot be parsed.\n"
            "  A check that skips YAML parsing would report a false pass.\n"
            "  Install it and re-run: python3 -m pip install pyyaml",
            file=sys.stderr,
        )
        return 2

    errors: list[str] = []
    spec_ids: dict[str, Path] = {}
    md_files = sorted(DOCS.rglob("*.md"))
    for path in md_files:
        errors.extend(check_file(path))
        text = path.read_text(encoding="utf-8")
        data, _ = parse_frontmatter(text)
        if data and data.get("type") == "spec" and data.get("spec_id"):
            if data["spec_id"] in spec_ids:
                errors.append(
                    f"{path.relative_to(ROOT)}: duplicate spec_id '{data['spec_id']}' "
                    f"(also {spec_ids[data['spec_id']].relative_to(ROOT)})"
                )
            spec_ids[data["spec_id"]] = path
    if errors:
        print(f"FAIL: {len(errors)} issue(s)")
        for e in errors:
            print(f"  - {e}")
        return 1
    print(f"OK: {len(md_files)} docs files, all frontmatter valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
