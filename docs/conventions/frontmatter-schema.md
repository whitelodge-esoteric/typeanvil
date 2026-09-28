---
title: Frontmatter Schema
type: convention
status: approved
owner: maintainers
created: 2026-08-16
updated: 2026-09-27
sidebar_position: 2
tags: [docs, metadata]
---

# Frontmatter schema

Every Markdown file under `docs/` starts with YAML frontmatter between `---` lines. Docusaurus uses `title` and `sidebar_position`; the remaining fields make documents consistent and machine-parseable.

## Required fields

| Field | Type | Meaning |
|---|---|---|
| `title` | string | Human-readable page title. |
| `type` | enum | `spec`, `architecture`, `lesson`, `runbook`, `convention`, or `research`. |
| `status` | enum | `draft`, `in-review`, `approved`, or `superseded`. |
| `owner` | string | Maintainer or team responsible for accuracy. |
| `created` | date | `YYYY-MM-DD`; never changes. |
| `updated` | date | `YYYY-MM-DD`; bump on every edit. |
| `sidebar_position` | integer | Ordering within the category. |
| `tags` | list | Lowercase kebab-case tags. |

The docs home `docs/README.md` is the Docusaurus index and is exempt from the typed-document fields. It requires `title` and `sidebar_position`.

## Fields by type

**`spec`** documents add:

| Field | Type | Meaning |
|---|---|---|
| `spec_id` | string | Stable unique specification ID. |
| `issue_id` | string | Optional public tracking reference when one exists. |
| `applies_to` | string | Component and scope. |
| `dependencies` | list | Related specifications. |
| `supersedes` | string | Replaced specification ID. |
| `slug` | string | Route beginning `/specifications/` and not ending `.spec`. |

**`runbook`** documents add `trigger`, which states when to run the procedure.

Lessons and research records use `status: approved` after completion. Architecture and convention documents have no extra fields.

## Docusaurus specifics

- Use kebab-case file names without dates or spaces.
- Specifications use `<feature>.spec.md`, but their explicit `slug` omits the `.spec` suffix.
- Put `_category_.yml` in categories that need custom sidebar labels or ordering.
- `unlisted: true` hides a page from the sidebar while keeping its URL.
- `draft: true` hides a page from the build.

## Examples

```yaml
---
title: Line Breaking
type: spec
status: approved
owner: maintainers
created: 2026-08-16
updated: 2026-09-27
sidebar_position: 1
tags: [engine, typography]
spec_id: line-breaking
applies_to: engine
dependencies: []
slug: /specifications/line-breaking
---
```

```yaml
---
title: Containerized Development
type: runbook
status: approved
owner: maintainers
created: 2026-08-16
updated: 2026-09-27
sidebar_position: 1
tags: [docker, development]
trigger: when building or testing the engine
---
```

## Rules

- Do not add a document without frontmatter.
- Use only the listed enum values.
- Keep `created` stable and bump `updated` in the same change.
- Give every specification a unique `spec_id` and valid `slug`.
- Keep category indexes as the documented exception.
- Run `scripts/validate_docs.py` before review.
