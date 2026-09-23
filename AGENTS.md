# AGENTS.md — Typeanvil

Typeanvil is an AI-first HTML/Markdown → PDF typesetting engine. Rust engine in
`engine/`, Python WPT conformance harness in `harness/`, docs in `docs/`.

## Read this first

1. `docs/README.md` — map of the docs.
2. `docs/conventions/doc-conventions.md` — the rules of the house.
3. `docs/conventions/css-standards-alignment.md` — CSS spec wins over
   PrinceXML, always. Read before any parity or compatibility decision.
4. `docs/conventions/frontmatter-schema.md` — the frontmatter contract.

## How we work: spec-driven

Every feature starts as a spec; code follows the spec; tests prove the spec.

1. **Spec first.** New feature → write its spec in
   `docs/specifications/<feature>.spec.md` before (or in the same PR as) the
   code. No spec, no feature.
2. **Implement to the spec.** The spec's Behavior ("shall" statements) and
   Acceptance Criteria are the contract. If code and spec disagree, one of them
   is wrong — raise it in the PR, never silently drift from an approved spec.
3. **Same-PR sync.** A PR that changes behavior updates the spec in the same
   PR. Docs are part of "done", not an afterthought.
4. **Verify.** Run the tests mapped from the spec's acceptance criteria, then
   `python3 scripts/validate_docs.py`.

## How to handle documentation

- Read the relevant doc before coding; write or update docs as you go, not
  after the fact.
- Every `docs/**/*.md` carries frontmatter: `title, type, status, owner,
  created, updated, sidebar_position, tags`.
- `type` ∈ `spec | architecture | lesson | runbook | convention | research`.
- Lifecycle: `draft` → `in-review` → `approved` → `superseded`. Bump `updated`
  on every edit, in the same commit.
- **Specs** live in `docs/specifications/<feature>.spec.md`, declare
  `type: spec`, carry a unique `spec_id`, and MUST have a `slug` starting with
  `/specifications/` that does NOT end in `.spec` (a dot-suffix URL breaks the
  Docusaurus site).
- Research → `docs/research/<area>/` (cited, self-contained). Runbooks →
  `docs/operations/`. Lessons → `docs/lessons/` — never delete a lesson,
  supersede it.
- Transient task prompts and agent scratch never belong in the repository —
  compose them in worktree scratch or the vault, and keep them out of commits.
- Write human-first (plain English, short sentences, lead with the point),
  machine-parseable second (stable section headings, "shall" for normative
  statements).

## Issue evidence checks

Before implementation or follow-up creation, read
`docs/conventions/issue-evidence.md` and follow
`docs/operations/issue-evidence-review.md`.

- Read the full issue and all comments. Follow supersession links and check
  later landings and unmerged work before accepting a diagnosis.
- Record observed behavior, expected CSS behavior, and suspected cause
  separately. Confirm a cause with isolating evidence. Unknown causes remain
  valid investigation work.
- Before implementation, rebuild the current branch point and rerun the
  smallest reproduction. Record `git rev-parse HEAD`, fixture revision,
  command, runner settings, and durable results. Documentation-only tasks
  check current document/source evidence and skip engine reproduction.
- Before filing a residual, remeasure at the landed commit and record the
  lead's evidence/scope review. Keep same-capability boundary cases on the
  canonical ticket.
- Correct the active description when evidence changes. Preserve superseded
  explanations in marked history. Resolve conflicting specification rules
  before implementing their behavior. State acceptance criteria as observable
  results; keep an unproved implementation mechanism provisional.

## Enforcement

`scripts/validate_docs.py` checks frontmatter and spec naming/slug rules. It
runs in pre-commit (`pre-commit run --all-files`) and CI
(`.github/workflows/docs-validation.yml`). Run it after touching `docs/`.
Issue evidence checks require human/agent review; this validator does not
inspect Linear or prove diagnoses.

## Workflow conventions

- Work is tracked in Linear (team Core, project Typeanvil, issue ids like
  CORE-57). Reference the issue id in commit messages: `feat: ... (CORE-57)`.
- Note the model tier on Linear issues when delegating, keyed to expected effort:
  `Model: Qwen3 Coder 480B A35B (nous/qwen/qwen3-coder)` for high effort,
  `Model: DeepSeek V4.1 Flash (nous/deepseek/deepseek-v4.1-flash)` for normal,
  `Model: GLM 5.3 Flash (nous/z-ai/glm-5.3-flash)` for low. The `model-qwen` /
  `model-deepseek` / `model-glm` labels mirror the tiers.
- Docs site: `cd docsite && npm run build` (Docusaurus; content lives in
  `../docs`, never edit `docsite/docs/`).
- When an issue closes, delete the Docker assets that belong to it: its build
  volume (`docker volume rm dev-target-<worktree-name>`), any container left
  behind, then the worktree. Each build volume holds about 14 GB. Keep the
  shared cargo caches and the `typeanvil-dev` image. Preserve live work,
  unmerged candidates, and evidence needed by a replacement ticket before
  cleanup; supersession alone does not authorize their deletion. See
  `docs/operations/dev-container.md` and `docs/conventions/issue-evidence.md`.

## Writing style — agent responses

Use ASD-STE100 (Simplified Technical English) when applicable: short sentences,
one instruction per sentence, active voice, approved and simple vocabulary.

The following rhetorical patterns are forbidden in all responses:

- **Staccato pairs** — two short, punchy fragment sentences placed back to back
  for dramatic effect.
- **Antithesis reframe / negative parallelism** — "it's not X, it's Y" or
  "X, not Y" constructions that define something by first negating an
  alternative.
- **Isocolon metaphor-pairs** — parallel-structured pairs of metaphors or
  analogies (for example, mirrored "like a house with two wings" comparisons).
- **Backward-references** — phrases like "as noted above," "as discussed
  earlier," or "this finding" that point to earlier content in the same piece
  instead of just restating it.
