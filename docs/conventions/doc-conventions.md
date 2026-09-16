---
title: Documentation Conventions
type: convention
status: approved
owner: elijah
created: 2026-08-16
updated: 2026-09-16
sidebar_position: 1
tags: [docs, meta]
---

# Documentation Conventions

These are the rules for `docs/`. Every doc in this directory follows them. If a
rule feels wrong, change the rule in a PR — don't quietly break it.

## Why docs exist

The docs are the intermediary between humans and machines:

- Humans read them to understand how Typeanvil works.
- AI agents read them to implement features correctly.
- The spec is the **source of truth** for behavior — above code comments, above
  memory, above lore.

## What goes where

| Folder | Purpose | Example |
|---|---|---|
| `conventions/` | How we work: doc rules, code style, git workflow | `doc-conventions.md` |
| `specifications/` | One spec per feature; the implementation contract | `line-breaking.spec.md` |
| `architecture/` | How the system fits together, and the why of big decisions | `overview.md` |
| `operations/` | Runbooks: release, deploy, tooling, incidents | `release.md` |
| `lessons/` | Things that didn't work, and what to do instead | `stylo-orphan.md` |
| `research/` | Research done by agents, categorized by directory | `layoutng-fragmentation/brief.md` |

## The golden rules

1. **Docs first.** To understand a feature, read its spec before the code. To
   release, read the runbook. Docs are the first reference for humans and AI
   alike.
2. **Specs are the source of truth.** When spec and code disagree, one of them
   is wrong. Fix it in a PR — never let code silently drift from an approved
   spec.
3. **Same-PR sync.** A PR that changes behavior MUST update the spec for that
   behavior in the same PR. Docs are part of the definition of done: a feature
   without a spec is not done.
4. **One spec per feature.** `specifications/<feature>.spec.md`. One feature,
   one file, one source of truth.
5. **Frontmatter on everything.** Every `.md` file carries the frontmatter from
   `frontmatter-schema.md`. Bump `updated` on every edit.
6. **Never delete a lesson.** Lessons are records. If one is wrong, mark it
   `superseded` — don't remove it.
7. **Keep scope tight.** User-changeable configuration and transient AI prompts
   do NOT belong in docs.

## What is out of scope

- **User-changeable configuration** → product documentation / in-app help. Not
  here.
- **Transient AI task prompts** → `prompts/` at the repo root.
- **Strategy, market, and product decisions** → Obsidian vault
  (`brain/Projects/Typeanvil/`). The vault is strategy; `docs/` is engineering
  truth. Research reports produced by agents go in `docs/research/` (below),
  not the vault.
- **Personal notes and scratch** → vault.

## Document lifecycle

Every doc has a `status`:

- `draft` — work in progress; not a reliable reference.
- `in-review` — being reviewed; close to done.
- `approved` — normative. The default for anything merged to main.
- `superseded` — replaced by a newer doc. Kept for history; link the replacement.

Rules:

- Code may only merge against specs that are `approved` (or `in-review` with an
  owner actively approving it).
- A doc that is no longer current becomes `superseded`, never deleted.
- `updated` changes on every edit, in the same commit as the edit.

## Writing a spec (`specifications/`)

A spec has two audiences: humans who want to understand, and AI agents who
implement. Serve both:

- **Human first:** plain English, short sentences, lead with the point.
  Diagrams (Mermaid) where they clarify.
- **Machine second:** stable section headings (below), testable statements,
  explicit examples.
- Use **"shall"** for normative statements: "The engine shall wrap at word
  boundaries." Everything else is explanation.
- One spec = one feature. If a spec grows a second feature, split it.
- Specs live in `specifications/` and are named `<feature>.spec.md`. Each spec
  carries an explicit `slug` frontmatter — a URL ending in `.spec` breaks the
  Docusaurus site (see `frontmatter-schema.md`). Both rules are enforced by the
  validator.

Canonical spec structure, in order:

1. `## Overview` — what the feature is, in one paragraph.
2. `## Goals / Non-Goals` — what it does, and deliberately doesn't do.
3. `## Behavior` — numbered, testable "shall" statements. This is the contract.
4. `## Interfaces` — public signatures: CLI flags, functions, config keys.
5. `## Acceptance Criteria` — Given/When/Then or a checklist; each item maps to
   a test in the harness.
6. `## Edge Cases` — empty input, huge input, invalid input.
7. `## References` — related specs, issues (e.g. `CORE-49`), lessons, runbooks.

AI agents implementing from a spec MUST:

- read `doc-conventions.md` + the spec before writing code;
- implement to the acceptance criteria;
- flag any conflict between spec and existing code instead of silently
  resolving it;
- update the spec (or raise the conflict) in the same PR.

## Writing a runbook (`operations/`)

Numbered steps, nothing clever:

1. `## When to Run` — trigger and frequency.
2. `## Prerequisites` — tools, access, environment.
3. `## Steps` — numbered, copy-pasteable commands.
4. `## Verification` — how you know it worked.
5. `## Rollback` — how to undo it.
6. `## Troubleshooting` — known failure modes, drawn from lessons.

No release is "done" until its runbook exists. Write the runbook before the
first release, not after the second one goes wrong.

## Writing a lesson (`lessons/`)

Write one when something fails, surprises, or costs time. Format:

1. `## Context` — what we were doing.
2. `## What We Tried` — the approach.
3. `## Why It Failed` — root cause, honestly.
4. `## What To Do Instead` — the working approach.

Rules:

- File name: `lessons/<short-slug>.md` (kebab-case, no dates).
- Tag generously (`engine`, `css`, `stylo`, `harness`, `release`, ...) so
  lessons surface in search.
- Link lessons from specs and runbooks when relevant ("see `lessons/...`").
- Never delete. If the advice changes, mark `superseded` and link the
  replacement.
- Lessons are factual records; they start at `status: approved` (they happened).

## Writing research (`research/`)

Any research performed by an agent or subagent lands in `docs/research/`,
categorized by directory — one directory per research area:

```
docs/research/<area>/<slug>.md
```

Existing areas: `wpt-harness/`, `rust-ecosystem/`, `layoutng-fragmentation/`.
Start a new directory when a study covers a new area.

Format:

1. `## Question` — what we wanted to know, and why.
2. `## Method` — sources consulted, how claims were checked.
3. `## Findings` — what we learned, with citations.
4. `## Implications` — what this means for Typeanvil's design or roadmap.
5. `## Sources` — the links, as a list.

Rules:

- Research is a record: `status: draft` while in progress, `approved` once the
  study is complete. Never delete; `superseded` when a newer study overturns it.
- Cite everything. A claim without a source is an opinion.
- Keep the file self-contained — an agent that reads it later has no other
  context.

## Writing conventions (`conventions/`)

Same rules as specs, but for how we work: code style, git workflow, review
standards. The doc you're reading is one. Conventions change by PR, like
everything else.

## AI agents and these docs

A Typeanvil rule, not a suggestion: an AI agent asked to implement, change,
debug, or release Typeanvil MUST load the relevant docs first — conventions,
then the spec or runbook — and treat them as authoritative. `prompts/` is for
composing task prompts; `docs/` is the ground truth those prompts point at.
Research an agent produces goes into `docs/research/`, categorized by
directory, with frontmatter.

## Enforcement

`scripts/validate_docs.py` enforces the frontmatter schema and the spec
naming/slug rules. It runs in pre-commit (`pre-commit run --all-files`) and in
CI (`.github/workflows/docs-validation.yml`), so a doc that breaks a convention
cannot be committed or pushed. `AGENTS.md` at the repo root points agents at
these conventions before they touch anything.

The validator requires PyYAML and refuses to run without it (exit 2). It must
not fall back to a partial check, because a skipped YAML parse reports a false
pass on frontmatter that CI rejects. The pre-commit hook provisions PyYAML
itself (`language: python` with `additional_dependencies`), so the check does
not depend on whichever python happens to be on PATH.
