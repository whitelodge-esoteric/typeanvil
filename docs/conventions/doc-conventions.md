---
title: Documentation Conventions
type: convention
status: approved
owner: maintainers
created: 2026-08-16
updated: 2026-09-27
sidebar_position: 1
tags: [docs, meta, public]
---

# Documentation Conventions

Repository documentation serves users, contributors, and AI agents with access
to the repository and public internet. It covers the engine and its tooling.

## What belongs here

Keep installation and usage instructions, current architecture, lasting design
rationale, feature contracts, tests, reusable runbooks, and cited technical
research. A document belongs here when it helps a reader use, understand, test,
or change Typeanvil.

Keep temporary investigations, work queues, raw run records, agent prompts,
private coordination, and hosted-service strategy in an external note store.
A narrow topic can remain here when it explains a lasting constraint or a test.

## Public references

- Link to public sources or repository-relative files with descriptive names.
- Explain each decision without requiring a private tracker or note vault.
- Replace private issue references with the behavior, test, source, or design
  document that readers need. Do not replace them with vague placeholders.
- Keep literal source identifiers and working commands accurate. Existing test
  names may contain historical issue numbers; do not rename code during a docs pass.
- Avoid personal absolute paths. Define prerequisites and configurable paths.
- Keep essential public evidence accessible when archiving an investigation.

## Documentation map

| Directory | Purpose |
|---|---|
| `architecture/` | Current composition, module boundaries, and design rationale |
| `specifications/` | Behavior contracts and observable acceptance criteria |
| `operations/` | Reproducible usage, testing, and release procedures |
| `conventions/` | Contributor and documentation rules |
| `lessons/` | Reusable engineering lessons |
| `research/` | Self-contained studies that support current technical decisions |

## Spec-driven development

1. Write a feature specification before or in the same change as implementation.
2. Follow its approved behavior and acceptance criteria.
3. Update the specification in the same change as behavior changes.
4. Run the mapped tests and documentation checks.

CSS standards take precedence over comparison-engine behavior. Read
[CSS standards alignment](css-standards-alignment.md). If code and a
specification disagree, identify the conflict. Describe observed implementation
separately from required behavior until the conflict is resolved.

Specifications use `specifications/<feature>.spec.md` and a unique `spec_id`.
Use these headings: Overview, Goals / Non-Goals, Behavior, Interfaces, Acceptance
Criteria, Edge Cases, and References. Use `shall` for requirements. Map acceptance
criteria to observable outcomes and tests. Label proposed behavior clearly.

## Lifecycle and preservation

Every typed document follows the [frontmatter schema](frontmatter-schema.md).
Keep `created` unchanged and bump `updated` with each edit.

- `draft`: proposed or incomplete content.
- `in-review`: content awaiting approval.
- `approved`: reviewed for its stated scope. An approved study is evidence;
  normative behavior remains in specifications.
- `superseded`: public history that links to its replacement.

Approval status does not establish that a feature has shipped. State current
implementation and limitations explicitly.

Review obsolete documents for continuing public value. Preserve useful history
in the external archive before removing it. Extract lasting conclusions into
current docs and repair incoming links. Keep a public evidence record when tests
or technical claims depend on it. Avoid maintaining duplicate sources of truth.

## Research and lessons

A study states its question, method, findings, implications, and public sources.
Date measurements and name the tested revisions and environment. Do not present
old benchmark results, ecosystem versions, or temporary diagnoses as current.

A lesson states the failure mode, its cause when proved, and the reusable
practice. Remove incident-specific coordination after preserving the history.

## Runbooks

State when to run the procedure, prerequisites, commands, verification,
rollback where applicable, and troubleshooting. Check commands against the
current tool interface. Explain required publishing privileges without assuming
that all contributors hold them.

## AI agents

Read this convention and the relevant specification or runbook before editing.
Use stable headings, plain English, explicit examples, and links that work from
the public repository. Keep task prompts and scratch outside tracked content.
Do not treat an old note or a test-pair match as proof of current correctness.

## Verification

Run `python3 scripts/validate_docs.py` with PyYAML installed, the configured
pre-commit checks, and `npm run build` from `docsite/`. Inspect the rendered
navigation and diagrams. Check incoming links to moved pages. Inspect the rendered
navigation and diagrams. Check private references and incoming links to moved
pages. The validator checks structure; it does not prove technical claims.
