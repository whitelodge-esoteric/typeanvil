---
title: Containers and packages
type: guide
status: approved
owner: maintainers
created: 2026-10-14
updated: 2026-10-17
sidebar_position: 4
tags: [guide, container, docker, npm, distribution, fonts]
slug: /guides/container
---

# Containers and packages

You can run TypeAnvil without building the native binary yourself. The
release workflow packages the same release binary in two forms:

- a **container image** on the GitHub Container Registry (GHCR), for hosts
  with Docker;
- **npm packages** (`typeanvil` plus one package per platform), for Node.js
  and Bun projects.

Neither form adds engine logic. Both run the release binary unchanged, so the
command line, flags and exit codes match the [CLI reference](./cli.md). To
build the binary from source instead, see [Getting started](./getting-started.md).

| Form | Status (checked 2026-10-14) |
|---|---|
| Container image | Published. `ghcr.io/whitelodge-esoteric/typeanvil` has tags `2026.9.3`, `2026.9.4` and `2026.9.5`. |
| npm packages | **Not published yet.** `typeanvil` and `@typeanvil/*` return 404 from the npm registry. |

## Container image

### Image name and tags

The release workflow (`.github/workflows/release.yml`) pushes the image to
`ghcr.io/<repository owner>/typeanvil`. For the public repository that is:

```text
ghcr.io/whitelodge-esoteric/typeanvil:<version>
```

`<version>` is the CalVer release version **without** the leading `v` of the
git tag. For example, tag `v2026.9.5` publishes `2026.9.5`. The workflow does
not manage a moving `latest` tag, so always give an explicit version.

The [container distribution spec](../specifications/container-distribution.spec.md)
gives the image name as `ghcr.io/typeanvil/typeanvil`. Nothing is published
under that name. Use the repository-owner path above.

### Pull and check the version

```bash
docker pull ghcr.io/whitelodge-esoteric/typeanvil:2026.9.5
docker run --rm ghcr.io/whitelodge-esoteric/typeanvil:2026.9.5 --version
```

`--version` prints `typeanvil <version>`, for example `typeanvil 2026.9.5`.
The release workflow fails if this output does not match the tag.

### Render a document

The image's entrypoint is the `typeanvil` binary and its working directory is
`/work`. Mount your project directory at `/work`:

```bash
docker run --rm -v "$PWD:/work" \
  ghcr.io/whitelodge-esoteric/typeanvil:2026.9.5 \
  render /work/in.html --page-width 8.5in --page-height 11in -o /work/out.pdf
```

`render` needs an output path (`-o`), `--page-width` and `--page-height`.
Without them it exits with an error such as `missing --page-width`. For every
flag, see the [CLI reference](./cli.md).

The engine is the same as the native binary, so the exit status is the same:
`0` on success, and non-zero with a `typeanvil: error: …` message on stderr
on failure.

Resource paths inside the container follow the native rules:

- `<link rel="stylesheet">` files and `@font-face` `url()` files resolve
  relative to the input HTML file's directory.
- `<img src>` paths resolve against `--base-url` when you set it. Without it
  they resolve against the working directory, which is `/work` in the image.
  If your HTML is in a subdirectory, pass that directory, for example
  `--base-url /work/site/`.
- The engine never fetches network URLs.

### Image layout

| Path | Contents |
|---|---|
| `/usr/bin/typeanvil` | The release binary (the entrypoint). |
| `/usr/share/fonts/typeanvil/` | The bundled Liberation fonts and their SIL Open Font License text. |
| `/usr/share/fonts/user/` | Mount point for your own fonts (a declared `VOLUME`). |
| `/work` | Working directory. Mount your documents here. |

The image is based on `gcr.io/distroless/base-debian12:nonroot`. It has no
shell and runs as a non-root user (uid 65532). It is multi-arch:

- `linux/amd64` contains the statically linked musl binary;
- `linux/arm64` contains the glibc binary.

The container stores no state between runs. To roll back, run an earlier
version tag.

### Fonts in the image

On Linux, the engine looks for its fallback faces in
`/usr/share/fonts/typeanvil/`. The image puts the Liberation fonts there.
They are metric-compatible with Arial, Times New Roman and Courier New, so a
document lays out alike on macOS and in the container.

| Image | Bundled faces |
|---|---|
| `2026.9.3` to `2026.9.5` | Liberation Sans (regular, bold, italic, bold italic). |
| Built from current source | Liberation Sans, Liberation Serif and Liberation Mono (12 faces). |

Font families resolve as follows (from `engine/src/fonts.rs`; see the
[font resolution spec](../specifications/font-resolution.spec.md)):

- A family defined by an `@font-face` rule resolves to that rule's faces.
- `sans-serif`, `Arial` and `Helvetica` always resolve to the bundled
  sans-serif set: Liberation Sans in the image, Arial on macOS. A mounted
  font cannot replace them.
- `serif` resolves to Times New Roman, then Liberation Serif. `monospace`
  resolves to Courier New, then Liberation Mono. If neither family is
  installed, the generic falls back to the bundled sans-serif set. In the
  `2026.9.x` images, which lack Liberation Serif and Liberation Mono, serif
  and monospace text therefore renders in Liberation Sans unless you mount
  suitable fonts.
- Any other family name resolves to an installed font with that name,
  including fonts you mount.
- If no family in the `font-family` list resolves, the text uses the bundled
  sans-serif set.

The engine does not download fonts. An `@font-face` source with an
`http://` or `https://` URL fails, and the engine tries the next source.

### Mount your own fonts

Mount a font directory read-only at `/usr/share/fonts/user`:

```bash
docker run --rm \
  -v "$PWD:/work" \
  -v "$HOME/.fonts:/usr/share/fonts/user:ro" \
  ghcr.io/whitelodge-esoteric/typeanvil:2026.9.5 \
  render /work/in.html --page-width 8.5in --page-height 11in -o /work/out.pdf
```

The engine scans the system font directories, including `/usr/share/fonts`,
so it finds the mounted fonts without an extra flag. There is no
`--font-dir` flag. A mounted family named Times New Roman or Courier New also
takes over the `serif` or `monospace` generic. As noted above, mounted fonts
do not change `sans-serif`, `Arial` or `Helvetica`.

You can also load font files with `@font-face`, using paths relative to the
HTML file inside `/work`.

### Troubleshooting

- **Image cannot be pulled:** check the version. The tag has no leading `v`,
  and there is no `latest` tag.
- **Input file cannot be opened:** mount the directory at `/work` and use
  paths under `/work` (or relative to it).
- **Output permission denied:** the container runs as uid 65532. Make the
  output directory writable by that user, for example with
  `chmod 777 "$PWD"` on a scratch directory or a directory owned by that uid.
- **Images are missing:** set `--base-url` to the directory that your
  `<img src>` paths are relative to.
- **Wrong font:** check that the family name in your CSS matches the
  installed font's family name, and that the font is mounted at
  `/usr/share/fonts/user`.
- **Unsupported platform:** the image has `linux/amd64` and `linux/arm64`
  manifests only.

## npm packages

:::caution[Not published yet]
The npm packages exist in the repository's `npm/` directory but have not been
published. `npm install typeanvil` will not work until a release publishes
them. The release workflow publishes only when an `NPM_TOKEN` secret is
configured. Without it, the workflow skips publishing with a notice and
still produces the GitHub Release binaries. The rest of this section
describes the packages as they are in the repository and in the
[npm distribution spec](../specifications/npm-distribution.spec.md).
:::

### Packages

The layout follows the esbuild pattern: a JavaScript wrapper plus one optional
dependency per release target. Each platform package contains only its native
binary at `bin/typeanvil`.

| Package | Platform |
|---|---|
| `typeanvil` | The wrapper. It finds and runs the binary, provides the `typeanvil` command and exports `render()`. |
| `@typeanvil/darwin-arm64` | macOS, Apple silicon |
| `@typeanvil/darwin-x64` | macOS, Intel |
| `@typeanvil/linux-x64-gnu` | Linux x64, glibc |
| `@typeanvil/linux-x64-musl` | Linux x64, musl (static) |
| `@typeanvil/linux-arm64-gnu` | Linux arm64, glibc |

Other platforms have no package, including Windows and Linux arm64 with musl.
Platform packages declare `os` and `cpu` fields, and the musl package also
declares `libc`, so npm skips packages that do not match the host. Every
package is licensed `AGPL-3.0-only`. The release workflow sets all package
versions from the git tag. The `0.0.0-staging` version in the repository is a
placeholder.

The wrapper requires Node.js 18 or later (`engines.node: ">=18"`). Bun
installs the same packages (`bun add typeanvil`).

### Command line

Once the packages are published:

```bash
npm install typeanvil
npx typeanvil render in.html --page-width 8.5in --page-height 11in -o out.pdf
```

The wrapper passes every argument to the native binary and inherits its
stdout and stderr. It exits with the binary's exit code. On `SIGINT` or
`SIGTERM` it stops the binary. If the binary was killed by a signal, the
wrapper exits with `128 +` the signal number. The page-size flags are
required, as with the native binary.

### Programmatic `render()`

```js
const { render } = require('typeanvil');
// or: import { render } from 'typeanvil';

const { pdf, code } = await render([
  'render', 'in.html',
  '--page-width', '8.5in', '--page-height', '11in',
  '-o', 'out.pdf',
]);
// pdf is a Uint8Array with the bytes written to out.pdf; code is 0.
```

- `render(args, opts)` takes the CLI arguments as an array. A string is split
  on whitespace.
- The arguments must include `-o <path>`, written with the short `-o` form.
  The engine has no stdout output mode, so `render()` reads the PDF back from
  that path. The file is not deleted.
- The returned promise resolves to `{ pdf, code }` when the binary exits with
  status 0.
- A non-zero exit rejects with an `Error` whose `code` and `stderr` properties
  carry the exit status and the binary's stderr.
- `opts.cwd` sets the binary's working directory. `opts.binPath` sets an
  explicit binary path.

### How the wrapper finds the binary

`npm/typeanvil/resolve.js` tries these in order:

1. The `TYPEANVIL_BIN` environment variable. If it is set but the file does
   not exist, the wrapper reports an error.
2. The platform package for the current OS, CPU and libc. musl is detected
   with a heuristic: a Linux Node runtime that reports no glibc version is
   treated as musl. An OS/CPU pair with no package fails here with an
   "unsupported platform" message that lists the supported platforms.
3. A search upward through `node_modules` directories for any
   `@typeanvil/*` platform package. This covers hoisted and symlinked
   installs.
4. If nothing is found, a "native binary not found" message explains that the
   optional dependency was skipped (for example by
   `npm install --omit=optional`) or the platform is unsupported. The CLI
   exits with status 1.

### Fonts with the npm packages

The npm packages contain only the binary, with no fonts. The engine looks for
its fallback sans-serif faces in macOS's Arial files
(`/System/Library/Fonts/Supplemental/`). On Linux it looks in
`/usr/share/fonts/typeanvil/LiberationSans-*.ttf`. That directory exists only
in the container image. To use the npm packages on a Linux host, install the
Liberation fonts at that path, or use the container image. Other families
resolve against the host's installed fonts, as described in
[Fonts in the image](#fonts-in-the-image).

## Related

- [CLI reference](./cli.md)
- [Getting started](./getting-started.md)
- [Use the container image (runbook)](../operations/container-image.md)
- [Container distribution spec](../specifications/container-distribution.spec.md)
- [npm distribution spec](../specifications/npm-distribution.spec.md)
- [Font resolution spec](../specifications/font-resolution.spec.md)
