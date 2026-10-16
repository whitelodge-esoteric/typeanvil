---
title: Paged media CSS
type: guide
status: approved
owner: maintainers
created: 2026-10-14
updated: 2026-10-14
sidebar_position: 5
tags: [guide, paged-media, css, pdf, accessibility]
slug: /guides/paged-media-css
---

# Paged media CSS

Typeanvil renders HTML to paginated PDF. Along with ordinary CSS layout, it
supports the paged-media and generated-content features that reports,
invoices, and books need: `@page` rules and margin boxes, named pages, page
counters, running headers, footnotes, cross-references, page floats,
fragmentation controls, hyperlinks, PDF bookmarks, PDF metadata, and tagged
PDF.

This page describes the CSS and HTML these features use. It also lists the
limits of the current implementation. For command-line flags, see the
[CLI guide](./cli.md) and the [CLI reference](../operations/cli.md). If you
have not rendered a document yet, start with
[Getting started](./getting-started.md). The behavior contracts are in the
linked specifications. Where Typeanvil differs from the CSS specifications,
this page says so.

## `@page` rules

An `@page` rule describes the page box: its size, its margins, and the
content of its margin boxes. Typeanvil reads `@page` rules from author
stylesheets (`<style>` elements and `<link rel="stylesheet">` files).

```css
@page {
  size: 8.5in 11in;
  margin: 1in;
}
```

A rule can be unnamed, which styles the default page, or named. Either form
can take a page pseudo-class:

```text
@page [<name>][:first | :left | :right] { ... }
```

Declarations inside `@page` that Typeanvil does not recognize are skipped.
They do not fail the render. In a prelude such as `@page a:first`, the name
and the pseudo-class are written without whitespace between them.

### Page size

`size` accepts:

- one or two lengths: `size: 8.5in 11in`, or `size: 6in` for a square page;
- a page-size keyword: `A5`, `A4`, `A3`, `B5`, `B4`, `JIS-B5`, `JIS-B4`,
  `letter`, `legal`, or `ledger`;
- an orientation keyword, `landscape` or `portrait`, alone or after a
  page-size keyword (`size: A4 landscape`).

```css
@page { size: A4; }
@page wide { size: letter landscape; }
```

`@page` also accepts `margin` with one to four values, the `margin-*`
longhands, `background`, and `page-orientation` (`rotate-left`,
`rotate-right`, `rotate-top`, `rotate-bottom`). `page-orientation` rotates
the laid-out content on the page.

### `@page` and the CLI page flags

Every `typeanvil render` call needs `--page-width` and `--page-height`. The
`--margin-*` flags default to `0`. These flags give the default page
geometry. An `@page` `size` or margin overrides the flags on the pages that
the rule matches. A page that no rule matches uses the CLI geometry and has no
margin boxes. A matching rule without `size` uses the CLI size. See the
[CLI reference](../operations/cli.md) for the flags.

### Page selectors

Typeanvil chooses each page's pseudo-class from the page's position in the
whole document:

- The first page of the document matches `:first`. It also matches `:right`,
  because pages progress left to right.
- After that, pages alternate between `:left` and `:right`.

So `@page cover:first` applies only when the document's first page is a
`cover` page.

```css
@page :first { margin-top: 2in; }
@page :left  { margin-left: 1.5in; margin-right: 1in; }
@page :right { margin-left: 1in; margin-right: 1.5in; }
```

When several rules match a page, a named rule beats an unnamed one. A rule
with a pseudo-class beats one without. If neither of those settles it, the
later rule in the source wins.

### Cascade layers

An `@page` rule inside an `@layer` block takes that layer's rank. Unlayered
rules beat every layer. Of two layers, the one declared later wins, wherever
its rules appear in the source. Layer rank takes priority over the
pseudo-class rule above.

```css
@layer base, overrides;

@layer overrides {
  @page { margin: 0.75in; }   /* wins: later-declared layer */
}
@layer base {
  @page { margin: 1in; }
}
```

### Viewport units and logical margins

`vh` and `vw` resolve against the page's content box, so `100vh` is the
height of the page content area.

In `@page`, Typeanvil maps the logical properties (`margin-inline-*`,
`margin-block-*`, and the matching `padding-*` properties) to physical edges.
It uses the page context's writing mode: the `writing-mode` declared in
`@page`, or else the root element's. Document content still lays out
horizontally. See [Known limitations](#known-limitations).

## Margin boxes

A margin box holds generated content in the page margin. You write margin-box
rules inside an `@page` rule:

```css
@page {
  margin: 1in;

  @top-center    { content: "Quarterly Report"; }
  @bottom-right  { content: "Page " counter(page) " of " counter(pages); }
}
```

Typeanvil supports all 16 page-margin boxes:

| Edge | Boxes |
| --- | --- |
| Top | `@top-left-corner`, `@top-left`, `@top-center`, `@top-right`, `@top-right-corner` |
| Right | `@right-top`, `@right-middle`, `@right-bottom` |
| Bottom | `@bottom-left-corner`, `@bottom-left`, `@bottom-center`, `@bottom-right`, `@bottom-right-corner` |
| Left | `@left-top`, `@left-middle`, `@left-bottom` |

A `@top-center` or `@bottom-center` box is centered on the midline of the
page content box. It is not limited to a fixed middle third, so text wider
than a third extends evenly to both sides.

### Content

The `content` property of a margin box accepts:

- quoted strings, including CSS escapes (`"\a"` is a newline) and non-ASCII
  characters;
- `string(name)` and `string(name, keyword)`, which give running-header
  values (see [Running headers](#running-headers));
- `counter(page)`, the current page number;
- `counter(pages)`, the total number of pages;
- `counter(<name>)`, a named counter;
- `url(<path>)`, an image painted at its intrinsic size (96 CSS pixels per
  inch), after the text of the last line;
- `none`, which removes the box's content.

A newline in the content starts a new line only when the box's `white-space`
preserves line breaks (`pre`, `pre-wrap`, `pre-line`, or `break-spaces`).
Otherwise the newline becomes a space. A margin box takes `white-space`,
`line-height`, and `font-family` from its own declarations, or else from the
`@page` context. If neither declares them, they take the initial values
(`normal` and `serif`). A forced break at the very end of the content does
not add an empty line.

Margin-box lines do not wrap. A line wider than its box is clipped.

### Box model and painting

Each margin box is a real CSS box. It accepts `width`, `height`, margins,
padding, `border` and the `border-<side>` properties, `background`,
`background-color`, and `background-image: url(...)`. When several `@page`
rules match a page, their margin-box declarations merge property by property.

- The three boxes on an edge share that edge. Typeanvil divides the space
  between `auto`-sized boxes according to their min-content and max-content
  sizes (css-page-3 §5.3.2).
- Across the margin, a box's margins, borders, padding, and size add up to
  the page margin on that edge. Two `auto` margins center the box.
- A corner box is as wide as the side margin and as tall as the top or bottom
  margin that meets it.
- A background image is tiled from the box's top-left corner, clipped to the
  box, and painted under the border.
- A box with `content: ""` is still created and painted.

Typeanvil paints each page in this order: page background, document canvas,
page borders, document content, then margin boxes. `z-index` works on margin
boxes as if they were positioned elements. A negative `z-index` paints the box
behind all document content, and `auto`, `0`, or a positive value paints it
in front. A margin box never paints between two pieces of document content.
Boxes with the same `z-index` paint clockwise from `@top-left-corner`,
whatever order you declare them in.

## Named pages

The `page` property assigns an element to a named page. The pages that the
element starts use the matching `@page` rule:

```css
section.wide { page: wide; }

@page wide { size: letter landscape; }
```

If no `@page` rule has that name, the page uses the default page rule. The
first page of the document can be a named page.

A box's page name is its own `page` value, if that is not `auto`. If the box
has none, Typeanvil uses the name declared by the nearest earlier sibling
that is an in-flow replaced element. If neither applies, the box takes the
nearest ancestor's `page` value, or else uses the default page.

Typeanvil forces a page break between adjacent boxes whose page names differ.
A box with no page name, after a named page, returns to the default page.
This lets you build a cover page:

```css
.cover  { page: cover; break-after: page; }
@page cover {
  @top-center { content: none; }   /* no running header on the cover */
}
```

A page that continues content from the previous page keeps that page's name.

`page` applies only to boxes that can start a page. An inline image (`<img>`
or inline `<svg>`) cannot, so a `page` value on it has no effect. A
block-level image keeps its `page` value. Typeanvil does not insert
page-change breaks inside flex containers, inside inline-blocks, or inside
content whose writing mode is orthogonal to the page flow.

## Page counters

The implicit `page` counter is the 1-based page number. `counter(pages)` is
the total number of pages, and it is the same on every page:

```css
@page {
  @bottom-right { content: "Page " counter(page) " of " counter(pages); }
}
```

`counter-reset: page <n>` on an element sets the page counter on the page
where that element's box starts. That page shows `<n>`, and later pages count
up from it:

```css
.appendix { break-before: page; counter-reset: page 1; }
```

Typeanvil only knows the page total after pagination. A document that uses
`counter(pages)` or page-number cross-references is therefore laid out again,
for at most three passes. Margin-box text cannot change the pagination, so
the result is stable and deterministic.

## Running headers

A running header shows a value that changes as the document flows, such as
the current chapter title. `string-set` captures the value, and `string()`
shows it in a margin box:

```css
h1 { string-set: chapter content(); }

@page {
  @top-left { content: string(chapter); }
}
```

`string-set: <name> content()` sets the named string to the element's text.
Typeanvil records an assignment on the page where the element's box starts.
An element split across pages counts only on its first page. A page without
an assignment shows the value carried over from earlier pages. Before the
first assignment, the value is empty.

`string(name, keyword)` chooses which value appears. The keyword is matched
case-insensitively. The default is `first`, and an unknown keyword is treated
as `first`.

| Keyword | Value on a page |
| --- | --- |
| `first` | The first assignment on the page, or the carried value |
| `last` | The last assignment on the page, or the carried value |
| `start` | The value carried from the previous page, always |
| `first-except` | Empty if the page has an assignment, otherwise the carried value |

```css
@page {
  @top-left  { content: string(chapter, first); }
  @top-right { content: string(chapter, last); }
}
```

`start` follows Prince's behavior. An element at the very top of a page does
not count for `start`. The CSS GCPM text would count it, so this is a
documented deviation from the standard. An element can set several strings
(`string-set: a content(), b content()`).

Typeanvil supports running strings only. It does not support running elements
(`position: running()` with `element()`).

## Footnotes

`float: footnote` takes an element out of the body text and moves it to the
footnote area at the bottom of the page where its call appears:

```css
.fn { float: footnote; font-size: 9pt; }
```

```html
<p>Revenue grew sharply.<span class="fn">Audited figures.</span></p>
```

- Typeanvil puts a superscript number (the call) in the body text where the
  element was.
- The page's footnote area lists, in call order, every note whose call is on
  that page. Each note starts with an `N.` marker that hangs about 8.5pt to
  the left of the content edge. Wrapped lines return to the content edge.
- Note text uses the footnote element's own style, so `.fn { font-size: 9pt }`
  applies.
- The footnote area grows up from the bottom margin, and the body text gets
  less room. Typeanvil does not split a note across pages. A note always
  appears on the same page as its call.
- Footnote numbers continue across the whole document. They do not restart
  on each page.
- Typeanvil draws no separator line above the footnote area.

## Page floats

The page-float values of `float` pin a box to the top or bottom of the page
content area, instead of the left or right of a line:

| Value | Behavior |
| --- | --- |
| `float: top` | Pinned to the top of the content area. Body text continues below it, not beside it. |
| `float: bottom` | Pinned to the bottom of the content area. |
| `float: next-page` | Moved to the top of the next page. |
| `float: snap` | Treated as `top`. |

Several floats on one edge stack in document order without overlapping. A
page float is never split across pages. A `top` or `bottom` float that does
not fit in the space left on a page moves to the next page whole. A page
float taller than a whole page is placed on its page and clipped.

## Fragmentation controls

These properties control where pages break:

| Property | Values Typeanvil supports |
| --- | --- |
| `break-before`, `break-after` | `auto`, `page`, `always`, `left`, `right`, `avoid`, `avoid-page`, `avoid-column`, `column` |
| `break-inside` | `auto`, `avoid`, `avoid-page` |
| `orphans`, `widows` | Integer (minimum lines at the bottom or top of a page) |
| `page-break-before`, `page-break-after`, `page-break-inside` | Legacy aliases of the properties above |

- `page` and `always` force a page break. A forced break wins over
  `break-inside: avoid` on an ancestor.
- `break-after: avoid` on a heading keeps it with the next block. When the
  next block does not fit on the page, the heading moves to the next page
  with it. Chains of `avoid` boundaries move together. This applies between
  adjacent in-flow blocks. If no earlier break point exists, Typeanvil breaks
  at the normal point.
- `break-inside: avoid` moves a block to the next page when it fits a whole
  page but not the space left on the current one. A block taller than a
  page breaks anyway.
- Typeanvil does not split monolithic content, such as an image or a line,
  across pages. If it is taller than a page, it overflows the page.
- At a page break, the margin of a split box is dropped at the top of the
  next page. A box that starts fresh at the top of a page keeps its top
  margin.

```css
h2 { break-after: avoid; }
figure { break-inside: avoid; }
.chapter { break-before: page; }
p { orphans: 2; widows: 2; }
```

## Cross-references

`target-counter()` and `target-text()` insert a value taken from another
element, the target. A common use is page numbers in a table of contents:

```css
.toc a { display: block; }
.toc-1 { content: "1. Revenue " leader('.') target-counter(attr(href), page); }
```

```html
<nav class="toc">
  <a class="toc-1" href="#revenue">Revenue</a>
</nav>
<h1 id="revenue">Revenue</h1>
```

- `target-counter(attr(<name>), <counter>)` gives a counter's value at the
  target. `<counter>` can be `page` (the target's page number, which is the
  default), `pages` (the total page count), or a named counter (its value at
  the target, after `counter-reset` and `counter-increment` up to and
  including the target).
- `target-text(attr(<name>))` gives all of the target element's text.
- The target is found from the attribute on the element that has the
  `content` declaration. A leading `#` is removed, and the first element with
  that `id` is the target.
- If the target is missing (no matching `id`, no attribute, or an element
  that is not rendered), both functions give `?`.
- `leader('.')` fills the rest of the line with the given character, up to
  the right content edge.

Typeanvil applies element `content` to block-level boxes only. It does not
apply it to inline boxes, which is why the example uses `display: block`. The
`content` value replaces the element's own text.

## Hyperlinks

An `<a href>` element becomes a clickable PDF link annotation:

- A link starting with `#` jumps to the page of the first element with that
  `id`. It goes to the top of that page.
- Typeanvil writes any other `href` value (`https:`, `http:`, `mailto:`, or a
  relative path) into the PDF unchanged, as a URI action.
- A link that wraps gets one clickable area for each line. A link split
  across pages gets clickable areas on each page, all going to the same
  target.
- A `#fragment` that matches no `id` produces no link and no error.

## PDF bookmarks

Headings become PDF bookmarks (the outline) automatically. `h1` to `h6` map
to outline levels 1 to 6, nested by level, in document order. By default the
label is the heading's text, and the bookmark points to the heading's
position on its page. A heading split across pages is bookmarked at its first
fragment. A document with no bookmarks has no outline. The exception is
tagged output, described in [Tagged PDF and PDF/UA](#tagged-pdf-and-pdfua).

Three properties adjust the outline:

| Property | Values | Effect |
| --- | --- | --- |
| `bookmark-level` | `none` or an integer from 1 to 6 | Sets the outline level. `none` removes the element from the outline. |
| `bookmark-label` | A content list | Sets the bookmark text. |
| `bookmark-state` | `open` or `closed` | Shows or hides the bookmark's children at first. The default is `open`. |

```css
h2.appendix { bookmark-level: none; }
h2 { bookmark-label: "Chapter " counter(chapter); }
h1.part { bookmark-state: closed; }
```

- `bookmark-label` uses the same parser as `content`. It accepts quoted
  strings, `contents()` (the element's text), `counter(name)`,
  `string(name)`, `counter(page)`, `counter(pages)`, and `leader()`. An empty
  or invalid label falls back to `contents()`.
- `bookmark-level` replaces the heading's own level. With
  `h1 { bookmark-level: 3 }`, the `h1` appears as a level-3 bookmark. A level
  outside 1 to 6 is ignored.
- Elements other than headings get bookmarks only when you give them a
  `bookmark-level`.
- If an element has a `bookmark-*` declaration but is not rendered (for
  example, it has `display: none`), Typeanvil adds no bookmark for it. In that
  case `--diagnostics` reports a `bookmark-anchor-unresolved` warning.

## PDF metadata

Typeanvil reads document metadata from the HTML `<head>`:

| Source | PDF field |
| --- | --- |
| The first `<title>` element (text, trimmed) | Title |
| `<meta name="author" content="...">` | Author |
| `<meta name="description" content="...">` | Subject |
| `<meta name="keywords" content="...">` (split on commas and trimmed) | Keywords |

The `name` values must be lowercase to match. `--title` and `--author`
override the values from the document:

```bash
typeanvil render in.html --page-width 8.5in --page-height 11in \
  --title "Q3 Report" --author "A. Author" -o out.pdf
```

If the document has no metadata and you pass no override, the PDF has no
metadata. Typeanvil never writes creation or modification dates, or creator
or producer strings, so the same input always produces the same bytes.

## Tagged PDF and PDF/UA

Tagged PDF adds a logical structure tree for assistive technology. Tagging is
**on by default**. Pass `--no-tagged` to leave the structure tree out.

```bash
typeanvil render in.html --page-width 8.5in --page-height 11in -o out.pdf
typeanvil render in.html --page-width 8.5in --page-height 11in --no-tagged -o out.pdf
typeanvil render in.html --page-width 8.5in --page-height 11in --ua -o out.pdf
```

`--ua` always tags, even with `--no-tagged`. It also runs a
PDF/UA-1 validator (built into krilla, the PDF library Typeanvil uses) when
writing the PDF. If the validator finds a problem, the render fails: the
command exits with status 1, prints the list of violations to stderr, and
writes no PDF. Without `--ua`, a tagged render does not check conformance,
so problems such as missing alt text do not stop the render.

With tagging on:

- The structure tree follows the HTML elements in document order. Typeanvil
  maps `h1`–`h6` to `H1`–`H6`, `p` to `P`, `ul`/`ol` and `li` to `L` and
  `LI`, table elements to `Table`, `THead`, `TBody`, `TFoot`, `TR`, `TH`, and
  `TD`, `figure` to `Figure`, `a` to `Link`, and `blockquote` to `BlockQuote`.
  Other elements become `Div`.
- The alt text of a figure comes from its `<img alt>`, or else from its
  `<figcaption>` text.
- `<html lang>` sets the document language.
- Headings carry their text as the structure element's title.
- Each link annotation carries alt text: the link's text, or else its `title`
  attribute, or else its `href`.
- Margin-box content, page backgrounds, and page borders are marked as
  artifacts (page decoration). This keeps a running header out of the logical
  tree, so it is not read on every page. Footnote text stays in the tree at
  the position of its call.
- A tagged document with no headings gets one bookmark: the document title,
  pointing at page 1. PDF/UA-1 requires an outline, so a heading-less letter
  with a `<title>` can now pass `--ua`. Without a title, `--ua` still fails,
  because PDF/UA-1 requires a title.
- The same input with the same flags gives byte-identical output, tagged or
  not.

To pass `--ua`, give the document a `<title>` (or pass `--title`), alt text
for every image, and fonts that contain every character you use. Set
`<html lang>` too. In our checks on this release, `--ua` accepted a document
without `lang`, but assistive technology needs the language to read the
document correctly.

## Worked example

This report uses a cover page with no running header, a table of contents
with leader dots and page numbers, running chapter headers, a "Page N of M"
footer, a footnote, an internal link, bookmarks, metadata, and tagged
PDF/UA output.

```html
<!DOCTYPE html>
<html lang="en">
<head>
<title>Quarterly Report</title>
<meta name="author" content="A. Author">
<meta name="description" content="Q3 results">
<meta name="keywords" content="finance, quarterly, report">
<style>
  @page {
    size: letter;
    margin: 0.75in;
    @top-left      { content: string(chapter); }
    @top-right     { content: "Quarterly Report"; }
    @bottom-center { content: "Page " counter(page) " of " counter(pages); }
  }
  @page cover {
    margin: 2in 1in;
    @top-left      { content: none; }
    @top-right     { content: none; }
    @bottom-center { content: none; }
  }

  body { font-size: 11pt; }
  .cover { page: cover; break-after: page; }
  h1 { string-set: chapter content(); break-after: avoid; }
  .chapter { break-before: page; }
  .toc a { display: block; }
  .toc-1 { content: "1. Revenue " leader('.') target-counter(attr(href), page); }
  .toc-2 { content: "2. Costs " leader('.') target-counter(attr(href), page); }
  .fn { float: footnote; font-size: 9pt; }
  p { orphans: 2; widows: 2; }
</style>
</head>
<body>
  <section class="cover">
    <p class="title">Quarterly Report</p>
    <p>Third quarter, 2026.</p>
  </section>

  <nav class="toc">
    <a class="toc-1" href="#revenue">Revenue</a>
    <a class="toc-2" href="#costs">Costs</a>
  </nav>

  <section class="chapter">
    <h1 id="revenue">Revenue</h1>
    <p>Revenue grew sharply.<span class="fn">Audited figures.</span>
    See <a href="#costs">Costs</a> for the other side of the ledger.</p>
  </section>

  <section class="chapter">
    <h1 id="costs">Costs</h1>
    <p>Costs held steady.</p>
  </section>
</body>
</html>
```

```bash
typeanvil render report.html \
  --page-width 8.5in --page-height 11in \
  --ua -o report.pdf
```

The result has four pages:

1. The cover, with no header or footer.
2. The table of contents, showing `1. Revenue ....... 3` and
   `2. Costs ....... 4`. The `chapter` string has no value yet, so the
   top-left header is empty. The footer reads `Page 2 of 4`.
3. The Revenue chapter, with header `Revenue` and footnote `1. Audited
   figures.` at the bottom of the page.
4. The Costs chapter, with header `Costs`.

The outline has two bookmarks, Revenue and Costs. The PDF's Title, Author,
Subject, and Keywords come from the `<head>`. The render passes PDF/UA-1
validation. The CLI page size is Letter, matching `size: letter`. The CLI
flags are still required, even when `@page` sets the size.

## Known limitations

**`@page` and margin boxes**

- Page pseudo-classes other than `:first`, `:left`, and `:right` (`:blank`,
  `:nth()`) are not supported. Neither are `bleed` and `marks`.
- Running elements (`element()`) are not supported. Use `string()` instead.
- Margin-box lines do not wrap. Long text is clipped. Vertical writing modes
  in margin boxes are not supported. Margin-box content always lays out
  horizontally.
- Vertical text inside the document is not supported: no glyph runs are
  rotated. Logical `@page` margins follow the page's writing mode, but
  document content lays out horizontally.
- `break-before: left` and `break-before: right` are treated as plain page
  breaks. Typeanvil does not add a blank page to reach a left or right page.
- Attribute selectors (`a[href]`, `[data-x]`) currently match nothing, in
  the paged-media properties and in the general cascade alike. Use classes or
  ids instead.

**Named pages**

- Typeanvil does not insert page-change breaks between flex items, inside
  inline-blocks, between bare inline runs, or inside a subtree whose writing
  mode is orthogonal to the page flow.

**Running headers**

- `attr()` and counters inside `string-set` are accepted but treated as
  `content()`.
- Keywords work only in margin boxes. `string()` in an element's `content`
  always shows the current value.
- Named strings do not appear in the PDF outline. A `bookmark-label` can use
  `string(name)` directly.

**Footnotes**

- `footnote-policy`, `@footnote` area styling, and the `::footnote-call`,
  `::footnote-marker`, and `::footnote-separator` pseudo-elements are not
  supported. Markers use fixed styles.
- Footnote numbers cannot restart on each page.
- Nested footnotes are not supported.
- Footnote calls are supported only in normal block flow. A note called from
  inside a table, float, flex container, or multicolumn layout still appears
  in the page's footnote area, laid out at the full content width.

**Page floats**

- `snap` always resolves to `top`. `snap-block` and `snap-inline` are not
  supported. `next-page` sends the float to the next page, not to the next
  page with the same name.
- A page float taller than the page content area is clipped. It does not
  continue onto the next page.

**Fragmentation**

- A `column` break value does not create a column break.
- `avoid`, `avoid-page`, and `avoid-column` act the same: each avoids any
  break at that boundary.
- `avoid` between siblings does not pass to a parent's own boundaries or into
  descendants. Flex items, grid items, and table rows ignore it. Balanced
  multicolumn sets ignore it too.
- `widows` defaults to `1` for Prince compatibility. The CSS initial value is
  `2`. Set `widows: 2` to get the CSS default.

**Cross-references**

- Only decimal numbers are supported. A counter style argument cannot be
  given.
- `target-text()` returns all of the target's text. The `first`, `before`,
  and `after` options are not supported.
- `target-counter()` and `target-text()` work in element `content`, not in
  margin boxes, where they produce no text.
- Only the `attr(<name>)` target form is supported. `url(#id)` is not.

**Hyperlinks**

- Typeanvil does not style links. Link text uses the surrounding text color,
  with no automatic blue or underline.
- Internal links go to the top of the target page, not to the target's exact
  position. No named destinations are written.
- An element whose `content` replaces its text, such as the table-of-contents
  entries in the worked example, produces no link annotation. A link that
  contains only an image produces no annotation either.

**Bookmarks**

- Outline colors and styles are not exposed.
- `contents()` in `bookmark-label` reads the element's text. It does not
  include `::before` or `::after` content.

**Metadata**

- Only the first `<title>` is read, and the `<meta>` names must be lowercase.
- Creation dates, modification dates, and custom XMP schemas are not
  supported. `--title` and `--author` are the only metadata overrides.

**Tagged PDF and PDF/UA**

- Some roles that Typeanvil maps do not reach the structure tree yet. Text
  inside `strong`/`b`, `em`/`i`, and `pre`/`code` belongs to the surrounding
  block, so no `Strong`, `Em`, or `Code` elements appear. An `<img>` outside a
  `<figure>` falls under a `Div` and loses its alt text. `<caption>` becomes
  `Div`. Most `LI` elements have no `LBody`. Some links have no `Link`
  element.
- Repeated table header and footer rows are tagged again on every page.
  Words split by hyphenation can be out of reading order.
- List labels are not tagged: `Lbl` stays empty and lists carry no
  numbering. Typeanvil does not draw list markers.
- Language changes within a document, `ActualText`, form fields, notes, and
  math are not tagged.
- A character missing from the resolved font fails `--ua`
  (`ContainsNotDefGlyph`), because there is no per-character font fallback.
- PDF/A conformance is not supported. The built-in PDF/UA-1 validator is the
  only conformance check. Typeanvil does not claim Acrobat or PAC
  certification.

## Related specifications

- [Paged-media CSS](../specifications/paged-media-css.spec.md) and
  [paged-media hardening](../specifications/paged-media-hardening.spec.md)
- [Named pages](../specifications/named-pages.spec.md)
- [Running headers](../specifications/string-set-running-headers.spec.md)
- [Footnotes](../specifications/footnotes.spec.md) and
  [page floats](../specifications/page-floats.spec.md)
- [Fragmentation core](../specifications/fragmentation-core.spec.md)
- [Cross-references](../specifications/cross-references.spec.md) and
  [hyperlinks](../specifications/hyperlinks.spec.md)
- [PDF outlines](../specifications/pdf-outlines.spec.md) and
  [PDF metadata](../specifications/pdf-metadata.spec.md)
- [Tagged PDF](../specifications/tagged-pdf.spec.md) and the
  [tagged PDF gate](../specifications/tagged-gate.spec.md)
- [CSS standards alignment](../conventions/css-standards-alignment.md)
