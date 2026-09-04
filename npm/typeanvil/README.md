# typeanvil

AI-first HTML/Markdown → PDF typesetting engine. This npm package wraps the
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
npx typeanvil render input.html -o output.pdf
typeanvil --help
```

All arguments pass through to the native binary; the exit code is preserved.

## Programmatic API

```js
import { render } from 'typeanvil';

const { pdf } = await render(['render', 'input.html', '-o', 'output.pdf']);
// `pdf` is a Uint8Array of the rendered PDF.
```

To render to a buffer instead of a file, pass `-o -` if your engine version
supports stdout output, or render to a temp file and read it.

## Override the binary

Set `TYPEANVIL_BIN` to an explicit binary path to bypass package resolution
(useful in CI or for development).

## License

AGPL-3.0-only. https://github.com/whitelodge-esoteric/typeanvil
