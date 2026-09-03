---
title: CLI surface — --version and --help
type: spec
status: approved
owner: Elijah Boston
created: 2026-09-03
updated: 2026-09-03
slug: /specifications/cli-surface
sidebar_position: 44
tags: [cli, version, help, core-134, distribution]
spec_id: SPEC-CORE-134-cli-surface
issue_id: CORE-134
applies_to:
  - engine/src/main.rs
  - engine/tests/cli_surface.rs
dependencies:
  - SPEC-CORE-112-diagnostics
---

# CLI surface: `--version` and `--help` (CORE-134)

Parent epic: CORE-133 (installable runtime). Release CI (CORE-135), the npm
package (CORE-136), and the Homebrew formula (CORE-137) all need a
machine-readable version and discoverable help text from the binary itself.

## Goals

1. `--version` / `-V` prints the crate version sourced from
   `CARGO_PKG_VERSION` (stays in sync with `engine/Cargo.toml`).
2. `--help` / `-h` prints usage: subcommands, flags, one-line descriptions.
3. Wrapper scripts can rely on exit codes.

## Non-Goals

- No arg-parsing framework (clap etc.) — the hand parser stays.
- No flag support in any position other than FIRST argument (see Behavior 3).
- No changes to the `render` flag contract used by `harness/engine.py`.

## Behavior

1. `typeanvil --version` (and `-V`) shall print `typeanvil <version>` to
   stdout, sourced from `CARGO_PKG_VERSION`, and exit 0.
2. `typeanvil --help` (and `-h`) shall print usage (subcommands, flags,
   one-line descriptions) to stdout and exit 0.
3. Both flags shall be honored only as the FIRST argument. In any other
   position they shall be rejected with a non-zero exit and a message on
   stderr.
4. `typeanvil` with no arguments shall print short usage to stderr and exit
   non-zero (unchanged from current behavior).
5. The existing render-flag contract (used by `harness/engine.py`) shall not
   change.

## Interfaces

```text
typeanvil --version | -V      # stdout: "typeanvil 0.0.1"; exit 0
typeanvil --help | -h         # stdout: usage text; exit 0
typeanvil render ...          # unchanged (see module doc in main.rs)
typeanvil <nothing>           # stderr: short usage; exit 1 (unchanged)
typeanvil render doc.html --version   # stderr: error; exit != 0
```

The version string is the compile-time `env!("CARGO_PKG_VERSION")` — no
runtime file reads, no network. CORE-135's release workflow stamps
`engine/Cargo.toml` from the git tag at build time, so the binary's version
always equals the tag.

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/cli_surface.rs`.

- **AC1** (Behavior 1): Given the binary, when invoked with `--version`,
  then stdout is `typeanvil <CARGO_PKG_VERSION>` and the exit status is 0.
  Same for `-V`.
- **AC2** (Behavior 2): when invoked with `--help`, then stdout contains
  `render`, `--page-width`, `--page-height`, `-o`, `--base-url`,
  `--diagnostics`, and `--version`/`--help` themselves; exit 0. Same for
  `-h`.
- **AC3** (Behavior 3): when invoked as `typeanvil render x.html --version`,
  the exit status is non-zero and stderr is non-empty. Same for `--help` in
  a non-first position.
- **AC4** (Behavior 4): with no arguments, exit status is non-zero and
  stderr is non-empty.
- **AC5** (Behavior 5): `cargo test` full suite green; no harness change
  (`harness/engine.py` untouched in the diff).

## Edge Cases

- `--version` with extra args (`typeanvil --version extra`) — first-arg
  match wins; extra args are ignored. Wrappers get a stable one-line output.
- The version line format is `typeanvil <version>` with a single space;
  CORE-135's tag-match check parses exactly this shape.

## References

- Epic: CORE-133 (installable runtime — npm + Homebrew distribution)
- Downstream: CORE-135 (release CI tag-match check), CORE-136, CORE-137
- Diagnostics spec for the CLI-doc conventions: `diagnostics.spec.md`
