---
title: Markdown to PDF
type: guide
status: approved
owner: maintainers
created: 2026-10-18
updated: 2026-10-18
sidebar_position: 4
tags: [guide, markdown, cli, pdf]
slug: /guides/markdown
---

# Markdown to PDF

`typeanvil render` reads Markdown as well as HTML. The engine converts the
Markdown to HTML, then lays out that HTML with the same pipeline as any other
input. Everything in [Paged media CSS](./paged-media-css.md) applies.

```bash
typeanvil render notes.md \
  --page-width 8.5in --page-height 11in \
  --margin-top 1in --margin-right 1in --margin-bottom 1in --margin-left 1in \
  -o notes.pdf
```

## Choose the input format

- The extensions `.md`, `.markdown` and `.mdown` (any letter case) select
  Markdown. Every other extension is HTML.
- `--from markdown` or `--from html` overrides the extension. Any other value
  is a usage error.
- Input from stdin is not supported.

## What Markdown the engine reads

The base is CommonMark. These GitHub-style extensions are on:

- tables, with column alignment
- strikethrough (`~~text~~`)
- task lists (`- [x] done`)
- bare URL autolinks
- footnotes (`[^id]`), printed as page footnotes
- heading ids, so `[link](#my-heading)` works and headings become PDF
  bookmarks

Fenced code blocks print in a monospace face. The engine does not highlight
syntax. Smart punctuation is off. Raw HTML in the file passes through
unchanged, so `<div class="note">` and `<style>` elements work.

## Math

Write TeX math between dollar signs. `$a^2 + b^2 = c^2$` is inline math.
`$$\frac{a}{b}$$` is display math. The engine turns the formula into MathML
and lays it out. It supports fractions, sub- and superscripts, roots, Greek
letters, common operator, relation and arrow symbols, big operators such as
`\sum` and `\int`, named functions such as `\sin`, `\left...\right` fences,
accents, `\text`, and the `matrix`, `pmatrix`, `bmatrix`, `vmatrix` and
`cases` environments.

If a formula uses TeX the engine does not know, the whole formula prints as
code (the TeX source) and the CLI reports a `markdown-math-untranslated`
diagnostic with the reason. The rest of the document is not affected. See the
[Markdown input specification](/specifications/markdown-input) for the full
rules.

## Frontmatter

A YAML block at the top of the file sets PDF metadata:

```markdown
---
title: Release Notes
author: A. Author
description: Changes in this release
keywords: [release, notes]
lang: en
---
```

| Key | Result |
|---|---|
| `title` | PDF title |
| `author` | PDF author (a string or a list) |
| `description` | PDF subject |
| `keywords` | PDF keywords (a string or a list) |
| `lang` | document language |

The `--title` and `--author` flags override frontmatter. Without a `title`
key, the PDF has no title: the engine does not use the first heading. Invalid
YAML stops the render with exit code `1` and names the line.

## Style

The engine adds a small default stylesheet: body font, a heading scale,
monospace code, table borders with a repeating header row, a blockquote rule,
and images that fit the page width. It sets no `@page` size or margins, so the
CLI page flags decide the page.

- `--css <file>` appends your stylesheet after the default one. You can repeat
  the flag. Later rules win.
- `<style>` elements in the Markdown file win over the default in the same way.
- `--no-default-css` removes the default stylesheet.

`--css` and `--no-default-css` apply to Markdown only. HTML input rejects them
with exit code `2`.

A stylesheet for running headers and page numbers:

```css
@page {
  size: Letter;
  margin: 1in;
  @bottom-center { content: "Page " counter(page) " of " counter(pages); }
}
```

```bash
typeanvil render notes.md --css print.css -o notes.pdf \
  --page-width 8.5in --page-height 11in
```

## Images and links

Relative image paths resolve against `--base-url` when you pass it, and
against the current working directory otherwise. This is the same rule as for
HTML input.

## Related references

- [CLI reference](./cli.md)
- [Markdown input specification](/specifications/markdown-input)
