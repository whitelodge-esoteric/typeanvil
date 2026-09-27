---
title: Use the Typeanvil Container Image
type: runbook
status: draft
owner: elijah
created: 2026-09-27
updated: 2026-09-27
sidebar_position: 9
tags: [container, docker, distribution, usage]
trigger: When a user wants to render HTML to PDF without installing a binary
issue_id: CORE-247
---

# Use the Typeanvil container image

The engine ships as a container image so users render without installing a
binary. The image wraps the release binary unchanged and bundles an
open-source font set so fallback text renders correctly on Linux.

## When to run

Use the container when you want to render an HTML file to a PDF and you have
Docker but not a Typeanvil binary installed.

## Prerequisites

- Docker with buildx (Docker Desktop includes it).
- The image is published to GHCR as `ghcr.io/<owner>/typeanvil:<tag>`.
  Replace `<owner>` with the GitHub owner of the repo (e.g.
  `whitelodge-esoteric` while the repo is private).

## Steps

### 1. Pull the image

```bash
docker pull ghcr.io/<owner>/typeanvil:<tag>
```

### 2. Render a document

Mount your working directory at `/work` and pass the input/output paths
relative to it:

```bash
docker run --rm -v "$PWD:/work" \
  ghcr.io/<owner>/typeanvil:<tag> \
  render /work/in.html -o /work/out.pdf
```

The CLI contract is identical to the native binary, exit codes included.

### 3. Use your own fonts

Mount a font directory at `/usr/share/fonts/user` (read-only). The engine
discovers it automatically and gives your fonts precedence over the bundled
set:

```bash
docker run --rm \
  -v "$PWD:/work" \
  -v "$HOME/.fonts:/usr/share/fonts/user:ro" \
  ghcr.io/<owner>/typeanvil:<tag> \
  render /work/in.html -o /work/out.pdf
```

### 4. Check the version

```bash
docker run --rm ghcr.io/<owner>/typeanvil:<tag> --version
# prints: typeanvil <tag>
```

## Verification

- The render exits 0 and writes a valid PDF at the output path.
- With no user fonts mounted, fallback text still renders (the bundled
  Liberation set is used — never empty font bytes).
- With a user font mounted, a document that names that family renders with
  it (user fonts take precedence over the bundled set).

## Rollback

The image is immutable and tagged by version. To roll back, pull and run an
earlier tag. There is no state inside the container to revert.

## Troubleshooting

- **`--version` prints a different version than the tag** — the image was
  built from a mismatched binary. Rebuild from the matching release artifact.
- **Fallback text is missing or broken with no user fonts** — the bundled
  font set is absent from the image. Rebuild the image with
  `docker/fonts/typeanvil/` present.
- **A user font does not take effect** — confirm the mount path is exactly
  `/usr/share/fonts/user` and the document names the family. fontdb scans
  `/usr/share/fonts` on Linux; a mount elsewhere is not discovered.
- **The container runs as root** — the image's default user must be a
  numeric non-root uid. Rebuild from `docker/Dockerfile` (distroless
  nonroot base).
