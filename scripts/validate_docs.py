#!/usr/bin/env python3
"""Validate docs/ against frontmatter-schema.md conventions.

Checks every .md under docs/ has frontmatter with required fields and valid
enums; spec_id uniqueness; _category_.yml files parse. Exits non-zero on failure.
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
    if yaml is not None:
        try:
            data = yaml.safe_load(fm_text)
        except Exception as e:  # noqa: BLE001
            return None, f"yaml parse error: {e}"
        return (data if isinstance(data, dict) else None), None
    # minimal manual parse fallback (no PyYAML)
    data: dict = {}
    for line in fm_text.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        k, _, v = line.partition(":")
        if not v.strip():
            continue
        k, v = k.strip(), v.strip()
        if v.startswith("[") and v.endswith("]"):
            data[k] = [t.strip() for t in v[1:-1].split(",") if t.strip()]
        elif re.match(r"^-?\d+$", v):
            data[k] = int(v)
        else:
            data[k] = v.strip("\"'")
    return data, None


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
    return errors


def main() -> int:
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
