//! Block fragmentation: layout as a pure function producing a fragment tree.
//!
//! LayoutNG's model, greenfield. Layout is
//! `(node, constraints, break_token) → (fragment, outgoing_token)` with no
//! engine-global mutable state. The [`Fragment`] tree (in [`crate::frag`]) is
//! the sole output; pages are first-class [`Fragmentainer`]s.
//!
//! ## Algorithm
//!
//! [`layout`] cascades styles once, then paginates: it lays out `<body>` into
//! fragmentainer 0 with a break-before token; whenever a box runs out of
//! fragmentainer space it returns an outgoing [`BreakToken`] carrying
//! `consumed_block_size` + `seen_all_children` and nested child tokens. Page
//! N+1 replays the same recursion with that token — finished children skipped,
//! unfinished resumed — so each box is laid out a bounded number of times and
//! pagination is O(n) (no relayout-from-scratch per page).
//!
//! `break-inside: avoid`, `orphans`, and `widows` are honored at the offending
//! breakpoint: an avoid-block that would break but fits whole on a fresh page
//! is aborted and deferred wholly to the next fragmentainer (the once-per-flow
//! abort-and-defer that bounds avoidance cost); orphans/widows shift a text
//! run's split so no page ends or begins with fewer than the required lines,
//! dropping the constraint (css-break-3 §4.4) when the run is too short to
//! satisfy both. [`BreakAppeal`] classifies breakpoint quality (forced/clean →
//! last-resort) for these decisions.
//!
//! Monolithic content (a line taller than the page, a fixed-height box) is
//! never sliced: when it does not fit and the page is otherwise empty it is
//! placed anyway (last resort), overflowing its fragmentainer.
//!
//! Text line layout keeps the skeleton's approximate 0.5-em-per-char advance
//! (real shaping is a later issue) but now flows through the fragment model.

use std::collections::BTreeMap;

use crate::css::{
    cascade, ComputedStyle, Display, Float, Hyphens, Position, StringSetValue, Stylesheet,
    TextAlign, NORMAL_LINE_HEIGHT_FACTOR,
};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::frag::{
    BorderBox, BreakInside, BreakToken, ChildToken, Fragment, FragmentContent, FragmentKind,
    Fragmentainer, TextRun,
};
use crate::geom::{PageGeometry, Point, Scalar};

mod flex;
mod multicol;
use crate::paged::{
    parse_page_rules, resolve_page_spec, ContentPiece, MarginAlign, MarginBoxName, MarginRow,
    PageRule, PageSpec, RunningStrings,
};
use crate::table::{measure_columns, measure_rows};
use crate::typography::{break_paragraph, LineResult};

/// Approximate average glyph advance as a fraction of the em (font-size).
const AVG_ADVANCE_EM: f64 = 0.5;
/// Safety cap: a runaway that emits more pages than this is a bug, not a
/// document. Sized generously above the O(n) test (1,000 pages).
const MAX_PAGES: usize = 100_000;
/// Bounded two-pass TOC resolution: hard cap on layout passes (spec §9). The
/// last pass wins whether or not page numbers converged.
const MAX_TOC_PASSES: usize = 3;

/// A document-outline entry, in DOM order (nesting by `level` is resolved at
/// PDF-emit time). Built from `h1`–`h6` elements, each targeted at the page
/// its heading fragment landed on.
#[derive(Clone, Debug)]
pub struct Heading {
    /// Heading level 1–6.
    pub level: u8,
    /// The heading's text content (the outline entry title).
    pub title: String,
    /// Zero-based page index the heading landed on.
    pub page_index: usize,
}

/// One PDF link-annotation rect, in document paint order (CORE-104).
#[derive(Clone, Debug)]
pub struct PageLink {
    /// Zero-based page index the rect sits on.
    pub page_index: usize,
    /// Rect in page coordinates (top-left origin, points): x, y, w, h.
    pub x: Scalar,
    pub y: Scalar,
    pub w: Scalar,
    pub h: Scalar,
    /// Where the link goes.
    pub target: LinkTarget,
}

/// The destination of a [`PageLink`].
#[derive(Clone, Debug)]
pub enum LinkTarget {
    /// Verbatim URI action (external links).
    Url(String),
    /// Zero-based index of the destination page (internal links, resolved
    /// from `#fragment` via the id → page map).
    Page(usize),
}

/// The full paginated layout. `pages` *are* fragmentainers.
#[derive(Clone, Debug)]
pub struct Layout {
    /// The CLI/default geometry (the fallback when no `@page` rule applies).
    /// Per-page size lives on each fragmentainer's root `size`.
    pub geometry: PageGeometry,
    pub pages: Vec<Fragmentainer>,
    /// Heading outline for PDF bookmarks, in DOM order.
    pub headings: Vec<Heading>,
    /// Link annotation records, in document paint order (CORE-104).
    pub links: Vec<PageLink>,
    /// Interned raster images (CORE-106), keyed by content hash. The PDF
    /// emitter reads bytes through this store; layout fragments carry only
    /// the key.
    pub images: crate::images::ImageStore,
}

/// The outcome of laying out one box into one fragmentainer.
struct BlockResult {
    /// The produced fragment (empty children if nothing fit).
    fragment: Fragment,
    /// Block size consumed in this fragmentainer by this fragment.
    used: Scalar,
    /// Continuation for the next fragmentainer, or `None` if the box finished.
    outgoing: Option<BreakToken>,
    /// True if this box produced no content at all in this fragmentainer (used
    /// to suppress spurious empty fragments after a forced break).
    empty: bool,
}

/// One ordered piece of a block's content, in document order.
enum Item {
    /// A run of inline text (this block's own text between block children),
    /// with the `<a href>` link spans embedded in it (CORE-104). Byte
    /// offsets are into the run's own text; `href` is the raw attribute.
    /// The third field carries `(marker byte offset, footnote NodeId)` pairs
    /// for footnote call markers spliced into the run text (CORE-107).
    Text(String, Vec<LinkSpan>, Vec<(usize, NodeId)>),
    /// A block-level child element.
    Block(NodeId),
}

/// A recorded hyperlink source span inside an item's text (CORE-104):
/// `[start_byte, end_byte)` of the anchor's folded text plus the raw href.
#[derive(Clone, Debug)]
struct LinkSpan {
    start: usize,
    end: usize,
    href: String,
}

/// Mutable per-flow bookkeeping threaded through pagination in document order:
/// running strings and the page counter. Kept deterministic by only ever being
/// updated in pre-order fragment placement. (Layout geometry stays a pure
/// function of inputs; this is presentation state resolved alongside it.)
#[derive(Clone, Debug, Default)]
struct Flow {
    /// Running strings (`string-set` → `string()`), current values.
    running: RunningStrings,
    /// Page-counter base: `page number = base + fragmentainer_index`. A
    /// `counter-reset: page N` sets `base = N - current_index` so the reset
    /// element's page reads `N`.
    page_base: i32,
    /// The fragmentainer index currently being laid out (for counter reads).
    current_index: usize,
    /// Floats active on the CURRENT page (intrusions for text wrapping).
    /// Reset at the start of each fragmentainer to a clone of
    /// [`Flow::pending_floats`]; floats placed during the page are appended.
    active_floats: Vec<PlacedFloat>,
    /// Floats whose content continues into the NEXT fragmentainer (a float
    /// that suspended at the page boundary). Seeded into the next page's
    /// active set so text wraps around the continuation.
    pending_floats: Vec<PlacedFloat>,
    /// The nearest positioned ancestor's padding-box origin + content width on
    /// the current page — the containing block for abspos descendants. `None`
    /// = the initial containing block (the page content box).
    abspos_cb: Option<(Point, Scalar)>,
    /// Out-of-flow fragments placed on the current page, with page-absolute
    /// offsets and their `z-index` (`None` = auto). Drained into the
    /// fragmentainer root's children after each page (css-break-3: the
    /// fragmentainer is their parent, not the CSS containing block).
    abspos: Vec<(Option<i32>, Fragment)>,
    /// Footnote elements registered during THIS page's body layout, in call
    /// order (CORE-107). Registration happens when a marker's line actually
    /// places (a call pushed to the next page takes its note along). Drained
    /// after each page's body completes; the notes render in that page's
    /// footnote area.
    pending_footnotes: Vec<(usize, NodeId)>,
}

/// A placed float box: its rectangle in the fragmentainer and its side.
/// `y + height` is the float's bottom; lines below it stop being intruded.
/// `id` is the float's DOM node, used to match a resumed float to its
/// carry-over rectangle on the next fragmentainer.
#[derive(Clone, Copy, Debug)]
struct PlacedFloat {
    id: NodeId,
    x: Scalar,
    y: Scalar,
    width: Scalar,
    height: Scalar,
    side: Float,
}

impl PlacedFloat {
    fn bottom(&self) -> Scalar {
        self.y + self.height
    }
}

impl Flow {
    fn page_number(&self) -> i32 {
        self.page_base + self.current_index as i32
    }
}

#[derive(Clone, Debug, Default)]
struct TableContinuation {
    row_index: usize,
    row_offset: Scalar,
}

#[derive(Clone, Debug)]
struct TableRowState {
    row_id: NodeId,
    row_height: Scalar,
    cell_heights: Vec<Scalar>,
    cells: Vec<NodeId>,
}

#[derive(Clone, Debug)]
struct TableGroupState {
    group_id: NodeId,
    rows: Vec<TableRowState>,
}

#[derive(Clone, Debug, Default)]
struct TableState {
    header: Option<TableGroupState>,
    body: Vec<TableGroupState>,
    footer: Option<TableGroupState>,
    columns: Vec<Scalar>,
}

/// Immutable layout inputs, threaded by shared reference.
struct Ctx<'a> {
    dom: &'a Dom,
    styles: &'a [ComputedStyle],
    /// Content-box left edge and width for the page currently being laid out.
    content_x: Scalar,
    content_width: Scalar,
    /// Content-box TOP edge for the page currently being laid out (floats that
    /// suspend resume at this y on the next page).
    content_y: Scalar,
    /// Content-box height of the page currently being laid out.
    page_height: Scalar,
    /// Resolved target-counter page numbers by element `NodeId` (from the
    /// previous layout pass). Empty on the first pass.
    target_pages: &'a BTreeMap<NodeId, usize>,
    /// Total page count from the previous layout pass (0 on the first pass).
    /// `counter(pages)` reads this; margin-box text never affects pagination,
    /// so the second pass's count equals the first's and resolution converges.
    total_pages: usize,
    /// Link-rect sink (CORE-104): appended in paint order during line
    /// placement. Drained once, by `layout`, after the final pass. A
    /// `RefCell` because the layout methods take `&self`.
    links: &'a std::cell::RefCell<Vec<CollectedLink>>,
    /// Zero-based index of the page currently being laid out (link rects are
    /// tagged with it as lines are placed).
    page_index: usize,
    /// Interned `<img>` info by source node id (CORE-106). Populated once,
    /// before pagination, in document order.
    image_infos: &'a BTreeMap<NodeId, ImageInfo>,
    /// Footnote number per footnote-floated element id (CORE-107), assigned
    /// once per pass in document order. Read by the item collector when it
    /// splices call-marker digits; immune to layout retries within a pass.
    fn_numbers: &'a BTreeMap<NodeId, usize>,
}

/// Interned `<img>` info for one source element (CORE-106). Layout reads the
/// pixel dimensions for CSS2.1 replaced-element sizing; the PDF emitter reads
/// the bytes through `Layout.images` by `key`.
#[derive(Clone, Debug)]
pub(crate) struct ImageInfo {
    pub key: [u8; 32],
    /// True when the source failed to load or decode (placeholder mode).
    pub broken: bool,
    pub width_px: u32,
    pub height_px: u32,
}

/// Run the pipeline stage: cascade, then paginate into fragmentainers.
///
/// Public signature is unchanged (spec §14). Internally: parse `@page` rules,
/// resolve each page's spec independently, thread running-string / page-counter
/// state, attach margin boxes, and — when `target-counter` appears — run the
/// bounded two-pass TOC resolution.
pub fn layout(dom: &Dom, stylesheet: &Stylesheet, geometry: PageGeometry) -> Layout {
    layout_with_images(dom, stylesheet, geometry, None)
}

/// [`layout`] with an explicit image base directory (CORE-106). Relative
/// `<img src>` paths resolve against `base_url` (the CLI's `--base-url`
/// value); `None` resolves against the process working directory.
pub fn layout_with_images(
    dom: &Dom,
    stylesheet: &Stylesheet,
    geometry: PageGeometry,
    base_url: Option<&std::path::Path>,
) -> Layout {
    let mut images = crate::images::ImageStore::new();
    layout_with_images_and_store(dom, stylesheet, geometry, base_url, &mut images)
}

/// [`layout`] with an explicit image store too (tests inspect interned
/// entries through it).
pub fn layout_with_images_and_store(
    dom: &Dom,
    stylesheet: &Stylesheet,
    geometry: PageGeometry,
    base_url: Option<&std::path::Path>,
    images: &mut crate::images::ImageStore,
) -> Layout {
    let styles = cascade(dom, stylesheet, &geometry);
    let page_rules = parse_page_rules(stylesheet.source());
    let root = dom.find_tag("body").unwrap_or(dom.root);

    // Intern every <img> source ONCE before pagination (document order):
    // layout then only reads the store through the shared info table. Keys
    // are content hashes, so repeated references collapse for free.
    let mut image_infos: BTreeMap<NodeId, ImageInfo> = BTreeMap::new();
    collect_image_sources(dom, root, images, &mut image_infos, base_url);

    // A `display: none` on the document root (html) suppresses the whole
    // document: one valid empty page, no page-box chrome (CORE-66,
    // root-element-display-none — a blank page must compare equal to a
    // blank reference).
    if styles[dom.root].display == Display::None {
        let mut blank = Fragmentainer::new(0, (geometry.width, geometry.height));
        blank.background = None;
        return Layout {
            geometry,
            pages: vec![blank],
            headings: Vec::new(),
            links: Vec::new(),
            images: std::mem::take(images),
        };
    }

    // Does any generated content reference target-counter? If so, run the
    // bounded multi-pass resolution; otherwise a single pass suffices.
    let needs_toc = styles.iter().any(|s| {
        s.content
            .iter()
            .any(|p| matches!(p, ContentPiece::TargetCounter { .. }))
    });
    // `counter(pages)` needs the total page count, which is only known after
    // pagination. Margin-box / generated text never affects pagination, so one
    // extra pass with the previous pass's total converges; reuse the bounded
    // multi-pass loop (hard cap 3, last result wins).
    let needs_total = styles.iter().any(|s| {
        s.content
            .iter()
            .any(|p| matches!(p, ContentPiece::CounterPages))
    }) || page_rules.iter().any(|r| {
        r.margin_boxes.iter().any(|mb| {
            mb.content
                .iter()
                .any(|p| matches!(p, ContentPiece::CounterPages))
        })
    });
    let passes = if needs_toc || needs_total {
        MAX_TOC_PASSES
    } else {
        1
    };

    let mut fn_numbers: BTreeMap<NodeId, usize> = BTreeMap::new();
    let mut target_pages: BTreeMap<NodeId, usize> = BTreeMap::new();
    let mut total_pages: usize = 0;
    let mut collected_links: Vec<CollectedLink> = Vec::new();
    let mut pages = Vec::new();
    for _ in 0..passes {
        // Footnote numbers are per-PASS (CORE-107): document order is stable,
        // so the numbers never change mid-pass even though individual pages
        // may be re-laid-out by the footnote retry loop inside `paginate`.
        fn_numbers = collect_footnote_numbers(dom, &styles);
        let sink = std::cell::RefCell::new(Vec::new());
        let (p, map) = paginate(
            dom,
            &styles,
            &page_rules,
            root,
            &geometry,
            &target_pages,
            total_pages,
            &sink,
            &image_infos,
            &fn_numbers,
        );
        collected_links = sink.into_inner();
        // Converged when the target map is unchanged between passes AND the
        // page count is stable: resolved page numbers are stable, so the next
        // pass would be identical. `counter(pages)` resolves against
        // `total_pages`, so a stable count means the totals are final too.
        let converged = map == target_pages && p.len() == total_pages;
        pages = p;
        target_pages = map;
        total_pages = pages.len();
        if converged {
            break;
        }
    }
    // Build the heading outline from the final pass's element→page map.
    let headings = build_headings(dom, &target_pages);
    // Resolve internal `#fragment` links against the FINAL pass's id → page
    // map (the same map target-counter uses); unresolved ids drop silently.
    let links = resolve_links(collected_links, dom, &target_pages);
    Layout {
        geometry,
        pages,
        headings,
        links,
        images: std::mem::take(images),
    }
}

/// Resolve collected link rects into [`Layout::links`] (CORE-104).
///
/// `#fragment` hrefs resolve against the element id → page map built from the
/// FINAL pagination pass (the same map target-counter reads); the first
/// element in document order carrying the id wins (the map only inserts when
/// absent). Any other href is a verbatim URI. Unresolved fragments drop the
/// annotation silently (Behavior 4).
fn resolve_links(
    collected: Vec<CollectedLink>,
    dom: &Dom,
    target_pages: &BTreeMap<NodeId, usize>,
) -> Vec<PageLink> {
    // id string → NodeId of the first element carrying it, in document
    // (pre-order) order. BTreeMap: deterministic construction, no hash order.
    let mut id_pages: BTreeMap<&str, usize> = BTreeMap::new();
    for (idx, node) in dom.nodes.iter().enumerate() {
        if let Some(el) = node.kind.element() {
            if let Some(id) = &el.id {
                id_pages
                    .entry(id.as_str())
                    .or_insert_with(|| target_pages.get(&idx).copied().unwrap_or(usize::MAX));
            }
        }
    }
    collected
        .into_iter()
        .filter_map(|c| {
            let target = if let Some(frag) = c.href.strip_prefix('#') {
                let page = *id_pages.get(frag)?;
                if page == usize::MAX {
                    // The id exists but its element produced no fragment on
                    // any page (suppressed box): nothing to link to.
                    return None;
                }
                LinkTarget::Page(page)
            } else {
                LinkTarget::Url(c.href)
            };
            Some(PageLink {
                page_index: c.page_index,
                x: c.x,
                y: c.y,
                w: c.w,
                h: c.h,
                target,
            })
        })
        .collect()
}

/// A link rect still carrying its raw href (pre-`#fragment`-resolution).
#[derive(Clone, Debug)]
struct CollectedLink {
    page_index: usize,
    x: Scalar,
    y: Scalar,
    w: Scalar,
    h: Scalar,
    href: String,
}

/// Intersect a placed line's source span against its run's recorded link
/// spans and append one rect per overlapping segment (CORE-104 Behavior 2).
///
/// `line_start` is the line's byte offset within the ITEM's text (the sum of
/// `lr.consumed` of all preceding lines of the same break); glyph byte ranges
/// are relative to the line's rebuilt text, which begins at that offset. The
/// rect covers the glyphs whose clusters overlap the link span, at height
/// `lh` with top edge `y`, on `page_index`.
fn collect_line_links(
    out: &mut Vec<CollectedLink>,
    page_index: usize,
    lr: &crate::typography::LineResult,
    line_start: usize,
    x: Scalar,
    y: Scalar,
    lh: Scalar,
    link_spans: &[LinkSpan],
) {
    if link_spans.is_empty() {
        return;
    }
    let line_end = line_start + lr.consumed;
    for span in link_spans {
        // Overlap of the link span with this line's source span. A link
        // breaking across pages produces one overlap per line (Behavior 3).
        let s = span.start.max(line_start);
        let e = span.end.min(line_end);
        if e <= s {
            continue;
        }
        // Map the overlapping item-text bytes to glyph x extents. Glyph
        // cluster ranges index the line's rebuilt text, which begins at the
        // item-text offset `line_start`.
        let mut x0 = f64::MAX;
        let mut x1 = f64::MIN;
        let mut cursor = 0.0f64;
        let scale = 1.0 + lr.expansion;
        for g in &lr.glyphs {
            let g_start = line_start + g.range.start;
            let g_end = line_start + g.range.end;
            let adv = g.x_advance.get() * scale;
            if g_end > s && g_start < e {
                x0 = x0.min(cursor + g.x_offset.get());
                x1 = x1.max(cursor + adv);
            }
            cursor += adv;
        }
        if x1 <= x0 {
            continue;
        }
        out.push(CollectedLink {
            page_index,
            x: x + Scalar(x0),
            y,
            w: Scalar(x1 - x0),
            h: lh,
            href: span.href.clone(),
        });
    }
}

/// One full pagination pass. Returns the pages and the element→page-index map
/// (first fragmentainer each sourced element appears on, in `NodeId` order).
/// Link rects (CORE-104) accumulate into `links` in paint order.
///
/// Collect footnote-floated elements in document (pre-order) order with
/// their 1-based numbers (CORE-107). Computed ONCE per pagination pass so
/// numbering is immune to layout retries within the pass.
fn collect_footnote_numbers(dom: &Dom, styles: &[ComputedStyle]) -> BTreeMap<NodeId, usize> {
    fn walk(
        dom: &Dom,
        id: NodeId,
        styles: &[ComputedStyle],
        next: &mut usize,
        out: &mut BTreeMap<NodeId, usize>,
    ) {
        if styles[id].float_footnote {
            *next += 1;
            out.insert(id, *next);
        }
        for &child in &dom.nodes[id].children {
            walk(dom, child, styles, next, out);
        }
    }
    let mut next = 0usize;
    let mut out = BTreeMap::new();
    walk(dom, dom.root, styles, &mut next, &mut out);
    out
}

/// Total height of the footnote band for one page's notes (CORE-107): the
/// sum of each note's monolithic content height at the content width (notes
/// never split — Prince probe 2). Empty notes contribute zero.
fn footnote_area_height(
    dom: &Dom,
    styles: &[ComputedStyle],
    notes: &[(usize, NodeId)],
    content_width: Scalar,
) -> Scalar {
    let mut h = Scalar::ZERO;
    for (_, id) in notes {
        let style = &styles[*id];
        if style.display == Display::None {
            continue;
        }
        let inner = content_width - style.margin_left - style.margin_right;
        let lines = break_paragraph(
            &dom.text_content(*id),
            inner,
            style,
            false,
            false,
        );
        h += style.line_height * lines.len() as f64;
    }
    h
}

/// Attach a page's footnotes into the band above the bottom margin edge
/// (CORE-107). Each note renders as hanging-indent lines bottom-up: the
/// first line starts `MARKER_HANG` left of the content edge with an
/// `"N."` marker; wrapped lines return to the content edge. Notes stack in
/// call order starting at the band top.
#[allow(clippy::too_many_arguments)]
fn attach_footnotes(
    fragmentainer: &mut Fragmentainer,
    dom: &Dom,
    styles: &[ComputedStyle],
    notes: &[(usize, NodeId)],
    content_x: Scalar,
    content_width: Scalar,
    content_bottom: Scalar,
    area_height: Scalar,
) {
    const MARKER_HANG: f64 = 8.5;
    let font_size = Scalar(9.0);
    let lh_note = Scalar(13.45);
    let face = crate::fonts::FACE_REGULAR;
    let mut y = content_bottom - area_height;
    for (num, id) in notes {
        let style = &styles[*id];
        if style.display == Display::None {
            continue;
        }
        let text = dom.text_content(*id);
        let inner = content_width - style.margin_left - style.margin_right;
        // First line reserves room for the "N. " prefix.
        let first_text_w = inner - Scalar(MARKER_HANG);
        let rest_w = inner;
        let first_lines =
            break_paragraph(&text, first_text_w, style, false, false);
        if first_lines.is_empty() {
            continue;
        }
        // Lay the note's lines: first with the marker, rest full width. The
        // note is monolithic — all its lines place inside the measured band.
        for (li, lr) in first_lines.iter().enumerate() {
            let (x, w) = if li == 0 {
                (content_x - Scalar(MARKER_HANG), first_text_w)
            } else {
                (content_x, rest_w)
            };
            let run = if li == 0 {
                // Marker "N. " shaped on its own; its glyph ranges already
                // index the marker text, which is the PREFIX of the combined
                // run text (ToUnicode stays correct). The body's first glyph
                // carries the marker's advance as an x_offset so the body
                // clears the marker.
                let marker = format!("{}. ", num);
                let m = crate::typography::shape_word(&marker, font_size, face);
                let mut glyphs = m.glyphs.clone();
                let marker_w = m.width;
                let mut bg = lr.glyphs.clone();
                if let Some(first) = bg.first_mut() {
                    first.x_offset += marker_w;
                }
                for g in &mut bg {
                    g.range.start += m.text.len();
                    g.range.end += m.text.len();
                }
                glyphs.extend(bg);
                let mut text_all = m.text;
                text_all.push_str(&lr.text);
                TextRun {
                    text: text_all,
                    baseline: Point::new(
                        x,
                        y + crate::typography::baseline_offset(font_size, lh_note, face),
                    ),
                    font_size,
                    color: style.color,
                    font_face: face,
                    glyphs,
                    expansion: 0.0,
                    protrude_left: Scalar::ZERO,
                    protrude_right: Scalar::ZERO,
                }
            } else {
                TextRun {
                    text: lr.text.clone(),
                    baseline: Point::new(
                        x,
                        y + crate::typography::baseline_offset(font_size, lh_note, face),
                    ),
                    font_size,
                    color: style.color,
                    font_face: face,
                    glyphs: lr.glyphs.clone(),
                    expansion: lr.expansion,
                    protrude_left: Scalar::ZERO,
                    protrude_right: Scalar::ZERO,
                }
            };
            let w_used = if li == 0 { inner } else { rest_w };
            let w_final = if w_used.get() > w.get() { w_used } else { w };
            fragmentainer
                .root
                .children
                .push(Fragment::line(Point::new(x, y), (w_final, lh_note), run));
            y += lh_note;
        }
    }
}

#[allow(clippy::type_complexity)]
fn paginate(
    dom: &Dom,
    styles: &[ComputedStyle],
    page_rules: &[PageRule],
    root: NodeId,
    cli: &PageGeometry,
    target_pages: &BTreeMap<NodeId, usize>,
    total_pages: usize,
    links: &std::cell::RefCell<Vec<CollectedLink>>,
    image_infos: &BTreeMap<NodeId, ImageInfo>,
    fn_numbers: &BTreeMap<NodeId, usize>,
) -> (Vec<Fragmentainer>, BTreeMap<NodeId, usize>) {
    let mut pages: Vec<Fragmentainer> = Vec::new();
    let mut incoming = Some(BreakToken::break_before());
    let mut flow = Flow {
        running: RunningStrings::new(),
        page_base: 1,
        current_index: 0,
        active_floats: Vec::new(),
        pending_floats: Vec::new(),
        abspos_cb: None,
        abspos: Vec::new(),
        pending_footnotes: Vec::new(),
    };
    // The named page in effect, carried across pages until an element switches
    // it (spec §4).
    let mut current_name: Option<String> = None;

    while let Some(token) = incoming.take() {
        let page_index = pages.len();
        flow.current_index = page_index;
        // Each fragmentainer starts with only the carry-over floats: floats
        // that suspended at the previous page boundary resume at the content
        // top and keep intruding on this page's text. The abspos containing
        // block resets too (origins are page-absolute).
        flow.active_floats = flow.pending_floats.clone();
        flow.abspos_cb = None;

        // Resolve the named page in effect for this page: a `page:<name>` box
        // that starts fresh at the top of this page switches the context; a
        // fresh box whose effective `page` is the default (auto/unset) resets
        // it; a pure continuation page carries the previous name (spec §4).
        match active_page_name(dom, styles, root, &token) {
            PageCtx::Named(name) => current_name = Some(name),
            PageCtx::Reset => current_name = None,
            PageCtx::Carry => {}
        }
        let spec = resolve_page_spec(page_rules, current_name.as_deref(), page_index, cli);
        let geo = spec.geometry();
        let content = geo.content_rect();

        let ctx = Ctx {
            dom,
            styles,
            content_x: content.x,
            content_width: content.width,
            content_y: content.y,
            page_height: content.height,
            target_pages,
            total_pages,
            links,
            page_index,
            image_infos,
            fn_numbers,
        };
        let mut fragmentainer = Fragmentainer::new(page_index, spec.size);
        fragmentainer.background = spec.background;
        fragmentainer.page_orientation = spec.page_orientation;

        // CORE-107: lay the body full-height first, then reserve the footnote
        // area if any call marker PLACED on this page (`pending_footnotes`).
        // When the body ran past the reserved band, re-lay the page with the
        // floor raised — Flow snapshots restore exactly, and the link sink
        // truncates so rects from the discarded attempt do not leak. The
        // retry places a strict SUBSET of the first attempt's lines, so the
        // second attempt's registrations never exceed the first's and the
        // measured band is always big enough.
        let content_bottom = content.y + content.height;
        let saved_flow = flow.clone();
        let saved_links_len = links.borrow().len();
        let mut res = ctx.layout_root(root, content.y, &token, &mut flow);
        if !fn_numbers.is_empty() && !flow.pending_footnotes.is_empty() {
            let area_h =
                footnote_area_height(dom, styles, &flow.pending_footnotes, content.width);
            if area_h.get() > 0.0 {
                let area_top = content_bottom - area_h;
                if content.y + res.used > area_top {
                    flow = saved_flow.clone();
                    links.borrow_mut().truncate(saved_links_len);
                    let ctx2 = Ctx {
                        dom,
                        styles,
                        content_x: content.x,
                        content_width: content.width,
                        content_y: content.y,
                        page_height: area_top - content.y,
                        target_pages,
                        total_pages,
                        links,
                        page_index,
                        image_infos,
                        fn_numbers,
                    };
                    res = ctx2.layout_root(root, content.y, &token, &mut flow);
                }
                // Attach THIS page's notes (the post-retry registrations)
                // into the band above the bottom margin.
                attach_footnotes(
                    &mut fragmentainer,
                    dom,
                    styles,
                    &std::mem::take(&mut flow.pending_footnotes),
                    content.x,
                    content.width,
                    content_bottom,
                    area_h,
                );
            } else {
                flow.pending_footnotes.clear();
            }
        } else {
            flow.pending_footnotes.clear();
        }
        if !res.empty {
            fragmentainer.root.children.push(res.fragment);
        }

        // Margin boxes resolve against the running-string / counter state now
        // in effect at the end of this page's flow (spec §6, §7).
        attach_margin_boxes(
            &mut fragmentainer,
            &spec,
            &geo,
            &flow,
            styles,
            dom,
            target_pages,
            total_pages,
        );

        // Out-of-flow fragments attach to the page root (css-break-3: the
        // fragmentainer is their parent, not the CSS containing block).
        // Stable sort by z-index (None/auto first = painted below); ties keep
        // document order. Offsets are adjusted into the root's coordinate
        // space (the root is the body fragment at the content origin).
        if !flow.abspos.is_empty() {
            flow.abspos.sort_by_key(|(z, _)| *z);
            for (_, mut frag) in std::mem::take(&mut flow.abspos) {
                frag.offset = Point::new(frag.offset.x - content.x, frag.offset.y - content.y);
                fragmentainer.root.children.push(frag);
            }
        }

        pages.push(fragmentainer);
        incoming = res.outgoing;

        if pages.len() >= MAX_PAGES {
            break;
        }
    }

    if pages.is_empty() {
        let spec = resolve_page_spec(page_rules, None, 0, cli);
        let mut fragmentainer = Fragmentainer::new(0, spec.size);
        let geo = spec.geometry();
        attach_margin_boxes(
            &mut fragmentainer,
            &spec,
            &geo,
            &flow,
            styles,
            dom,
            target_pages,
            total_pages,
        );
        pages.push(fragmentainer);
    }

    // Build the element→page-index map from fragment sources (first page each
    // element lands on), in NodeId order for determinism.
    let mut map: BTreeMap<NodeId, usize> = BTreeMap::new();
    for page in &pages {
        record_sources(&page.root, page.index, &mut map);
    }

    (pages, map)
}

impl<'a> Ctx<'a> {
    /// Lay out an `<img>` as a monolithic replaced-element box (CORE-106).
    ///
    /// Sizing (spec Behavior 6): CSS width/height wins; then the HTML
    /// attributes; then the intrinsic size at 96 dpi (1 px = 0.75 pt); one
    /// specified dimension scales the other by the intrinsic ratio. A broken
    /// source uses the same rules against the attribute sizes, falling back
    /// to the CSS2.1 suggested size 300×150 px, and draws its alt text.
    ///
    /// Fragmentation (spec Behavior 8): monolithic. When the box doesn't fit
    /// in the remaining fragmentainer space it defers whole (a break-before
    /// token), with the empty-page last-resort exception used by text lines
    /// (an over-tall image on an empty page overflows instead of looping).
    /// Resolve an `<img>`'s used content-box size in points (CORE-106 spec
    /// Behavior 6). Shared by [`Self::layout_image`] and
    /// [`Self::measure_image`] so measured == laid-out.
    fn image_used_size(
        &self,
        id: NodeId,
        avail_width: Scalar,
        style: &ComputedStyle,
    ) -> (Scalar, Scalar, bool) {
        let info = self.image_infos.get(&id);
        let el = self.dom.nodes[id].kind.element();

        // Attribute lengths are CSS px (HTML spec): 1 px = 0.75 pt.
        let attr_w = el
            .and_then(|e| e.attr("width"))
            .and_then(parse_px_attr)
            .map(|v| v * 0.75);
        let attr_h = el
            .and_then(|e| e.attr("height"))
            .and_then(parse_px_attr)
            .map(|v| v * 0.75);
        // CSS percentages resolve against the containing block's content
        // width; height percentages resolve to auto (v1).
        let css_w = style
            .width
            .or_else(|| style.width_percent.map(|p| Scalar(p * avail_width.get())));
        let css_h = style.height;

        let intrinsic_w = info
            .filter(|i| !i.broken)
            .map(|i| Scalar(i.width_px as f64 * 0.75));
        let intrinsic_h = info
            .filter(|i| !i.broken)
            .map(|i| Scalar(i.height_px as f64 * 0.75));
        // Broken images: attribute size, else COLLAPSE to zero height (v1,
        // CORE-106 gate finding). The CSS2.1 300×150 default suggestion is
        // for visual placeholders; here a missing file must not inject a
        // large box into documents that never expected the image to render
        // (the engine ignores `position:absolute` inline styles, so WPT refs
        // that lean on abspos images would gain a 112.5pt in-flow block and
        // reflow). Alt text still draws (Behavior 7); an author who wants a
        // visible placeholder sets explicit width/height.
        let fallback_w = attr_w.map(Scalar).unwrap_or(Scalar::ZERO);
        let fallback_h = attr_h.map(Scalar).unwrap_or(Scalar::ZERO);

        let (used_w, used_h) = match (css_w, css_h) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (
                w,
                match (intrinsic_w, intrinsic_h) {
                    (Some(iw), Some(ih)) if iw.get() > 0.0 => Scalar(w.get() * ih.get() / iw.get()),
                    _ => attr_h.map(Scalar).unwrap_or(fallback_h),
                },
            ),
            (None, Some(h)) => (
                match (intrinsic_w, intrinsic_h) {
                    (Some(iw), Some(ih)) if ih.get() > 0.0 => Scalar(h.get() * iw.get() / ih.get()),
                    _ => attr_w.map(Scalar).unwrap_or(fallback_w),
                },
                h,
            ),
            (None, None) => match (attr_w, attr_h) {
                (Some(w), Some(h)) => (Scalar(w), Scalar(h)),
                (Some(w), None) => (
                    Scalar(w),
                    match (intrinsic_w, intrinsic_h) {
                        (Some(iw), Some(ih)) if iw.get() > 0.0 => Scalar(w * ih.get() / iw.get()),
                        _ => fallback_h,
                    },
                ),
                (None, Some(h)) => (
                    match (intrinsic_w, intrinsic_h) {
                        (Some(iw), Some(ih)) if ih.get() > 0.0 => Scalar(h * iw.get() / ih.get()),
                        _ => fallback_w,
                    },
                    Scalar(h),
                ),
                (None, None) => (
                    intrinsic_w.unwrap_or(fallback_w),
                    intrinsic_h.unwrap_or(fallback_h),
                ),
            },
        };
        // Never exceed the available width (shrink, keep ratio).
        if used_w.get() > avail_width.get() && used_w.get() > 0.0 {
            let scale = avail_width.get() / used_w.get();
            (avail_width, Scalar(used_h.get() * scale), true)
        } else {
            (used_w, used_h, false)
        }
    }

    /// Greedy height measure of an `<img>` box for break-inside:avoid /
    /// float measurement contexts (mirrors layout_image's placement).
    fn measure_image(&self, id: NodeId, avail_width: Scalar) -> Scalar {
        let style = &self.styles[id];
        let (used_w, used_h, _) = self.image_used_size(id, avail_width, style);
        style.margin_top + style.padding_top + used_h + style.padding_bottom + style.margin_bottom
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_image(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        first_in_flow: bool,
        style: &ComputedStyle,
        flow: &mut Flow,
    ) -> BlockResult {
        let el = self.dom.nodes[id].kind.element();
        let info = self.image_infos.get(&id);

        // ---- resolve used width/height (points), shared with measure ----
        let (used_w, used_h, _) = self.image_used_size(id, avail_width, style);

        // Margins: fresh box at a fragmentainer start truncates top margin
        // like any block (css-break-3 / CORE-95).
        let fresh = true; // images never resume; they are monolithic
        let margin_top = if fresh && first_in_flow {
            Scalar::ZERO
        } else {
            style.margin_top
        };
        let box_top = top + margin_top;
        let box_left = origin_x + style.margin_left;
        let box_w = used_w + style.margin_left + style.margin_right;

        // Monolithic placement: fits, or defer whole; on an empty page place
        // anyway (last resort — never loop).
        let fits = box_top + used_h <= bottom_limit;
        let last_resort = !page_has_content;
        if !fits && !last_resort {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: Some(BreakToken::break_before()),
                empty: true,
            };
        }

        let mut fragment = Fragment::block(Point::new(box_left, box_top), (box_w, used_h));
        if let Some(info) = info {
            fragment.content = FragmentContent::Image(crate::frag::ImageRun {
                key: info.key,
                alt: el.and_then(|e| e.attr("alt")).map(|s| s.to_string()),
                broken: info.broken,
            });
            // Broken images draw their alt text inside the placeholder box
            // (spec Behavior 7): one shaped line at the box's first baseline.
            // v1 gate finding: only when the author gave the box explicit
            // WIDTH space (a width attr/CSS size). A fully collapsed broken
            // box draws nothing — documents that never expected the image to
            // render must stay pixel-identical.
            if info.broken && used_w.get() > 0.0 {
                if let Some(alt) = el.and_then(|e| e.attr("alt")) {
                    if !alt.trim().is_empty() {
                        let face = style.font_face;
                        let lh = style.line_height;
                        let baseline =
                            box_top + crate::typography::baseline_offset(style.font_size, lh, face);
                        let shaped = crate::typography::shape_word(alt, style.font_size, face);
                        let run = TextRun {
                            text: shaped.text,
                            baseline: Point::new(box_left, baseline),
                            font_size: style.font_size,
                            color: style.color,
                            font_face: face,
                            glyphs: shaped.glyphs,
                            expansion: 0.0,
                            protrude_left: Scalar::ZERO,
                            protrude_right: Scalar::ZERO,
                        };
                        fragment.children.push(Fragment::line(
                            Point::new(box_left, box_top),
                            (used_w, lh),
                            run,
                        ));
                    }
                }
            }
        }

        BlockResult {
            used: used_h + margin_top,
            fragment,
            outgoing: None,
            empty: false,
        }
    }

    /// Lay the root/body box directly into the page content box. The root box
    /// itself carries no page margin; its children flow from `content_top`.
    fn layout_root(
        &self,
        id: NodeId,
        content_top: Scalar,
        token: &BreakToken,
        flow: &mut Flow,
    ) -> BlockResult {
        // The root is laid out like any block, positioned at the content-box
        // origin. `bottom_limit` is the absolute y of the content-box bottom.
        let bottom_limit = content_top + self.page_height;
        self.layout_box(
            id,
            self.content_x,
            self.content_width,
            content_top,
            bottom_limit,
            false,
            true,
            token,
            flow,
        )
    }

    /// Lay one block into the current fragmentainer.
    ///
    /// - `origin_x`: left edge for this box's border-box (points).
    /// - `avail_width`: width available for this box's border-box.
    /// - `top`: y where this box starts in the fragmentainer (points).
    /// - `bottom_limit`: y beyond which content does not fit (the fragmentainer
    ///   content bottom).
    /// - `page_has_content`: whether the fragmentainer already holds any
    ///   content (in-flow or out-of-flow) at or above this box's flow
    ///   position — drives last-resort monolithic placement only.
    /// - `first_in_flow`: whether this box is the first IN-FLOW content on
    ///   the fragmentainer (floats/abspos do not consume it). Drives
    ///   margin-top truncation at fragmentainer starts (css-break-3).
    /// - `token`: incoming continuation (break-before = start fresh).
    fn layout_box(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        first_in_flow: bool,
        token: &BreakToken,
        flow: &mut Flow,
    ) -> BlockResult {
        let style = &self.styles[id];
        if style.display == Display::None {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (Scalar::ZERO, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: None,
                empty: true,
            };
        }

        // Replaced element: `<img>` lays out as a monolithic block-level box
        // (CORE-106 spec Behavior 2). Sizing per spec Behavior 6; a box that
        // does not fit defers whole to the next fragmentainer (Behavior 8);
        // broken sources render an alt-text placeholder (Behavior 7).
        let is_img = self.dom.nodes[id]
            .kind
            .element()
            .is_some_and(|el| el.tag == "img");
        if is_img {
            return self.layout_image(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                first_in_flow,
                style,
                flow,
            );
        }

        // Table-family boxes dispatch through layout_table_like, which keeps
        // `first_in_flow` for its layout_box fall-throughs (tables themselves
        // do not apply margins yet, so truncation only matters once their
        // cells delegate back into the block path).
        if matches!(
            style.display,
            Display::Table
                | Display::TableRowGroup
                | Display::TableHeaderGroup
                | Display::TableFooterGroup
        ) {
            return self.layout_table_like(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                first_in_flow,
                token,
                flow,
            );
        }
        if matches!(style.display, Display::TableRow) {
            return self.layout_table_like(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                first_in_flow,
                token,
                flow,
            );
        }
        // Flex containers (css-flexbox-1, CORE-65): rows pack lines down the
        // page, columns stack items like blocks. `inline-flex` is treated as
        // a block-level flex container in paged flow (spec Goal 1).
        if matches!(style.display, Display::Flex | Display::InlineFlex) {
            // A flex container whose children are all TEXT (no block-level
            // items, e.g. `display: flex` on a leaf) falls back to the block
            // path so the text renders as a normal paragraph. Anonymous
            // text flex items are a spec non-goal; without the fallback the
            // container would render empty and diverge from block
            // references (page-name-fixed-pos-001).
            let has_items = self
                .collect_items(id)
                .iter()
                .any(|i| matches!(i, Item::Block(_)));
            if has_items {
                return self.layout_flex_container(
                    id,
                    origin_x,
                    avail_width,
                    top,
                    bottom_limit,
                    page_has_content,
                    token,
                    flow,
                );
            }
        }
        // NOTE: TableCell deliberately does NOT dispatch here — layout_table_cell
        // delegates back into layout_box to lay out the cell's content as a
        // block; routing it through layout_table_like again would recurse forever.
        // TableRowGroup/HeaderGroup/FooterGroup are handled above (they are
        // laid out by layout_table_group, which calls layout_table_row directly).

        let fresh = token.is_break_before();
        let mut first_in_flow = first_in_flow;

        // Margins/padding adjoining a fragmentainer break truncate to zero
        // (css-break-3). On resume (not fresh) the top margin/padding is gone.
        // Additionally (CORE-95), when a fresh box is the FIRST in-flow box on
        // a fragmentainer, its top margin adjoins the fragmentainer boundary
        // and truncates to zero too — matching Prince, which renders an
        // unstyled h1 at the top of a page flush with the content top (its
        // 16pt UA margin is honored mid-page, never at a page start).
        // Floats/abspos do not consume `first_in_flow`: a paragraph after a
        // top-of-page float is still the first in-flow box (verified vs
        // Prince 16.2, 2026-08-20).
        let margin_top = if fresh && first_in_flow {
            Scalar::ZERO
        } else if fresh {
            style.margin_top
        } else {
            Scalar::ZERO
        };
        let padding_top = if fresh {
            style.padding_top
        } else {
            Scalar::ZERO
        };

        let box_top = top + margin_top;
        let inner_left = origin_x + style.margin_left + style.padding_left;
        let inner_width = avail_width
            - style.margin_left
            - style.margin_right
            - style.padding_left
            - style.padding_right;
        let content_top = box_top + padding_top;

        // Build the ordered child-item list (stable, document order).
        // When the box carries generated `content`, that content *replaces*
        // the element's children (css-gcpm): only the generated line is laid
        // out, never the box's own text/children.
        let items = if style.content.is_empty() {
            self.collect_items(id)
        } else {
            Vec::new()
        };

        // Apply this element's running-string / counter state when its box
        // starts on this page (spec §7, §8). `content()` assigns the element's
        // full text content; `counter-reset: page N` rebases the page counter
        // so this element's page reads N.
        if fresh {
            for (name, val) in &style.string_set {
                match val {
                    StringSetValue::Content => {
                        flow.running.set(name, self.dom.text_content(id));
                    }
                }
            }
            for (name, n) in &style.counter_reset {
                if name.eq_ignore_ascii_case("page") {
                    flow.page_base = *n - flow.current_index as i32;
                }
            }
            for (name, n) in &style.counter_increment {
                if name.eq_ignore_ascii_case("page") {
                    flow.page_base += *n;
                }
            }
        }

        // Cursor within the fragmentainer for this box's children.
        let mut y = content_top;
        let mut children: Vec<Fragment> = Vec::new();
        let mut outgoing_children: Vec<ChildToken> = Vec::new();
        let mut broke = false;
        let mut seen_all = true;
        // Whether the *fragmentainer* holds any content at or above this box's
        // flow position — threaded so monolithic last-resort placement only
        // fires on a genuinely empty page, not merely an empty (just-started)
        // box. Becomes true once this box places anything.
        let mut placed = page_has_content;

        // Resume bookkeeping: which child index to start from, and its token.
        // `child_tokens` come positionally; a break-before child token means
        // "start that child fresh here".
        // `HasSeenAllChildren` with no pending child tokens means this box
        // finished every child on an earlier fragmentainer: nothing remains,
        // so terminate rather than emit a spurious trailing page.
        if !fresh && token.seen_all_children && token.child_tokens.is_empty() {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: None,
                empty: true,
            };
        }

        // A positioned box becomes the containing block for its abspos
        // descendants (its padding box on this page). Save/restore so this
        // box's SIBLINGS resolve against the OUTER block.
        let saved_abspos_cb = flow.abspos_cb;
        if matches!(
            style.position,
            Position::Relative | Position::Absolute | Position::Fixed
        ) {
            flow.abspos_cb = Some((Point::new(inner_left, box_top), inner_width));
        }

        // Multi-column: a box with `column-count`/`column-width` engaging >= 2
        // columns hands its items to the column fragmentainers (CORE-63).
        if let Some(mc) = self.multicol_geometry(style, inner_width) {
            let res = self.layout_multicol_container(
                id,
                inner_left,
                inner_width,
                top,
                bottom_limit,
                placed,
                first_in_flow,
                token,
                flow,
                style,
                mc,
            );
            flow.abspos_cb = saved_abspos_cb;
            return res;
        }

        let start_index = if fresh {
            0
        } else {
            // Resume at the first unfinished child (lowest index in the token).
            token
                .child_tokens
                .first()
                .map(|c| c.index)
                .unwrap_or(items.len())
        };

        let line_height = |s: &ComputedStyle| s.line_height;

        // Generated content (`content` property, e.g. a TOC entry) is emitted
        // as one line at the start of the box, when the box starts fresh. Its
        // width is the inner width; a `leader('.')` fills to the right content
        // edge, so pagination does not depend on the resolved number glyphs
        // (spec §9, §10).
        if fresh && !style.content.is_empty() {
            let lh = line_height(style);
            if y + lh <= bottom_limit || !placed {
                let text = self.resolve_content(id, &style.content, inner_width, style, flow);
                let face = style.font_face;
                let baseline = y + crate::typography::baseline_offset(style.font_size, lh, face);
                // Shape the resolved content so non-ASCII (em dash, curly
                // quotes, ·) renders as a real glyph with a ToUnicode mapping
                // — never raw UTF-8 bytes (CORE-83). Generated content is one
                // line; no microtypography.
                let shaped = crate::typography::shape_word(&text, style.font_size, face);
                let run = TextRun {
                    text: shaped.text,
                    baseline: Point::new(inner_left, baseline),
                    font_size: style.font_size,
                    color: style.color,
                    font_face: face,
                    glyphs: shaped.glyphs,
                    expansion: 0.0,
                    protrude_left: Scalar::ZERO,
                    protrude_right: Scalar::ZERO,
                };
                children.push(Fragment::line(
                    Point::new(inner_left, y),
                    (inner_width, lh),
                    run,
                ));
                y += lh;
                placed = true;
                first_in_flow = false;
            }
        }

        let mut i = start_index;
        while i < items.len() {
            // Forced break-before on a block child starts a new fragmentainer.
            // NOTE: a mid-flow `page` name change does NOT force a break here —
            // the WPT page-name-* references render without breaks, and the
            // harness compares test-vs-ref through the SAME engine, so the
            // name (resolved at page starts by `active_page_name`) is all the
            // page geometry needs. (Tried forcing breaks on name change for
            // CORE-66; it regressed 38 page-name tests.)
            if let Item::Block(child) = &items[i] {
                let cstyle = &self.styles[*child];
                let child_fresh = self.child_incoming(token, i).is_break_before();
                if child_fresh
                    && cstyle.break_before.is_forced()
                    && (!children.is_empty() || broke || i > start_index)
                {
                    // Defer the rest to the next page, starting at this child.
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: i,
                        token: BreakToken::break_before(),
                    });
                    broke = true;
                    break;
                }
            }

            match &items[i] {
                Item::Text(text, link_spans, fn_markers) => {
                    let child_tok = self.child_incoming(token, i);
                    let lh = line_height(style);
                    // Resume: a run that began beside a float resumes by source
                    // offset (its page's available width may differ from the
                    // previous page's); otherwise the legacy line-count resume.
                    let resume_offset = child_tok.consumed_chars;
                    if flow.active_floats.is_empty() && resume_offset.is_none() {
                        let lines = self.break_paragraph(text, inner_width, style);
                        // How many lines already consumed by earlier fragments.
                        let consumed_lines =
                            (child_tok.consumed_block_size.get() / lh.get()).round() as usize;
                        let mut li = consumed_lines;
                        // Link rects placed by THIS fragment, keyed by line
                        // index so an orphans/widows rewind can drop the
                        // pulled-back lines' rects (CORE-104).
                        let mut line_links: Vec<(usize, CollectedLink)> = Vec::new();
                        // Place as many lines as fit. A run that is the first thing
                        // on an otherwise-empty page places at least one line even
                        // when taller than the page (last resort → monolithic
                        // overflow, never sliced).
                        while li < lines.len() {
                            let fits = y + lh <= bottom_limit;
                            // Last resort only on a genuinely empty fragmentainer.
                            let last_resort = !placed && li == consumed_lines;
                            if !fits && !last_resort {
                                break;
                            }
                            let face = style.font_face;
                            let baseline =
                                y + crate::typography::baseline_offset(style.font_size, lh, face);
                            let lr = &lines[li];
                            let x =
                                self.aligned_x(inner_left, inner_width, lr.drawn_width(), style);
                            // Link rect for this line (CORE-104): spans are
                            // item-relative; the line's byte offset in the item
                            // text is the sum of consumed bytes of all preceding
                            // lines of the full-text break.
                            let line_start: usize = lines[..li].iter().map(|l| l.consumed).sum();
                            if !link_spans.is_empty() {
                                let mut rects = Vec::new();
                                collect_line_links(
                                    &mut rects,
                                    self.page_index,
                                    lr,
                                    line_start,
                                    x,
                                    y,
                                    lh,
                                    link_spans,
                                );
                                for r in rects {
                                    line_links.push((li, r));
                                }
                            }
                            let run = TextRun {
                                text: lr.text.clone(),
                                baseline: Point::new(x, baseline),
                                font_size: style.font_size,
                                color: style.color,
                                font_face: style.font_face,
                                glyphs: lr.glyphs.clone(),
                                expansion: lr.expansion,
                                protrude_left: lr.protrude_left,
                                protrude_right: lr.protrude_right,
                            };
                            children.push(Fragment::line(
                                Point::new(inner_left, y),
                                (inner_width, lh),
                                run,
                            ));
                            // CORE-107: register call markers placed by THIS
                            // line (item-relative byte → absolute span).
                            let ls: usize = lines[..li].iter().map(|l| l.consumed).sum();
                            let le = ls + lines[li].consumed;
                            self.register_placed_markers(flow, fn_markers, 0, ls, le);
                            y += lh;
                            li += 1;
                            placed = true;
                            first_in_flow = false;
                            // A last-resort line that overflowed: stop here so the
                            // rest of the run continues on the next fragmentainer.
                            if last_resort && y > bottom_limit {
                                break;
                            }
                        }

                        if li < lines.len() {
                            // The run breaks. `Tolerable` unless orphans/widows are
                            // violated at the natural split, then `AvoidViolating`.
                            let (split, _moved) = apply_orphans_widows(
                                consumed_lines,
                                li,
                                lines.len(),
                                style.orphans as usize,
                                style.widows as usize,
                            );
                            // NEVER consume more lines than were placed on this
                            // fragmentainer (CORE-97). `apply_orphans_widows` can
                            // return split > li when the orphans bump would fix an
                            // impossible violation (e.g. the paragraph starts with
                            // 0 lines fitting at the page bottom: orphans=2 bumps
                            // split to first+2, but nothing was placed). Claiming
                            // those lines consumed drops real text — the next
                            // fragmentainer resumes past them. Clamp to the lines
                            // actually placed; the constraint is dropped when it
                            // cannot be honored (css-break-3 §4.4).
                            let split = split.min(li);
                            // Natural split honored orphans/widows when `split ==
                            // li`; otherwise the constraint pulled the break back.
                            // If widows/orphans pulled the split back, drop the
                            // now-excess lines from this page.
                            if split < li {
                                for _ in 0..(li - split) {
                                    children.pop();
                                    y = y - lh;
                                }
                            }
                            // Flush this fragment's link rects, dropping any
                            // pulled back by the rewind above (CORE-104).
                            for (key, r) in line_links.drain(..) {
                                if key >= split {
                                    continue;
                                }
                                self.links.borrow_mut().push(r);
                            }
                            let consumed = lh * (split as f64);
                            seen_all = false;
                            outgoing_children.push(ChildToken {
                                index: i,
                                token: BreakToken {
                                    consumed_block_size: consumed,
                                    seen_all_children: false,
                                    child_tokens: Vec::new(),
                                    break_before: false,
                                    consumed_chars: None,
                                    flex: None,
                                },
                            });
                            broke = true;
                            break;
                        } else {
                            // The whole run fit on this fragmentainer: flush its
                            // link rects (no rewind happened, keys are all < li)
                            // (CORE-104).
                            for (_, r) in line_links.drain(..) {
                                self.links.borrow_mut().push(r);
                            }
                        }
                    } else {
                        // Floats are (or were) active: break the remaining text
                        // in segments, one per distinct available-width state.
                        // Lines beside a float use a reduced width; lines below
                        // its bottom return to full width. Runs resume by source
                        // offset so a width change across a page break is safe.
                        let mut src_offset = resume_offset.unwrap_or(0).min(text.len());
                        let mut run_broke = false;
                        'segments: while src_offset < text.len() {
                            // Floats whose bottom has passed stop intruding.
                            flow.active_floats.retain(|f| f.bottom().get() > y.get());
                            let (seg_x, seg_w) =
                                self.segment_geometry(inner_left, inner_width, y, lh, flow);
                            if seg_w.get() <= 0.0 {
                                // The float consumes the whole line area: jump
                                // below the lowest float bottom.
                                let below = flow.active_floats.iter().map(|f| f.bottom()).fold(
                                    Scalar::ZERO,
                                    |a, b| if b.get() > a.get() { b } else { a },
                                );
                                if below.get() > y.get() {
                                    y = below;
                                    continue;
                                }
                                break;
                            }
                            let lines = self.break_paragraph(&text[src_offset..], seg_w, style);
                            if lines.is_empty() {
                                break;
                            }
                            let mut li = 0usize;
                            let mut page_bottom_break = false;
                            // Link rects for this segment, keyed by line
                            // index within the segment (rewind-safe).
                            let mut seg_links: Vec<(usize, CollectedLink)> = Vec::new();
                            while li < lines.len() {
                                let fits = y + lh <= bottom_limit;
                                // Last resort only on a genuinely empty
                                // fragmentainer.
                                let last_resort = !placed && li == 0;
                                if !fits && !last_resort {
                                    page_bottom_break = true;
                                    break;
                                }
                                let face =
                                    style.font_face;
                                let baseline = y + crate::typography::baseline_offset(
                                    style.font_size,
                                    lh,
                                    face,
                                );
                                let lr = &lines[li];
                                let x = self.aligned_x(seg_x, seg_w, lr.drawn_width(), style);
                                // Link rect (CORE-104): the segment's text is
                                // a slice starting at `src_offset`, so this
                                // line's item-text byte offset is
                                // src_offset + sum of preceding lines' bytes.
                                let line_start = src_offset
                                    + lines[..li].iter().map(|l| l.consumed).sum::<usize>();
                                if !link_spans.is_empty() {
                                    let mut rects = Vec::new();
                                    collect_line_links(
                                        &mut rects,
                                        self.page_index,
                                        lr,
                                        line_start,
                                        x,
                                        y,
                                        lh,
                                        link_spans,
                                    );
                                    for r in rects {
                                        seg_links.push((li, r));
                                    }
                                }
                                let run = TextRun {
                                    text: lr.text.clone(),
                                    baseline: Point::new(x, baseline),
                                    font_size: style.font_size,
                                    color: style.color,
                                    font_face: style.font_face,
                                    glyphs: lr.glyphs.clone(),
                                    expansion: lr.expansion,
                                    protrude_left: lr.protrude_left,
                                    protrude_right: lr.protrude_right,
                                };
                                children.push(Fragment::line(
                                    Point::new(seg_x, y),
                                    (seg_w, lh),
                                    run,
                                ));
                                // CORE-107: register call markers placed by
                                // THIS segment line (segment text starts at
                                // `src_offset` in the item's text).
                                let ls = src_offset
                                    + lines[..li].iter().map(|l| l.consumed).sum::<usize>();
                                let le = ls + lines[li].consumed;
                                self.register_placed_markers(flow, fn_markers, 0, ls, le);
                                y += lh;
                                li += 1;
                                // True source bytes consumed (CORE-91): rebuilt
                                // `text` length undercounts whitespace runs, so
                                // the next segment must resume at the line's
                                // real source end or it re-breaks inside the
                                // previous line's last word (duplicated glyph).
                                src_offset += lr.consumed;
                                placed = true;
                                // A last-resort line that overflowed: stop here.
                                if last_resort && y > bottom_limit {
                                    page_bottom_break = true;
                                    break;
                                }
                                // The segment ends when the intrusion set
                                // changes at the NEXT line (a float's bottom
                                // crossed): the remaining text re-breaks at
                                // the new width in the next segment.
                                let (nx, nw) =
                                    self.segment_geometry(inner_left, inner_width, y, lh, flow);
                                if (nw - seg_w).get().abs() > 1e-9
                                    || (nx - seg_x).get().abs() > 1e-9
                                {
                                    break;
                                }
                            }
                            if page_bottom_break && li < lines.len() {
                                // The run breaks at the page bottom inside this
                                // segment. Apply orphans/widows to the segment's
                                // lines, pulling excess lines back (and rewinding
                                // the source offset past them).
                                let (split, _moved) = apply_orphans_widows(
                                    0,
                                    li,
                                    lines.len(),
                                    style.orphans as usize,
                                    style.widows as usize,
                                );
                                if split < li {
                                    // Rewind src_offset by the DROPPED lines'
                                    // true source consumption (CORE-91) —
                                    // summing rebuilt text lengths would
                                    // undercount and re-break mid-word.
                                    let rewound: usize =
                                        lines[split..li].iter().map(|l| l.consumed).sum();
                                    src_offset -= rewound;
                                    for _ in 0..(li - split) {
                                        children.pop().expect("line fragment present");
                                        y = y - lh;
                                    }
                                }
                                // Flush the segment's link rects, dropping any
                                // pulled back by the rewind above (CORE-104).
                                for (key, r) in seg_links.drain(..) {
                                    if key >= split {
                                        continue;
                                    }
                                    self.links.borrow_mut().push(r);
                                }
                                run_broke = true;
                                break 'segments;
                            }
                            // All of this segment's lines placed. If the next
                            // segment would have the same geometry, the run is
                            // finished.
                            let (nx, nw) =
                                self.segment_geometry(inner_left, inner_width, y, lh, flow);
                            if (nw - seg_w).get().abs() < 1e-9 && (nx - seg_x).get().abs() < 1e-9 {
                                // Run finished: this segment's link rects are
                                // final, flush them (CORE-104).
                                for (_, r) in seg_links.drain(..) {
                                    self.links.borrow_mut().push(r);
                                }
                                break;
                            }
                        }
                        if run_broke {
                            seen_all = false;
                            outgoing_children.push(ChildToken {
                                index: i,
                                token: BreakToken {
                                    consumed_block_size: Scalar::ZERO,
                                    seen_all_children: false,
                                    child_tokens: Vec::new(),
                                    break_before: false,
                                    consumed_chars: Some(src_offset),
                                    flex: None,
                                },
                            });
                            broke = true;
                            break;
                        }
                    }
                }
                Item::Block(child) => {
                    let child_tok = self.child_incoming(token, i);
                    let cstyle = &self.styles[*child];
                    if matches!(cstyle.position, Position::Absolute | Position::Fixed) {
                        // ---- out-of-flow branch ----
                        // The box is taken out of flow: no cursor advance, no
                        // in-flow height, and its fragment attaches to the
                        // fragmentainer root (css-break-3), not to this box's
                        // subtree. `position: fixed` resolves against the page
                        // content box; `absolute` against the nearest
                        // positioned ancestor's padding box (or the page).
                        let page_cb = (
                            Point::new(self.content_x, self.content_y),
                            self.content_width,
                        );
                        let (cb_origin, cb_width) = match cstyle.position {
                            Position::Fixed => page_cb,
                            _ => flow.abspos_cb.unwrap_or(page_cb),
                        };
                        let (fw, fh) = self.measure_float(*child, cb_width);
                        // Insets position the margin box against the
                        // containing block's padding edges (css-position-3).
                        // `bottom` resolves against the fragmentainer content
                        // height (the containing block's own height is not
                        // known mid-layout); `auto` insets sit at the padding
                        // box origin (basic static position).
                        let x = match cstyle.inset_left {
                            Some(l) => cb_origin.x + l,
                            None => match cstyle.inset_right {
                                Some(r) => cb_origin.x + cb_width - fw - r,
                                None => cb_origin.x,
                            },
                        };
                        let y = match cstyle.inset_top {
                            Some(t) => cb_origin.y + t,
                            None => match cstyle.inset_bottom {
                                Some(b) => cb_origin.y + self.page_height - fh - b,
                                None => cb_origin.y,
                            },
                        };
                        // Monolithic placement (spec Behavior 9): the box lays
                        // its full content once — no page-bottom break, no
                        // resume token — even when taller than the page.
                        let res = self.layout_box(
                            *child,
                            x,
                            fw,
                            y,
                            Scalar(f64::MAX),
                            placed,
                            first_in_flow,
                            &child_tok,
                            flow,
                        );
                        flow.abspos.push((cstyle.z_index, res.fragment));
                        // Advance the ITEM index explicitly (`continue` skips
                        // the trailing `i += 1` — the CORE-62 lesson).
                        i += 1;
                        continue;
                    }
                    if cstyle.float != Float::None {
                        if child_tok.is_break_before() {
                            // ---- float first placement ----
                            // Measure the float's margin box (width: explicit
                            // or shrink-to-fit; height: content measure).
                            let (fw, fh) = self.measure_float(*child, inner_width);
                            let fits = y + fh <= bottom_limit;
                            let last_resort = !placed;
                            if !fits && !last_resort {
                                // CORE-101: match Prince — place the float
                                // when ANY room remains and fragment its
                                // content, instead of deferring the whole
                                // box. Exception: a monolithic float (no
                                // text content to split across pages, or
                                // `break-inside: avoid`) still defers whole.
                                let avoid = cstyle.break_inside == BreakInside::Avoid;
                                let splittable = self.float_is_splittable(*child);
                                if avoid || !splittable {
                                    seen_all = false;
                                    outgoing_children.push(ChildToken {
                                        index: i,
                                        token: BreakToken::break_before(),
                                    });
                                    broke = true;
                                    break;
                                }
                                // Fall through to placement below: the float
                                // lays out at `y` against the real bottom
                                // limit, its content fragments naturally, and
                                // its continuation rides `pending_floats`.
                            }
                            let fx = match cstyle.float {
                                Float::Left => inner_left,
                                Float::Right => inner_left + inner_width - fw,
                                _ => inner_left,
                            };
                            // A float's own content lays in-flow and ignores
                            // the active intrusion set: floats do not wrap
                            // around sibling floats in this model, and the
                            // shared retain() below must not prune siblings.
                            let saved_floats = std::mem::take(&mut flow.active_floats);
                            let res = self.layout_box(
                                *child,
                                fx,
                                fw,
                                y,
                                bottom_limit,
                                placed,
                                first_in_flow,
                                &child_tok,
                                flow,
                            );
                            flow.active_floats = saved_floats;
                            children.push(res.fragment);
                            // The float intrudes on following in-flow text
                            // until its bottom passes.
                            flow.active_floats.push(PlacedFloat {
                                id: *child,
                                x: fx,
                                y,
                                width: fw,
                                height: fh,
                                side: cstyle.float,
                            });
                            placed = true;
                            // A float whose content broke across the page
                            // carries its remaining rectangle to the next
                            // fragmentainer (which resumes at the content top),
                            // and its OWN content resumes there too: push the
                            // float's break token so the next page re-enters
                            // this item in the resume branch.
                            if res.outgoing.is_some() {
                                let r = fh - res.used;
                                let remaining = if r.get() > 0.0 { r } else { Scalar::ZERO };
                                if remaining.get() > 0.0 {
                                    flow.pending_floats.push(PlacedFloat {
                                        id: *child,
                                        x: fx,
                                        y: self.content_y,
                                        width: fw,
                                        height: remaining,
                                        side: cstyle.float,
                                    });
                                }
                                seen_all = false;
                                outgoing_children.push(ChildToken {
                                    index: i,
                                    token: res.outgoing.unwrap(),
                                });
                            }
                            // The in-flow cursor does NOT advance past a float:
                            // the next sibling starts at the same y and wraps
                            // around it. Advance the ITEM index explicitly:
                            // `continue` skips the loop's trailing `i += 1`.
                            i += 1;
                            continue;
                        }
                        // ---- float resume across fragmentainers ----
                        // The carry-over rectangle was seeded into the active
                        // set at the page start. Lay the continuation at the
                        // same x/width; do NOT advance the in-flow cursor so
                        // text wraps beside it (css-break-3 parallel flow).
                        if let Some(pf) = flow.active_floats.iter().find(|f| f.id == *child) {
                            let pf = *pf;
                            // The float's own content lays in-flow: the active
                            // intrusion set (including this float's own
                            // carry-over rect) does not apply to it.
                            let saved_floats = std::mem::take(&mut flow.active_floats);
                            let res = self.layout_box(
                                *child,
                                pf.x,
                                pf.width,
                                y,
                                bottom_limit,
                                placed,
                                first_in_flow,
                                &child_tok,
                                flow,
                            );
                            flow.active_floats = saved_floats;
                            children.push(res.fragment);
                            placed = true;
                            if res.outgoing.is_some() {
                                // Still not finished: update the carry-over
                                // rectangle for the next fragmentainer and
                                // propagate the float's own break token.
                                let r = pf.height - res.used;
                                let remaining = if r.get() > 0.0 { r } else { Scalar::ZERO };
                                flow.pending_floats.retain(|f| f.id != *child);
                                if remaining.get() > 0.0 {
                                    flow.pending_floats.push(PlacedFloat {
                                        id: *child,
                                        x: pf.x,
                                        y: self.content_y,
                                        width: pf.width,
                                        height: remaining,
                                        side: cstyle.float,
                                    });
                                }
                                seen_all = false;
                                outgoing_children.push(ChildToken {
                                    index: i,
                                    token: res.outgoing.unwrap(),
                                });
                            } else {
                                // CORE-101: the float finished on this page —
                                // drop its carry-over rectangle so it stops
                                // intruding on later fragmentainers. Without
                                // this, stale rects force every later page
                                // into the segmented text path with no resume
                                // offset, restarting paragraphs from zero.
                                flow.pending_floats.retain(|f| f.id != *child);
                            }
                        }
                        // Advance the ITEM index explicitly (`continue` skips
                        // the loop's trailing `i += 1`).
                        i += 1;
                        continue;
                    }
                    let res = self.layout_box(
                        *child,
                        inner_left,
                        inner_width,
                        y,
                        bottom_limit,
                        placed,
                        first_in_flow,
                        &child_tok,
                        flow,
                    );

                    // break-inside: avoid — if the child broke but *could* fit
                    // whole on a fresh page, move it wholly to the next page.
                    // Once-per-flow abort-and-defer: on the next page the child
                    // arrives break-before and (fitting a full page) does not
                    // re-trigger this, so the cost is bounded.
                    let cstyle = &self.styles[*child];
                    if cstyle.break_inside == BreakInside::Avoid
                        && res.outgoing.is_some()
                        && child_tok.is_break_before()
                        && placed
                    {
                        let fits_fresh = self.block_fits_fresh(*child, inner_width);
                        if fits_fresh {
                            seen_all = false;
                            outgoing_children.push(ChildToken {
                                index: i,
                                token: BreakToken::break_before(),
                            });
                            broke = true;
                            break;
                        }
                    }

                    if !res.empty {
                        children.push(res.fragment);
                        y += res.used;
                        placed = true;
                        // A non-empty in-flow block is no longer the first
                        // in-flow content: later siblings keep their margins.
                        first_in_flow = false;
                    }

                    if let Some(tok) = res.outgoing {
                        seen_all = false;
                        outgoing_children.push(ChildToken {
                            index: i,
                            token: tok,
                        });
                        broke = true;
                        break;
                    }

                    // Forced break-after: rest goes to the next page.
                    if cstyle.break_after.is_forced() && i + 1 < items.len() {
                        seen_all = false;
                        outgoing_children.push(ChildToken {
                            index: i + 1,
                            token: BreakToken::break_before(),
                        });
                        broke = true;
                        break;
                    }
                }
            }
            i += 1;
        }

        flow.abspos_cb = saved_abspos_cb;

        // Padding-bottom / margin-bottom only apply when the box finished.
        let padding_bottom = if broke {
            Scalar::ZERO
        } else {
            style.padding_bottom
        };
        y += padding_bottom;

        let box_height = y - box_top;

        // Rebase children to be parent-relative: each child's offset (and any
        // text baseline) is stored relative to this fragment's own top-left, so
        // the tree carries LayoutNG-style parent-relative geometry. The PDF
        // walk re-accumulates absolutes from the fragmentainer down.
        let origin = Point::new(origin_x, box_top);
        for child in &mut children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }

        // Background fill spans the box's border box in this fragmentainer.
        let mut fragment = Fragment::block(origin, (avail_width, box_height));
        if let Some(bg) = style.background_color {
            if box_height.get() > 0.0 {
                fragment.content = FragmentContent::Background(bg);
            }
        }
        fragment.children = children;
        // Map this fragment back to its DOM node so target-counter and PDF
        // bookmarks can find the page it landed on.
        fragment.source = Some(id);

        let outgoing = if broke {
            let consumed = token.consumed_block_size + box_height;
            let tok = BreakToken {
                consumed_block_size: consumed,
                seen_all_children: seen_all,
                child_tokens: outgoing_children,
                break_before: false,
                consumed_chars: None,
                flex: None,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };

        // margin-bottom advances the *parent* cursor, not the box height.
        let margin_bottom = if broke {
            Scalar::ZERO
        } else {
            style.margin_bottom
        };
        let used = (box_top - top) + box_height + margin_bottom;

        let empty = children_empty(&fragment) && outgoing.is_none() && box_height.get() <= 0.0;

        BlockResult {
            fragment,
            used,
            outgoing,
            empty,
        }
    }

    fn layout_table_like(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        first_in_flow: bool,
        token: &BreakToken,
        flow: &mut Flow,
    ) -> BlockResult {
        match self.styles[id].display {
            Display::Table => self.layout_table_block(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                token,
                flow,
            ),
            Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup => {
                let columns = self.frozen_table_columns(id, avail_width);
                self.layout_table_group(
                    id,
                    origin_x,
                    avail_width,
                    top,
                    bottom_limit,
                    page_has_content,
                    token,
                    flow,
                    &columns,
                )
            }
            Display::TableRow => {
                let columns = self.frozen_table_columns(id, avail_width);
                self.layout_table_row(
                    id,
                    origin_x,
                    avail_width,
                    top,
                    bottom_limit,
                    page_has_content,
                    token,
                    flow,
                    &columns,
                )
            }
            Display::TableCell => self.layout_table_cell(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                first_in_flow,
                token,
                flow,
            ),
            _ => self.layout_box(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                first_in_flow,
                token,
                flow,
            ),
        }
    }

    /// CORE-89 first-page column freeze: resolve the frozen column widths for
    /// a table-family box. Walks up to the nearest table ancestor (the box
    /// itself when it IS the table), resolves the frozen [`MeasureScope`]
    /// against `self.page_height` as the first-fragmentainer height proxy,
    /// and measures scoped. Pure: identical input → identical widths on every
    /// page and every layout pass.
    fn frozen_table_columns(&self, id: NodeId, avail_width: Scalar) -> crate::table::ColumnWidths {
        let mut table_id = id;
        let mut cur = Some(id);
        while let Some(n) = cur {
            if self.styles[n].display == Display::Table {
                table_id = n;
                break;
            }
            cur = self.dom.nodes[n].parent;
        }
        let used = crate::table::table_used_width(self.styles, table_id, avail_width);
        let scope = crate::table::resolve_freeze_scope(
            self.dom,
            self.styles,
            table_id,
            avail_width,
            used,
            self.page_height,
        );
        let widths = crate::table::measure_columns_scoped(
            self.dom,
            self.styles,
            table_id,
            avail_width,
            used,
            scope.clone(),
        );
        widths
    }

    fn layout_table_block(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        token: &BreakToken,
        flow: &mut Flow,
    ) -> BlockResult {
        let fresh = token.is_break_before();
        // Resume bookkeeping (mirrors layout_box): child_tokens are positional
        // per child index; the first unfinished child is where we restart.
        if !fresh && token.seen_all_children && token.child_tokens.is_empty() {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: None,
                empty: true,
            };
        }
        // CORE-89 first-page column freeze: resolve the frozen widths once per
        // table-block layout. `resolve_freeze_scope` is a pure function of
        // (dom, styles, geometry) — the first-fragmentainer height proxy is
        // `self.page_height` (the page content height, constant for uniform
        // geometry) — so every page and every layout pass derives the SAME
        // scope and the SAME frozen widths. The widths are threaded down to
        // rows, which no longer re-measure per row (also a CORE-89 perf win).
        let columns = self.frozen_table_columns(id, avail_width);
        let children: Vec<NodeId> = self.dom.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|c| matches!(self.dom.nodes[*c].kind, NodeKind::Element(_)))
            .collect();
        let start_index = if fresh {
            0
        } else {
            token
                .child_tokens
                .first()
                .map(|c| c.index)
                .unwrap_or(children.len())
        };

        let mut y = top;
        let mut frag_children = Vec::new();
        let mut outgoing_children = Vec::new();
        let mut seen_all = true;
        let mut placed = page_has_content;
        let mut placed_any = false;

        // Footer repetition (spec rule 8): a `tfoot`/`table-footer-group` is
        // laid out at the bottom of EVERY fragment that contains table content
        // (including the last), so its height is reserved out of the row
        // budget below — rows break early enough that rows + footer always
        // fit. A footer taller than a full page gets no reservation (it
        // overflows monolithically at the table's end, rule 11).
        let footer_indices: Vec<usize> = children
            .iter()
            .enumerate()
            .filter(|(_, c)| self.styles[**c].display == Display::TableFooterGroup)
            .map(|(i, _)| i)
            .collect();
        let mut footer_height = Scalar::ZERO;
        for &fi in &footer_indices {
            let footer_rows: Vec<NodeId> = self.dom.nodes[children[fi]]
                .children
                .iter()
                .copied()
                .filter(|c| {
                    matches!(self.dom.nodes[*c].kind, NodeKind::Element(_))
                        && self.styles[*c].display == Display::TableRow
                })
                .collect();
            if footer_rows.is_empty() {
                continue;
            }
            let (heights, _) =
                measure_rows(self.dom, self.styles, &footer_rows, &columns, avail_width);
            for h in heights {
                footer_height = footer_height + h;
            }
        }
        let reserve_footer =
            footer_height.get() > 0.0 && footer_height.get() < self.page_height.get();
        let row_limit = if reserve_footer {
            bottom_limit - footer_height
        } else {
            bottom_limit
        };

        // Repeating header (spec rule 7): on continuation fragments the
        // table-header-group re-lays-out at the top of the page even though
        // its child index is before the resume point. (A header is small;
        // if it itself overflows we let it fragment like any box.)
        if !fresh {
            for (i, child) in children.iter().enumerate() {
                if self.styles[*child].display != Display::TableHeaderGroup {
                    continue;
                }
                let child_tok = self.child_incoming(token, i);
                let res = self.layout_table_group(
                    *child,
                    origin_x,
                    avail_width,
                    y,
                    row_limit,
                    placed,
                    &child_tok,
                    flow,
                    &columns,
                );
                if !res.empty {
                    y += res.used;
                    placed = true;
                    placed_any = true;
                    frag_children.push(res.fragment);
                }
                if let Some(tok) = res.outgoing {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: i,
                        token: tok,
                    });
                }
            }
        }

        for i in start_index..children.len() {
            let child = children[i];
            // Skip non-table-family children entirely (they are not part of
            // the table grid; anonymous-box wrapping is a later pass).
            let cd = self.styles[child].display;
            if !matches!(
                cd,
                Display::TableHeaderGroup
                    | Display::TableRowGroup
                    | Display::TableFooterGroup
                    | Display::TableRow
            ) {
                continue;
            }
            // The footer is placed by the footer-repetition block below, not
            // by the in-order loop (it would otherwise render only once, at
            // the end of the table — spec rule 8 requires it on every
            // fragment).
            if cd == Display::TableFooterGroup {
                continue;
            }
            let child_tok = self.child_incoming(token, i);
            let res = match cd {
                Display::TableHeaderGroup | Display::TableRowGroup => self.layout_table_group(
                    child,
                    origin_x,
                    avail_width,
                    y,
                    row_limit,
                    placed,
                    &child_tok,
                    flow,
                    &columns,
                ),
                Display::TableRow => self.layout_table_row(
                    child,
                    origin_x,
                    avail_width,
                    y,
                    row_limit,
                    placed,
                    &child_tok,
                    flow,
                    &columns,
                ),
                _ => continue,
            };
            if !res.empty {
                y += res.used;
                placed = true;
                placed_any = true;
                frag_children.push(res.fragment);
            }
            if let Some(tok) = res.outgoing {
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: i,
                    token: tok,
                });
                break;
            }
        }

        // Footer repetition: place the footer group(s) at the bottom of this
        // fragment, after the last row that fit. Only when the fragment
        // actually contains table content (an empty fragment — e.g. a table
        // that starts at the very bottom of a page — defers everything,
        // footer included, to the next fragment).
        if placed_any && reserve_footer {
            for &fi in &footer_indices {
                let child_tok = self.child_incoming(token, fi);
                let res = self.layout_table_group(
                    children[fi],
                    origin_x,
                    avail_width,
                    y,
                    bottom_limit,
                    placed,
                    &child_tok,
                    flow,
                    &columns,
                );
                if !res.empty {
                    y += res.used;
                    placed = true;
                    frag_children.push(res.fragment);
                }
                if let Some(tok) = res.outgoing {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: fi,
                        token: tok,
                    });
                }
            }
        }

        // Rebase children to be parent-relative.
        let origin = Point::new(origin_x, top);
        for child in &mut frag_children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let height = y - top;
        let mut fragment = Fragment::block(origin, (avail_width, height));
        fragment.children = frag_children;
        fragment.source = Some(id);
        let outgoing = if !seen_all {
            let consumed = token.consumed_block_size + height;
            let tok = BreakToken {
                consumed_block_size: consumed,
                seen_all_children: seen_all,
                child_tokens: outgoing_children,
                break_before: false,
                consumed_chars: None,
                flex: None,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };
        let empty = height.get() <= 0.0 && outgoing.is_none();
        BlockResult {
            fragment,
            used: height,
            outgoing,
            empty,
        }
    }

    fn layout_table_group(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        token: &BreakToken,
        flow: &mut Flow,
        columns: &crate::table::ColumnWidths,
    ) -> BlockResult {
        let fresh = token.is_break_before();
        // Resume bookkeeping (mirrors layout_box / layout_table_block):
        // child_tokens are positional per row index.
        if !fresh && token.seen_all_children && token.child_tokens.is_empty() {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: None,
                empty: true,
            };
        }
        let rows: Vec<NodeId> = self.dom.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|c| {
                matches!(self.dom.nodes[*c].kind, NodeKind::Element(_))
                    && self.styles[*c].display == Display::TableRow
            })
            .collect();
        let start_index = if fresh {
            0
        } else {
            token
                .child_tokens
                .first()
                .map(|c| c.index)
                .unwrap_or(rows.len())
        };

        let mut y = top;
        let mut frag_children = Vec::new();
        let mut outgoing_children = Vec::new();
        let mut seen_all = true;
        let mut placed = page_has_content;

        for i in start_index..rows.len() {
            let child_tok = self.child_incoming(token, i);
            let res = self.layout_table_row(
                rows[i],
                origin_x,
                avail_width,
                y,
                bottom_limit,
                placed,
                &child_tok,
                flow,
                columns,
            );
            if !res.empty {
                y += res.used;
                placed = true;
                frag_children.push(res.fragment);
            }
            if let Some(tok) = res.outgoing {
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: i,
                    token: tok,
                });
                break;
            }
        }

        let origin = Point::new(origin_x, top);
        for child in &mut frag_children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let height = y - top;
        let mut fragment = Fragment::block(origin, (avail_width, height));
        fragment.children = frag_children;
        fragment.source = Some(id);
        let outgoing = if !seen_all {
            let consumed = token.consumed_block_size + height;
            let tok = BreakToken {
                consumed_block_size: consumed,
                seen_all_children: seen_all,
                child_tokens: outgoing_children,
                break_before: false,
                consumed_chars: None,
                flex: None,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };
        let empty = height.get() <= 0.0 && outgoing.is_none();
        BlockResult {
            fragment,
            used: height,
            outgoing,
            empty,
        }
    }

    fn layout_table_row(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        token: &BreakToken,
        flow: &mut Flow,
        columns: &crate::table::ColumnWidths,
    ) -> BlockResult {
        let row_ids = [id];
        let (row_heights, _) = measure_rows(self.dom, self.styles, &row_ids, columns, avail_width);
        let row_height = row_heights.first().copied().unwrap_or(Scalar::ZERO);

        if top + row_height > bottom_limit
            && page_has_content
            && row_height.get() <= self.page_height.get()
        {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: Some(BreakToken::break_before()),
                empty: true,
            };
        }

        let mut x = origin_x;
        let mut children = Vec::new();
        // Cell ordinal, NOT the raw child index: DOM children include whitespace
        // text nodes between cells, so enumerate() would misalign columns.
        // CORE-96: a cell with `colspan` occupies that many column slots (its
        // box spans the summed widths); the next cell starts after them.
        let mut col = 0usize;
        for &cell in self.dom.nodes[id].children.iter() {
            if let NodeKind::Element(_) = &self.dom.nodes[cell].kind {
                if self.styles[cell].display != Display::TableCell {
                    continue;
                }
                let span = crate::table::cell_colspan(self.dom, cell);
                let mut col_w = Scalar::ZERO;
                for j in 0..span {
                    col_w = col_w + columns.widths.get(col + j).copied().unwrap_or(Scalar::ZERO);
                }
                let res = self.layout_table_cell(
                    cell,
                    x,
                    col_w,
                    top,
                    bottom_limit,
                    page_has_content,
                    // Table cells do not participate in top-of-page margin
                    // truncation (tables are out of CORE-95 scope; cell
                    // content keeps its declared margins).
                    false,
                    token,
                    flow,
                );
                if !res.empty {
                    children.push(res.fragment);
                }
                x += col_w;
                col += span;
            }
        }

        let origin = Point::new(origin_x, top);
        for child in &mut children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let mut fragment = Fragment::block(origin, (avail_width, row_height));
        fragment.children = children;
        fragment.source = Some(id);
        BlockResult {
            fragment,
            used: row_height,
            outgoing: None,
            empty: row_height.get() <= 0.0,
        }
    }

    fn collect_table_state(&self, table_id: NodeId, avail_width: Scalar) -> TableState {
        let used = crate::table::table_used_width(self.styles, table_id, avail_width);
        let columns = measure_columns(self.dom, self.styles, table_id, avail_width, used);
        let mut header: Option<TableGroupState> = None;
        let mut footer: Option<TableGroupState> = None;
        let mut body: Vec<TableGroupState> = Vec::new();

        for &child in &self.dom.nodes[table_id].children {
            if let NodeKind::Element(_) = &self.dom.nodes[child].kind {
                match self.styles[child].display {
                    Display::TableHeaderGroup => {
                        header = Some(self.collect_table_group(child, &columns, avail_width));
                    }
                    Display::TableFooterGroup => {
                        footer = Some(self.collect_table_group(child, &columns, avail_width));
                    }
                    Display::TableRowGroup => {
                        body.push(self.collect_table_group(child, &columns, avail_width));
                    }
                    Display::TableRow => {
                        body.push(self.collect_table_group(child, &columns, avail_width));
                    }
                    _ => {}
                }
            }
        }

        TableState {
            header,
            body,
            footer,
            columns: columns.widths,
        }
    }

    fn collect_table_group(
        &self,
        group_id: NodeId,
        columns: &crate::table::ColumnWidths,
        avail_width: Scalar,
    ) -> TableGroupState {
        let rows: Vec<NodeId> = self.dom.nodes[group_id]
            .children
            .iter()
            .copied()
            .filter(|id| match &self.dom.nodes[*id].kind {
                NodeKind::Element(_) => self.styles[*id].display == Display::TableRow,
                _ => false,
            })
            .collect();
        let (row_heights, cell_heights) =
            measure_rows(self.dom, self.styles, &rows, columns, avail_width);
        let mut out_rows = Vec::new();
        for (idx, row_id) in rows.iter().enumerate() {
            let cells: Vec<NodeId> = self.dom.nodes[*row_id]
                .children
                .iter()
                .copied()
                .filter(|id| match &self.dom.nodes[*id].kind {
                    NodeKind::Element(_) => self.styles[*id].display == Display::TableCell,
                    _ => false,
                })
                .collect();
            out_rows.push(TableRowState {
                row_id: *row_id,
                row_height: row_heights.get(idx).copied().unwrap_or(Scalar::ZERO),
                cell_heights: cell_heights.get(idx).cloned().unwrap_or_default(),
                cells,
            });
        }
        TableGroupState {
            group_id,
            rows: out_rows,
        }
    }

    fn table_group_height(group: &TableGroupState) -> Scalar {
        group
            .rows
            .iter()
            .fold(Scalar::ZERO, |acc, row| acc + row.row_height)
    }

    fn layout_table_cell(
        &self,
        id: NodeId,
        origin_x: Scalar,
        avail_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        first_in_flow: bool,
        token: &BreakToken,
        flow: &mut Flow,
    ) -> BlockResult {
        let mut res = self.layout_box(
            id,
            origin_x,
            avail_width,
            top,
            bottom_limit,
            page_has_content,
            first_in_flow,
            token,
            flow,
        );
        // Attach the cell's border box (border-collapse: collapse — the cell's
        // four sides). The emitter strokes each side whose width > 0.
        let st = &self.styles[id];
        if st.border_top.get() > 0.0
            || st.border_right.get() > 0.0
            || st.border_bottom.get() > 0.0
            || st.border_left.get() > 0.0
        {
            if let Some(color) = st.border_color {
                let border_box = BorderBox {
                    top: st.border_top,
                    right: st.border_right,
                    bottom: st.border_bottom,
                    left: st.border_left,
                    color,
                };
                // CORE-100: background + border must COEXIST on the cell.
                // layout_box set FragmentContent::Background(bg) when the cell
                // has a background color; one fragment holds ONE content kind,
                // so the old code overwrote the background with Border
                // whenever any border was present — every corpus table header
                // and footer rendered WHITE (invoice p1: 0 #a8dadc px vs
                // Prince 4,016). Keep the background and push the border as a
                // child fragment instead; the pdf emitter draws backgrounds,
                // then borders, then text, so the border strokes on top of the
                // fill with no order change.
                match res.fragment.content {
                    FragmentContent::Background(_) => {
                        let mut bf = Fragment::block(
                            Point::new(Scalar::ZERO, Scalar::ZERO),
                            (res.fragment.size.0, res.fragment.size.1),
                        );
                        bf.content = FragmentContent::Border(border_box);
                        res.fragment.children.push(bf);
                    }
                    _ => {
                        res.fragment.content = FragmentContent::Border(border_box);
                    }
                }
            }
        }
        res
    }

    /// The incoming token for child `index` of a box: its nested continuation
    /// if present, else a start-fresh (break-before) token.
    fn child_incoming(&self, parent: &BreakToken, index: usize) -> BreakToken {
        parent
            .child_tokens
            .iter()
            .find(|c| c.index == index)
            .map(|c| c.token.clone())
            .unwrap_or_else(BreakToken::break_before)
    }

    /// Whether a block laid out fresh would fit within one full fragmentainer
    /// (used for `break-inside: avoid`). Measures height greedily.
    fn block_fits_fresh(&self, id: NodeId, avail_width: Scalar) -> bool {
        let h = self.measure_block(id, avail_width);
        h <= self.page_height
    }

    /// Measure a block's fresh height greedily (no fragmentation). Bounded and
    /// memo-free but O(subtree); called at most once per avoid-box per flow.
    fn measure_block(&self, id: NodeId, avail_width: Scalar) -> Scalar {
        let style = &self.styles[id];
        if style.display == Display::None {
            return Scalar::ZERO;
        }
        let inner_width = avail_width
            - style.margin_left
            - style.margin_right
            - style.padding_left
            - style.padding_right;
        let mut h =
            style.margin_top + style.padding_top + style.padding_bottom + style.margin_bottom;
        for item in self.collect_items(id) {
            match item {
                Item::Text(text, _, _) => {
                    // The SAME breaker layout uses, so measured heights match
                    // laid-out heights (`break-inside: avoid` correctness).
                    let lines = self.break_paragraph(&text, inner_width, style);
                    h = h + style.line_height * (lines.len() as f64);
                }
                Item::Block(child) => {
                    // A float does not add in-flow height: its own fragment
                    // carries it, and text wraps around it rather than below.
                    // An abspos box is likewise out of flow (no in-flow
                    // height, fragment attaches to the fragmentainer root).
                    if self.styles[child].float != Float::None
                        || matches!(
                            self.styles[child].position,
                            Position::Absolute | Position::Fixed
                        )
                    {
                        continue;
                    }
                    // A monolithic `<img>` contributes its used box height
                    // (CORE-106) — measured the same way layout_image places.
                    let child_el = self.dom.nodes[child].kind.element();
                    if child_el.is_some_and(|el| el.tag == "img") {
                        h = h + self.measure_image(child, inner_width);
                        continue;
                    }
                    h = h + self.measure_block(child, inner_width);
                }
            }
        }
        h
    }

    /// Measure a float's margin box: width (explicit `width` clamped to the
    /// containing block, else shrink-to-fit) and height (content measure at
    /// that width, mirrors [`Ctx::measure_block`] so measured == laid-out).
    fn measure_float(&self, id: NodeId, inner_width: Scalar) -> (Scalar, Scalar) {
        let style = &self.styles[id];
        // A monolithic `<img>` measures by its used box size, never
        // shrink-to-fit (CORE-106).
        if self.dom.nodes[id]
            .kind
            .element()
            .is_some_and(|el| el.tag == "img")
        {
            let (used_w, used_h, _) = self.image_used_size(id, inner_width, style);
            let w = used_w + style.margin_left + style.margin_right;
            return (
                w,
                style.margin_top + style.padding_top + used_h + style.padding_bottom,
            );
        }
        let content_w = match style.width {
            Some(w) => {
                if w.get() > inner_width.get() {
                    inner_width
                } else {
                    w
                }
            }
            None => self.shrink_to_fit(id, inner_width),
        };
        let w = content_w + style.margin_left + style.margin_right;
        let w = if w.get() > inner_width.get() {
            inner_width
        } else {
            w
        };
        let h = self.measure_block(id, w);
        (w, h)
    }

    /// Whether a float's content can usefully fragment across fragmentainers:
    /// it must contain at least one text run somewhere in its subtree. A
    /// float with no text (empty/decorative box) gains nothing from
    /// fragmentation and defers whole (CORE-101).
    fn float_is_splittable(&self, id: NodeId) -> bool {
        for item in self.collect_items(id) {
            match item {
                Item::Text(text, _, _) => {
                    if !text.trim().is_empty() {
                        return true;
                    }
                }                Item::Block(child) => {
                    if self.float_is_splittable(child) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Shrink-to-fit width: the widest line of the float's content (text or
    /// nested blocks), capped at the containing block's inner width.
    fn shrink_to_fit(&self, id: NodeId, max_width: Scalar) -> Scalar {
        let style = &self.styles[id];
        let mut width = Scalar::ZERO;
        for item in self.collect_items(id) {
            match item {
                Item::Text(text, _, _) => {
                    let lines = self.break_paragraph(&text, max_width, style);
                    for line in &lines {
                        let w = line.drawn_width();
                        if w.get() > width.get() {
                            width = w;
                        }
                    }
                }
                Item::Block(child) => {
                    let w = self.shrink_to_fit(child, max_width);
                    if w.get() > width.get() {
                        width = w;
                    }
                }
            }
        }
        if width.get() <= 0.0 {
            max_width
        } else if width.get() > max_width.get() {
            max_width
        } else {
            width
        }
    }

    /// The line origin + available width at `y` for a text segment, given the
    /// active floats. Left floats indent the line start; right floats shorten
    /// the line end. Multiple floats on the same side: the widest wins (the
    /// simplified stacking rule).
    fn segment_geometry(
        &self,
        inner_left: Scalar,
        inner_width: Scalar,
        y: Scalar,
        lh: Scalar,
        flow: &Flow,
    ) -> (Scalar, Scalar) {
        let mut left = Scalar::ZERO;
        let mut right = Scalar::ZERO;
        for f in &flow.active_floats {
            let overlaps = f.y.get() < y.get() + lh.get() && f.bottom().get() > y.get();
            if !overlaps {
                continue;
            }
            match f.side {
                Float::Left => {
                    if f.width.get() > left.get() {
                        left = f.width;
                    }
                }
                Float::Right => {
                    if f.width.get() > right.get() {
                        right = f.width;
                    }
                }
                Float::None => {}
            }
        }
        let reduced = inner_width - left - right;
        let w = if reduced.get() > 0.0 {
            reduced
        } else {
            Scalar::ZERO
        };
        (inner_left + left, w)
    }

    /// Collect a block's children as an ordered item list: contiguous inline
    /// text becomes one `Text` item; each block child becomes a `Block` item.
    /// Inline `<a href>` anchors record their folded-text byte span alongside
    /// the run (CORE-104).
    ///
    /// Footnote-floated elements (CORE-107) never fold their text into the
    /// run: the collector splices a call-marker digit at the element's source
    /// position and records `(marker byte offset, footnote NodeId)` pairs so
    /// line placement can register notes on the page where their call lands.
    fn collect_items(&self, id: NodeId) -> Vec<Item> {
        let mut items: Vec<Item> = Vec::new();
        let mut pending = String::new();
        let mut spans: Vec<LinkSpan> = Vec::new();
        let mut markers: Vec<(usize, NodeId)> = Vec::new();
        self.collect_items_rec(id, &mut items, &mut pending, &mut spans, &mut markers);
        if !pending.trim().is_empty() || !markers.is_empty() {
            items.push(Item::Text(
                std::mem::take(&mut pending),
                std::mem::take(&mut spans),
                std::mem::take(&mut markers),
            ));
        }
        items
    }

    /// The assigned number for one footnote element (CORE-107). Elements
    /// outside any laid-out flow get a stable fallback (0).
    fn footnote_number(&self, id: NodeId) -> usize {
        self.fn_numbers.get(&id).copied().unwrap_or(0)
    }

    /// Register every call marker whose byte span lies fully within
    /// `[line_start, line_end)` as placed on THIS fragmentainer (CORE-107):
    /// the note renders on the page where its marker's line landed.
    fn register_placed_markers(
        &self,
        flow: &mut Flow,
        markers: &[(usize, NodeId)],
        item_line_start: usize,
        line_start: usize,
        line_end: usize,
    ) {
        if markers.is_empty() {
            return;
        }
        for (byte_off, id) in markers {
            let abs = item_line_start + *byte_off;
            if abs >= line_start && abs < line_end && !flow.pending_footnotes.iter().any(|(_, n)| n == id)
            {
                flow.pending_footnotes.push((self.footnote_number(*id), *id));
            }
        }
    }

    fn collect_items_rec(
        &self,
        id: NodeId,
        items: &mut Vec<Item>,
        pending: &mut String,
        spans: &mut Vec<LinkSpan>,
        markers: &mut Vec<(usize, NodeId)>,
    ) {
        for &child in &self.dom.nodes[id].children {
            match &self.dom.nodes[child].kind {
                NodeKind::Text(t) => pending.push_str(t),
                NodeKind::Element(_) => {
                    // CORE-107: a footnote-floated element is replaced in the
                    // run by its call-marker digit. Its own text never folds
                    // into the body; layout registers the note when the line
                    // carrying the marker places.
                    if self.styles[child].float_footnote {
                        let num = self.footnote_number(child);
                        pending.push_str(&num.to_string());
                        markers.push((pending.len() - num.to_string().len(), child));
                        continue;
                    }
                    // Block-level children (including the table family and
                    // flex containers) start a new item so their layout path
                    // is reached; everything else is inline and folds its
                    // text into the current run. Flex containers must be
                    // `Item::Block` or their children get folded into the
                    // parent's text run (CORE-65).
                    //
                    // An `<img>` is a replaced element laid out as a
                    // monolithic block-level box in v1 (CORE-106 spec
                    // Behavior 2); it must reach layout_box as its own item
                    // or it folds into the parent's text run and vanishes.
                    let is_img = self.dom.nodes[child]
                        .kind
                        .element()
                        .is_some_and(|el| el.tag == "img");
                    if is_img
                        || self.styles[child].display == Display::Block
                        || matches!(
                            self.styles[child].display,
                            Display::Table
                                | Display::TableRowGroup
                                | Display::TableHeaderGroup
                                | Display::TableFooterGroup
                                | Display::TableRow
                                | Display::TableCell
                                | Display::Flex
                                | Display::InlineFlex
                        )
                    {
                        if !pending.trim().is_empty() || !spans.is_empty() || !markers.is_empty() {
                            items.push(Item::Text(
                                std::mem::take(pending),
                                std::mem::take(spans),
                                std::mem::take(markers),
                            ));
                        } else {
                            pending.clear();
                        }
                        items.push(Item::Block(child));
                    } else {
                        // A hyperlink anchor records its text span before its
                        // content folds into the run (CORE-104). Nested inline
                        // elements inside it fold under the same span.
                        if self.dom.nodes[child]
                            .kind
                            .element()
                            .is_some_and(|el| el.tag == "a")
                        {
                            if let Some(href) = self.dom.nodes[child]
                                .kind
                                .element()
                                .and_then(|el| el.attr("href"))
                            {
                                let start = pending.len();
                                self.collect_items_rec(child, items, pending, spans, markers);
                                // Whitespace-only anchors yield a zero-width
                                // overlap later → no rect (spec edge case).
                                let end = pending.len();
                                if end > start {
                                    spans.push(LinkSpan {
                                        start,
                                        end,
                                        href: href.to_string(),
                                    });
                                }
                                continue;
                            }
                        }
                        // Inline element: fold its text into the current run.
                        self.collect_items_rec(child, items, pending, spans, markers);
                    }
                }
                NodeKind::Root => {}
            }
        }
    }

    /// Break a main-text run with the typography layer's Knuth-Plass breaker
    /// (real shaped widths, glue, hyphenation, justification). The `hyphens`
    /// and `text-align` computed values reach layout here: `hyphens: auto`
    /// enables Liang hyphenation, `text-align: justify` distributes glue on
    /// non-final lines. Generated content (TOC, margin boxes) does not go
    /// through this — it is shaped single-line via `shape_word` (CORE-83).
    fn break_paragraph(
        &self,
        text: &str,
        max_width: Scalar,
        style: &ComputedStyle,
    ) -> Vec<LineResult> {
        let hyphenate = style.hyphens == Hyphens::Auto;
        let justify = style.text_align == TextAlign::Justify;
        break_paragraph(text, max_width, style, hyphenate, justify)
    }

    /// The x origin for a line under `text-align`, given its drawn ink width.
    /// `inner_left`/`inner_width` are the content box; the line hangs into
    /// the margin by its left protrusion only when flush at the start edge.
    /// Justified lines fill the width (offset zero), so this only matters for
    /// the final (unjustified) line and non-justified alignment.
    fn aligned_x(
        &self,
        inner_left: Scalar,
        inner_width: Scalar,
        drawn: Scalar,
        style: &ComputedStyle,
    ) -> Scalar {
        let free = (inner_width.get() - drawn.get()).max(0.0);
        let off = match style.text_align {
            TextAlign::Start | TextAlign::Left | TextAlign::Justify => 0.0,
            TextAlign::Center => free * 0.5,
            TextAlign::Right | TextAlign::End => free,
        };
        inner_left + Scalar(off)
    }

    /// Resolve a generated-content piece list to a single line of text.
    ///
    /// `string(name)` and `counter(page)` read the current [`Flow`] state;
    /// `counter(pages)` reads the previous pass's total page count (0 on the
    /// first pass); `target-counter(attr(N), page)` reads the previous pass's
    /// page map (empty → `?`); `leader(ch)` fills the remaining line width to
    /// the right
    /// content edge with the repeating character. The leader's fill count is
    /// computed from the fixed-width parts, so it does not depend on the
    /// resolved number glyphs — the property that makes the two-pass TOC
    /// converge (spec §9, §10).
    fn resolve_content(
        &self,
        id: NodeId,
        pieces: &[ContentPiece],
        inner_width: Scalar,
        style: &ComputedStyle,
        flow: &Flow,
    ) -> String {
        // Width RESERVED for the fixed before/after text. Literal pieces are
        // pass-invariant, so they are shaped at their REAL width; resolved
        // pieces (`counter`, `target-counter`) keep the flat 0.5em
        // per-character heuristic so the reservation never depends on the
        // resolved glyphs — the property that makes the two-pass TOC converge
        // (spec §9, §10).
        let reserve_advance = style.font_size.get() * AVG_ADVANCE_EM;
        let face = style.font_face;
        let mut reserved_width = Scalar::ZERO;
        let mut count_reserved = |s: &str, literal: bool| {
            if literal {
                reserved_width =
                    reserved_width + crate::typography::shape_word(s, style.font_size, face).width;
            } else {
                reserved_width =
                    reserved_width + Scalar(s.chars().count() as f64 * reserve_advance);
            }
        };
        // Resolve every non-leader piece to text; note the leader position.
        let mut before = String::new();
        let mut after = String::new();
        let mut leader_char: Option<char> = None;
        for piece in pieces {
            match piece {
                ContentPiece::Literal(s) => {
                    count_reserved(s, true);
                    push_side(&mut before, &mut after, leader_char, s);
                }
                ContentPiece::StringRef(name) => {
                    let v = flow.running.get(name).to_string();
                    count_reserved(&v, false);
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::CounterPage => {
                    let v = flow.page_number().to_string();
                    count_reserved(&v, false);
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::CounterPages => {
                    let v = self.total_pages.to_string();
                    count_reserved(&v, false);
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::CounterRef(_) => {
                    // Named counters are parsed but not tracked yet; render 0.
                    count_reserved("0", false);
                    push_side(&mut before, &mut after, leader_char, "0");
                }
                ContentPiece::TargetCounter { attr } => {
                    let v = self.resolve_target(id, attr);
                    count_reserved(&v, false);
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::Leader(ch) => leader_char = Some(*ch),
            }
        }
        match leader_char {
            None => before,
            Some(ch) => {
                // Fill from the end of `before` to the right edge, leaving room
                // for `after`. No room → no fill (spec edge case).
                //
                // The fill PITCH is the leader char's real shaped advance
                // (CORE-99): a '.' in Arial is ~0.28em, not the 0.5em
                // heuristic, so filling at 0.5em both under-counts the dots
                // and stops the run short of Prince's right-edge fill.
                // The reservation stays literal-accurate / glyph-independent
                // for resolved pieces, so the count never depends on the
                // resolved number glyphs (spec §9, §10).
                let fill_advance =
                    crate::typography::shape_word(&ch.to_string(), style.font_size, face)
                        .width
                        .get()
                        .max(0.01);
                let used = reserved_width.get();
                let room = inner_width.get() - used;
                let count = if room > 0.0 {
                    (room / fill_advance).floor() as usize
                } else {
                    0
                };
                let mut out = before;
                for _ in 0..count {
                    out.push(ch);
                }
                out.push_str(&after);
                out
            }
        }
    }

    /// Resolve a `target-counter(attr(name), page)` on element `id` to its
    /// target page's 1-based number, or `?` when the target is missing (spec
    /// edge case). The named attribute (`href`) is read from `id`; a leading
    /// `#` is stripped and the matching element's recorded page is used.
    fn resolve_target(&self, id: NodeId, attr: &str) -> String {
        let href = match &self.dom.nodes[id].kind {
            NodeKind::Element(e) => e.attr(attr).map(|s| s.to_string()),
            _ => None,
        };
        let Some(href) = href else {
            return "?".to_string();
        };
        let anchor = href.trim_start_matches('#');
        let target = self.dom.nodes.iter().position(|n| match &n.kind {
            NodeKind::Element(e) => e.id.as_deref() == Some(anchor),
            _ => false,
        });
        match target.and_then(|t| self.target_pages.get(&t)) {
            Some(page_index) => (page_index + 1).to_string(),
            None => "?".to_string(),
        }
    }
}

/// Adjust a text-run split for `orphans`/`widows`.
///
/// `first..split` lines stay on the current page; `split..total` move to the
/// next. `orphans` requires at least that many lines *before* the break;
/// `widows` requires at least that many *after*. css-break-3 §4.4: when the
/// constraints cannot both hold (the run is too short), the constraints are
/// dropped rather than looping — we clamp to a valid split. Returns
/// `(adjusted_split, lines_moved)`.
fn apply_orphans_widows(
    first: usize,
    natural_split: usize,
    total: usize,
    orphans: usize,
    widows: usize,
) -> (usize, usize) {
    let orphans = orphans.max(1);
    let widows = widows.max(1);
    let available = total - first;
    // Not enough lines to honor both constraints → drop them (§4.4).
    if available < orphans + widows {
        return (natural_split, total - natural_split);
    }
    let mut split = natural_split;
    // Orphans: keep at least `orphans` lines before the break.
    if split - first < orphans {
        split = first + orphans;
    }
    // Widows: leave at least `widows` lines after the break.
    if total - split < widows {
        split = total - widows;
    }
    // Clamp to a sane range.
    if split <= first {
        split = first + orphans;
    }
    if split >= total {
        split = total - widows;
    }
    (split, total - split)
}

/// A block fragment is "empty" for suppression if it drew nothing and holds no
/// visible children.
fn children_empty(f: &Fragment) -> bool {
    f.children.is_empty() && matches!(f.content, FragmentContent::None)
}

/// Push text either before or after a leader, per whether a leader has been
/// seen yet in the piece list.
fn push_side(before: &mut String, after: &mut String, leader: Option<char>, s: &str) {
    if leader.is_some() {
        after.push_str(s);
    } else {
        before.push_str(s);
    }
}

/// The outcome of resolving the named page in effect for a fragmentainer.
enum PageCtx {
    /// No box starts fresh at the top of this page (pure continuation page) —
    /// the caller carries the previous page's name.
    Carry,
    /// A fresh box's effective `page` is the default page (`page: auto` or no
    /// named declaration on the ancestor-or-self chain) — reset to default.
    Reset,
    /// A fresh box's effective `page` is a named page — switch to it.
    Named(String),
}

/// The named page in effect entering the page laid out with `token`.
///
/// Descends the incoming break-token tree following the fresh-start (break-
/// before) path from `root`; the deepest `page`-declaring box on that path
/// that starts fresh at the top of this page determines the context. Returns
/// `Carry` when no box starts fresh here (the caller keeps the previous
/// page's name), `Reset` when a fresh box's effective page is the default,
/// and `Named(name)` when it is a named page.
///
/// Effective `page` (css-page-3 §4): a box's page name is its own non-auto
/// `page` declaration, else the nearest ancestor-or-self with one, else the
/// default page. This is why `page: auto` on a fresh box *resets* the
/// context even though the parsed declaration is `None` — a named ancestor
/// would carry the name instead.
fn active_page_name(
    dom: &Dom,
    styles: &[ComputedStyle],
    root: NodeId,
    token: &BreakToken,
) -> PageCtx {
    // The deepest fresh-start element seen so far on the walk. A box switches
    // the page context only when it starts fresh here.
    let mut deepest_fresh: Option<NodeId> = None;
    let mut id = root;
    let mut tok = token.clone();
    loop {
        // A box switches the page context only when it starts fresh here.
        if tok.is_break_before() {
            deepest_fresh = Some(id);
        }
        // Descend to the first (lowest-index) unfinished child, mapping the
        // child-item index back to a DOM node. A fresh box with no child
        // tokens yet (e.g. the very first page) starts its first block child
        // fresh too — without this, page 1 can never activate a named page.
        let next = if let Some(ct) = tok.child_tokens.first() {
            nth_block_child(dom, styles, id, ct.index).map(|c| (c, ct.token.clone()))
        } else if tok.is_break_before() {
            nth_block_child(dom, styles, id, 0).map(|c| (c, BreakToken::break_before()))
        } else {
            None
        };
        let Some((child_id, child_tok)) = next else {
            break;
        };
        id = child_id;
        tok = child_tok;
    }

    let Some(fresh) = deepest_fresh else {
        return PageCtx::Carry;
    };
    // Effective page: nearest ancestor-or-self with a non-auto declaration.
    let mut cur = Some(fresh);
    while let Some(n) = cur {
        if let Some(name) = &styles[n].page {
            return PageCtx::Named(name.clone());
        }
        cur = dom.nodes[n].parent;
    }
    PageCtx::Reset
}

/// The DOM node id of the `index`-th block child item of `id`, matching how
/// `collect_items` orders items (contiguous inline text folds into one item).
fn nth_block_child(
    dom: &Dom,
    styles: &[ComputedStyle],
    id: NodeId,
    index: usize,
) -> Option<NodeId> {
    // Reconstruct the item ordering: walk children, treat runs of inline/text
    // as a single item, block children as their own item.
    let mut item = 0usize;
    let mut pending_inline = false;
    for &child in &dom.nodes[id].children {
        match &dom.nodes[child].kind {
            NodeKind::Text(t) => {
                if !t.trim().is_empty() {
                    if !pending_inline {
                        pending_inline = true;
                    }
                }
            }
            NodeKind::Element(_) => {
                if styles[child].display == Display::Block
                    || styles[child].display == Display::Table
                {
                    if pending_inline {
                        item += 1;
                        pending_inline = false;
                    }
                    if item == index {
                        return Some(child);
                    }
                    item += 1;
                } else {
                    pending_inline = true;
                }
            }
            NodeKind::Root => {}
        }
    }
    None
}

/// Walk the DOM in document (pre-order) order and intern every `<img src>`.
/// Fills `keys` with the store key for each image element. Relative file
/// paths resolve against `base_url`.
fn collect_image_sources(
    dom: &Dom,
    root: NodeId,
    store: &mut crate::images::ImageStore,
    infos: &mut BTreeMap<NodeId, ImageInfo>,
    base_url: Option<&std::path::Path>,
) {
    collect_image_sources_rec(dom, root, store, infos, base_url);
}

fn collect_image_sources_rec(
    dom: &Dom,
    id: NodeId,
    store: &mut crate::images::ImageStore,
    infos: &mut BTreeMap<NodeId, ImageInfo>,
    base_url: Option<&std::path::Path>,
) {
    if let Some(el) = dom.nodes[id].kind.element() {
        if el.tag == "img" {
            let src = el.attr("src").unwrap_or("").to_string();
            let alt = el.attr("alt").map(|s| s.to_string());
            // Interning is deterministic: identical sources collapse to one
            // entry, and a broken source still gets a stable key.
            if let Ok(key) = store.intern(&src, base_url, alt) {
                let (broken, width_px, height_px) = match store.get(&key) {
                    Some(crate::images::ImageEntry::Loaded(img)) => {
                        (false, img.width_px, img.height_px)
                    }
                    _ => (true, 0, 0),
                };
                infos.insert(
                    id,
                    ImageInfo {
                        key,
                        broken,
                        width_px,
                        height_px,
                    },
                );
            }
            return; // void element — no children to walk
        }
    }
    for &child in &dom.nodes[id].children {
        collect_image_sources_rec(dom, child, store, infos, base_url);
    }
}

/// Record the first fragmentainer index each sourced element appears on, into
/// `map` (only inserting when absent so the *first* page wins).
fn record_sources(frag: &Fragment, page_index: usize, map: &mut BTreeMap<NodeId, usize>) {
    if let Some(src) = frag.source {
        map.entry(src).or_insert(page_index);
    }
    for child in &frag.children {
        record_sources(child, page_index, map);
    }
}

/// Build and attach margin-box fragments to a fragmentainer's root, positioned
/// in the page-margin area (top row above the content box, bottom row below;
/// side boxes in the left/right margins). Content is resolved against the
/// running-string / counter state now in effect and clipped to one line.
#[allow(clippy::too_many_arguments)]
fn attach_margin_boxes(
    fragmentainer: &mut Fragmentainer,
    spec: &PageSpec,
    geo: &PageGeometry,
    flow: &Flow,
    _styles: &[ComputedStyle],
    _dom: &Dom,
    _target_pages: &BTreeMap<NodeId, usize>,
    total_pages: usize,
) {
    if spec.margin_boxes.is_empty() {
        return;
    }
    let content = geo.content_rect();
    // A fixed margin-box font size (points). Margin boxes are one line.
    let font_size = Scalar(10.0);
    let lh = font_size * NORMAL_LINE_HEIGHT_FACTOR;

    for (name, pieces) in &spec.margin_boxes {
        let text = render_margin_content(pieces, flow, total_pages);
        if text.is_empty() {
            continue;
        }
        let face = crate::fonts::FACE_REGULAR;
        // Shape the resolved content so non-ASCII (em dash, curly quotes, ·)
        // renders as a real glyph with a ToUnicode mapping — never raw UTF-8
        // bytes (CORE-83). Margin boxes are one line; no microtypography.
        let shaped = crate::typography::shape_word(&text, font_size, face);
        let (slot_x, slot_w, slot_y) = margin_box_slot(*name, geo, &content, lh);
        let text_w = shaped.width;
        let x = match name.align() {
            MarginAlign::Start => slot_x,
            MarginAlign::Center => slot_x + Scalar((slot_w.get() - text_w.get()).max(0.0) * 0.5),
            MarginAlign::End => slot_x + Scalar((slot_w.get() - text_w.get()).max(0.0)),
        };
        let baseline = slot_y + crate::typography::baseline_offset(font_size, lh, face);
        let run = TextRun {
            text: shaped.text,
            baseline: Point::new(x, baseline),
            font_size,
            color: crate::css::Color::BLACK,
            font_face: face,
            glyphs: shaped.glyphs,
            expansion: 0.0,
            protrude_left: Scalar::ZERO,
            protrude_right: Scalar::ZERO,
        };
        let mut line = Fragment::line(Point::new(x, slot_y), (slot_w, lh), run);
        line.kind = FragmentKind::Line;
        fragmentainer.root.children.push(line);
    }
}

/// The (x, width, y) slot for a margin box in the page margin area.
fn margin_box_slot(
    name: MarginBoxName,
    geo: &PageGeometry,
    content: &crate::geom::Rect,
    lh: Scalar,
) -> (Scalar, Scalar, Scalar) {
    let third = content.width * (1.0 / 3.0);
    match name.row() {
        MarginRow::Top => {
            // Vertically centered in the top margin band.
            let y = Scalar((geo.margin_top.get() - lh.get()).max(0.0) * 0.5);
            let (x, w) = horizontal_slot(name, content, third);
            (x, w, y)
        }
        MarginRow::Bottom => {
            let band_top = geo.height - geo.margin_bottom;
            let y = band_top + Scalar((geo.margin_bottom.get() - lh.get()).max(0.0) * 0.5);
            let (x, w) = horizontal_slot(name, content, third);
            (x, w, y)
        }
        MarginRow::Left => {
            let x = Scalar::ZERO;
            let w = geo.margin_left;
            let y = content.y + vertical_offset(name, content.height, lh);
            (x, w, y)
        }
        MarginRow::Right => {
            let x = geo.width - geo.margin_right;
            let w = geo.margin_right;
            let y = content.y + vertical_offset(name, content.height, lh);
            (x, w, y)
        }
    }
}

/// Horizontal slot (x, width) for a top/bottom margin box.
fn horizontal_slot(
    name: MarginBoxName,
    content: &crate::geom::Rect,
    third: Scalar,
) -> (Scalar, Scalar) {
    match name.align() {
        MarginAlign::Start => (content.x, third),
        MarginAlign::Center => (content.x + third, third),
        MarginAlign::End => (content.x + third + third, third),
    }
}

/// Vertical offset within a side margin band for top/middle/bottom boxes.
fn vertical_offset(name: MarginBoxName, band_height: Scalar, lh: Scalar) -> Scalar {
    match name.align() {
        MarginAlign::Start => Scalar::ZERO,
        MarginAlign::Center => Scalar((band_height.get() - lh.get()).max(0.0) * 0.5),
        MarginAlign::End => Scalar((band_height.get() - lh.get()).max(0.0)),
    }
}

/// Resolve a margin box's content pieces to a single string. Margin boxes do
/// not fill leaders (no line-break context) and have no target-counter in the
/// demos; leader/target pieces are rendered inertly. `counter(pages)` reads
/// the total page count from the previous layout pass.
fn render_margin_content(pieces: &[ContentPiece], flow: &Flow, total_pages: usize) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            ContentPiece::Literal(s) => out.push_str(s),
            ContentPiece::StringRef(name) => out.push_str(flow.running.get(name)),
            ContentPiece::CounterPage => out.push_str(&flow.page_number().to_string()),
            ContentPiece::CounterPages => out.push_str(&total_pages.to_string()),
            ContentPiece::CounterRef(_) => out.push('0'),
            ContentPiece::TargetCounter { .. } => {}
            ContentPiece::Leader(_) => {}
        }
    }
    out
}

/// Collect `h1`–`h6` headings in DOM (pre-order / document) order, each with
/// its text content and the page its fragment landed on. Headings whose box
/// was suppressed (no fragment) fall back to page 0 so a bookmark still emits
/// (spec edge case).
fn build_headings(dom: &Dom, target_pages: &BTreeMap<NodeId, usize>) -> Vec<Heading> {
    let mut out = Vec::new();
    collect_headings(dom, dom.root, target_pages, &mut out);
    out
}

fn collect_headings(
    dom: &Dom,
    id: NodeId,
    target_pages: &BTreeMap<NodeId, usize>,
    out: &mut Vec<Heading>,
) {
    if let NodeKind::Element(el) = &dom.nodes[id].kind {
        let level = match el.tag.as_str() {
            "h1" => Some(1),
            "h2" => Some(2),
            "h3" => Some(3),
            "h4" => Some(4),
            "h5" => Some(5),
            "h6" => Some(6),
            _ => None,
        };
        if let Some(level) = level {
            let title = dom
                .text_content(id)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let page_index = target_pages.get(&id).copied().unwrap_or(0);
            out.push(Heading {
                level,
                title,
                page_index,
            });
        }
    }
    for &child in &dom.nodes[id].children {
        collect_headings(dom, child, target_pages, out);
    }
}

/// Parse an HTML `width`/`height` attribute as a CSS px length (CORE-106).
/// Integer or decimal digits only; percentages and invalid values are None.
fn parse_px_attr(v: &str) -> Option<f64> {
    let t = v.trim();
    if t.is_empty() || t.ends_with('%') {
        return None;
    }
    t.parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0)
}
