# Issue pickup skill corrections

This directory preserves the exact workflow edits made during the issue-pickup
optimization audit. The installed skills live outside the Typeanvil Git repository.
Committing this patch does not install or reload a skill.

## Contents

- `issue-pickup.patch`: changes to `software-development/typeanvil-issue-loop/SKILL.md`
  and `software-development/typeanvil-project/SKILL.md`, relative to a profile's
  `skills/` directory. It includes only the audit's changes and diff context.
- `issue-pickup.manifest.json`: SHA-256 identities before and after those edits.
- The [audit summary](../../docs/research/development-workflow/issue-pickup-optimization.md)
  records the portable findings and remaining proposals.

## Scope and adoption

The edits are already installed in the `mastermind-typeanvil-core` profile.
Other profiles were not changed. The Obsidian strategy note remains outside Git;
the repository research document preserves the reusable audit conclusions.
No full profile, unrelated skill history, credentials, or conversation transcript
is included here.

Use the supported skill-edit tool to port these changes into another profile
only with authorization for that profile. Read its current skill first. Review
the patch and reconcile any newer rules; never replace a skill wholesale with a
historical version. If both after hashes match, the patch is already installed.

For an isolated compatibility check, create a scratch copy of the two target
files with the same relative directory layout, then run from that scratch root:

```sh
git apply --check /absolute/path/to/issue-pickup.patch
git apply /absolute/path/to/issue-pickup.patch
```

Compare the result with the manifest. A context mismatch requires review. Do not
force a failed patch or apply it directly to another profile as a side effect of
checkout. The audit verified forward and reverse application in temporary copies.

## Behavioral changes

- Claim the issue when investigation begins and verify its state.
- Pin an explicit release source commit before worktree creation.
- Use Docker paths and the actual background process result for build completion.
- Keep reproduction output under the persistent worktree mount.
- Initialize semantic search on demand.
- Treat pickup reproduction and premise review as one gate when inputs match.
- Stop broad research when a bounded fix has sufficient causal evidence.
- Reuse documented probe interfaces and turn useful probes into regression checks.
- Route the project skill to the issue-loop procedure.

The patch does not complete the broader skill cleanup. Historical commands in
later sections still require reconciliation with the current Docker and release
runbooks. It does not add the proposed pickup command or shared-cache tooling.
