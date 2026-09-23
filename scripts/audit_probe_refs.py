#!/usr/bin/env python3
"""Audit which tracked files under a directory are still referenced.

Classifies each tracked file as:
  doc-cited   — referenced by docs/, AGENTS.md, or README.md (keep; it is
                reproducibility evidence)
  probe-only  — referenced only by other files inside the audited directory
                (keep while its root diagnosis doc is itself cited)
  dead        — referenced nowhere in the repository (candidate for removal)

Also prints WARN lines for "probe/..." mentions that point at non-tracked
paths (e.g. worktree-scratch probes, the runtime issue-evidence dir), so
editing a doc after a prune does not silently break a citation.

Usage::

    python3 scripts/audit_probe_refs.py [DIR]     # default DIR = probe

Exit code is always 0; the report is the output.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

# Directories that are never part of the reference corpus.
EXCLUDE_DIRS = {".git", ".wpt", "node_modules", ".venv", "docsite", "demo"}


def tracked_files(directory: str) -> list[str]:
    """List files under directory that git tracks (normalized paths)."""
    out = subprocess.run(
        ["git", "ls-files", directory], capture_output=True, text=True, check=True
    )
    return [f for f in out.stdout.split() if f]


def corpus_files() -> list[str]:
    """All text files that can reference a probe path."""
    corpus: list[str] = []
    for root, dirs, files in os.walk("."):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for fn in files:
            if fn.endswith((".md", ".py", ".sh", ".yml", ".yaml", ".json", ".toml")):
                corpus.append(os.path.normpath(os.path.join(root, fn)))
    return corpus


def references(path: str, directory: str, files: list[str]) -> list[str]:
    """Files whose text mentions path by base name or as ``<dir>/<stem>``."""
    name = os.path.basename(path)
    stem = name.rsplit(".", 1)[0]
    hits: set[str] = set()
    for f in files:
        try:
            text = open(f, encoding="utf-8", errors="ignore").read()
        except OSError:
            continue
        if name in text or re.search(re.escape(directory + "/") + re.escape(stem), text):
            hits.add(os.path.normpath(f))
    hits.discard(os.path.normpath(path))
    return sorted(hits)


def dangling_mentions(tracked: set[str]) -> list[tuple[str, str]]:
    """Doc mentions of ``probe/...`` that are not a tracked file."""
    found: list[tuple[str, str]] = []
    for root, dirs, files in os.walk("docs"):
        for fn in files:
            if not fn.endswith(".md"):
                continue
            path = os.path.join(root, fn)
            text = open(path, encoding="utf-8", errors="ignore").read()
            for m in re.findall(r"probe/[A-Za-z0-9_./-]+", text):
                cand = m.strip("`.,;:)(")
                if cand not in tracked:
                    found.append((path, cand))
    return sorted(set(found))


def classify(directory: str) -> None:
    tracked = tracked_files(directory)
    corpus = corpus_files()
    doc_cited, probe_only, dead = {}, {}, []
    for f in tracked:
        refs = references(f, directory, corpus)
        doc = [r for r in refs if r.startswith("docs/") or r in ("AGENTS.md", "README.md")]
        inner = [r for r in refs if r.startswith(directory + "/")]
        if doc:
            doc_cited[f] = doc
        elif inner:
            probe_only[f] = inner
        else:
            dead.append(f)

    print(f"=== {len(doc_cited)} cited directly by docs ===")
    for f, refs in sorted(doc_cited.items()):
        print(f"  {f}  <- {refs}")
    print(f"\n=== {len(probe_only)} cited only inside {directory}/ (chains) ===")
    for f, refs in sorted(probe_only.items()):
        print(f"  {f}  <- {refs}")
    print(f"\n=== {len(dead)} dead (no reference anywhere) ===")
    for f in sorted(dead):
        print(f"  {f}")
    print(
        f"\nTOTAL {len(tracked)} = "
        f"{len(doc_cited)} doc-cited + {len(probe_only)} {directory}-only + {len(dead)} dead"
    )

    warns = dangling_mentions(set(tracked))
    if warns:
        print("\nWARN: docs mention non-tracked probe paths (scratch/runtime — verify intent):")
        for path, cand in warns:
            print(f"  {path}: {cand}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dir", nargs="?", default="probe", help="directory to audit (default: probe)")
    args = parser.parse_args()
    classify(args.dir)
    sys.exit(0)


if __name__ == "__main__":
    main()