---
title: Git Worktree and Landing Discipline
type: lesson
status: approved
owner: maintainers
created: 2026-09-16
updated: 2026-09-27
sidebar_position: 4
tags: [git, worktree, landing, release]
---

# Git worktree and landing discipline

## Context

Several worktrees or contributors can move the branch while a change is being tested. A landing must preserve live work and the exact source used for verification.

## Durable practices

- **`probe/` is tracked in the release branch; never `rm -rf` it as scratch cleanup**: the release worktree carries 38 committed files under `probe/` from earlier sessions, so deleting the directory to tidy up reads as 38 staged deletions and looks like destroying landed work. Restore with `git checkout -- probe/` (the commit is untouched; only the working tree was); verify with `git ls-files probe/ | wc -l`. Delete scratch files by explicit name, one at a time, and never remove a whole directory in a worktree you do not own.
- **Another session can push mid-flight; re-fetch before you squash**: `origin/release/2026.9` moved from `7237b81` to another session's `d984e2e` while the fix was in test. A stale squash would have landed on the old tip. `git fetch origin` first, confirm your branch point is an ancestor of the new tip (`git merge-base --is-ancestor <mine> <theirs>`), squash on top of the new tip, then rebuild and re-run both gate sides against that tip; a baseline built from the old tip mixes their flips into your report. A plain `git push` then works (fast-forward); no force needed.
- **Another session may be using the shared release worktree; check it before you merge**: the squash aborted with "Your local changes to the following files would be overwritten by merge: engine/src/layout.rs". The shared release worktree held a second session's live work (writing-mode threading in `css.rs`/`paged.rs`/`layout.rs` plus a `[SPECDBG]` `eprintln!`). Run `git status -s` in the release worktree before merging. If it is dirty, do not stash, checkout, or reset it; land from a throwaway detached worktree instead:

  ```bash
  git worktree add --detach /tmp/land-<issue> origin/release/<name>
  cd /tmp/land-<issue> && git merge --squash <branch> && git commit -F msg.txt
  git push --force-with-lease origin HEAD:release/<name>
  git worktree remove /tmp/land-<issue>
  ```

  Then tell the user: the shared worktree is now behind the remote and its next pull will conflict in whichever file both you and they touched.
- **Prove a squash did not change the tree with `git diff --stat <branch> HEAD`**: after the squash-landing commit, an empty diff against the branch you tested means the landed tree is byte-for-byte the tree the suite and gate ran against. That is cheaper and stronger than re-running the suite in the landing worktree, and it works when the landing worktree has no Docker volume built.
- **A landing must be re-gated when another session's commit arrives underneath the squash**: the squash merged cleanly on top of a landed change, but that change had added the UA body margin; two new grid tests (a 216pt grid as a direct body child, implied margin 0) then correctly rendered 2 pages and failed on the landed tree even though the WPT gate showed 0 broken. Fix: pin `body { margin: 0 }` in tests whose invariant needs a page-fitting container, and re-run the suite on the landed tree before pushing; the branch's green suite proves nothing once their commit is underneath. Watch the suite binary count too: a `tail` pipe of `cargo test` hides other binaries' failures.
- **User-reported visual bugs may already be half-fixed by newer commits; check `demo/out/` artifact dates against commit dates first**: a user screenshotted Aug-20 demo artifacts while the centering fix had landed at noon on Aug 24. Reproduce with a fresh render of the current worktree binary before diagnosing, then diff what still reproduces.
- **rustfmt noise floods a worktree's `git status`** (verified 2026-08-24): after any `cargo fmt` run in a worktree, ~25 files showed modifications the session never made (import reordering, line wraps), likely an editor or lint hook running tree-wide fmt. Before committing, run `git status -s`, split real changes from fmt-only ones (`git diff <file> | head`), and `git checkout --` everything you did not touch. Commit only your files by explicit path, never `git add -A`.
- **Concurrent sessions**: re-check `git worktree list` before assuming a worktree exists (they can vanish mid-session), and expect `origin/main` to move under you between merge and push.
- **SVG support is on the release branch, not main** (verified): the corpus rule that `feat:` commits target main excludes it. Public fixtures may not use inline `<svg>` or `<img src=*.svg>` until it reaches main.
- **Mid-flight landing, cheap attribution: rebase and re-run your side only when the flips clearly belong to the other feature** (verified 2026-09-14): the release tip moved between fork and gate and the first A/B showed 3 PASS→FAIL flips, none of whose fixtures declared the changed property (`text-align`) and all in the other feature's family (abspos fragmentation). Instead of building a three-way attribution, `git stash && git rebase origin/release/<name> && git stash pop`, rebuild, and re-run only your side against the same baseline commit. Zero flips after is clean. Do this only when fixture inspection is decisive; a three-way gate is still the tool when attribution is ambiguous.
- **Three-way gate attribution when a squash lands mid-flight** (verified 2026-09-13): a landed change arrived on `release/2026.9` between the baseline run and the squash, so old-versus-new showed 4 flips that were not mine. Resolve with a three-way gate: old-base binary (`a14f135`), new base without your change (`b1b1acf`, a temp worktree with its own `dev-target` volume), and the tip with your squash. Compare pairwise; every flip belongs to the commit that introduced it (`b1b1acf`-versus-`b322697` was 0; all 4 flips were the other change's ledger). Record the attribution on the issue; do not chase another feature's regressions.
- **Cutting a new issue branch from main misses release-branch-only fixtures**: demo, showcase, and corpus refreshes live on the release branch (main is behind). Before reproducing, `git merge-base`-check main against `origin/release/<name>`; if main is behind and the worktree is fresh and clean, `git reset --hard origin/release/<name>` (the WebUI consent gate will block this; ask the user once; for a fresh branch it loses nothing).
- **Release-CI bring-up lessons**: CalVer month is not zero-padded (`2026.9.0`); pipx tools need `GITHUB_PATH`; download zig directly (setup-zig mirrors return 404); cross binaries run under `qemu-aarch64-static`; build `x86_64-apple-darwin` on macos-14 (Intel runners are pool-starved); re-point tags with `git tag -d` plus `git tag <tag> HEAD`, never `git tag -f` twice.
- **Re-squashing the same branch after an earlier squash-landing conflicts** (verified 2026-09-15): the second `git merge --squash <branch>` conflicts add/add or content in every file the first squash already landed, because the merge base is still the original branch point. Do not rebase or reset to fix it. Verify the branch version is a strict superset (`git diff HEAD <branch> -- <file>` should show only your new edits), then `git checkout --theirs <file>`, `git add`, and confirm `git diff --cached --quiet <branch> -- <file>` is empty before committing.
- **Another session can move the release branch under you mid-slice** (verified 2026-09-15): a squash-landing conflicted because another change landed between my branch point and my landing. Procedure: `git merge --abort` does not work for `--squash` (no `MERGE_HEAD`); reset the release worktree with `git reset --hard HEAD` (the consent gate blocks chained destructive commands; ask the user explicitly, then run single-purpose commands), rebase the branch, then handle the near-certain test-module collision at EOF (both sessions append `#[cfg(test)] mod ...` at file end; reconstruct via `git show HEAD:<file>` plus `git show <commit>:<file>`, concatenate, write, `git add`, `git rebase --continue`). Then re-measure the WPT baseline on the new tip; your old baseline report is stale the moment the engine changes, and another session's `*-gate-base.json` is their pre-change measurement (that change itself flipped `inapplicable-properties-print.html` FAIL→PASS, so a stale baseline would have misread my +1 as +2). Measure by checking out the new tip in your worktree (detached), incremental build on your warm volume (~6s), harness run, then checkout back.

## Verification

Useful read-only checks are:

```bash
git status --short --branch
git worktree list
git fetch origin
git rev-parse HEAD
git merge-base --is-ancestor <tested-base> <target-tip>
git diff --stat <tested-tree> <landed-tree>
```

Run the project's tests and documentation checks again when the source tree changed after the original verification.

## References

- [Development guidelines](../conventions/development-guidelines.md)
- [Containerized development environment](../operations/dev-container.md)
