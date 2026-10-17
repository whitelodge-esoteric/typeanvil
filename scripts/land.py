#!/usr/bin/env python3
"""Land the current issue branch through a GitHub PR with auto-merge.

Usage:
  python3 scripts/land.py CORE-<N> [--base release/X] [--message-file F]
                                   [--no-auto] [--wait] [--dry-run]

Steps: refuse unsafe states, fetch, rebase onto the newest release branch,
squash to ONE commit, push with --force-with-lease, create or update the PR,
enable auto-merge (squash). See docs/operations/ci.md ("Landing a change").

Exit codes: 0 landed (or nothing to land), 1 failed (conflict, push, PR,
closed PR, timeout), 2 refused (bad state or bad input).

Stdlib only. The pure helpers at the top have no side effects on import.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECEIPT = ROOT / ".gate" / "result.json"

IDENT_RE = re.compile(r"^CORE-\d+$")
RELEASE_RE = re.compile(r"^release/(\d{4})\.(\d{1,2})$")
SUBJECT_RE = re.compile(r"^(fix|feat|docs|chore): \S.* \((CORE-\d+)\)$")
TRAILER_RE = re.compile(r"^[A-Za-z][A-Za-z0-9-]*: \S")
PR_URL_RE = re.compile(r"/pull/(\d+)")

LABEL = "agent-landed"
POLL_SECONDS = 30
WAIT_LIMIT_SECONDS = 60 * 60

NO_RECEIPT = "gate: no verified receipt for this tree"


# ---------------------------------------------------------------------------
# Pure helpers
# ---------------------------------------------------------------------------


def pick_release(names: list[str]) -> str | None:
    """Return the newest `release/YYYY.M` name by numeric CalVer, else None.

    Names that do not match `release/<4 digits>.<1-2 digits>` are ignored, so
    `release/2026.10` sorts above `release/2026.9`.
    """
    best: tuple[int, int] | None = None
    best_name: str | None = None
    for name in names:
        m = RELEASE_RE.match(name)
        if not m:
            continue
        key = (int(m.group(1)), int(m.group(2)))
        if best is None or key > best:
            best, best_name = key, name
    return best_name


def validate_ident(s: str) -> bool:
    """True when `s` is `CORE-<digits>`."""
    return bool(IDENT_RE.match(s))


def subject_ok(subject: str, ident: str) -> bool:
    """True when the subject is `fix|feat|docs|chore: <text> (<ident>)`."""
    m = SUBJECT_RE.match(subject)
    return bool(m) and m.group(2) == ident


def split_message(message: str) -> tuple[str, str]:
    """Split a commit message into (subject, body). Body is stripped."""
    text = message.strip("\n")
    subject, _, rest = text.partition("\n")
    return subject.strip(), rest.strip()


def add_signoff(msg: str, name: str, email: str) -> str:
    """Append `Signed-off-by: name <email>` once. Idempotent.

    The result ends with exactly one newline. The trailer joins an existing
    trailer block; otherwise a blank line separates it from the body.
    """
    trailer = f"Signed-off-by: {name} <{email}>"
    text = msg.strip("\n")
    if trailer in text.splitlines():
        return text + "\n"
    paragraphs = text.split("\n\n")
    last = paragraphs[-1].splitlines()
    in_trailer_block = (
        len(paragraphs) > 1 and bool(last) and all(TRAILER_RE.match(x) for x in last)
    )
    sep = "\n" if in_trailer_block else "\n\n"
    return f"{text}{sep}{trailer}\n"


def strip_trailers(body: str) -> str:
    """Drop `Signed-off-by:` lines from a commit body."""
    lines = [x for x in body.splitlines() if not x.startswith("Signed-off-by:")]
    return "\n".join(lines).strip()


def format_receipt(data: dict | None, verified: bool) -> str:
    """One-line gate summary from a parsed receipt and its --verify result."""
    if not data or not verified:
        return NO_RECEIPT
    counts = {s.get("name"): s.get("passed") for s in data.get("steps", [])}
    cargo = counts.get("cargo-test") or 0
    pytest = counts.get("pytest") or 0
    head = str(data.get("head", ""))[:7]
    return f"gate: PASSED at {head} — cargo {cargo} passed, pytest {pytest} passed"


def render_body(ident: str, subject: str, message_body: str, receipt: str) -> str:
    """Render the PR body from the template sections in the PR template."""
    summary = strip_trailers(message_body) or subject
    return (
        "## Summary\n\n"
        f"{summary}\n\n"
        "## Issue\n\n"
        f"Fixes {ident}\n\n"
        "## Verification\n\n"
        f"{receipt}\n\n"
        "## Visual changes\n\n"
        "<!-- visual-diff:start -->\n"
        "_Filled in by CI once the regression gate lands (CORE-498)._\n"
        "<!-- visual-diff:end -->\n"
    )


# ---------------------------------------------------------------------------
# I/O
# ---------------------------------------------------------------------------


class Stop(Exception):
    """Abort with an exit code and a message."""

    def __init__(self, code: int, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


def run(cmd: list[str], input: str | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd, check=False, capture_output=True, text=True, cwd=ROOT, input=input
    )


def run_ok(cmd: list[str], input: str | None = None, code: int = 1) -> str:
    """Run a command, return stdout, raise Stop with stderr on failure."""
    cp = run(cmd, input=input)
    if cp.returncode != 0:
        detail = (cp.stderr or cp.stdout).strip()
        raise Stop(code, f"land: `{' '.join(cmd[:3])}` failed ({cp.returncode})\n{detail}")
    return cp.stdout


def git(*args: str, input: str | None = None, code: int = 1) -> str:
    return run_ok(["git", *args], input=input, code=code).strip()


def gh(*args: str) -> str:
    return run_ok(["gh", *args]).strip()


def check_state(ident: str) -> str:
    """Step 1. Return the current branch name or raise Stop(2)."""
    if not validate_ident(ident):
        raise Stop(2, f"land: bad issue id {ident!r}; expected CORE-<digits>")
    cp = run(["git", "symbolic-ref", "--short", "-q", "HEAD"])
    branch = cp.stdout.strip()
    if cp.returncode != 0 or not branch:
        raise Stop(2, "land: detached HEAD; check out the issue branch")
    if branch == "main" or branch.startswith("release/"):
        raise Stop(2, f"land: refusing to land from {branch!r}; use an issue branch")
    dirty = git("status", "--porcelain", "--untracked-files=no")
    if dirty:
        raise Stop(2, f"land: tree is dirty; commit or stash first\n{dirty}")
    return branch


def resolve_base(requested: str | None) -> str:
    """Step 2 (after fetch). Return `release/X` that exists on origin."""
    if requested:
        base = requested.removeprefix("origin/")
    else:
        refs = git("for-each-ref", "--format=%(refname:short)", "refs/remotes/origin/release/")
        names = [r.removeprefix("origin/") for r in refs.splitlines()]
        base = pick_release(names)
        if base is None:
            raise Stop(2, "land: no origin/release/YYYY.M branch found; pass --base")
    if run(["git", "rev-parse", "--verify", "--quiet", f"origin/{base}"]).returncode != 0:
        raise Stop(2, f"land: origin/{base} does not exist")
    return base


def rebase(base: str) -> None:
    """Step 3."""
    cp = run(["git", "rebase", f"origin/{base}"])
    if cp.returncode == 0:
        return
    conflicts = run(["git", "diff", "--name-only", "--diff-filter=U"]).stdout.split()
    abort = run(["git", "rebase", "--abort"])
    msg = [f"land: rebase onto origin/{base} failed", (cp.stderr or cp.stdout).strip()]
    if conflicts:
        msg.append("conflicting files:\n" + "\n".join(f"  {f}" for f in conflicts))
    if abort.returncode != 0:
        msg.append(f"git rebase --abort also failed:\n{abort.stderr.strip()}")
    raise Stop(1, "\n".join(msg))


def choose_message(ident: str, base: str, message_file: str | None) -> str:
    """Step 5 message selection and validation. Returns the signed message."""
    if message_file:
        try:
            raw = Path(message_file).read_text(encoding="utf-8")
        except OSError as e:
            raise Stop(2, f"land: cannot read --message-file: {e}")
    else:
        raw = ""
        log = git("log", "--format=%H", f"origin/{base}..HEAD").splitlines()
        suffix = f"({ident})"
        for sha in log:  # newest first
            full = git("log", "-n1", "--format=%B", sha)
            if split_message(full)[0].endswith(suffix):
                raw = full
                break
        if not raw.strip():
            raise Stop(2, f"land: no commit subject ends with ({ident}); pass --message-file")
    subject, _ = split_message(raw)
    if not subject_ok(subject, ident):
        raise Stop(
            2,
            f"land: subject {subject!r} shall match "
            f"'fix|feat|docs|chore: <text> ({ident})'",
        )
    name = git("config", "user.name")
    email = git("config", "user.email")
    if not name or not email:
        raise Stop(2, "land: git config user.name and user.email shall be set")
    return add_signoff(raw, name, email)


def gate_receipt() -> str:
    """Summary line for `.gate/result.json`, or the no-receipt line."""
    if not RECEIPT.is_file():
        return NO_RECEIPT
    try:
        data = json.loads(RECEIPT.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return NO_RECEIPT
    verified = run([sys.executable, str(ROOT / "scripts" / "gate.py"), "--verify"])
    return format_receipt(data, verified.returncode == 0)


def squash(base: str, branch: str, message: str) -> str:
    """Step 5. Rewrite `branch` to one commit on origin/base. Returns new sha."""
    old = git("rev-parse", "HEAD")
    count = int(git("rev-list", "--count", f"origin/{base}..HEAD"))
    current = git("log", "-n1", "--format=%B", "HEAD")
    if count == 1 and current.strip() == message.strip():
        return old
    base_sha = git("rev-parse", f"origin/{base}")
    new = git("commit-tree", "HEAD^{tree}", "-p", base_sha, "-F", "-", input=message)
    git("update-ref", f"refs/heads/{branch}", new, old)
    return new


def find_pr(branch: str, base: str) -> dict | None:
    out = gh("pr", "list", "--head", branch, "--base", base, "--state", "open",
             "--json", "number,url")
    prs = json.loads(out or "[]")
    return prs[0] if prs else None


def open_or_update_pr(branch: str, base: str, subject: str, body: str) -> tuple[int, str]:
    """Step 7. Returns (number, url)."""
    with tempfile.TemporaryDirectory() as tmp:
        body_file = Path(tmp) / "body.md"
        body_file.write_text(body, encoding="utf-8")
        pr = find_pr(branch, base)
        if pr:
            gh("pr", "edit", str(pr["number"]), "--title", subject,
               "--body-file", str(body_file))
            return int(pr["number"]), str(pr["url"])
        url = gh("pr", "create", "--base", base, "--head", branch, "--title", subject,
                 "--body-file", str(body_file), "--label", LABEL).splitlines()[-1].strip()
    m = PR_URL_RE.search(url)
    if not m:
        raise Stop(1, f"land: cannot read the PR number from gh output: {url!r}")
    return int(m.group(1)), url


def wait_for_merge(number: int) -> int:
    """Step 9 with --wait. Poll every 30 s for up to 60 min."""
    deadline = time.monotonic() + WAIT_LIMIT_SECONDS
    while True:
        view = json.loads(gh("pr", "view", str(number), "--json", "state,mergeCommit"))
        state = view.get("state")
        if state == "MERGED":
            sha = (view.get("mergeCommit") or {}).get("oid", "")
            print(f"merged: {sha}")
            return 0
        if state == "CLOSED":
            print(f"land: PR #{number} was closed without merging", file=sys.stderr)
            return 1
        if time.monotonic() + POLL_SECONDS > deadline:
            print(f"land: PR #{number} not merged after 60 minutes", file=sys.stderr)
            return 1
        time.sleep(POLL_SECONDS)


def land(args: argparse.Namespace) -> int:
    ident: str = args.ident
    branch = check_state(ident)
    git("fetch", "origin", "--prune")
    base = resolve_base(args.base)
    rebase(base)
    ahead = int(git("rev-list", "--count", f"origin/{base}..HEAD"))
    if ahead == 0:
        print(f"land: nothing to land; {branch} has no commits ahead of origin/{base}")
        return 0

    message = choose_message(ident, base, args.message_file)
    subject, body = split_message(message)
    receipt = gate_receipt()
    pr_body = render_body(ident, subject, body, receipt)

    if args.dry_run:
        print(f"land (dry run): {ident} on {branch} -> origin/{base}")
        print(f"  {ahead} commit(s) ahead; would squash to one commit")
        print(f"  subject: {subject}")
        print(f"  {receipt}")
        print(f"  would run: git push --force-with-lease origin HEAD:refs/heads/{branch}")
        print(f"  would open or update the PR (label {LABEL}) against {base}")
        if args.no_auto:
            print("  would leave auto-merge off (--no-auto)")
        else:
            print("  would run: gh pr merge <n> --auto --squash")
        print("  PR body:\n" + "\n".join(f"    {x}" for x in pr_body.splitlines()))
        return 0

    new = squash(base, branch, message)
    print(f"land: {branch} is now one commit {new[:9]} on origin/{base}")
    git("push", "--force-with-lease", "origin", f"HEAD:refs/heads/{branch}")
    number, url = open_or_update_pr(branch, base, subject, pr_body)
    if not args.no_auto:
        gh("pr", "merge", str(number), "--auto", "--squash",
           "--subject", subject, "--body", body)
    print(url)
    if args.wait:
        return wait_for_merge(number)
    return 0


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(
        prog="land.py", description="Land the current issue branch through a PR."
    )
    p.add_argument("ident", help="issue id, CORE-<N>")
    p.add_argument("--base", help="base branch, release/X (default: newest origin/release/YYYY.M)")
    p.add_argument("--message-file", help="file with the squash commit message")
    p.add_argument("--no-auto", action="store_true", help="do not enable auto-merge")
    p.add_argument("--wait", action="store_true", help="wait up to 60 min for the merge")
    p.add_argument("--dry-run", action="store_true",
                   help="run the checks and print the plan; change nothing")
    try:
        args = p.parse_args(argv)
    except SystemExit as e:
        return int(e.code) if isinstance(e.code, int) else 2
    try:
        return land(args)
    except Stop as s:
        print(s.message, file=sys.stderr)
        return s.code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
