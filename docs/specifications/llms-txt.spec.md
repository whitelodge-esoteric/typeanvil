---
title: Docs site AI discovery — llms.txt and Markdown pages
type: spec
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
slug: /specifications/llms-txt
sidebar_position: 60
tags: [docs, docusaurus, llms-txt, ai-discovery]
spec_id: llms-txt
issue_id: CORE-507
applies_to:
  - docsite/plugins/llms-txt/index.js
  - docsite/docusaurus.config.ts
  - scripts/check_llms_txt.py
dependencies: []
---

# Docs site AI discovery

Agents read clean Markdown better than rendered HTML. The docs site shall
publish `llms.txt`, `llms-full.txt` and a Markdown copy of each page.

## Goals

1. Publish an index agents find at a known path.
2. Publish all page text in one file.
3. Publish each page as plain Markdown.

## Non-Goals

- Search, ranking, or a hosted-cloud docs index.
- Rewriting MDX or links inside page bodies.

## Behavior

1. A local Docusaurus plugin (`docsite/plugins/llms-txt`) shall run in
   `postBuild`. No third-party plugin is used: the output is small and the
   plugin reads the docs plugin's loaded content directly.
2. The plugin shall cover every page the docs plugin publishes. Draft and
   unlisted pages are excluded by the docs plugin.
3. For each page, the plugin shall write `<permalink>.md` (the home page
   writes `/index.md`). The file is the source Markdown with frontmatter
   removed. If the first lines have no H1, the page title is added as H1.
4. `/llms.txt` shall open with the H1 `# TypeAnvil` (the product name; the
   binary stays lowercase `typeanvil`) and a one-paragraph summary, then a
   when-to-use line, a not-a-fit line, and a link to the install guide
   (`/guides/getting-started.md`) and a link to the agent authoring guide
   (`/guides/agent-authoring.md`, generated from the capability manifest,
   CORE-580). It then lists one link per page, grouped
   by top-level section. A page description appears only when the page sets
   it in frontmatter and it ends in sentence punctuation. Descriptions the
   docs plugin derives from body text are not used.
5. `/llms-full.txt` shall hold every page's Markdown, in permalink order,
   each preceded by a `Source:` comment with the page URL.
6. `scripts/check_llms_txt.py` shall fail when any artifact is missing, when
   a page in `sitemap.xml` has no `.md` copy, no link in `llms.txt`, or no
   text in `llms-full.txt`, or when frontmatter remains in a copy.
   Generated tag and blog index pages are not docs pages and are skipped.

## Acceptance Criteria

1. Given a build, when `npm run build` finishes, then `llms.txt`,
   `llms-full.txt` and one `.md` per docs page exist in `docsite/build/`.
   Test: `python3 scripts/check_llms_txt.py` exits 0.
2. Given a build missing a page copy or an artifact, the check exits 1.
   Test: `tests/test_llms_txt.py`.

## References

- CORE-507
- `docs/operations/docs-site.md`
