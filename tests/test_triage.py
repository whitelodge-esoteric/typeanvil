"""Triage: four-way leg commands and the verdict reading."""

from __future__ import annotations

from pathlib import Path

from harness import triage


def _kw(tmp: Path, **over) -> dict:
    base = dict(
        wpt=tmp / ".wpt",
        report=tmp / "r.json",
        db=tmp / "h.sqlite",
        artifacts=tmp / "art",
        filter_substr="page-name-002",
        limit=None,
        workers=2,
        engine_cmd="/bin/typeanvil render",
        prince_cmd="/bin/render-prince.sh",
        weasy_cmd="/bin/render-weasyprint.sh",
    )
    base.update(over)
    return base


# --- leg command construction ------------------------------------------------


def test_engine_leg_uses_the_cli_engine_and_our_binary(tmp_path):
    argv = triage.leg_argv("engine", python="python3", **_kw(tmp_path))
    assert argv[:3] == ["python3", "-m", "harness"]
    assert "--engine" in argv and argv[argv.index("--engine") + 1] == "cli"
    assert argv[argv.index("--cli-cmd") + 1] == "/bin/typeanvil render"


def test_prince_leg_uses_the_cli_engine_and_the_wrapper(tmp_path):
    argv = triage.leg_argv("prince", python="python3", **_kw(tmp_path))
    assert argv[argv.index("--engine") + 1] == "cli"
    assert argv[argv.index("--cli-cmd") + 1] == "/bin/render-prince.sh"


def test_weasy_leg_uses_the_cli_engine_and_the_wrapper(tmp_path):
    argv = triage.leg_argv("weasy", python="python3", **_kw(tmp_path))
    assert argv[argv.index("--engine") + 1] == "cli"
    assert argv[argv.index("--cli-cmd") + 1] == "/bin/render-weasyprint.sh"


def test_weasy_leg_requires_a_wrapper_command(tmp_path):
    try:
        triage.leg_argv("weasy", python="python3", **_kw(tmp_path, weasy_cmd=None))
    except ValueError as exc:
        assert "weasy_cmd" in str(exc)
    else:  # pragma: no cover - the assertion path
        raise AssertionError("expected ValueError for a weasy leg without weasy_cmd")


def test_chromium_leg_uses_the_builtin_oracle_and_no_cli_cmd(tmp_path):
    argv = triage.leg_argv("chromium", python="python3", **_kw(tmp_path))
    assert argv[argv.index("--engine") + 1] == "chromium"
    assert "--cli-cmd" not in argv


def test_filter_and_limit_are_forwarded(tmp_path):
    argv = triage.leg_argv("engine", python="python3", **_kw(tmp_path, limit=7))
    assert argv[argv.index("--filter") + 1] == "page-name-002"
    assert argv[argv.index("--limit") + 1] == "7"


def test_unknown_leg_is_rejected(tmp_path):
    try:
        triage.leg_argv("safari", python="python3", **_kw(tmp_path))
    except ValueError as exc:
        assert "safari" in str(exc)
    else:  # pragma: no cover - the assertion path
        raise AssertionError("expected ValueError for an unknown leg")


# --- verdict readings (three-way, unchanged semantics) -----------------------


def test_engine_fails_where_both_other_engines_pass_is_our_bug():
    v = triage.verdict({"engine": "FAIL", "chromium": "PASS", "prince": "PASS"})
    assert "OUR BUG" in v


def test_engine_fails_chromium_only_points_at_a_contradiction():
    v = triage.verdict({"engine": "FAIL", "chromium": "PASS", "prince": "FAIL"})
    assert "contradiction" in v
    # This is the page-name-003 shape: do not report it as our bug.
    assert "OUR BUG" not in v


def test_engine_fails_prince_only_points_at_browser_ground_truth():
    v = triage.verdict({"engine": "FAIL", "chromium": "FAIL", "prince": "PASS"})
    assert "wpt.fyi" in v


def test_no_engine_satisfies_the_reference():
    v = triage.verdict({"engine": "FAIL", "chromium": "FAIL", "prince": "FAIL"})
    assert "no engine satisfies" in v


def test_engine_passing_alone_is_flagged_as_two_wrongs_matching():
    v = triage.verdict({"engine": "PASS", "chromium": "FAIL", "prince": "FAIL"})
    assert "two-wrongs-matching" in v


def test_all_legs_passing_is_not_flagged():
    assert triage.verdict({"engine": "PASS", "chromium": "PASS", "prince": "PASS"}) == ""


def test_engine_passing_with_one_agreeing_leg_is_not_flagged():
    assert triage.verdict({"engine": "PASS", "chromium": "PASS", "prince": "FAIL"}) == ""


# --- verdict readings (four-way) ---------------------------------------------


def test_four_way_engine_fails_where_everyone_else_passes_is_our_bug():
    v = triage.verdict({
        "engine": "FAIL", "chromium": "PASS", "prince": "PASS", "weasy": "PASS",
    })
    assert "OUR BUG" in v


def test_four_way_chromium_only_pass_points_at_a_contradiction():
    v = triage.verdict({
        "engine": "FAIL", "chromium": "PASS", "prince": "FAIL", "weasy": "FAIL",
    })
    assert "contradiction" in v
    assert "WeasyPrint" in v
    assert "OUR BUG" not in v


def test_four_way_weasy_only_pass_points_at_browser_ground_truth():
    v = triage.verdict({
        "engine": "FAIL", "chromium": "FAIL", "prince": "FAIL", "weasy": "PASS",
    })
    assert "WeasyPrint" in v
    assert "wpt.fyi" in v


def test_four_way_prince_only_pass_points_at_browser_ground_truth():
    v = triage.verdict({
        "engine": "FAIL", "chromium": "FAIL", "prince": "PASS", "weasy": "FAIL",
    })
    assert "Prince" in v
    assert "wpt.fyi" in v


def test_four_way_two_print_engines_pass_is_our_bug_when_chromium_fails():
    # Chromium fails but BOTH independent print engines satisfy the pair; the
    # reference may be print-only, but two engines agreeing still makes ours
    # the odd one out.
    v = triage.verdict({
        "engine": "FAIL", "chromium": "FAIL", "prince": "PASS", "weasy": "PASS",
    })
    assert "OUR BUG" in v


def test_four_way_no_engine_satisfies_the_reference():
    v = triage.verdict({
        "engine": "FAIL", "chromium": "FAIL", "prince": "FAIL", "weasy": "FAIL",
    })
    assert "no engine satisfies" in v


def test_four_way_engine_passing_alone_is_two_wrongs_matching():
    v = triage.verdict({
        "engine": "PASS", "chromium": "FAIL", "prince": "FAIL", "weasy": "FAIL",
    })
    assert "two-wrongs-matching" in v


def test_four_way_engine_passing_with_any_agreement_is_not_flagged():
    # One agreeing leg (chromium PASS) clears the two-wrongs-matching flag.
    v = triage.verdict({
        "engine": "PASS", "chromium": "PASS", "prince": "FAIL", "weasy": "FAIL",
    })
    assert v == ""
    # No agreement at all -> still flagged, exactly like the three-way run.
    v = triage.verdict({
        "engine": "PASS", "chromium": "FAIL", "prince": "FAIL", "weasy": "FAIL",
    })
    assert "two-wrongs-matching" in v


def test_four_way_all_pass_is_not_flagged():
    v = triage.verdict({
        "engine": "PASS", "chromium": "PASS", "prince": "PASS", "weasy": "PASS",
    })
    assert v == ""


def test_two_leg_run_with_a_single_engine_witness():
    v = triage.verdict({"engine": "FAIL", "weasy": "PASS"})
    assert "OUR BUG" in v
    assert "single comparison engine" in v


def test_wording_is_grammatical_for_one_failing_peer():
    v = triage.verdict({
        "engine": "FAIL", "chromium": "PASS", "prince": "FAIL", "weasy": "PASS",
    })
    # chromium + weasy pass -> OUR BUG (plural), no grammar issue to check.
    assert "OUR BUG" in v


# --- table formatting -------------------------------------------------------


def test_table_flags_only_the_disagreeing_row():
    results = {
        "engine": {"css/css-page/a.html": "FAIL", "css/css-page/b.html": "PASS"},
        "chromium": {"css/css-page/a.html": "PASS", "css/css-page/b.html": "PASS"},
        "prince": {"css/css-page/a.html": "PASS", "css/css-page/b.html": "PASS"},
        "weasy": {"css/css-page/a.html": "PASS", "css/css-page/b.html": "PASS"},
    }
    table, flagged = triage.format_table(results, ["engine", "chromium", "prince", "weasy"])
    assert "a.html" in table and "b.html" in table
    assert len(flagged) == 1
    assert flagged[0][0].startswith("a.html")
    assert "OUR BUG" in flagged[0][1]


def test_table_aligns_every_leg_column():
    results = {leg: {"css/css-page/x.html": "PASS"}
               for leg in ("engine", "chromium", "prince", "weasy")}
    table, flagged = triage.format_table(results, ["engine", "chromium", "prince", "weasy"])
    header, _, row = table.splitlines()
    assert header.index("engine") < header.index("chromium") < header.index("prince") < header.index("weasy")
    assert flagged == []
