"""The showcase workflow must start after release.yml, which uses GITHUB_TOKEN."""

from __future__ import annotations

import re
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parent.parent / ".github/workflows/showcase.yml"


def _on_block() -> str:
    text = WORKFLOW.read_text()
    m = re.search(r"^on:\n((?:[ \t]+.*\n|[ \t]*\n|#.*\n)+)", text, re.M)
    assert m, "no on: block"
    return m.group(1)


def test_workflow_trigger_is_workflow_run():
    on = _on_block()
    # Events created by GITHUB_TOKEN start no workflows, so `release:` never fires.
    assert not re.search(r"^  release:", on, re.M)
    assert re.search(r"^  workflow_run:", on, re.M)
    assert re.search(r"workflows:\s*\[release\]", on)
    assert re.search(r"^  workflow_dispatch:", on, re.M)


def test_workflow_gates_on_release_success_and_reads_tag_from_run():
    text = WORKFLOW.read_text()
    assert "workflow_run.conclusion == 'success'" in text
    assert "github.event.workflow_run.head_branch" in text
    assert "github.event.release.tag_name" not in text
