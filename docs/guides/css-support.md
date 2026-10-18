---
title: CSS support
type: guide
status: approved
owner: maintainers
created: 2026-10-14
updated: 2026-10-17
sidebar_position: 6
tags: [guide, css, layout, tables, fonts, typography]
slug: /guides/css-support
---

# CSS support

This page lists the layout and CSS features that the TypeAnvil engine
supports, with the known limitations of each. Every section links to the
specification that defines the behavior in full. For page size, margin boxes,
counters and running headers, see [Paged media CSS](./paged-media-css.md). For
command-line flags, see the [CLI reference](./cli.md).

CSS standards define the expected behavior. Where TypeAnvil deliberately
departs from a standard, the specification records the reason (see
[CSS standards alignment](../conventions/css-standards-alignment.md)). This
page describes what the engine does today, gaps included.

Some ground rules apply to every feature:

- **Styles.** The engine reads `<style>` elements, local
  `<link rel="stylesheet">` files and `style=""` attributes. Linked
  stylesheets resolve relative to the input file. It never fetches network
  URLs and never runs scripts.
- **Cascade.** The cascade comes from Servo's style engine (stylo). Properties
  that stylo's Servo build does not compute, such as `orphans`, `widows`,
  `hyphens` and the paged-media properties, are read by the engine's own
  passes.
- **Determinism.** The same input always produces a byte-identical PDF.

## Selectors

Specs: [Selector matching](../specifications/selector-matching.spec.md) and
[Paged media CSS, Behavior 19](../specifications/paged-media-css.spec.md).

The following selectors are supported:

- Type, class and ID selectors.
- The descendant, child (`>`), next-sibling (`+`) and subsequent-sibling (`~`)
  combinators.
- Tree-structural pseudo-classes, such as `:first-child`, `:nth-of-type()`,
  `:last-of-type` and `:only-of-type`. Text and comment nodes between
  elements do not affect matching.

The engine's own pass for paged-media properties (`page`, `string-set`,
counters, `content`, `bookmark-*`, footnote and page floats, `writing-mode`)
uses the same selector parser and matcher as the main cascade. A rule
therefore styles the same elements in both. Each property takes the
declaration with the highest specificity and, on a tie, the later one. A
selector list that contains an invalid selector drops the whole rule.

:::note[Known limitations]

- **Attribute selectors never match.** This includes `[href]`, `[type=…]` and
  `[lang|=…]`, both in the cascade and in the paged pass.
- Pseudo-classes that are not tree-structural never match. Examples are
  `:link`, `:hover` and `:lang()`.

:::

## Print media queries and the user-agent stylesheet

Specs: [Print media queries](../specifications/print-media-queries.spec.md) and
[UA print defaults](../specifications/ua-print-defaults.spec.md).

### Media queries

The engine evaluates every `@media` rule once per render, before the cascade,
against a print device:

- `print` matches and `screen` does not.
- A comma list matches when any query in it matches. `not` negates a query.
  An invalid query is treated as `not all`.
- Size features such as `width` and `height` are measured against the page
  box given by `--page-width` and `--page-height`. An `@page size`
  declaration does not change them.
- `@page` rules, break properties and other declarations inside a matching
  `@media` block apply. Inside a block that does not match, they do not.
- A true `@supports` block applies and a false one is removed. `@layer`
  blocks keep their cascade order.
- Unknown at-rules are removed. This includes `@scope`, `@container` and
  `@document`.

```css
@media print and (min-width: 7in) {
  .sidebar { float: right; width: 2in; }
}
```

:::note[Known limitations]

- `vw`, `vh`, `vmin` and `vmax` resolve against a fixed 1024×768 px viewport,
  not the page box.
- Custom media queries are not supported. The engine keeps `@keyframes` and
  `@counter-style` rules in the stylesheet but does not run animations or
  render counter styles.

:::

### User-agent defaults

The built-in stylesheet (`UA_CSS` in `engine/src/css.rs`) uses print-oriented
defaults:

| Element | Default |
|---|---|
| `h1`–`h6` | Bold, at fixed sizes: 24, 18, 14, 12, 10 and 8 pt. Margins are fixed: 16, 15, 14, 16, 16.5 and 21 pt, top and bottom. Headings do not scale with the body font size. Each heading level also maps to a PDF bookmark level. |
| `p` | `margin: 1.12em 0` |
| `ul`, `ol` | `margin: 1.12em 0; padding-left: 40pt` |
| `blockquote` | `margin: 1.12em 22.5pt` |
| `pre` | `margin: 1.12em 0; font-family: monospace; white-space: pre` |
| `body` | `margin: 8px` (the browser default) |
| `th` | `font-weight: bold` |
| `thead`, `tfoot` | `break-inside: avoid`, which lets them repeat on every page |
| `caption` | `display: table-caption; text-align: center` |

Some engine defaults differ from the CSS initial values:

- The default font family is stylo's initial value, `serif`.
- `line-height: normal` is `1.2`.
- `widows` defaults to `1`, where CSS specifies `2`. This matches Prince. An
  explicit `widows` declaration overrides it.

Margins at page breaks follow css-break-3. A box that starts fresh at the top
of a page keeps its top margin. The continuation of a box that was split
across pages has its top margin truncated to zero.

Spec: [Margin collapse](../specifications/margin-collapse.spec.md).

:::note[Known limitations]

- Only adjacent in-flow siblings collapse margins: the gap is the larger of
  the two margins. Parent and child margins do not collapse, and neither do
  margins through empty boxes.
- Floats and bare text between blocks break the sibling chain.
- The [UA print defaults spec](../specifications/ua-print-defaults.spec.md)
  still lists `body { margin: 0 }` and top-margin truncation for fresh boxes.
  The engine was later changed to the behavior described above.

:::

## Tables

Specs: [Tables × page breaks](../specifications/tables-fragmentation.spec.md),
[Auto table layout](../specifications/auto-table-layout.spec.md),
[Table first-page column freeze](../specifications/table-first-page-column-freeze.spec.md)
and [Table backgrounds](../specifications/table-backgrounds.spec.md).

### Table structure

| `display` value | Behavior |
|---|---|
| `table` | The table box. |
| `table-header-group`, `table-row-group`, `table-footer-group` | Row groups. |
| `table-row` | A row. Rows are the table's break points. |
| `table-cell` | A cell, with its own padding and borders. |
| `table-caption` | A caption, laid out outside the table grid (see [Captions](#captions)). |

The engine generates missing table boxes, as CSS Tables Level 3 requires:

- A run of rows, row groups or cells without a table parent is wrapped in an
  anonymous table.
- Content directly inside a table or row group is wrapped in an anonymous
  row.
- Non-cell content in a row is wrapped in an anonymous cell.

So `<div style="display: table">text</div>`, or a bare `display: table-row`
`<div>`, still renders its content. White space between rows creates no
boxes.

### Column widths

The engine uses the CSS automatic table layout algorithm:

- **min-content** is the widest whitespace-delimited word in a column, plus
  the cell's padding and borders. Slashes and hyphens do not split a word.
  This is a deliberate deviation from CSS, made to match Prince.
- **max-content** is the cell's text on a single line.
- An image, an inline SVG, or a block with a declared width inside a cell
  counts at its used width.
- The table's used width is its specified `width` (a length or a percentage).
  With no width, it is the smaller of the available width and the sum of
  max-content widths.
- Extra width is shared in two passes, as CSS Tables Level 3 §10.4.2
  describes. If the columns cannot fit, each keeps its min-content width and
  the table overflows.
- A `colspan` cell gives an equal share of its width to each column it spans.

When a table spans several pages, column widths are measured from the header,
the footer and the body rows that start on the first page. Those widths are
then kept for the whole table, which matches Prince. A table that fits on one
page is measured from all its rows.

### Borders

| Property | Behavior |
|---|---|
| `border-collapse: separate` | The default. Each cell keeps its own borders. `border-spacing` adds gaps between cells and between the table border and the outer cells. |
| `border-collapse: collapse` | Adjacent borders are drawn as a single line, and `border-spacing` is treated as 0. |

The table's border and the cell borders survive page breaks. The top border
is drawn on the first page, the side borders on every page and the bottom
border on the last page.

### Page breaks in tables

- A table breaks **between rows**. A row splits only when one of its cells is
  taller than a whole page. The interrupted cell then continues on the next
  page. Cells that had finished are not drawn again.
- A row that does not fit in the remaining space moves to the next page. It
  moves at most once. If it still does not fit there, it is placed anyway and
  overflows, so pagination always ends.
- `break-inside: avoid` on a row or row group moves it whole to the next
  page, unless it is taller than a page.
- **Repeating header and footer.** Only the first `thead`
  (`table-header-group`) is the table header, and only the first `tfoot` is
  the table footer. A header or footer repeats on later pages when it has
  `break-inside: avoid` (the default) and is no taller than a quarter of the
  page. Set `break-inside: auto` to stop it repeating.
- A header always stays with the first body row. If only the header fits at
  the bottom of a page, the whole table moves to the next page.

```html
<table>
  <thead><tr><th>Item</th><th>Qty</th></tr></thead>
  <tbody>
    <tr><td>Widget</td><td>12</td></tr>
    <tr><td>Gadget</td><td>4</td></tr>
  </tbody>
  <tfoot><tr><td>Total</td><td>16</td></tr></tfoot>
</table>
```

### Captions

A `<caption>`, or any `display: table-caption` child of a table, is laid out
outside the table border at the table's width:

- With `caption-side: top`, the caption goes above the grid. With `bottom`,
  it goes below the grid, after the footer.
- A caption does not repeat on later pages.
- A top caption stays with the first row.

### Backgrounds

`background-color` works on the table, row groups, rows and cells. The layers
are painted in this order: table, then row group, then row, then cell, then
borders and text. A table or row group that continues across pages paints
only the part on each page.

:::note[Known limitations]

- `table-layout: fixed` is ignored, and the automatic algorithm is used.
- Column widths set with `<col>` or a percentage on a cell are ignored. Only
  the table's own `width` (length or percentage) is used.
- **`rowspan` is not supported.** A row-spanning cell occupies one slot.
- `table-column`, `table-column-group` and `inline-table` are laid out as
  blocks. `<col>` and `<colgroup>` have no effect.
- Nested tables, right-to-left tables, and `caption-side` values other than
  `top` and `bottom` are not supported.
- Only `background-color` paints. Background images and gradients do not, and
  neither do `col` or `colgroup` backgrounds.
- Tables inside multi-column layouts paginate correctly, and repeated headers
  are sized against the column. Their column widths do not yet match other
  engines.

:::

## Floats

Specs: [CSS floats](../specifications/css-floats.spec.md),
[Floats × fragmentation](../specifications/floats-fragmentation.spec.md) and
[Page floats](../specifications/page-floats.spec.md).

| Property | Behavior |
|---|---|
| `float: left` or `right` | Takes the box out of the flow. It sits against the content edge, and the following text wraps beside it. |
| `width` | The float's width. Without it, the float shrinks to fit its content, up to the containing block's width. |
| `height` | A declared height sizes the float, even when it is empty. |

- A float that has no room beside earlier floats on the same side moves down
  below them.
- A box that establishes a block formatting context, such as
  `display: flow-root`, grows to contain its floats.
- **Page breaks.** A float taller than the space left on the page continues
  at the top of the next page. In-flow content continues beside it (parallel
  flow). A float that cannot fit at any lower position on the page moves
  whole to the next page.
- **Page floats.** `float: top`, `bottom` and `next-page` (and `snap`, which
  is treated as `top`) place a box at the top or bottom edge of the page's
  content area. `next-page` places it on the following page.

```css
figure { float: left; width: 2in; margin-right: 8pt; }
```

:::note[Known limitations]

- **`clear` is not implemented.** The engine has no computed `clear` value,
  so a `clear` declaration has no effect.
- Margins do not collapse across a float.
- Floats inside multi-column layouts, flex containers or absolutely positioned
  boxes are not supported. A `float` inside a table cell is ignored.
- Only the computed values `left` and `right` are used. `shape-outside` is not
  supported.
- Text beside a float is line-broken in segments, not as one whole paragraph.
- A page float is monolithic. If it is taller than the page, it is clipped.

:::

## Multi-column layout

Spec: [Multi-column layout](../specifications/multicol.spec.md).

| Property | Behavior |
|---|---|
| `column-count`, `column-width` | Column geometry, using the CSS multicol algorithm: `column-width` is the ideal width and `column-count` the maximum number of columns. |
| `column-gap` | The gap between columns. The default is `1em`. |
| `column-span: all` | The element spans every column. Content before it finishes in its own columns, and a new set of balanced columns starts after it. |
| `column-fill` | `balance` (the default) evens out column heights. With a definite `height`, `auto` fills each column in turn. |

- Columns are fragmentainers: content flows from column to column, then from
  page to page.
- Balanced columns differ in height by at most one line, after a bounded
  number of passes.
- When a multicol container breaks across pages, the columns on each page
  but the last fill to the bottom of the page. Only the last page is
  balanced. A column never slices a line.
- A container with a definite `height` that fits on the page has fixed-height
  columns. Content that does not fit goes into extra columns on the same page.
- `break-inside: avoid`, `widows`, `orphans`, and `break-before` or
  `break-after: avoid` between siblings all apply within each column. Adjacent
  sibling margins collapse inside columns.
- Multicol containers can be nested.

```css
.article { column-count: 2; column-gap: 12pt; }
h2 { column-span: all; }
```

:::note[Known limitations]

- **Column rules (`column-rule-*`) are not painted.**
- `break-before: column` and `break-after: column` do not force a column
  break.
- Floats, absolutely positioned boxes and flex containers inside multicol
  layouts are not supported.
- A balanced column set ignores `break-*: avoid` rather than spilling onto a
  new page.
- Vertical writing modes and `column-span` values other than `none` and `all`
  are not supported.

:::

## Flexbox

Spec: [Flexbox × fragmentation](../specifications/flexbox-fragmentation.spec.md).

`display: flex` creates a flex container. `inline-flex` is treated as a
block-level flex container.

| Property | Supported values |
|---|---|
| `flex-direction` | `row`, `row-reverse`, `column`, `column-reverse` |
| `flex-grow`, `flex-shrink` | Non-negative numbers. `flex: 1` means grow 1, shrink 1, basis 0%. |
| `flex-basis` | `auto`, `content`, a length or a percentage |
| `flex-wrap` | `nowrap`, and `wrap` for items with fixed bases (see the limitations) |
| `align-items`, `align-self` | `stretch` (the default), `flex-start`, `flex-end`, `center` |
| `justify-content` | `flex-start`, `flex-end`, `center`, `space-between` |
| `row-gap`, `column-gap` | Lengths |

- Cross sizes, including `stretch`, are measured before the container is split
  across pages.
- Auto margins on the cross axis absorb free space and override `align-self`.
- A column container with a definite height grows and shrinks its items along
  the main axis.
- Container borders are painted, and the items sit inside the border box.
- The container and its items break across pages. `break-inside: avoid` on an
  item moves it whole. A line with several items is never sliced. A line
  with a single item can split like a block.
- A flex container can be nested inside a flex item.

```css
.row { display: flex; column-gap: 10pt; }
.row > * { flex: 1; }
```

:::note[Known limitations]

- `flex-wrap: wrap` packs items only by their resolved base sizes (for
  example `flex: 0 0 2in`). Wrapping based on content size is not supported.
  `wrap-reverse` behaves like `wrap`.
- `order` is computed but ignored. Items are laid out in document order.
- `align-content`, `justify-content: space-around` and `space-evenly`, and
  baseline alignment are not supported.
- A declared `height` on a flex item is not used as its cross size.
- Flex containers inside floats, multicol layouts or absolutely positioned
  boxes are not supported.
- `break-before` and `break-after: avoid` between flex items are ignored.

:::

## Grid

Spec: [Grid layout](../specifications/grid-layout.spec.md).

`display: grid` creates a grid container. `inline-grid` is treated as
block-level. Grid support is a small subset of CSS Grid.

- Tracks come from `grid-template-columns` and `grid-template-rows`. Lengths
  and percentages resolve against the container. `auto` tracks fit their
  items and share any space left over equally. `fr` tracks share the
  remaining space in proportion. `minmax(a, b)` uses `a`, and
  `fit-content(b)` uses `b`.
- Items are the container's in-flow child elements. They are auto-placed row
  by row in document order.
- Each item is laid out like a normal block, so tables, flex containers and
  blocks work inside grid items.
- Rows do not split. A row that does not fit moves whole to the next page.

```css
.gallery { display: grid; grid-template-columns: 1fr 1fr 1fr; }
```

:::note[Known limitations]

- If a track list contains `repeat()`, the engine ignores the whole list and
  makes every track `auto`.
- Named lines and areas, explicit placement (`grid-column`, `grid-row`),
  `span`, dense packing, subgrid and masonry are not supported.
- `min-content` and `max-content` track sizes, non-default
  `align-self`/`justify-self`, and baseline alignment are not supported.
- Percentage row heights resolve against the page height.

:::

## Inline-block

Spec: [Inline-block](../specifications/inline-block.spec.md).

`display: inline-block` creates an atomic inline box:

- The box paints its own border, padding, background and margins (each
  margin applied once). Its width is the declared `width`, or shrink-to-fit
  without one.
- Inline-blocks that follow each other sit side by side on one line and wrap
  to a new line when they do not fit. An inline-block that follows text on
  the same line starts after the text and one collapsed space.
- All atomic boxes on a line share the line box, which is as tall as its
  tallest box.
- The box's baseline is its last line's baseline. A box with no lines aligns
  its bottom margin edge with the text baseline. A box taller than the line
  pushes the line's baseline down, and the line's text moves with it.
- `text-align` (`left`, `right`, `center`, `justify`, `start`, `end`) aligns
  text and inline-blocks together.

:::note[Known limitations]

- An inline-block is never split across pages. If it does not fit, it moves
  whole to the next page.
- `inline-table` and other inline variants are not supported (see
  [Tables](#tables)).

:::

## Positioning

Spec: [Out-of-flow positioning](../specifications/out-of-flow-positioning.spec.md).

| Value | Behavior |
|---|---|
| `position: relative` | Offsets the box without moving its siblings. Creates a containing block. |
| `position: absolute` | Takes the box out of the flow. It is positioned against the padding box of the nearest positioned ancestor, or against the page content box if there is none. |
| `position: fixed` | Laid out once against the page content box, and repeated on **every page**. |
| `position: sticky` | Treated as `relative`. |
| `top`, `right`, `bottom`, `left` | Lengths, resolved against the containing block's padding box. |
| `z-index` | Orders painting among positioned siblings. Positioned boxes paint after in-flow content. |

How absolutely positioned boxes break across pages:

- A box with no insets, anchored to the page, breaks across pages like an
  in-flow box.
- A box pinned by insets or by a positioned ancestor is placed once. If an
  inset puts it past the bottom of the page, the document grows to the page
  that contains it.
- A pinned box with a declared height that starts on a page but does not fit
  breaks across the following pages. Inside an ancestor that clips overflow
  (`overflow` other than `visible`), it overflows in place instead.

:::note[Known limitations]

- With `auto` insets, the box goes to a simplified static position: the
  containing block's top-left padding corner. The full CSS static-position
  rules are not implemented.
- Stacking contexts are not modeled. `z-index` only orders siblings.
- Absolutely positioned boxes inside floats, multicol layouts or flex
  containers are not supported. Transforms do not create containing blocks.

:::

## Images

Spec: [Images](../specifications/images.spec.md).

| Source | Behavior |
|---|---|
| `src="file.png"` | A relative path resolves against `--base-url`, or against the process working directory without it. Absolute paths are used as they are. |
| `src="data:image/png;base64,…"` | Decoded directly. `image/jpeg` and `image/gif` also work. |
| PNG, JPEG | Embedded without re-encoding. |
| GIF | Only the first frame is used, converted to PNG. |
| `content: url(…)` | Paints as a replaced image, including in `@page` margin boxes. |

Sizing follows the CSS 2.1 rules for replaced elements:

- The intrinsic size is the image's pixel size at 96 dpi, so 1 px is 0.75 pt.
- CSS `width` and `height` take precedence over the HTML attributes. HTML
  attributes are in CSS pixels. A percentage width resolves against the
  containing block.
- If both dimensions are given, they are used exactly, even if that changes
  the aspect ratio. If only one is given, the other follows the intrinsic
  aspect ratio.
- An image in a table cell sets the width of its column and the height of its
  row.

An image never splits across pages. If it does not fit, it moves whole to the
next page. Identical image bytes are embedded only once. A broken image does
not stop the render:

- With no explicit size, it collapses to zero size.
- With an explicit `width`, it draws a box of that width with the `alt` text
  inside.

:::note[Known limitations]

- **`<img>` is laid out as a block-level box.** An image in the middle of a
  paragraph splits the paragraph at that point.
- `object-fit` and `object-position` are not supported.
- WebP and other formats render as broken images.
- An `<img>` element inside a margin box is not supported. Use
  `content: url(…)` in the margin box instead.
- An image taller than the page is still not split. It is clipped at the page
  edge.

:::

## SVG

Spec: [SVG images](../specifications/svg-images.spec.md).

Both linked SVG (`<img src="chart.svg">`) and inline `<svg>` elements render:

- SVG is recognized by its content (an XML prolog or an `<svg` tag), not by
  the file extension.
- Each SVG is rasterized with resvg to a PNG with a transparent background, at
  1 SVG user unit = 1 CSS px (96 dpi). The raster is at most 16384 px on each
  side.
- An `<svg>` without `width` and `height` takes its size from its `viewBox`.
- An inline `<svg>` is a block-level replaced element. Its markup, including
  attributes such as `viewBox`, is passed on unchanged.
- Identical SVGs are embedded only once. Text in SVGs uses Arial as the
  default font.

:::note[Known limitations]

- **SVG is embedded as a raster image, not as vector PDF content.**
- Animation is ignored, and only the static image renders.
- External files referenced from inside an SVG are not loaded.
- Only the `width` and `height` attributes size an inline `<svg>`. CSS sizes
  do not.
- An SVG that is invalid, empty, or too large renders as a broken image.

:::

## Fonts

Spec: [Font resolution](../specifications/font-resolution.spec.md).

The engine resolves the whole `font-family` list in order. The first family
that resolves becomes the primary face.

- **`@font-face`.** Sources are `local(…)` and file `url(…)` paths, tried in
  order. Relative paths resolve against the input HTML file's directory. The
  supported descriptors are `font-family`, `font-weight` (a single value or a
  range) and `font-style`. An `@font-face` family takes precedence over an
  installed font with the same name.
- **Installed fonts.** The engine scans the system font directories once per
  process. Family names match case-insensitively, and `.ttc` collections
  work.
- **Weight and style** are matched as CSS Fonts Level 4 describes. Normal
  width is preferred. Below weight 400, lighter faces are tried first; at
  400 and above, heavier faces are tried first. When italic is requested,
  italic and oblique faces are tried first.
- **Per-character fallback.** If the primary face lacks a glyph, that text
  is reshaped with the next face in the list that has it. If no face has the
  glyph, the primary face's `.notdef` glyph is drawn.

Generic families resolve through a fixed table. The first installed family
wins:

| Generic | Candidates |
|---|---|
| `serif` (the default) | Times New Roman, then Liberation Serif |
| `sans-serif` | The bundled set: Arial on macOS, Liberation Sans in the container image |
| `monospace` | Courier New, then Liberation Mono |
| `cursive` | Comic Sans MS |
| `fantasy` | Papyrus, then Impact |

When a generic has no installed candidate, or no family in the list
resolves, the text falls back to the bundled sans-serif set. `Arial` and
`Helvetica` always resolve to the bundled set. Other generics, such as
`system-ui`, are treated as `sans-serif`. For the font paths in the container
image, see [Containers and packages](./container.md#fonts-in-the-image).

```css
@font-face {
  font-family: "Brand Sans";
  src: url("fonts/brand-sans.ttf");
  font-weight: 400;
}
body { font-family: "Brand Sans", Georgia, serif; }
```

:::note[Known limitations]

- WOFF and WOFF2 are not supported. Use TTF, OTF or TTC files.
- `http://` and `https://` font sources fail, and the next source is tried.
- Variable fonts use their default instance only.
- The bundled sans-serif faces are not embedded in the binary. They are read
  from macOS's Arial files or from `/usr/share/fonts/typeanvil/` (the
  container image).

:::

### OpenType features

Spec: [OpenType features](../specifications/opentype-features.spec.md).

| Property | Effect |
|---|---|
| `font-feature-settings` | Sets feature tags directly, for example `"smcp" 1, "liga" 0`. Applied last, so it overrides the `font-variant-*` properties. |
| `font-variant-ligatures` | `none` turns off `liga`, `clig`, `dlig`, `hlig` and `calt`. `no-common-ligatures` turns off `liga` and `clig`. `discretionary-ligatures` turns on `dlig`. |
| `font-variant-caps` | `small-caps` turns on `smcp`. |
| `font-variant-numeric` | `lining-nums`, `oldstyle-nums`, `proportional-nums`, `tabular-nums` and `diagonal-fractions` turn on `lnum`, `onum`, `pnum`, `tnum` and `frac`. |
| `font-variant-east-asian` | The JIS variants turn on `jp78`, `jp83` or `jp90`. `simplified` and `traditional` turn on `smpl` and `trad`. |

Standard ligatures (`liga`) and kerning (`kern`) are on by default. If a tag
appears twice, the last one wins. A face that lacks a feature ignores it.

:::note[Known limitations]

- `font-variant-position` is not supported, and neither are petite caps,
  unicase or titling caps.
- `font-variation-settings` is not supported, and neither are feature
  descriptors in `@font-face`.
- Features apply to whole words, not to individual characters. Small caps
  are not synthesized when a font lacks `smcp`.

:::

## Text, line breaking and hyphenation

Specs: [Typography layer](../specifications/typography-layer.spec.md),
[Hyphenation and line-break parity](../specifications/hyphenation-line-break-parity.spec.md),
[Existing-hyphen breaks](../specifications/existing-hyphen-break.spec.md),
[Line height](../specifications/line-height.spec.md),
[White space](../specifications/white-space-pre.spec.md) and
[Inline base direction](../specifications/inline-base-direction.spec.md).

### Line breaking and justification

- Text is shaped with HarfRust using the font's real glyph advances.
- Paragraphs are broken with the Knuth-Plass total-fit algorithm, which looks
  at the whole paragraph instead of filling one line at a time. Break
  opportunities follow Unicode line-breaking rules (UAX #14). For text that
  is not justified, the longest line that scores equally well wins.
- `text-align` accepts `left`, `right`, `center`, `justify`, `start` and
  `end`. With `justify`, every line except the last fills the measure. Font
  expansion of up to ±2% per line helps the fit.
- Punctuation at the start or end of a line hangs slightly into the margin
  (character protrusion). This changes the appearance only, not where lines
  break.
- `<br>` forces a line break. A line that ends at a `<br>` is not justified.
- Copying or searching text in the PDF gives the source text.

### Hyphenation

- `hyphens: auto` turns on hyphenation with Liang's English patterns.
  Hyphenation is off by default.
- A word is hyphenated only if it has at least 5 characters. A break leaves
  at least 2 characters before the hyphen.
- When a line breaks at a hyphen already in the text (such as `page-margin`),
  the line ends with exactly one hyphen.
- An automatic hyphen at the end of a line extracts as U+FFFE in the PDF's
  text layer.

### Line height, white space and direction

- `line-height` accepts `normal` (1.2), a number, a length or a percentage.
  The baseline sits in the line box using half-leading, as CSS 2.1 §10.8.1
  describes.
- `white-space` supports `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line` and
  `break-spaces`. In the preserving values, each newline is a forced break,
  spaces are kept and those lines are never justified. `pre` lines do not
  wrap and overflow if too long. `pre-wrap` and `pre-line` wrap at the
  content width. Long `pre` blocks split across pages between lines.
- `direction: rtl` makes `start` and `end` alignment right-to-left and
  places block boxes against the right edge. Under a right-to-left root, page
  1 becomes a `:left` page.
- `widows` and `orphans` apply at page and column breaks. Text is never lost
  at a break. A paragraph with no room for any lines moves to the next page.

:::note[Known limitations]

- Hyphenation patterns are English only, whatever the document's `lang`.
  `hyphens: manual` behaves like `none`, and soft hyphens (`&shy;`) are not
  used as break points. `hyphenate-character` is not supported.
- `line-height: normal` is always 1.2. It is not taken from the font's
  metrics.
- Mixed-direction text is not reordered: the bidi algorithm and
  `unicode-bidi` are not supported. Vertical writing modes are not supported
  for normal content.
- Tab characters keep their glyph advance. There are no tab stops.
- A word wider than the line overflows rather than being broken.
- On one line, the tallest box sets the line height. Superscript and
  subscript are not shifted.

:::

## Page breaks

Spec: [Fragmentation core](../specifications/fragmentation-core.spec.md).
For page-level properties, see [Paged media CSS](./paged-media-css.md).

- `break-before` and `break-after` with `page`, `always`, `left` or `right`
  (or the legacy `page-break-*` aliases) force a page break. A forced break
  wins over `break-inside: avoid`. A forced break at the very end of the
  document adds no page.
- `break-before` or `break-after: avoid` (and `avoid-page`, `avoid-column`)
  keeps adjacent block siblings together. For example, a heading moves to the
  next page with its paragraph.
- `break-inside: avoid`, `orphans` and `widows` steer where breaks happen. A
  box taller than a page still breaks.
- Content that cannot be split, such as lines and images, overflows the page
  rather than being sliced.
- A block with a declared `height` keeps that height across pages.

:::note[Known limitations]

- An `avoid` value does not pass from a first or last child up to its
  parent's boundaries.
- Flex items, grid items and table rows ignore `avoid` between siblings.

:::
