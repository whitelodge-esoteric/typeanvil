# Writing HTML for TypeAnvil

TypeAnvil renders HTML (or Markdown) to a paged PDF. It is a document
compiler, not a browser: it runs no scripts, fetches no network URLs and
reads only the CSS listed below. Write for print. Load this guide before you
write a document.

## Workflow

1. Write one self-contained HTML file. Put all CSS in one `<style>` element.
2. Check it: call the `validate` tool (or run
   `typeanvil render doc.html --diagnostics json -o doc.pdf`).
3. Fix every finding (see "Read the render report" below). Repeat until the
   report holds no warnings.
4. Call `render` for the final PDF.

## Document skeleton

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Quarterly report</title>
<style>
  @page {
    size: 8.5in 11in;
    margin: 1in;
    @top-center { content: "Quarterly report"; font-size: 9pt; }
    @bottom-center { content: "Page " counter(page) " of " counter(pages); font-size: 9pt; }
  }
  body { font-family: "Liberation Sans", Arial, sans-serif; font-size: 11pt; line-height: 1.4; }
  h1, h2 { break-after: avoid; }
  table, figure { break-inside: avoid; }
  .new-section { break-before: page; }
</style>
</head>
<body>
  <h1>Quarterly report</h1>
  <p>Opening text.</p>
  <h2 class="new-section">Results</h2>
  <p>Text.<span class="fn">A footnote.</span></p>
</body>
</html>
```

## Paged-media idioms that work

- **Page box:** `@page { size: A4; margin: 20mm; }`. Named pages:
  `@page cover { margin: 0; }` with `.cover { page: cover; }`.
  Pseudo-pages: `:first`, `:left`, `:right`.
- **Margin boxes:** `@top-left`, `@top-center`, `@top-right`,
  `@bottom-left`, `@bottom-center`, `@bottom-right` and the side boxes, with
  `content:` text, `counter(page)` and `counter(pages)`.
- **Running headers:** `h1 { string-set: chapter content(); }` and
  `@top-center { content: string(chapter); }`. For a styled header element
  use `position: running(name)` and `content: element(name)`.
- **Footnotes:** `.fn { float: footnote; }`. The note moves to the bottom of
  the page that holds its call.
- **Breaks:** `break-before`, `break-after` and `break-inside` with `page`,
  `avoid` or `auto`. Use `break-after: avoid` on headings and
  `break-inside: avoid` on tables, figures and cards.
- **Tables:** `thead` repeats on each page. Give long tables a `thead`.
- **Page numbers in text:** `target-counter(attr(href), page)`.

## Capability lists

The lists below come from the engine's capability manifest
(`typeanvil capabilities --json`, schema {{SCHEMA}}). They are generated.
A property that is not listed as supported has no effect: the engine ignores
it and the render report warns.

{{GENERATED}}

## Avoid, and use instead

The table lists unsupported features that authors often write.

{{AVOID}}

General rules:

- On elements, write `margin-top` / `-right` / `-bottom` / `-left` and
  `padding-top` / `-right` / `-bottom` / `-left`. The `margin` and `padding`
  shorthands are not in the supported list, so the engine ignores them. (The
  `margin` shorthand is fine inside `@page`.)
- Do not use `border-collapse`. Tables render with the engine's own borders;
  put `border` on `td` and `th`.
- Use `<img>` or inline SVG for any image and any rounded or shadowed shape.
- Do not use JavaScript, web fonts from URLs, `@import` or remote
  stylesheets. Inline everything or use a local file next to the document.
- Do not size content in `vw`/`vh` for page layout; use `pt`, `mm`, `in` or `%`.
- Keep wide content inside the page content width. A fixed width wider than
  the page overflows and the report flags it.

## Read the render report

`--diagnostics json` (and the `validate` and `render` tools) return a list of
events. Each has a `code`, a `message`, a `line` and `column` in your source,
and for layout events a `page`, a `rect` in points and an `element`
selector. Fix the source line, then re-run.

| Code | Meaning | Fix |
|---|---|---|
| `unsupported-property` | The engine ignores a CSS property. | Remove it, or use the alternative named in the message. |
| `layout-overflow` | A box is wider or taller than the page content area. | Reduce its `width`, use `max-width: 100%`, or allow it to wrap. |
| `empty-page` | A page holds nothing. | Remove a stray `break-before: page` or an empty block that forces a break. |
| `orphaned-heading` | A heading is the last item on a page. | Add `break-after: avoid` to the heading selector. |
| `table-header-only` | A page holds only the table header rows. | Add `break-inside: avoid` to the table, or break before it. |
| `table-single-row-fragment` | A split table leaves one row on a page. | Add `break-inside: avoid` to the rows or shorten the table. |
| `image-missing` | An `<img>` file could not be loaded. | Fix the `src` path, relative to the document. |
| `font-fallback` | A named font is not available. | Use a family from the font list, or `sans-serif` / `serif`. |
| `target-unresolved` | `target-counter()` or `target-text()` names no id. | Add the missing `id`, or fix the `href`. |
| `link-target-missing` | `<a href="#id">` points at no id. | Add the `id` or fix the link. |

## Done check

A finished document renders with zero warnings, no `layout-overflow`, the
page count you expect, and every heading with text after it on the same page.
Use the `page_count` assertion in `validate` to state the expected page count.
