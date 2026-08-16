# AGENTS.md — Typeanvil

Typeanvil is an AI-first HTML/Markdown → PDF typesetting engine. Rust engine in
`engine/`, Python WPT conformance harness in `harness/`, docs in `docs/`.

## Read this first

1. `docs/README.md` — map of the docs.
2. `docs/conventions/doc-conventions.md` — the rules of the house.
3. `docs/conventions/frontmatter-schema.md` — the frontmatter contract.

## The docs are the source of truth

- **Read the docs before you write code.** To implement or change a feature,
  read its spec in `docs/specifications/` first. To release, read the runbook
  in `docs/operations/`.
- When spec and code disagree, one of them is wrong — raise it in the PR, never
  let code silently drift from an approved spec.
- A PR that changes behavior MUST update the spec in the same PR. A feature
  without a spec is not done.

## Doc conventions (enforced — see below)

- Every `docs/**/*.md` carries frontmatter: `title, type, status, owner,
  created, updated, sidebar_position, tags`.
- `type` ∈ `spec | architecture | lesson | runbook | convention | research`.
- **Specs** live in `docs/specifications/`, are named `<feature>.spec.md`,
  declare `type: spec`, and MUST have a `slug` frontmatter that starts with
  `/specifications/` and does NOT end in `.spec` (a `.spec` URL suffix breaks
  the Docusaurus site).
- Research goes in `docs/research/<area>/`; runbooks in `docs/operations/`;
  lessons in `docs/lessons/` — never delete a lesson, supersede it.
- Transient AI task prompts go in `prompts/` at the repo root, never in `docs/`.

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
