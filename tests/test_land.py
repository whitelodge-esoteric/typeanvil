"""scripts/land.py pure helpers: release pick, ident and subject checks, sign-off, PR body.

No git and no network. The I/O paths of the script are exercised by hand with
`python3 scripts/land.py CORE-<N> --dry-run`.
"""

from __future__ import annotations

from scripts.land import (
    NO_RECEIPT,
    add_signoff,
    format_receipt,
    pick_release,
    render_body,
    split_message,
    subject_ok,
    validate_ident,
)


# ---------------------------------------------------------------------------
# Release pick
# ---------------------------------------------------------------------------


def test_release_pick_sorts_numerically_not_lexicographically():
    assert pick_release(["release/2026.9", "release/2026.10"]) == "release/2026.10"
    assert pick_release(["release/2026.10", "release/2026.9"]) == "release/2026.10"


def test_release_pick_prefers_the_newest_year():
    names = ["release/2026.9", "release/2026.10", "release/2027.1"]
    assert pick_release(names) == "release/2027.1"


def test_release_pick_ignores_names_that_are_not_releases():
    names = ["main", "release/2026.9", "release/next", "release/2026.10-rc1", "feature/2030.1"]
    assert pick_release(names) == "release/2026.9"


def test_release_pick_returns_none_without_a_release():
    assert pick_release([]) is None
    assert pick_release(["main", "ebboston/core-1"]) is None


# ---------------------------------------------------------------------------
# Ident and subject
# ---------------------------------------------------------------------------


def test_ident_accepts_only_core_digits():
    assert validate_ident("CORE-497")
    for bad in ["core-497", "CORE-", "CORE-12a", "CORE-1 ", "OPS-5", "497", "", "CORE-1\nx"]:
        assert not validate_ident(bad), bad


def test_subject_needs_type_prefix_and_matching_ident_suffix():
    assert subject_ok("fix: close the gap (CORE-497)", "CORE-497")
    assert subject_ok("chore: land issues — land.py (CORE-497)", "CORE-497")
    assert not subject_ok("fix: close the gap (CORE-498)", "CORE-497")
    assert not subject_ok("fix: close the gap", "CORE-497")
    assert not subject_ok("refactor: close the gap (CORE-497)", "CORE-497")
    assert not subject_ok("fix: close the gap (CORE-497) extra", "CORE-497")
    assert not subject_ok("fix: (CORE-497)", "CORE-497")


# ---------------------------------------------------------------------------
# Sign-off
# ---------------------------------------------------------------------------

NAME, EMAIL = "Elijah Boston", "e.b.boston@gmail.com"
TRAILER = f"Signed-off-by: {NAME} <{EMAIL}>"


def test_signoff_is_appended_after_a_blank_line():
    out = add_signoff("fix: x (CORE-1)\n\nBody text.\n", NAME, EMAIL)
    assert out == f"fix: x (CORE-1)\n\nBody text.\n\n{TRAILER}\n"


def test_signoff_on_a_subject_only_message():
    assert add_signoff("fix: x (CORE-1)", NAME, EMAIL) == f"fix: x (CORE-1)\n\n{TRAILER}\n"


def test_signoff_is_added_once():
    once = add_signoff("fix: x (CORE-1)\n\nBody.", NAME, EMAIL)
    assert add_signoff(once, NAME, EMAIL) == once
    assert once.count("Signed-off-by:") == 1


def test_signoff_joins_an_existing_trailer_block():
    msg = "fix: x (CORE-1)\n\nBody.\n\nCo-authored-by: A <a@example.com>\n"
    out = add_signoff(msg, NAME, EMAIL)
    assert out.endswith(f"Co-authored-by: A <a@example.com>\n{TRAILER}\n")


def test_other_signoff_does_not_block_ours():
    msg = "fix: x (CORE-1)\n\nSigned-off-by: Other <o@example.com>\n"
    out = add_signoff(msg, NAME, EMAIL)
    assert "Signed-off-by: Other <o@example.com>" in out
    assert TRAILER in out


# ---------------------------------------------------------------------------
# PR body
# ---------------------------------------------------------------------------

RECEIPT = "gate: PASSED at abc1234 — cargo 100 passed, pytest 50 passed"


def test_body_has_issue_receipt_and_both_visual_markers():
    body = render_body("CORE-497", "chore: x (CORE-497)", "Why and what.", RECEIPT)
    assert "Fixes CORE-497" in body
    assert RECEIPT in body
    assert "<!-- visual-diff:start -->" in body
    assert "<!-- visual-diff:end -->" in body
    assert body.index("visual-diff:start") < body.index("visual-diff:end")
    assert "Why and what." in body


def test_body_sections_follow_the_template_order():
    body = render_body("CORE-1", "fix: x (CORE-1)", "b", RECEIPT)
    heads = [h for h in ("## Summary", "## Issue", "## Verification", "## Visual changes")]
    positions = [body.index(h) for h in heads]
    assert positions == sorted(positions)


def test_body_summary_drops_signoff_and_falls_back_to_subject():
    msg_body = f"Details.\n\n{TRAILER}"
    body = render_body("CORE-1", "fix: x (CORE-1)", msg_body, RECEIPT)
    assert "Signed-off-by" not in body
    empty = render_body("CORE-1", "fix: x (CORE-1)", "", RECEIPT)
    assert "fix: x (CORE-1)" in empty


# ---------------------------------------------------------------------------
# Receipt line and message split
# ---------------------------------------------------------------------------


def test_receipt_line_uses_verified_counts():
    data = {
        "head": "abcdef1234567",
        "steps": [
            {"name": "clippy", "passed": None},
            {"name": "cargo-test", "passed": 100},
            {"name": "pytest", "passed": 50},
        ],
    }
    assert format_receipt(data, True) == "gate: PASSED at abcdef1 — cargo 100 passed, pytest 50 passed"


def test_receipt_line_without_a_verified_receipt():
    assert format_receipt(None, False) == NO_RECEIPT
    assert format_receipt({"head": "abc", "steps": []}, False) == NO_RECEIPT


def test_split_message_separates_subject_and_body():
    assert split_message("fix: x (CORE-1)\n\nBody line.\n") == ("fix: x (CORE-1)", "Body line.")
    assert split_message("fix: x (CORE-1)\n") == ("fix: x (CORE-1)", "")
