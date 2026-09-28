---
title: Use the Typeanvil Container Image
type: runbook
status: draft
owner: maintainers
created: 2026-09-27
updated: 2026-09-28
sidebar_position: 9
tags: [container, docker, distribution, usage]
trigger: When rendering HTML to PDF without a native binary
---

# Use the Typeanvil container image

Use a published Typeanvil image when Docker is available and a native binary is not installed.

## Prerequisites

- Docker with `docker pull` and `docker run` support.
- A public image registry location and tag supplied by the project release.

## Steps

Pull the release image:

```bash
docker pull REGISTRY/OWNER/typeanvil:TAG
```

Replace `REGISTRY/OWNER` and `TAG` with values from the public release instructions. Do not copy a private registry owner into public documentation.

Render from a working directory mounted at `/work`. The render command requires a page size and the output must be writable by the image's non-root user:

```bash
docker run --rm -v "$PWD:/work" \
  REGISTRY/OWNER/typeanvil:TAG \
  render /work/in.html --page-width 8.5in --page-height 11in -o /work/out.pdf
```

Mount additional fonts read-only when the image documentation for that release supports the font path:

```bash
docker run --rm \
  -v "$PWD:/work" \
  -v "$HOME/.fonts:/usr/share/fonts/user:ro" \
  REGISTRY/OWNER/typeanvil:TAG \
  render /work/in.html --page-width 8.5in --page-height 11in -o /work/out.pdf
```

Check the image version:

```bash
docker run --rm REGISTRY/OWNER/typeanvil:TAG --version
```

## Verification

The render exits 0 and writes a readable PDF at `/work/out.pdf`. The version output matches the selected release tag or the release metadata.

## Rollback

Run an earlier public image tag. The container has no persistent application state.

## Troubleshooting

- **Image cannot be pulled:** check the public release registry and tag.
- **Input cannot be opened:** confirm that the working directory is mounted at `/work` and that the input path uses `/work`.
- **Output permission denied:** the container runs as a non-root user; make the mounted output directory writable by it (e.g. `chmod 777 "$PWD"` or an output dir owned by the container uid).
- **Fonts are missing:** use the font mount documented for the image release and verify the family name in the document.
- **Version does not match the tag:** report the release-image mismatch and use a matching image.
