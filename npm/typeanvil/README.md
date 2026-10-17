# typeanvil

AI-first HTML → PDF typesetting engine. This npm package wraps the
native binary — it works in Node.js (≥ 18) and Bun.

## Install

```
npm install typeanvil
# or
bun add typeanvil
```

The right platform binary (`@typeanvil/darwin-arm64`, `@typeanvil/darwin-x64`,
`@typeanvil/linux-x64-gnu`, `@typeanvil/linux-x64-musl`,
`@typeanvil/linux-arm64-gnu`) is installed automatically as an optional
dependency.

## CLI

```
npx typeanvil render input.html \
  --page-width 8.5in --page-height 11in -o output.pdf
typeanvil --help
```

`--page-width` and `--page-height` are required.

All arguments pass through to the native binary; the exit code is preserved.

## Programmatic API

```js
import { render } from 'typeanvil';

const { pdf } = await render([
  'render', 'input.html',
  '--page-width', '8.5in', '--page-height', '11in',
  '-o', 'output.pdf',
]);
// `pdf` is a Uint8Array of the rendered PDF.
```

The engine does not write to stdout. `-o -` creates a file named `-`.
Always pass a real output path.

## Override the binary

Set `TYPEANVIL_BIN` to an explicit binary path to bypass package resolution
(useful in CI or for development).

## License

AGPL-3.0-only. https://github.com/whitelodge-esoteric/typeanvil
