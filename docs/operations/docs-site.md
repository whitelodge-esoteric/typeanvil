---
title: Running the Docs Site
type: runbook
status: approved
owner: maintainers
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 1
tags: [operations, docusaurus, docs]
trigger: when you want to browse or deploy the docs site
---

# Running the Docs Site

The docs site is a Docusaurus app in `docsite/` that serves the repo's
`docs/` directory (the single source of truth) as an interactive site. The
docs content lives in `docs/`; the site is just a renderer — edit markdown in
`docs/`, never in `docsite/`.

## When to Run

- Browse docs interactively (sidebar, search, rendered Mermaid diagrams).
- Preview changes before committing docs.

## Prerequisites

- Node ≥ 20.
- Dependencies installed: `cd docsite && npm install`.

## Steps

1. Install dependencies (first time, or after package.json changes):

   ```bash
   cd docsite
   npm install
   ```

2. Start the dev server:

   ```bash
   npm run start
   ```

   Serves at `http://localhost:3000`. Edits to `docs/**` hot-reload.

3. Build a production bundle:

   ```bash
   npm run build
   ```

   Output goes to `docsite/build/` (gitignored). The build fails on broken
   links, so it doubles as a docs sanity check.

4. Serve the built bundle:

   ```bash
   npm run serve
   ```

## Verification

- `npm run build` exits 0.
- `http://localhost:3000` shows the docs home, with the categories in the
  sidebar (Conventions, Specifications, Architecture, Operations, Lessons,
  Research).
- Mermaid diagrams in docs/README.md render.

## Rollback

Nothing is deployed — this is a local/static site. To revert, reset the
content (`git checkout -- docs/`), not the site.

## Troubleshooting

- **Build fails with a broken-link error** — a doc references a missing file.
  Fix the link (or the target); do not relax `onBrokenLinks`.
- **Mermaid diagrams don't render** — the theme needs `@docusaurus/theme-mermaid`
  in package.json dependencies and `themes: ['@docusaurus/theme-mermaid']` plus
  `markdown: { mermaid: true }` in `docusaurus.config.ts`.
- **Docs changes don't appear** — make sure the edit landed in `docs/` at the
  repo root, not `docsite/docs/` (the template dir was deleted).
