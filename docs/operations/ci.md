---
title: Continuous Integration
type: runbook
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 7
tags: [ci, github-actions, clippy, cargo, harness]
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
scripts/ci-local.sh
```

The script runs the same steps with the same flags. Rust runs in the dev container, so Docker shall be running and the `typeanvil-dev` image shall exist (see [Containerized development environment](./dev-container.md)). The image needs the `clippy` component. Rebuild the image if clippy is missing:

```bash
docker build -f docker/Dockerfile.dev -t typeanvil-dev .
```

The harness tests run on the host with the venv from the main checkout. Set `PY=/path/to/python` to use another interpreter.

The script stops at the first failing step and prints `ci-local: FAILED at step '<name>'`. On success it prints one `PASS` line per step.

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
3. To retire a lint class: remove its `allow` line and its comment, fix all hits, and commit. Run `scripts/ci-local.sh` before you push. The ratchet only moves toward fewer allowed lints.
4. Lints that are deny-by-default in clippy, such as `mut_from_ref`, are also listed as `allow` when they fire in existing code.

## Not in CI yet

| Check | Where it runs today |
|---|---|
| `cargo fmt --check` | Not run. The current tree has about 8900 lines of formatting diff. Format the tree in one dedicated commit, then add the step to `ci.yml` and `scripts/ci-local.sh`. |
| WPT release gate | Run by hand; see the [release gate runbook](./release-gate.md). |
| Tagged-PDF gate | Run by hand; see the [tagged gate runbook](./tagged-gate.md). |
| Corpus guard | Not part of CI. The repository has no corpus guard script yet. |
| Docs validation | Has its own workflow, `.github/workflows/docs-validation.yml`. |

## Verification

A passing run exits 0 from `scripts/ci-local.sh` and shows a green `ci` check on the pull request. To confirm the ratchet still works, add a line such as `let _x = 1 == 1;` to an engine source file and run the clippy step. The step shall fail. Revert the line afterwards.

## Troubleshooting

- **`cargo-clippy` is not installed (local run):** rebuild the dev image. It installs `clippy` and `rustfmt`.
- **Container build killed for memory:** stop other builds. The script already passes `-j 2`.
- **A test passes locally and fails in CI (or the reverse):** local runs use the macOS Arial mounted into the container. CI uses Liberation Sans. Check for a font-dependent assertion.
- **Clippy fails after a Rust version bump:** new lints may fire. Update the Rust version in `ci.yml` and `docker/Dockerfile.dev` together, then add or fix lints in the same commit.
