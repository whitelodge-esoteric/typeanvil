---
title: Continuous Integration
type: runbook
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 7
tags: [ci, github-actions, clippy, cargo, harness, gate]
trigger: When a CI run fails, when you change CI, or before you push engine or harness changes
---

# Continuous integration

CI runs the engine tests, clippy, and the Python harness tests on every push to `main` and `release/**`, on every pull request, and on manual dispatch. The workflow is `.github/workflows/ci.yml`.

## What CI runs

| Job | Steps |
|---|---|
| `engine` | `cargo clippy --all-targets -- -D warnings`, then `cargo test --no-fail-fast`, both in `engine/` |
| `harness` | `uv sync`, then `uv run pytest tests -q`, on Python 3.12 |

Details:

- The `engine` job uses Rust 1.97, the same version as `docker/Dockerfile.dev`. It caches `engine/target` with `Swatinem/rust-cache`.
- GitHub runners have no macOS Arial. The `engine` job copies `docker/fonts/typeanvil/*` to `/usr/share/fonts/typeanvil/`. The engine uses the Liberation Sans faces there as its bundled fallback.
- The `harness` job installs the dependencies from `pyproject.toml` and `uv.lock`.
- A newer push to the same ref cancels the older run.

## Run CI locally

```bash
python3 scripts/gate.py
```

`scripts/ci-local.sh` is a thin wrapper around `scripts/gate.py`. The gate runs the same steps with the same flags as CI. CI runs the same commands natively in `.github/workflows/ci.yml` and does not call the gate. Rust runs in the dev container, so Docker shall be running and the `typeanvil-dev` image shall exist (see [Containerized development environment](./dev-container.md)). The image needs the `clippy` component. Rebuild the image if clippy is missing:

```bash
docker build -f docker/Dockerfile.dev -t typeanvil-dev .
```

The harness tests run on the host with the venv from the main checkout. Set `PY=/path/to/python` to use another interpreter.

The gate runs all three steps even when one fails. It prints a table and a final `GATE PASSED` or `GATE FAILED: <reasons>` line. See [Gate receipt](#gate-receipt).

## Gate receipt

`scripts/gate.py` runs the CI checks and writes `.gate/result.json`. The coordinator uses the receipt to confirm that the checks ran on the exact commit to land. Do not claim "gate passed" without a receipt that `--verify` accepts.

What the gate checks:

1. The tree shall be clean. Untracked files under `probe/`, `gate/captures/`, and `.gate/` are allowed. Any other change stops the run with exit 2 and `gate: tree is dirty — commit first`.
2. Each step runs with `subprocess.run`, with no shell pipes. The real exit code is recorded. Output goes to `.gate/<step>.log`.
3. The steps are `clippy` (`-- -D warnings --all-targets`), `cargo-test` (`--no-fail-fast`), and `pytest` (`tests -q -o addopts=`, with `PYTHONPATH` unset; `pyproject.toml` already sets `-q`, and a second `-q` would hide the summary line the gate parses). Rust runs in the dev container with `-j 2`.
4. The gate sums every `test result:` line from cargo, and reads the final pytest summary line. A test step that passes zero tests fails the gate, even with exit 0.
5. The gate passes only when every step exits 0, no test failed, each test step passed at least one test, and the run is not filtered.

`GATE_TEST_FILTER=<name>` appends a cargo test name filter. Use it for fast local runs. A filtered run records `"filtered": true` and `"ok": false`. A filtered run never counts as a gate.

Exit codes: 0 pass, 1 fail, 2 refused (dirty tree or no harness python).

Receipt fields (schema `typeanvil.gate/1`):

| Field | Meaning |
|---|---|
| `ok` | `true` only when the gate passed |
| `filtered` | `true` when `GATE_TEST_FILTER` was set |
| `head`, `tree`, `branch` | `git rev-parse HEAD`, `git rev-parse HEAD^{tree}`, and the branch name at run time |
| `created_at` | UTC time in ISO 8601 |
| `steps[]` | `name`, `cmd`, `exit`, `passed`, `failed`, `skipped`, `log` (clippy has `null` counts) |
| `reasons[]` | Why the gate failed. Empty on a pass. |

The gate deletes the old receipt when a run starts, and writes a new one on pass and on fail.

Verify a receipt:

```bash
python3 scripts/gate.py --verify [path]
```

`--verify` exits 0 only when the receipt exists, `ok` is `true`, `filtered` is `false`, and `tree` equals the current `git rev-parse HEAD^{tree}`. Otherwise it names each failed check and exits 1. A commit that changes the tree makes the old receipt stale. Amend or rebase without a tree change keeps the receipt valid, because the check uses the tree hash.

## Landing a change

Changes land through a pull request. Do not push to `release/**` or `main`.

```bash
python3 scripts/land.py CORE-<N> [--base release/X] [--message-file F] [--no-auto] [--wait] [--dry-run]
```

Run it from the issue branch with a clean tree, after `python3 scripts/gate.py` passes. The script:

1. Refuses (exit 2) on a dirty tree, on `main` or `release/*`, on a detached HEAD, or when the id is not `CORE-<digits>`.
2. Fetches `origin`. The base is `--base`, or the newest `origin/release/YYYY.M`. The newest branch is chosen by numeric CalVer order, so `release/2026.10` is newer than `release/2026.9`.
3. Rebases the branch onto the base. On a conflict it aborts the rebase, lists the conflicting files, and exits 1.
4. Exits 0 with "nothing to land" when the branch has no commit ahead of the base.
5. Squashes the branch to one commit with `git commit-tree`, and moves the branch ref with `git update-ref`. The tree does not change. The subject shall be `fix|feat|docs|chore: <text> (CORE-<N>)`. The message comes from `--message-file`, or from the newest commit whose subject ends with `(CORE-<N>)`. The script adds a `Signed-off-by` trailer when it is missing.
6. Pushes with `--force-with-lease`.
7. Opens the PR, or updates the open PR for the same branch and base. The PR body follows `.github/PULL_REQUEST_TEMPLATE.md`. The Verification section holds the gate receipt summary when `.gate/result.json` passes `gate.py --verify`. Otherwise it reads `gate: no verified receipt for this tree`.
8. Enables auto-merge with squash, unless `--no-auto` is set. GitHub merges the PR when the required checks are green.
9. Prints the PR URL. With `--wait` it polls every 30 seconds for up to 60 minutes, prints the merge sha, and exits 1 if the PR closes or the wait ends.

`--dry-run` runs the refusal checks, the fetch, and the rebase. It prints the plan and changes nothing else.

Rules:

- Each issue lands as one squash commit. The repository allows squash merges only.
- The `agent-landed` label marks a PR that automation opened. Create the label once in the repository.
- The `visual-diff` markers in the PR body are reserved for CI (CORE-498). Do not edit them.

## Branch rules

GitHub rulesets protect the integration branches.

| Branch | Rule |
|---|---|
| `release/**` | A change shall arrive through a pull request. The checks `engine`, `harness`, and `validate` shall pass. Force-push and deletion are blocked. |
| `main` | The same checks shall pass on push. Force-push is blocked. |

Details:

- `engine` and `harness` come from `ci.yml`. `validate` comes from `docs-validation.yml`.
- A required check that never reports blocks the pull request forever. For this reason `docs-validation.yml` has no `paths:` filter and runs on every push and pull request. Do not add a path filter to a workflow that provides a required check.
- Promotion to `main` stays a fast-forward push of a tip that is already green on `release/**`.
- Merged branches are deleted automatically.

## Fonts in CI

CI has the Liberation Sans faces only. It has no macOS Arial, and the repository shall not ship Arial (licence). The engine picks the first readable file from each face's candidate list in `engine/src/fonts.rs`: macOS Arial first, Liberation Sans second.

Rules for tests:

1. A test shall not assert a macOS font path or an `Arial*MT` PostScript name for a bundled face.
2. A test shall read the expected name from the face file the engine resolved. `engine/tests/it/fonts.rs` does this with `expected_ps_name(face)`, which reads nameID 6 from `typeanvil::fonts::face_path(face)`.
3. A test that needs a macOS system face shall carry `#[cfg(target_os = "macos")]`.

`scripts/dev-container.sh` mounts the Mac Arial, so the tests also pass there when they depend on Arial by mistake. To run the CI font setup locally, run the tests in a container without that mount. The container copies Liberation to `/usr/share/fonts/typeanvil/`, then runs cargo as the `dev` user. Run it from the worktree root:

```bash
W="$(basename "$PWD")"
docker run --rm --user root --memory 6g --cpus 4 --env CARGO_HOME=/cargo-home \
  -v dev-cargo-home:/cargo-home -v dev-cargo-git:/cargo-home/git \
  -v dev-cargo-registry:/cargo-home/registry \
  -v "dev-target-$W:/work/engine/target" -v "$PWD:/work:ro" -w /work \
  typeanvil-dev bash -c 'mkdir -p /usr/share/fonts/typeanvil \
    && cp docker/fonts/typeanvil/*.ttf /usr/share/fonts/typeanvil/ \
    && exec setpriv --reuid=dev --regid=dev --init-groups "$@"' bash \
  cargo test --manifest-path /work/engine/Cargo.toml -j 2 --test it fonts::
```

Replace the last line to run other tests, for example `--lib fonts::`. The `:ro` mount of the worktree is enough for `cargo test` because the build output lives in the volume.

## The clippy ratchet

CI shall fail on any new warning and shall not fail on existing ones. `engine/Cargo.toml` achieves this with two tables:

- `[lints.clippy]` sets each clippy lint that fired on 2026-10-17 to `"allow"`.
- `[lints.rust]` does the same for rustc lints such as `unused_variables`.

Each entry has a comment of the form `# ratchet: N existing hits as of 2026-10-17, CORE-474`. CI runs clippy with `-D warnings`, so a lint that is not in the tables fails the build.

Rules:

1. A new warning from an allowed lint class does not fail CI. Do not add new hits to an allowed class. Fix the code.
2. A warning from any other lint fails CI. Fix the code. Do not add an `allow` line to hide new work.
3. To retire a lint class: remove its `allow` line and its comment, fix all hits, and commit. Run `python3 scripts/gate.py` before you push. The ratchet only moves toward fewer allowed lints.
4. Lints that are deny-by-default in clippy, such as `mut_from_ref`, are also listed as `allow` when they fire in existing code.

## Not in CI yet

| Check | Where it runs today |
|---|---|
| `cargo fmt --check` | Not run. The current tree has about 8900 lines of formatting diff. Format the tree in one dedicated commit, then add the step to `ci.yml` and `scripts/gate.py`. |
| WPT release gate | Run by hand; see the [release gate runbook](./release-gate.md). |
| Tagged-PDF gate | Run by hand; see the [tagged gate runbook](./tagged-gate.md). |
| Corpus guard | Not part of CI. The repository has no corpus guard script yet. |
| Docs validation | Has its own workflow, `.github/workflows/docs-validation.yml`. |

## Verification

A passing run exits 0 from `scripts/gate.py`, writes a receipt that `--verify` accepts, and shows a green `ci` check on the pull request. To confirm the ratchet still works, add a line such as `let _x = 1 == 1;` to an engine source file and run the clippy step. The step shall fail. Revert the line afterwards.

## Troubleshooting

- **`cargo-clippy` is not installed (local run):** rebuild the dev image. It installs `clippy` and `rustfmt`.
- **Container build killed for memory:** stop other builds. The script already passes `-j 2`.
- **A test passes locally and fails in CI (or the reverse):** local runs use the macOS Arial mounted into the container. CI uses Liberation Sans. Check for a font-dependent assertion.
- **Clippy fails after a Rust version bump:** new lints may fire. Update the Rust version in `ci.yml` and `docker/Dockerfile.dev` together, then add or fix lints in the same commit.
