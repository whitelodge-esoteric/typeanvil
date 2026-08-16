# docsite

Docusaurus site that renders the repo's `docs/` directory (the single source
of truth) as an interactive site.

The content is **not** here — it lives in `../docs`. This package is just the
renderer: config, theme, and build tooling.

```bash
npm install
npm run start   # dev server on :3000
npm run build   # production bundle → build/ (gitignored)
```

See `../docs/operations/docs-site.md` for the runbook.
