# AGENTS.md — Typeanvil

Typeanvil is an AI-first HTML/Markdown → PDF typesetting engine. Rust engine in
`engine/`, Python WPT conformance harness in `harness/`, docs in `docs/`.

## Read this first

1. `docs/README.md` — map of the docs.
2. `docs/conventions/doc-conventions.md` — the rules of the house.
3. `docs/conventions/frontmatter-schema.md` — the frontmatter contract.

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
- Transient task prompts → `prompts/` at the repo root, never `docs/`.
- Write human-first (plain English, short sentences, lead with the point),
  machine-parseable second (stable section headings, "shall" for normative
  statements).

## Enforcement

`scripts/validate_docs.py` enforces all of the above. It runs in pre-commit
(`pre-commit run --all-files`) and CI (`.github/workflows/docs-validation.yml`),
so a doc that breaks a convention cannot be committed or pushed. Run it
yourself after touching `docs/`.

## Workflow conventions

- Work is tracked in Linear (team Core, project Typeanvil, issue ids like
  CORE-57). Reference the issue id in commit messages: `feat: ... (CORE-57)`.
- Note the model tier on Linear issues when delegating (`Model: deepseek` by
  default; opus-class only for architecture/engine work where a mistake is
  expensive).
- Docs site: `cd docsite && npm run build` (Docusaurus; content lives in
  `../docs`, never edit `docsite/docs/`).
