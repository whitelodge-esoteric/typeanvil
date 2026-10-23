---
title: Showcase Publish
slug: /specifications/showcase-publish
type: spec
status: draft
owner: maintainers
created: 2026-10-17
updated: 2026-10-23
sidebar_position: 7
tags: [showcase, release, github-pages, publish]
spec_id: showcase-publish
issue_id: CORE-494
applies_to: release 2026.10
dependencies: [showcase-render, release-ci]
---

# Showcase Publish

## Overview

Rendered showcase output stays out of the engine repo. On each release, a workflow renders the showcase documents and publishes them to the public repo `whitelodge-esoteric/typeanvil-showcase`. GitHub Pages serves the same files.

## Behavior

1. The workflow `.github/workflows/showcase.yml` shall run through a `workflow_run` trigger when the `release` workflow completes with conclusion `success` on a `v*` tag (tag = `github.event.workflow_run.head_branch`), and on manual dispatch with a `tag` input. It shall not use a `release:` trigger: `release.yml` creates the release with `GITHUB_TOKEN`, and events made by that token start no workflows.
2. The workflow shall render the showcase documents with the release binary of that tag, using `scripts/build-showcase.sh`.
3. `scripts/showcase_publish.py` shall write `versions/<tag>/<doc>/README.md` with inline page images, a TypeAnvil and Prince side-by-side table per document, and `versions/<tag>/README.md` as the version index.
4. Published images shall be at a DPI between 150 and 200 (default 150). Source pages are 300 DPI and are downscaled; they are never upscaled.
5. The tag shall match `vYYYY.M.PATCH`. Tags sort numerically, so `v2026.10.0` is newer than `v2026.9.0`.
6. `latest/` shall be a copy of the newest full version. It is a copy, because GitHub Pages does not follow symlinks.
7. `perf/<tag>.json` shall record tag, commit, DPI, pages per document, total pages, and render seconds. `perf/README.md` and `perf/chart.svg` shall be rebuilt from all JSON files.
8. Retention: the newest N versions (default 5) keep full-size pages. Older versions keep 240 px wide TypeAnvil thumbnails only. Perf JSON is never removed.
9. A re-run for the same tag shall replace that tag's output and no other version.
10. The workflow shall push with a write deploy key stored in the secret `SHOWCASE_DEPLOY_KEY`. When the secret is absent, the job shall skip with a notice and stay green.
11. The first publish shall work on an empty target repo that has no default branch; it creates `main`.
12. The engine `README.md` shall link to the showcase repo and its Pages site.
13. (CORE-529) A document with more than `MAX_PAGES` pages (default 10, flag `--max-pages`) shall publish PNGs for pages 1 to `MAX_PAGES` only, for both engines. Its page shall say `Showing 10 of N pages` and link `full-typeanvil.pdf` and `full-prince.pdf`, which are published beside the images. The version index shall show `10 of N`. Perf JSON keeps the real page count (`pages`) and adds `published_pages`. No page shall link to an unpublished file. Documents with `MAX_PAGES` pages or fewer are unchanged.
14. (CORE-529) The full PDFs are read from `--pdfs` (default `corpus/out/showcase/.work`, kept by `build-showcase.sh --keep-work`). Versions past the retention window drop the PDFs with the full-size pages.

## Decisions

- DECIDED 2026-10-17 (agent): trigger on `workflow_run` of `release`, separate workflow file. A `release: published` trigger never fires for a release made with `GITHUB_TOKEN`, and it would race the tarball upload. `workflow_run` fires after all release jobs end. It keeps the `release.yml` build matrix unchanged and lets a failed publish re-run alone.
- DECIDED 2026-10-17 (agent): deploy key, not a GitHub App. One repo, one write scope, no app to maintain.
- DECIDED 2026-10-17 (agent): default retention 5 versions, 150 DPI.

- DECIDED 2026-10-20 (agent): cap at 10 pages per document, publish full PDFs. A local build of one version was 737 MB (1,487 PNGs); every release adds permanently to git history. With the cap one version is 64 MB.

## Acceptance Criteria

1. Given a fixture images tree and a tag, when `showcase_publish.py` runs, then the version tree, `latest/`, perf JSON, and chart exist (`tests/test_showcase_publish.py::test_publish_builds_tree_and_latest`).
2. Given more versions than N, then older ones hold thumbnails only and keep perf JSON (`test_retention_turns_old_versions_into_thumbnails`).
3. Given `--dpi 300` or a non-CalVer tag, then the script exits 2 (`test_cli_rejects_dpi_out_of_range_and_bad_tag`).
4. Given a real tag, when the release is published, then `versions/<tag>/` appears in the showcase repo.
5. Given `showcase.yml`, then it triggers on `workflow_run` of `release` and has no `release:` trigger (`tests/test_showcase_publish.py::test_workflow_trigger_is_workflow_run`). Criterion 4 needs the secret and is verified by the owner on the first tag.
6. (CORE-529) Given a 12-page document and `--max-pages 10`, then only pages 1 to 10 exist, both full PDFs exist and are linked, and every relative link resolves (`tests/test_showcase_publish.py::test_long_document_is_capped_and_links_full_pdfs`). Short documents are unchanged (`test_short_document_unchanged`, `test_exactly_max_pages_is_not_capped`).

## Non-Goals

- Pushing to main or release branches of the engine repo.
- Custom Pages theme work beyond `_config.yml`.
