---
title: Frontmatter Schema
type: convention
status: approved
owner: elijah
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 2
tags: [docs, meta]
---

# Frontmatter Schema

Every `.md` file under `docs/` starts with YAML frontmatter (between `---`
lines). Docusaurus renders `title` and uses `sidebar_position` for ordering; the
custom fields (`type`, `status`, ...) are preserved by Docusaurus and are what
make the docs AI-parseable.

## Required fields (all docs)

| Field | Type | Meaning |
|---|---|---|
| `title` | string | Human title. Docusaurus page title. |
| `type` | enum | `spec` \| `architecture` \| `lesson` \| `runbook` \| `convention` \| `research` |
| `status` | enum | `draft` \| `in-review` \| `approved` \| `superseded` |
| `owner` | string | Who keeps it accurate (GitHub handle or name). |
| `created` | date | `YYYY-MM-DD`. Never changes. |
| `updated` | date | `YYYY-MM-DD`. Bump on every edit, same commit. |
| `sidebar_position` | int | Ordering within its Docusaurus category. |
| `tags` | list | Lowercase kebab tags, e.g. `[engine, css, stylo]`. |

## Fields added per type

**`spec`** adds:

| Field | Type | Meaning |
|---|---|---|
| `spec_id` | string | Stable id, e.g. `line-breaking`. Unique across specs. |
| `issue_id` | string | Tracking issue, e.g. `CORE-49`. |
| `applies_to` | string | Component + scope, e.g. `engine 0.0.x`. |
| `dependencies` | list | Specs this one builds on. |
| `supersedes` | string | Spec id this one replaces. |

**`runbook`** adds:

| Field | Type | Meaning |
|---|---|---|
| `trigger` | string | "When to run", e.g. `on tagged release`. Also stated in the body. |

**`lesson`** and **`research`**: no extra fields; `status` starts at `approved`
once complete — they are records of what happened. In-progress research may be
`draft`.

**`architecture`** and **`convention`**: no extra fields.

## Docusaurus specifics

- File names are kebab-case: `line-breaking.spec.md` → URL
  `/docs/specifications/line-breaking.spec`. No dates or spaces in file names.
- **Spec slugs end in a dot-suffix.** `.spec.md` produces a URL ending in
  `.spec`, which Docusaurus (and some static hosts) treat as a file extension
  and fail to serve as a clean route. Give every spec an explicit slug without
  the suffix: `slug: /specifications/wpt-conformance-harness`.
- `_category_.yml` in each folder sets the sidebar label and position:

```yaml
label: Specifications
position: 2
collapsible: true
collapsed: false
```

- `unlisted: true` hides a doc from the sidebar (still URL-accessible) — good
  for WIP.
- `draft: true` hides it from the build entirely.
- Custom fields are preserved in the page's frontmatter and can be queried by
  tooling.

## Examples

**Spec:**

```yaml
---
title: Knuth-Plass Line Breaking
type: spec
status: approved
owner: elijah
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 1
tags: [engine, typography, line-breaking]
spec_id: line-breaking
issue_id: CORE-52
applies_to: engine 0.0.x
dependencies: []
---
```

**Lesson:**

```yaml
---
title: Orphaned Stylo Trait Impls
type: lesson
status: approved
owner: elijah
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 1
tags: [engine, css, stylo, spike]
---
```

**Runbook:**

```yaml
---
title: Release Procedure
type: runbook
status: draft
owner: elijah
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 1
tags: [operations, release]
trigger: on tagged release
---
```

**Research:**

```yaml
---
title: Rust Typesetting Architecture Brief
type: research
status: approved
owner: elijah
created: 2026-08-14
updated: 2026-08-16
sidebar_position: 1
tags: [rust, ecosystem, typst, build-vs-wrap]
---
```

## Rules

- No doc without frontmatter. A `.md` file missing it is a review failure.
- No invalid enum values.
- `updated` is the date of the last substantive change, bumped in the same
  commit as the change.
- Every spec has a unique `spec_id`.
- `_category_.yml` files need no frontmatter.
- The category index page (`docs/README.md`) only needs `title` and
  `sidebar_position` — it is Docusaurus's site home, not a typed doc.
- `scripts/validate_docs.py` checks all of this; run it before pushing.
