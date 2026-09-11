// SPDX-License-Identifier: AGPL-3.0-only

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
    ComputedStyle, Display, Float, Hyphens, Position, StringSetValue, Stylesheet, TextAlign,
    ViewportLen,
};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::frag::{
    BorderBox, BreakInside, BreakToken, ChildToken, Fragment, FragmentContent, FragmentKind,
    Fragmentainer, TextRun,
};
use crate::geom::{PageGeometry, Point, Rect, Scalar};

mod flex;
mod grid;
mod multicol;
use crate::paged::{
    parse_page_rules, resolve_page_spec, ContentPiece, CounterValue, MarginBoxName, MarginBoxSpec,
    MarginRow, PageMargins, PageRule, PageSpec, RunningStrings, VerticalAlign,
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

/// A document-outline entry (CORE-128), in DOM order. Built during layout
/// from elements whose computed `bookmark-level` is set (UA defaults give
/// `h1`–`h6` levels 1–6); nesting by level resolves at PDF-emit time.
#[derive(Clone, Debug)]
pub struct Heading {
    /// Outline level 1–6 (from `bookmark-level`).
    pub level: u8,
    /// The resolved `bookmark-label` (defaults to the element's text).
    pub title: String,
    /// `bookmark-state`: `true` = open (children visible).
    pub state_open: bool,
    /// Zero-based page index the heading's first fragment landed on.
    pub page_index: usize,
    /// The first fragment's top y within its page (points, top-left origin).
    /// `None` = the element produced no fragment (anchor unresolved — the
    /// entry is dropped with a diagnostic, spec Behavior 8).
    pub y: Option<f64>,
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
    /// The DOM node of the source `<a>` element (CORE-111): the tagged
    /// emitter maps link annotations to their `Link` structure group.
    pub node: NodeId,
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
    /// An inline-block child element: atomic on the line, flow inside
    /// (CORE-120).
    Atomic(NodeId),
}

/// A recorded hyperlink source span inside an item's text (CORE-104):
/// `[start_byte, end_byte)` of the anchor's folded text plus the raw href.
#[derive(Clone, Debug)]
struct LinkSpan {
    start: usize,
    end: usize,
    href: String,
    /// The `<a>` element this span belongs to (CORE-111 tagging).
    node: NodeId,
}

/// Mutable per-flow bookkeeping threaded through pagination in document order:
/// running strings and the page counter. Kept deterministic by only ever being
/// updated in pre-order fragment placement. (Layout geometry stays a pure
/// function of inputs; this is presentation state resolved alongside it.)
#[derive(Clone, Debug, Default)]
struct Flow {
    /// Running strings (`string-set` → `string()`), current values.
    running: RunningStrings,
    /// The `page` counter's value for the current page. Threaded: the
    /// `@page` context increments it at page start (default 1, or the
    /// `counter-increment: page N` value); a document `counter-reset: page N`
    /// overwrites it, a document `counter-increment: page N` adds.
    page_counter: i32,
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
    /// Assignments made during THIS page's body layout: (name, value) in
    /// capture order (spec §Behavior 2). Margin-box keyword resolution reads
    /// this to distinguish first/last on the page; cleared after each page's
    /// margin boxes attach.
    page_string_sets: Vec<(String, String)>,
    /// Named counter values (`counter-reset`/`counter-increment` on non-`page`
    /// names, CORE-128): threaded in document order, read by
    /// `bookmark-label: ... counter(name)` resolution. A sorted Vec keeps any
    /// iteration deterministic (same shape as `RunningStrings`).
    counters: Vec<(String, i32)>,
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
        self.page_counter
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
    /// The initial containing block's content-box dimensions (page 0's content
    /// box, CORE-127/CORE-140). Viewport units (`vw`/`vh`) resolve against
    /// THIS, document-wide — not the page currently being laid out (WPT print
    /// reftests resolve vw/vh against the first page's content box, e.g.
    /// page-size-009: a 100vw box on a named 300px page is 200px — the
    /// :first page's size).
    icb_width: Scalar,
    icb_height: Scalar,
    /// Resolved target-counter values by element `NodeId` (from the
    /// previous layout pass). Empty on the first pass.
    target_pages: &'a BTreeMap<NodeId, usize>,
    /// Per-element named-counter snapshots (from the PREVIOUS layout pass).
    /// Read by `target-counter(attr(N), counter-name)`; empty on the first
    /// pass. Captured during layout at every element that declares
    /// `counter-reset`/`counter-increment` or carries a bookmark (CORE-128's
    /// snapshot sink, unified with CORE-129's named-counter resolution).
    target_counters: &'a BTreeMap<NodeId, Vec<(String, i32)>>,
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
    /// Named-counter snapshot sink (CORE-128): each bookmarked element's
    /// counter state captured when its box starts, for `bookmark-label`
    /// resolution after the pass. A `RefCell` like `links`.
    counter_snaps: &'a std::cell::RefCell<BTreeMap<NodeId, Vec<(String, i32)>>>,
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
    // ONE media evaluation per render (CORE-156): the evaluated text feeds
    // both the cascade (stylo + manual passes) and the @page parser. A
    // false @media cannot leak a @page rule, a true one must still apply.
    let evaluated_css = crate::css::evaluate_media(stylesheet.source(), &geometry);
    let mut styles = crate::css::cascade_evaluated(dom, &evaluated_css, &geometry);
    let page_rules = parse_page_rules(&evaluated_css);
    let root = dom.find_tag("body").unwrap_or(dom.root);

    // Canvas background propagation (CORE-144, css-backgrounds-3 §2.2): the
    // html (else body) background paints the CANVAS — over the page content
    // area, under all content — and the donor box paints none of its own.
    // Resolved once here; the suppression is a plain style mutation (layout
    // is otherwise a pure function of styles, which this preserves: the
    // donor's fill moves to the page, it does not disappear).
    let (canvas_background, body_is_donor) = crate::css::resolve_canvas_background(dom, &styles);
    if body_is_donor {
        if let Some(body) = dom.find_tag("body") {
            styles[body].background_color = None;
        }
    }

    // Intern every <img> source ONCE before pagination (document order):
    // layout then only reads the store through the shared info table. Keys
    // are content hashes, so repeated references collapse for free.
    let mut image_infos: BTreeMap<NodeId, ImageInfo> = BTreeMap::new();
    collect_image_sources(dom, root, images, &mut image_infos, base_url);

    // A `display: none` on the document root ELEMENT (html) suppresses the
    // whole document: one valid empty page, no page-box chrome (CORE-66,
    // root-element-display-none — a blank page must compare equal to a
    // blank reference). The check must read the html element's computed
    // display: `dom.root` is the synthetic document node, which is never
    // element-styled (its display is the initial `inline`), so testing it
    // never fired and the document laid out normally.
    let root_element = dom.find_tag("html").unwrap_or(dom.root);
    if styles[root_element].display == Display::None {
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

    // Does any generated content reference target-counter / target-text? If
    // so, run the bounded multi-pass resolution; otherwise a single pass
    // suffices. (`target-text` is synchronous DOM resolution, but sharing the
    // pass loop keeps one code path and costs one extra pass at most.)
    let needs_toc = styles.iter().any(|s| {
        s.content
            .iter()
            .any(|p| matches!(p, ContentPiece::TargetCounter { .. } | ContentPiece::TargetText { .. }))
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
                .as_ref()
                .and_then(|c| c.as_ref())
                .map(|pieces| pieces.iter().any(|p| matches!(p, ContentPiece::CounterPages)))
                .unwrap_or(false)
        })
    });
    let passes = if needs_toc || needs_total {
        MAX_TOC_PASSES
    } else {
        1
    };

    let mut fn_numbers: BTreeMap<NodeId, usize> = BTreeMap::new();
    let mut target_pages: BTreeMap<NodeId, usize> = BTreeMap::new();
    // Per-element named-counter snapshots (CORE-128 capture site, unified
    // with CORE-129's target-counter resolution — one mechanism, one map).
    let mut target_counters: BTreeMap<NodeId, Vec<(String, i32)>> = BTreeMap::new();
        let mut total_pages: usize = 0;
    let mut collected_links: Vec<CollectedLink> = Vec::new();
    let mut pages = Vec::new();
    for _ in 0..passes {
        // Footnote numbers are per-PASS (CORE-107): document order is stable,
        // so the numbers never change mid-pass even though individual pages
        // may be re-laid-out by the footnote retry loop inside `paginate`.
        fn_numbers = collect_footnote_numbers(dom, &styles);
        let sink = std::cell::RefCell::new(Vec::new());
        let (p, map, counters) = paginate(
            dom,
            &styles,
            &page_rules,
            root,
            &geometry,
            &target_pages,
            &target_counters,
            total_pages,
            &sink,
            &image_infos,
            &fn_numbers,
            canvas_background,
        );
        collected_links = sink.into_inner();
        // Converged when the target map is unchanged between passes AND the
        // page count is stable: resolved page numbers are stable, so the next
        // pass would be identical. `counter(pages)` resolves against
        // `total_pages`, so a stable count means the totals are final too.
        let converged = map == target_pages && p.len() == total_pages;
        pages = p;
        target_pages = map;
        target_counters = counters;
        total_pages = pages.len();
        if converged {
            break;
        }
    }
    // CORE-144: only the FINAL pass's pages emit. The canvas fill paints over
    // every root child at emit time, so an earlier pass's fill would erase a
    // later pass's fixed-position clones. Each pass produces its own
    // fragmentainers; dropping the superseded ones (this `pages` binding is
    // the final pass) is all that's needed.
    // Build the bookmark outline from the final pass's element→page map.
    // `target_counters` IS the final pass's snapshot map (same mechanism).
    let headings = build_headings(dom, &styles, &pages, &target_pages, &target_counters);
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
                node: c.node,
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
    /// The source `<a>` element (CORE-111).
    node: NodeId,
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
            node: span.node,
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
                let m = crate::typography::shape_word_with_features(
                    &marker,
                    font_size,
                    face,
                    &style.ot_features,
                );
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

/// Collect every `position: fixed` element in document order (CORE-127
/// slice b). Fixed boxes are removed from body layout (the out-of-flow
/// branch skips them) and laid out once after pagination, so the scan walks
/// the whole DOM subtree of the layout root up front.
fn collect_fixed_ids(dom: &Dom, styles: &[ComputedStyle], root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    fn walk(dom: &Dom, styles: &[ComputedStyle], id: NodeId, out: &mut Vec<NodeId>) {
        if styles[id].position == Position::Fixed {
            // Nested fixed boxes: the outer one repeats with its subtree,
            // so the inner one is consumed by it (no separate clone).
            out.push(id);
            return;
        }
        for &child in &dom.nodes[id].children {
            walk(dom, styles, child, out);
        }
    }
    walk(dom, styles, root, &mut out);
    out
}

#[allow(clippy::type_complexity)]
fn paginate(
    dom: &Dom,
    styles: &[ComputedStyle],
    page_rules: &[PageRule],
    root: NodeId,
    cli: &PageGeometry,
    target_pages: &BTreeMap<NodeId, usize>,
    target_counters: &BTreeMap<NodeId, Vec<(String, i32)>>,
    total_pages: usize,
    links: &std::cell::RefCell<Vec<CollectedLink>>,
    image_infos: &BTreeMap<NodeId, ImageInfo>,
    fn_numbers: &BTreeMap<NodeId, usize>,
    canvas_background: Option<crate::css::Color>,
) -> (
    Vec<Fragmentainer>,
    BTreeMap<NodeId, usize>,
    BTreeMap<NodeId, Vec<(String, i32)>>,
) {
    let mut pages: Vec<Fragmentainer> = Vec::new();
    let mut incoming = Some(BreakToken::break_before());
    let mut flow = Flow {
        running: RunningStrings::new(),
        page_counter: 0,
        active_floats: Vec::new(),
        pending_floats: Vec::new(),
        abspos_cb: None,
        abspos: Vec::new(),
        pending_footnotes: Vec::new(),
        page_string_sets: Vec::new(),
        counters: Vec::new(),
    };
    // The `html` root is never laid out (layout starts at `body`), but its
    // `counter-reset`/`counter-increment` seed the document counter state the
    // page and margin contexts read (css-page-3 §8; content-010/011/012 reset
    // `foo` on `html`). Apply them once, before the first page.
    if let Some(html_id) = dom.find_tag("html") {
        apply_document_counters(&styles[html_id], &mut flow);
    }
    // Collect `position: fixed` elements once, in document order (CORE-127
    // slice b). The body-layout out-of-flow branch SKIPS them; after the
    // page loop each is laid out exactly once against the first page's
    // content box (the initial containing block — the anchor page), and the
    // resulting fragment CLONES onto every page (css-position-3 §fixed in
    // paged media: fixed content repeats on all pages; Chromium resolves
    // the box against the ICB and repeats it verbatim, fixedpos-010's ref).
    let fixed_ids = collect_fixed_ids(dom, styles, root);
    // The page context's `inherit` source: the root element's computed
    // margins (css-page-3 §3; page-margin-006's `@page { margin: inherit }`
    // takes the root's 0.5in).
    let inherit_margins = dom
        .find_tag("html")
        .map(|id| PageMargins {
            top: styles[id].margin_top,
            right: styles[id].margin_right,
            bottom: styles[id].margin_bottom,
            left: styles[id].margin_left,
        })
        .unwrap_or_else(PageMargins::zero);
    // The ICB content rect, captured when page 0 lays out (declared before
    // the loop so the loop can fill it; CORE-144).
    let mut icb_content: Option<Rect> = None;
    // CORE-128: per-element counter snapshots, filled as bookmarked boxes
    // start (read by `build_headings` after the pass).
    let counter_snaps = std::cell::RefCell::new(BTreeMap::new());
    // The named page in effect, carried across pages until an element switches
    // it (spec §4).
    let mut current_name: Option<String> = None;

    while let Some(mut token) = incoming.take() {
        let page_index = pages.len();
        // A deferred page-change break (CORE-127 slice a) carried out of a
        // page whose LAST fragment placed nothing (all items were floats
        // that deferred whole) would start an empty page: re-absorb it —
        // the break's purpose (a fresh page for the target's context) is
        // already satisfied by the break that moved us here
        // (page-size-007/008 empty-page fix, CORE-145).
        if !token.is_break_before() && token.child_tokens.len() == 1 {
            let only = &token.child_tokens[0];
            if only.token.is_break_before() && only.index == 0 {
                token = only.token.clone();
            }
        }
        // Snapshot the carried string values BEFORE this page's body lays out:
        // `string(name, start)` must show the value entering the page (spec
        // §Behavior 4; Prince-verified — a top-of-page assignment does not
        // count). Margin boxes resolve against this snapshot.
        let page_start_strings = flow.running.clone();
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
        let spec = resolve_page_spec(
            page_rules,
            current_name.as_deref(),
            page_index,
            cli,
            inherit_margins,
        );
        // The `page` counter's @page-context reset/increment applies at page
        // start (css-page-3 §8): increment BEFORE the body lays out, so a
        // document `counter-reset: page N` later overwrites the auto value.
        apply_page_counter(&spec, &mut flow);
        let geo = spec.geometry();
        let content = geo.content_rect();

        let icb = icb_content.unwrap_or(content);
        let ctx = Ctx {
            dom,
            styles,
            content_x: content.x,
            content_width: content.width,
            content_y: content.y,
            page_height: content.height,
            icb_width: icb.width,
            icb_height: icb.height,
            target_pages,
            target_counters,
            total_pages,
            links,
            page_index,
            image_infos,
            fn_numbers,
            counter_snaps: &counter_snaps,
        };
        // The initial containing block for `position: fixed` boxes: the FIRST
        // page's content box (CORE-127 slice b). With per-page @page rules the
        // page geometry varies, so the anchor content rect must be captured
        // from page 0's own resolved spec (named pages included) — resolving
        // it separately after the loop cannot know which name was in effect.
        if page_index == 0 {
            icb_content = Some(content);
        }
        let mut fragmentainer = Fragmentainer::new(page_index, spec.size);
        fragmentainer.background = spec.background;
        fragmentainer.page_orientation = spec.page_orientation;
        fragmentainer.canvas_background = canvas_background;
        // Record the content-box origin (CORE-127 slice b): the fixed-position
        // attachment pass shifts anchor-page fragments to each page's own
        // geometry when a named page resolves a different size/margin.
        fragmentainer.content_origin = Point::new(content.x, content.y);
        // The content SIZE joins it (CORE-144): the canvas-background fill
        // covers the content rect, which needs width and height.
        fragmentainer.content_size = (content.width, content.height);

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
        let mut res =
            ctx.layout_root(root, content.y, &token, &mut flow);
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
                        icb_width: icb.width,
                        icb_height: icb.height,
                        target_pages,
                        target_counters,
                        total_pages,
                        links,
                        page_index,
                        image_infos,
                        fn_numbers,
                        counter_snaps: &counter_snaps,
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

        // @page named-counter reset/increment applies AFTER the body (so the
        // document's own declarations — e.g. `html { counter-reset: foo }` on
        // page 1 — are seen first). A reset shadows the threaded counter for
        // this page only; an increment on a non-reset name threads.
        let page_local = apply_page_named_counters(&spec, &mut flow);
        // Margin boxes resolve against the running-string / counter state now
        // in effect at the end of this page's flow (spec §6, §7); `start`
        // reads the page-start snapshot.
        attach_margin_boxes(
            &mut fragmentainer,
            &spec,
            &geo,
            &flow,
            styles,
            dom,
            target_pages,
            total_pages,
            &page_start_strings,
            &page_local,
        );
        // The page's assignment log is consumed: the next page's `first`/
        // `last`/`first-except` must see only THEIR assignments; `running`
        // keeps the carried values (spec §Behavior 5).
        flow.page_string_sets.clear();

        // Out-of-flow fragments attach to the page root (css-break-3: the
        // fragmentainer is their parent, not the CSS containing block).
        // Stable sort by z-index (None/auto first = painted below); ties keep
        // document order. Their offsets are ALREADY page-absolute (the
        // out-of-flow branch lays against `content.x`/`content.y` directly),
        // matching the margin-box and footnote attachments — the emitter's
        // root walk treats every root child as page-absolute. (The old
        // content-origin subtraction double-shifted out-of-flow paint up-left
        // by the margin size; slice (a)'s simple fixtures passed only because
        // test and ref shifted identically.)
        if !flow.abspos.is_empty() {
            flow.abspos.sort_by_key(|(z, _)| *z);
            for (_, frag) in std::mem::take(&mut flow.abspos) {
                fragmentainer.root.children.push(frag);
            }
        }

        // A trailing page that paints NOTHING is dropped: the page loop is
        // deterministic, so nothing can depend on a blank final page existing
        // (no cross-page state resolves against the page count mid-loop).
        // Without this, a float that deferred whole at the page end could
        // leave a body-only page with an empty fragment (zero-height root
        // child, no abspos, no margin boxes) — an extra blank page the refs
        // do not have (page-size-007/008, CORE-145). Only the LAST page can
        // be blank: every other page's outgoing token means content resumes
        // after it, and content-after implies the break boundary carried
        // paint on one side.
        let page_blank = fragmentainer.root.children.is_empty()
            && fragmentainer.background.is_none()
            && fragmentainer.canvas_background.is_none();
        let is_last = incoming.is_none();

        pages.push(fragmentainer);
        incoming = res.outgoing;

        if pages.len() >= MAX_PAGES {
            break;
        }
    }

    if pages.is_empty() {
        let spec = resolve_page_spec(page_rules, None, 0, cli, inherit_margins);
        apply_page_counter(&spec, &mut flow);
        let page_local = apply_page_named_counters(&spec, &mut flow);
        let mut fragmentainer = Fragmentainer::new(0, spec.size);
        let geo = spec.geometry();
        let content = geo.content_rect();
        fragmentainer.content_origin = Point::new(content.x, content.y);
        fragmentainer.content_size = (content.width, content.height);
        fragmentainer.canvas_background = canvas_background;
        attach_margin_boxes(
            &mut fragmentainer,
            &spec,
            &geo,
            &flow,
            styles,
            dom,
            target_pages,
            total_pages,
            &flow.running,
            &page_local,
        );
        pages.push(fragmentainer);
    }

    // Fixed-position attachment (CORE-127 slice b). Each fixed box lays out
    // ONCE against the initial containing block (page 0's content box —
    // Chromium resolves fixed against the ICB even when later named pages
    // resolve different page boxes, fixedpos-010's ref), then the fragment
    // CLONES onto every page, shifted by each page's content-origin delta so
    // a named page with a different size/margin still positions it correctly
    // (the box itself keeps its resolved geometry; only the page box moves).
    if !fixed_ids.is_empty() {
        let content0 = icb_content.unwrap_or_else(|| {
            // Degenerate: MAX_PAGES=0 path (no pages at all). Fall back to the
            // CLI geometry's content rect — nothing fixed exists to place
            // anyway, but the code must stay total.
            resolve_page_spec(page_rules, None, 0, cli, inherit_margins)
                .geometry()
                .content_rect()
        });
        // Fresh Flow: the fixed layout must not touch the drained body state.
        let mut fx_flow = Flow {
            running: RunningStrings::new(),
            page_counter: 0,
            active_floats: Vec::new(),
            pending_floats: Vec::new(),
            abspos_cb: None,
            abspos: Vec::new(),
            pending_footnotes: Vec::new(),
            page_string_sets: Vec::new(),
            counters: Vec::new(),
        };
        let mut fixed_frags: Vec<(Option<i32>, Fragment, Point)> = Vec::new();
        for &fid in &fixed_ids {
            // Fresh Ctx per fixed box: lays monolithic against the ICB.
            let anchor = Ctx {
                dom,
                styles,
                content_x: content0.x,
                content_width: content0.width,
                content_y: content0.y,
                page_height: content0.height,
                icb_width: content0.width,
                icb_height: content0.height,
                target_pages,
                target_counters,
                total_pages,
                links,
                page_index: 0,
                image_infos,
                fn_numbers,
                counter_snaps: &counter_snaps,
            };
            let fstyle = &styles[fid];
            // Measure the fixed box's margin box at the ICB width (the same
            // path as the abspos branch: explicit width, else shrink-to-fit).
            let (fw, fh) = anchor.measure_float(fid, content0.width);
            // Insets position the margin box against the ICB (css-position-3
            // §fixed in paged media = the page content box; bottom/right
            // resolve against the ICB height).
            let fx = match anchor.resolved_inset(fstyle.inset_left, fstyle.inset_left_viewport) {
                Some(l) => content0.x + l,
                None => match anchor.resolved_inset(fstyle.inset_right, fstyle.inset_right_viewport) {
                    Some(r) => content0.x + content0.width - fw - r,
                    None => content0.x,
                },
            };
            let fy = match anchor.resolved_inset(fstyle.inset_top, fstyle.inset_top_viewport) {
                Some(t) => content0.y + t,
                None => match anchor.resolved_inset(fstyle.inset_bottom, fstyle.inset_bottom_viewport) {
                    Some(b) => content0.y + content0.height - fh - b,
                    None => content0.y,
                },
            };
            let fx_flow = &mut fx_flow;
            let res = anchor.layout_box(
                fid,
                fx,
                fw,
                fy,
                Scalar(f64::MAX),
                false,
                &BreakToken::break_before(),
                fx_flow,
            );
            let z = styles[fid].z_index;
            // Abspos spilled out of the fixed subtree (e.g. an absolute
            // descendant of the fixed box, fixedpos-with-abspos-with-link)
            // lands in fx_flow.abspos in page-absolute coords: rebase each
            // into the fixed fragment's coordinate space so the clones carry
            // them. Paint order: negative z below the fixed box itself, the
            // rest after (auto/positive) — matches the drain's sort.
            let mut spilled = std::mem::take(&mut fx_flow.abspos);
            spilled.sort_by_key(|(z, _)| *z);
            let origin = Point::new(fx, fy);
            let mut frag = res.fragment;
            for (sz, mut sf) in spilled {
                sf.offset = Point::new(sf.offset.x - origin.x, sf.offset.y - origin.y);
                if sz.map_or(true, |z| z < 0) {
                    frag.children.insert(0, sf);
                } else {
                    frag.children.push(sf);
                }
            }
            fixed_frags.push((z, frag, origin));
        }
        for page in &mut pages {
            let dx = page.content_origin.x.get() - content0.x.get();
            let dy = page.content_origin.y.get() - content0.y.get();
            // Fixed clones ride ABOVE the canvas fill: every root child is
            // treated as canvas by the emitter's layering, and painting the
            // canvas fill over the fixed clone would erase it. The clone's
            // own fragments carry the box's background.
            let keep = page.canvas_background;
            page.canvas_background = None;
            for (z, frag, origin) in &fixed_frags {
                let mut clone = frag.clone();
                clone.offset = Point::new(
                    Scalar(origin.x.get() + dx),
                    Scalar(origin.y.get() + dy),
                );
                page.root.children.push(clone);
            }
            // Restore the canvas fill AFTER the fixed clones attach: at emit
            // time the canvas paints over root children, so the fill must not
            // cover the fixed boxes. Keeping the field (not just skipping
            // emit) preserves the propagation for any later pass.
            page.canvas_background = keep;
        }
    }

    // Build the element→page-index map from fragment sources (first page each
    // element lands on), in NodeId order for determinism.
    let mut map: BTreeMap<NodeId, usize> = BTreeMap::new();
    for page in &pages {
        record_sources(&page.root, page.index, &mut map);
    }

    // CORE-128/129: the layout-captured named-counter snapshots double as
    // `target-counter(attr(N), counter-name)`'s resolution map (captured at
    // every element that declares counters or carries a bookmark — see the
    // capture site in `layout_box`). Values are pure document-order facts, so
    // the map is identical across passes and resolution converges on pass 2.
    (pages, map, counter_snaps.into_inner())
}

/// Paint-time offset of a relatively-positioned box (css-position-3 §6.2):
/// a relative box shifts its PAINTED position by its insets without moving
/// its flow position — sibling layout advances past the box's static spot.
///
/// Over-constrained pairs resolve LTR/top-first: `left` beats `right`,
/// `top` beats `bottom`; an absent side leaves that axis unshifted.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RelativeInsetShift {
    /// Horizontal paint shift (points; positive = right).
    dx: Scalar,
    /// Vertical paint shift (points; positive = down).
    dy: Scalar,
}

impl RelativeInsetShift {
    /// Resolve the (dx, dy) pair from the computed insets. Zero when the box
    /// is not relatively positioned, so every caller can apply it
    /// unconditionally.
    pub(crate) fn resolve(
        style: &ComputedStyle,
        content_w: Scalar,
        content_h: Scalar,
    ) -> RelativeInsetShift {
        if style.position != Position::Relative {
            return RelativeInsetShift {
                dx: Scalar::ZERO,
                dy: Scalar::ZERO,
            };
        }
        let l = style.inset_left_viewport.map(|v| v.resolve(content_w, content_h));
        let r = style.inset_right_viewport.map(|v| v.resolve(content_w, content_h));
        let t = style.inset_top_viewport.map(|v| v.resolve(content_w, content_h));
        let b = style.inset_bottom_viewport.map(|v| v.resolve(content_w, content_h));
        // left wins over-constrained cases (LTR); else mirror right.
        let dx = match l.or(style.inset_left) {
            Some(l) => l,
            None => match r.or(style.inset_right) {
                Some(r) => Scalar::ZERO - r,
                None => Scalar::ZERO,
            },
        };
        // top wins over-constrained cases; else mirror bottom.
        let dy = match t.or(style.inset_top) {
            Some(t) => t,
            None => match b.or(style.inset_bottom) {
                Some(b) => Scalar::ZERO - b,
                None => Scalar::ZERO,
            },
        };
        RelativeInsetShift { dx, dy }
    }

    /// Shift a point by the resolved pair (used for the abspos containing
    /// block origin: a relative box's positioned descendants resolve against
    /// its SHIFTED position — css-position-3 §6.2 plus the containing-block
    /// rule for `position: relative` ancestors).
    pub(crate) fn point(&self, p: Point) -> Point {
        Point::new(p.x + self.dx, p.y + self.dy)
    }

    /// Additively shift a fragment's parent-relative origin. Every FRAGMENT
    /// of the box gets the same shift (each page slice lays out through the
    /// same exit), so a box broken across pages paints all slices shifted.
    pub(crate) fn apply(&self, frag: &mut Fragment) {
        frag.offset.x += self.dx;
        frag.offset.y += self.dy;
    }
}


impl<'a> Ctx<'a> {
    /// Resolve a stored viewport length against the initial containing block's
    /// content box (page 0's content — `vw` against ICB width, `vh` against
    /// ICB height). Document-wide: WPT print reftests resolve vw/vh against
    /// the first page's content box, not the page currently being laid out
    /// (css-values-4 §7.8 + page-size-009's assertion, CORE-140).
    fn viewport_pt(&self, v: ViewportLen) -> Scalar {
        v.resolve(self.icb_width, self.icb_height)
    }

    /// Resolve a computed `width`: viewport fraction first, else the absolute
    /// length, else the percentage against `avail`. One seam every width
    /// consumer shares so viewport widths can't drift from percentage widths.
    fn resolved_width(&self, style: &ComputedStyle, avail: Scalar) -> Option<Scalar> {
        if let Some(v) = style.width_viewport {
            return Some(self.viewport_pt(v));
        }
        style
            .width
            .or_else(|| style.width_percent.map(|p| Scalar(p * avail.get())))
    }

    /// Resolve a computed `height`: viewport fraction first, else the absolute
    /// length. Percentage height stays on `height_percent` (flex-only).
    fn resolved_height(&self, style: &ComputedStyle) -> Option<Scalar> {
        if let Some(v) = style.height_viewport {
            return Some(self.viewport_pt(v));
        }
        style.height
    }

    /// Resolve one inset: viewport fraction first, else the absolute length.
    fn resolved_inset(&self, abs: Option<Scalar>, vp: Option<ViewportLen>) -> Option<Scalar> {
        vp.map(|v| self.viewport_pt(v)).or(abs)
    }

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
    /// True when `id` is a replaced image element: `<img>`, or an inline
    /// `<svg>` (CORE-131) that went through the rasterizer bridge.
    fn is_replaced_image(&self, id: NodeId) -> bool {
        self.dom.nodes[id]
            .kind
            .element()
            .is_some_and(|el| el.tag == "img" || el.tag == "svg")
    }

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
        let css_w = self.resolved_width(style, avail_width);
        let css_h = self.resolved_height(style);

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
        style: &ComputedStyle,
        flow: &mut Flow,
    ) -> BlockResult {
        let el = self.dom.nodes[id].kind.element();
        let info = self.image_infos.get(&id);

        // ---- resolve used width/height (points), shared with measure ----
        let (used_w, used_h, _) = self.image_used_size(id, avail_width, style);

        // A monolithic image never resumes, so its top margin applies on
        // every page it lands on (css-break-3 truncates only a fragmented
        // box's continuation top margin — the `!fresh` branch in the block
        // path; images always lay fresh).
        let margin_top = style.margin_top;
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
                        let shaped = crate::typography::shape_word_with_features(
                            alt,
                            style.font_size,
                            face,
                            &style.ot_features,
                        );
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

        // css-position-3 §6.2: a relatively-positioned replaced element
        // paints at its static position plus its insets; the parent cursor
        // advanced via `used`, so following content never reflows.
        RelativeInsetShift::resolve(style, self.icb_width, self.icb_height).apply(&mut fragment);

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
        // The body box's own top margin follows the ordinary css-break-3 rule
        // inside layout_box: applied on the fresh first page (fresh =>
        // margin_top), truncated on continuation pages (!fresh). No post-layout
        // shift is needed — the fragment offset already carries the margin
        // (page-size-006: page-1 content sits at @page margin + body 8px;
        // page 2+ at @page margin alone). parent-child margin collapse is not
        // modeled (documented deviation).
        let bottom_limit = content_top + self.page_height;
        self.layout_box(
            id,
            self.content_x,
            self.content_width,
            content_top,
            bottom_limit,
            false,
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
    /// - `token`: incoming continuation (break-before = start fresh).
    fn layout_box(
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
        let style = &self.styles[id];
        if style.display == Display::None {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (Scalar::ZERO, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: None,
                empty: true,
            };
        }

        // Replaced element: `<img>` and inline `<svg>` (CORE-131) lay out as
        // a monolithic block-level box (CORE-106 spec Behavior 2). Sizing per
        // spec Behavior 6; a box that does not fit defers whole to the next
        // fragmentainer (Behavior 8); broken sources render an alt-text
        // placeholder (Behavior 7).
        let is_img = self.is_replaced_image(id);
        if is_img {
            return self.layout_image(
                id,
                origin_x,
                avail_width,
                top,
                bottom_limit,
                page_has_content,
                style,
                flow,
            );
        }

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
        // Grid containers (css-grid-1, CORE-139): items auto-place into
        // track cells; rows fragment monolithically like flex lines.
        // `inline-grid` is treated as block-level (the inline-flex model).
        if matches!(style.display, Display::Grid) {
            let has_items = self
                .collect_items(id)
                .iter()
                .any(|i| matches!(i, Item::Block(_) | Item::Atomic(_)));
            if has_items {
                return self.layout_grid_container(
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
            // A leaf grid container (text-only children) falls back to the
            // block path so the text renders (flex's leaf fallback model).
        }
        // NOTE: TableCell deliberately does NOT dispatch here — layout_table_cell
        // delegates back into layout_box to lay out the cell's content as a
        // block; routing it through layout_table_like again would recurse forever.
        // TableRowGroup/HeaderGroup/FooterGroup are handled above (they are
        // laid out by layout_table_group, which calls layout_table_row directly).

        let fresh = token.is_break_before();

        // Margins/padding adjoining a fragmentainer break truncate to zero
        // (css-break-3 §3.1). On resume (not fresh) the top margin/padding is
        // gone. A fresh box keeps its full top margin — including the first
        // in-flow box at the top of a page (document start or forced break):
        // css-break-3 truncates margins only after an *unforced* break, and a
        // forced break preserves the margin after it (page-left-right-001).
        // The natural-break first box is a CONTINUATION (the overflowing
        // box's next fragment), which takes the `!fresh` branch below, so its
        // margin still truncates. The prior CORE-95 first-in-flow truncation
        // (matching Prince, not Chromium) dropped the first box's top margin
        // at every page start, regressing page-left-right-001/002 and
        // page-box-006.
        let margin_top = if fresh {
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
        // Declared `width` on a block (CORE-126): the fragment (border box)
        // is the smaller of the declared width and the available width — the
        // stretch-to-fill default only applies when width is auto
        // (css-sizing-3 §5.1). Border-box sizing subtracts chrome like the
        // atomic path (css-ui-3). The parent cursor still advances by the
        // full `avail_width` via `used` (a block keeps its containing-block
        // footprint), so following siblings do not reflow.
        let declared_border_w = {
            let w = self.resolved_width(style, avail_width);
            w.map(|w| {
                if style.box_sizing == crate::css::StyloBoxSizing::BorderBox {
                    w
                } else {
                    w + style.padding_left
                        + style.padding_right
                        + style.border_left
                        + style.border_right
                }
            })
        };
        let box_border_w = match declared_border_w {
            Some(w) => {
                let avail = avail_width - style.margin_left - style.margin_right;
                if w.get() > avail.get() {
                    avail
                } else {
                    w
                }
            }
            None => {
                let avail = avail_width - style.margin_left - style.margin_right;
                if avail.get() < 0.0 {
                    Scalar::ZERO
                } else {
                    avail
                }
            }
        };
        let inner_left = Self::frag_border_x(style, origin_x) + style.padding_left;
        let inner_width = box_border_w
            - style.padding_left
            - style.padding_right
            - style.border_left
            - style.border_right;
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
                        let v = self.dom.text_content(id);
                        flow.page_string_sets.push((name.clone(), v.clone()));
                        flow.running.set(name, v);
                    }
                }
            }
            apply_document_counters(style, flow);
            // CORE-128/129: snapshot the counter state at this box start.
            // A bookmarked element's label may read `counter(name)`; a
            // `target-counter(attr(N), name)` may point at a
            // counter-declaring element. Either fact warrants a snapshot.
            // The LAST snapshot wins (a box that starts on multiple pages
            // keeps its label at the final pass's first-fragment state — the
            // fragment-level map in `record_anchor` uses first occurrence, so
            // the earliest snapshot survives here via entry-or-insert
            // semantics).
            if !matches!(style.bookmark_level, crate::css::BookmarkLevel::None)
                || !style.bookmark_label.is_empty()
                || !style.counter_reset.is_empty()
                || !style.counter_increment.is_empty()
            {
                self.counter_snaps
                    .borrow_mut()
                    .entry(id)
                    .or_insert_with(|| flow.counters.clone());
            }
        }

        // Cursor within the fragmentainer for this box's children.
        let mut y = content_top;
        let mut children: Vec<Fragment> = Vec::new();
        let mut outgoing_children: Vec<ChildToken> = Vec::new();
        let mut broke = false;
        let mut seen_all = true;
        // CORE-127 slice (a): a page-change break at item index j deferred to
        // the top of the item loop — items i+1..j-1 (floats, out-of-flow)
        // still lay on the current page before the break.
        let mut deferred_break_at: Option<usize> = None;
        // Whether the *fragmentainer* holds any content at or above this box's
        // flow position — threaded so monolithic last-resort placement only
        // fires on a genuinely empty page, not merely an empty (just-started)
        // box. Becomes true once this box places anything.
        let mut placed = page_has_content;
        // Out-of-flow content (float/abspos) actually PLACED on this page,
        // distinct from `placed` (which also reflects the parent's pre-existing
        // content). Drives the leading forced-break guard: a SKIPPED fixed box
        // must not make a following break-before fire (it is cloned onto every
        // page, not placed here) — fixedpos-with-link-with-inline-child.
        let mut oof_placed = false;

        // Resume bookkeeping: which child index to start from, and its token.
        // `child_tokens` come positionally; a break-before child token means
        // start that child fresh there.
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
            // A relative box's abspos descendants resolve against the
            // box's SHIFTED origin (css-position-3 §6.2): the paint shift
            // moves the containing block with it. Absolute/fixed resolve
            // against their own insets here, never through this shift.
            flow.abspos_cb = Some((
                RelativeInsetShift::resolve(style, self.icb_width, self.icb_height).point(Point::new(inner_left, box_top)),
                inner_width,
            ));
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
        } else if token.seen_all_children && token.child_tokens.is_empty() {
            // CORE-116: a resumed row whose cells ALL finished on an earlier
            // fragmentainer. `start_index = items.len()` would skip the text
            // entirely, but the block path would then treat "no child token
            // for item 0" as a fresh start and RE-render it — so the row
            // re-laid its first cell on every page (the `[first] [three]`
            // duplication). Marking seen-all with an explicit past-the-end
            // child token makes the resume walk emit nothing: the early-out
            // above fires on the next pass, and this pass places no lines.
            items.len()
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
                let shaped = crate::typography::shape_word_with_features(
                    &text,
                    style.font_size,
                    face,
                    &style.ot_features,
                );
                // A leader fill must land its trailing piece (the page
                // number) FLUSH at the right content edge. The fill count is
                // floored, so the natural dot run can stop up to one pitch
                // short; distribute the exact residual over the run as font
                // expansion (draw-time advance scaling, spec Behavior §8).
                // Expansion is bounded at ±2% like justified text — a leader
                // that would need more than that has no room (spec edge
                // case: "no room → no flush guarantee").
                let drawn = shaped.width.get();
                let target = inner_width.get();
                let expansion = if shaped.text.contains('\u{00b7}') || text.contains('.') && drawn < target {
                    ((target - drawn) / drawn).clamp(-0.02, 0.02)
                } else {
                    0.0
                };
                let run = TextRun {
                    text: shaped.text,
                    baseline: Point::new(inner_left, baseline),
                    font_size: style.font_size,
                    color: style.color,
                    font_face: face,
                    glyphs: shaped.glyphs,
                    expansion,
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
            }
        }

        // CORE-118: adjacent vertical margins collapse to the max
        // (CSS 2.1 §8.3.1). The parent cursor is advanced by each in-flow
        // sibling's margin-bottom (via `used`); a fresh following sibling
        // would otherwise ADD its own full margin-top on top of it. Track
        // the previous sibling's bottom margin so the overlap can be
        // removed before the next fresh block lays out.
        let mut prev_margin_bottom = Scalar::ZERO;
        // Inline-level atomic items (CORE-120) share the current line:
        // `pen_x` is the next placement x, `line_top` the top of the line
        // box, `line_h` its grown height. Declared OUTSIDE the item loop so
        // consecutive atomic items pack side by side on one line.
        let mut atomic_pen_x = inner_left;
        let mut atomic_line_top = y;
        let mut atomic_line_h = Scalar::ZERO;
        let mut atomic_line_active = false;
        let mut i = start_index;
        while i < items.len() {
            // A deferred page-change break fires when the loop reaches the
            // target item (CORE-127 slice (a)).
            if deferred_break_at == Some(i) {
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: i,
                    token: BreakToken::break_before(),
                });
                broke = true;
                break;
            }
            // Forced break-before on a block child starts a new fragmentainer.
            // NOTE: a mid-flow `page` name change does NOT force a break here —
            // the WPT page-name-* references render without breaks, and the
            // harness compares test-vs-ref through the SAME engine, so the
            // name (resolved at page starts by `active_page_name`) is all the
            // page geometry needs. (Tried forcing breaks on name change for
            // CORE-66; it regressed 38 page-name tests.)
            if let Item::Block(child) | Item::Atomic(child) = &items[i] {
                let cstyle = &self.styles[*child];
                let child_fresh = self.child_incoming(token, i).is_break_before();
                if child_fresh
                    && cstyle.break_before.is_forced()
                    && (!children.is_empty() || broke || oof_placed)
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

            // Inline-level atomic items (CORE-120) share the current line:
            // `pen_x`/`line_top`/`line_h` alias the loop-persistent atomic
            // line state so consecutive items pack side by side.
            let lh = line_height(style);
            if !atomic_line_active {
                atomic_pen_x = inner_left;
                atomic_line_top = y;
                atomic_line_h = lh;
                atomic_line_active = true;
            }
            let mut pen_x = atomic_pen_x;
            let line_top = atomic_line_top;
            let mut line_h = atomic_line_h;
            match &items[i] {
                Item::Text(text, link_spans, fn_markers) => {
                    // CORE-118: a text run between blocks is not an adjacent
                    // block sibling — no collapse state carries across it.
                    let child_tok = self.child_incoming(token, i);
                    // Resume: a run that began beside a float resumes by source
                    // offset (its page's available width may differ from the
                    // previous page's); otherwise the legacy line-count resume.
                    let resume_offset = child_tok.consumed_chars;
                    if flow.active_floats.is_empty()
                        && resume_offset.is_none()
                        // A run after atomic items on the same line must
                        // continue beside them (segmented path), not start a
                        // fresh full-width line at the left edge.
                        && !atomic_line_active
                    {
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
                deferred_once: false,
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
                            // Base byte offset of THIS segment in the item's
                            // text (CORE-146 fix): marker byte windows inside
                            // the segment are `seg_base + Σconsumed[..li]`.
                            // `src_offset` itself advances per line below (it
                            // seeds the next segment's re-break), so using it
                            // here double-counted every line and marker
                            // windows drifted to 2× the true offset — notes
                            // called after the first line of a floated-wrap
                            // run never registered.
                            let seg_base = src_offset;
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
                                let line_start = seg_base
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
                                // `seg_base` in the item's text — the
                                // segment's entry offset; `src_offset` has
                                // already advanced past this segment's placed
                                // lines and would double-count them,
                                // CORE-146).
                                let ls = seg_base
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
                deferred_once: false,
                                },
                            });
                            broke = true;
                            break;
                        }
                    }

                    // CORE-158: a bare text run is in-flow content, so a
                    // following in-flow sibling with a DIFFERENT page context
                    // defers a page break before it. The run itself carries its
                    // containing block's context (css-page-3 §4.2), which is
                    // why a boundary can exist on EITHER side of a run. Without
                    // this the boundary after a run was never compared: this
                    // arm placed the text without any comparison, and the
                    // block path's scan only runs after a block child.
                    if placed && !self.orthogonal_flow(id) {
                        let prev_end = self.effective_page(id);
                        let mut j = i + 1;
                        let mut target: Option<(usize, Option<&str>)> = None;
                        while j < items.len() {
                            match &items[j] {
                                Item::Atomic(_) => j += 1,
                                Item::Text(..) => {
                                    target = Some((j, self.effective_page(id)));
                                    break;
                                }
                                Item::Block(b) => {
                                    let cs = &self.styles[*b];
                                    if cs.float != Float::None
                                        || matches!(
                                            cs.position,
                                            Position::Absolute | Position::Fixed
                                        )
                                        || cs.display == Display::None
                                        || cs.height == Some(Scalar::ZERO)
                                    {
                                        j += 1;
                                        continue;
                                    }
                                    let next = match self.page_context_leaf(*b, false) {
                                        Some(leaf) => self.effective_page(leaf),
                                        None => self.effective_page(*b),
                                    };
                                    target = Some((j, next));
                                    break;
                                }
                            }
                        }
                        if let Some((j, next_ctx)) = target {
                            if next_ctx != prev_end {
                                deferred_break_at = Some(j);
                            }
                        }
                    }
                }
                Item::Atomic(child) => {
                    // ---- inline-block (atomic inline-level) placement
                    // (CORE-120) ----
                    let child_tok = self.child_incoming(token, i);
                    let cstyle = &self.styles[*child];
                    if matches!(cstyle.position, Position::Absolute | Position::Fixed) {
                        i += 1;
                        continue;
                    }
                    // Forced break-before propagates to the line: rest of the
                    // items go to the next fragmentainer.
                    if child_tok.is_break_before()
                        && cstyle.break_before.is_forced()
                        && (!children.is_empty() || broke || i > start_index)
                    {
                        seen_all = false;
                        outgoing_children.push(ChildToken {
                            index: i,
                            token: BreakToken::break_before(),
                        });
                        broke = true;
                        break;
                    }


                    // Measure the atomic box's margin box at its used width
                    // (explicit width / percentage, else shrink-to-fit).
                    let (_, avail_w) =
                        self.segment_geometry(inner_left, inner_width, y, lh, flow);
                    let max_w = if avail_w.get() < inner_width.get() {
                        avail_w
                    } else {
                        inner_width
                    };
                    let style_w = self.resolved_width(cstyle, inner_width);
                    // `box-sizing: border-box` (css-ui-3): the declared width
                    // includes padding + border, so subtract them to get the
                    // content width handed to the block layout path.
                    let style_w = style_w.map(|w| {
                        if cstyle.box_sizing == crate::css::StyloBoxSizing::BorderBox {
                            let chrome =
                                cstyle.padding_left + cstyle.padding_right
                                    + cstyle.border_left + cstyle.border_right;
                            if w.get() > chrome.get() {
                                w - chrome
                            } else {
                                Scalar::ZERO
                            }
                        } else {
                            w
                        }
                    });
                    let content_w = match style_w {
                        Some(w) => {
                            if w.get() > max_w.get() {
                                max_w
                            } else {
                                w
                            }
                        }
                        None => self.shrink_to_fit(*child, inner_width),
                    };
                    let aw = content_w + cstyle.margin_left + cstyle.margin_right;
                    let ah = self.measure_block(*child, content_w)
                        + cstyle.margin_top
                        + cstyle.padding_top
                        + cstyle.padding_bottom
                        + cstyle.margin_bottom;
                    // An explicit CSS `height` wins over the measured content
                    // height (the block measure path ignores declared height,
                    // CORE-66 model) — the deferral decision needs the real
                    // box height.
                    let ah = self.resolved_height(cstyle).map_or(ah, |h| {
                        h + cstyle.margin_top
                            + cstyle.padding_top
                            + cstyle.padding_bottom
                            + cstyle.margin_bottom
                    });
                    // Whitespace between inline-blocks collapses per normal
                    // inline whitespace processing; the pen never carries
                    // trailing space across an atomic box.
                    let fits_line = (pen_x.get() - inner_left.get()) + aw.get() <= max_w.get();
                    if !fits_line && placed {
                        // Wrap to a new line below everything placed so far.
                        y = line_top + line_h;
                        pen_x = inner_left;
                    }
                    // Monolithic deferral (spec Behavior 5): a box taller than
                    // the remaining page space defers whole — unless this page
                    // is genuinely empty (last resort: overflow, never loop).
                    let fits_page = y + ah <= bottom_limit;
                    let last_resort = !placed;
                    if !fits_page && !last_resort {
                        seen_all = false;
                        outgoing_children.push(ChildToken {
                            index: i,
                            token: BreakToken::break_before(),
                        });
                        broke = true;
                        break;
                    }

                    // Lay the box through the block path at the resolved x:
                    // border/padding/background paint for free. Its own
                    // content lays without intrusion from sibling atomics on
                    // the same line (like float first placement).
                    let saved_floats = std::mem::take(&mut flow.active_floats);
                    let mut res = self.layout_box(
                        *child,
                        pen_x + cstyle.margin_left,
                        content_w,
                        y + cstyle.margin_top,
                        Scalar(f64::MAX),
                        placed,
                        &child_tok,
                        flow,
                    );
                    // The block path ignores declared `height` (CORE-66
                    // model); size the box fragment to the resolved margin
                    // box so the painted border/background covers it.
                    let inner_h = ah - cstyle.margin_top - cstyle.margin_bottom;
                    res.fragment.size.1 = inner_h;
                    flow.active_floats = saved_floats;

                    // Baseline alignment: the surrounding text baseline sits
                    // `baseline_offset` into the line box; the box's bottom
                    // margin edge aligns to baseline + descent (spec Behavior
                    // 6 fallback). Shift the whole subtree by the delta.
                    let (_, desc_em) = crate::typography::font_metrics(style.font_face);
                    let base_y = line_top
                        + crate::typography::baseline_offset(
                            style.font_size,
                            lh,
                            style.font_face,
                        );
                    let shift = base_y + Scalar(style.font_size.get() * -desc_em)
                        - (y + cstyle.margin_top + ah);
                    fn rebase_subtree(fragment: &mut Fragment, shift: Scalar) {
                        fragment.offset.y = fragment.offset.y + shift;
                        if let FragmentContent::Text(run) = &mut fragment.content {
                            run.baseline.y = run.baseline.y + shift;
                        }
                        for child in &mut fragment.children {
                            rebase_subtree(child, shift);
                        }
                    }
                    if shift.get() != 0.0 {
                        rebase_subtree(&mut res.fragment, shift);
                    }

                    children.push(res.fragment);
                    y += cstyle.margin_top + ah;

                    // Advance the pen past the margin box and grow the line.
                    // Write back through the loop-persistent atomic state so
                    // the NEXT atomic item packs beside this one.
                    atomic_pen_x = pen_x + aw;
                    let bh = y + cstyle.margin_top + ah - line_top;
                    if bh.get() > line_h.get() {
                        atomic_line_h = bh;
                    }
                    placed = true;
                    prev_margin_bottom = Scalar::ZERO;

                    // css-page-3 §4.2 — the placed side of the boundary
                    // comparison, mirrored for an inline-block atomic item:
                    // the box itself is its own leaf, and its effective
                    // context is class-A aware (an inline-level box's own
                    // `page` declaration is inert — page-name-inline-block-
                    // 002's `page:c` inline-block inherits the default
                    // context). A following in-flow BLOCK whose context
                    // differs defers a break before it. The scan skips
                    // atomics (the NEXT atomic hosts its own boundary when
                    // placed) and terminates at bare text (contextless).
                    if !self.orthogonal_flow(*child) && matches!(style.position, Position::Static | Position::Relative)
                    {
                        let prev_end = self.context_effective_page(*child);
                        let mut j = i + 1;
                        let mut target: Option<(usize, Option<&str>)> = None;
                        while j < items.len() {
                            match &items[j] {
                                Item::Atomic(_) => j += 1,
                                Item::Text(..) => {
                                    // CORE-158: a bare text run IS in-flow
                                    // content, and it takes its containing
                                    // block's page context (css-page-3 §4.2).
                                    // The itemizer has already dropped
                                    // whitespace-only runs, so a surviving
                                    // `Item::Text` holds real content.
                                    target = Some((j, self.effective_page(id)));
                                    break;
                                }
                                Item::Block(b) => {
                                    let cs = &self.styles[*b];
                                    if cs.float != Float::None
                                        || matches!(
                                            cs.position,
                                            Position::Absolute | Position::Fixed
                                        )
                                        || cs.display == Display::None
                                        || cs.height == Some(Scalar::ZERO)
                                    {
                                        j += 1;
                                        continue;
                                    }
                                    match self.page_context_leaf(*b, false) {
                                        Some(leaf) => {
                                            target = Some((j, self.effective_page(leaf)));
                                            break;
                                        }
                                        None => {
                                            target = Some((j, self.effective_page(*b)));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if let (Some(prev), Some((j, next_ctx))) = (Some(prev_end), target) {
                            if prev != next_ctx {
                                deferred_break_at = Some(j);
                            }
                        }
                    }
                    i += 1;
                    continue;
                }
                Item::Block(child) => {
                    let child_tok = self.child_incoming(token, i);
                    let cstyle = &self.styles[*child];
                    if cstyle.position == Position::Fixed {
                        // CORE-127 slice b: fixed boxes are laid out ONCE
                        // after the page loop (against the initial
                        // containing block) and their fragment clones onto
                        // every page. Skip them here entirely — laying one
                        // into a page's abspos pool would pin it to a
                        // single page.
                        i += 1;
                        continue;
                    }
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
                        let x = match self.resolved_inset(cstyle.inset_left, cstyle.inset_left_viewport) {
                            Some(l) => cb_origin.x + l,
                            None => match self.resolved_inset(cstyle.inset_right, cstyle.inset_right_viewport) {
                                Some(r) => cb_origin.x + cb_width - fw - r,
                                None => cb_origin.x,
                            },
                        };
                        let y = match self.resolved_inset(cstyle.inset_top, cstyle.inset_top_viewport) {
                            Some(t) => cb_origin.y + t,
                            None => match self.resolved_inset(cstyle.inset_bottom, cstyle.inset_bottom_viewport) {
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
                            &child_tok,
                            flow,
                        );
                        flow.abspos.push((cstyle.z_index, res.fragment));
                        oof_placed = true;
                        // Advance the ITEM index explicitly (`continue` skips
                        // the trailing `i += 1` — the CORE-62 lesson).
                        i += 1;
                        continue;
                    }
                    if cstyle.float != Float::None {
                        // CORE-118: floats do not participate in margin
                        // collapse (documented deviation, spec §Scope) — the
                        // in-flow cursor does not advance past a float, so no
                        // bottom-margin state can carry across one either.
                        prev_margin_bottom = Scalar::ZERO;
                        if child_tok.is_break_before() {
                            // ---- float first placement ----
                            // Measure the float's margin box (width: explicit
                            // or shrink-to-fit; height: content measure).
                            let (fw, fh) = self.measure_float(*child, inner_width);
                            // css floats + fragmentation (css2 §9.5.1 rules
                            // 2+5+7, css-break-3): the float needs vertical
                            // room AND a stacking-free x lane — when no lane
                            // exists beside the overlapping floats, the float
                            // moves DOWN below them (page-size-007/008's
                            // packed floats row across, wrap to a second row,
                            // then overflow to the next page). `last_resort`
                            // (CORE-101) still places on a genuinely empty
                            // page. Fits and placement share one probe so the
                            // lowered y is where the float actually lands.
                            let float_y = self.float_placement_y(
                                fw, fh, inner_width, y, bottom_limit, flow,
                            );
                            let fits = placed && float_y.is_some();
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
                                    // Nothing in-flow was placed before this
                                    // float (it is the page's first item):
                                    // the parent box's fragment carries the
                                    // float via its SUBTREE only when the
                                    // float's own fragment rides `children`
                                    // — but this defer emits a break token
                                    // with NO fragment, so the box paints
                                    // empty here. Mark `broke` WITHOUT
                                    // clearing `empty`-ish paint: the page
                                    // loop's trailing-blank-page check sees
                                    // the empty root and drops the page
                                    // (CORE-145: the deferral itself already
                                    // moved the float to the next page —
                                    // carrying an empty container fragment
                                    // would paint nothing either way).
                                    broke = true;
                                    break;
                                }
                                // Fall through to placement below: the float
                                // lays out at `y` against the real bottom
                                // limit, its content fragments naturally, and
                                // its continuation rides `pending_floats`.
                            }
                            // Place at the probe's y (may be lowered below an
                            // earlier row — css2 §9.5.1 rule 7). The in-flow
                            // cursor `y` does NOT advance: floats are
                            // out-of-flow, so following text stays beside them.
                            let fy = match float_y {
                                Some(v) => v,
                                None => y,
                            };
                            let fx = self.float_x(
                                cstyle.float,
                                fw,
                                inner_left,
                                inner_width,
                                fy,
                                fh,
                                flow,
                            );
                            // A float's own content lays in-flow and ignores
                            // the active intrusion set: floats do not wrap
                            // around sibling floats in this model, and the
                            // shared retain() below must not prune siblings.
                            let saved_floats = std::mem::take(&mut flow.active_floats);
                            let res = self.layout_box(
                                *child,
                                fx,
                                fw,
                                fy,
                                bottom_limit,
                                placed,
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
                                y: fy,
                                width: fw,
                                height: fh,
                                side: cstyle.float,
                            });
                            placed = true;
                            oof_placed = true;
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
                    // CORE-118 margin collapse: the cursor already carries
                    // the previous sibling's margin-bottom; a fresh sibling
                    // adds its own full margin-top inside layout_box. Remove
                    // the overlap so the gap is max(mb_prev, mt_this).
                    if child_tok.is_break_before() && placed && prev_margin_bottom.get() > 0.0 {
                        let mt = self.styles[*child].margin_top;
                        let overlap = if mt.get() < prev_margin_bottom.get() {
                            mt
                        } else {
                            prev_margin_bottom
                        };
                        y = y - overlap;
                    }

                    let res = self.layout_box(
                        *child,
                        inner_left,
                        inner_width,
                        y,
                        bottom_limit,
                        placed,
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
                    // CORE-142: an empty first fragment defers whole. If the
                    // child STARTED on this page but placed no line content
                    // (only padding/background sliced onto the page bottom —
                    // its first line did not fit) and it fits a fresh page,
                    // move it whole to the next page (css-break-3 §3.3: a box
                    // whose first fragment would be empty does not split;
                    // Chromium/Prince behavior on page-size-006). Same
                    // once-per-flow bound as the avoid-defer above: on the
                    // next page the first line fits (the box fits fresh), so
                    // it cannot defer forever.
                    // EXCEPTION (CORE-145): when the page holds floats that
                    // will not move (their carry-over rectangle rides the
                    // next page), deferring the first text line would
                    // misplace it BELOW the float row — the line must place
                    // beside the floats instead of vacating. Chromium packs
                    // the first line beside the carried floats (page-name-
                    // float-002, page-size-007 'second' beside float 9).
                    let floats_carry = !flow.pending_floats.is_empty();
                    let placed_nothing = res.fragment.children.is_empty()
                        && !matches!(res.fragment.content, FragmentContent::Text(_))
                        && !floats_carry;
                    // The fragment must be page-sized or smaller: a monolithic
                    // overflow box (taller than the page, placed with
                    // overflow) also "places nothing" on later fragmentainers
                    // and must keep flowing, not re-defer (monolithic-
                    // overflow-007).
                    let not_huge = res.fragment.size.1.get() <= self.page_height.get();
                    if res.outgoing.is_some()
                        && child_tok.is_break_before()
                        && placed
                        && placed_nothing
                        && not_huge
                        && self.block_fits_fresh(*child, inner_width)
                    {
                        seen_all = false;
                        outgoing_children.push(ChildToken {
                            index: i,
                            token: BreakToken::break_before(),
                        });
                        broke = true;
                        break;
                    }

                    if !res.empty {
                        children.push(res.fragment);
                        y += res.used;
                        // CORE-118: remember this sibling's bottom margin so
                        // the next fresh sibling collapses against it.
                        prev_margin_bottom = cstyle.margin_bottom;
                        placed = true;
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

                    // css-page-3 §4.2 (CORE-127 slice a): a page break is
                    // forced between in-flow boxes whose page contexts differ.
                    // The comparison uses the effective page of the LAST
                    // content leaf of the box just placed vs the FIRST content
                    // leaf of the next content-bearing sibling — declared
                    // values on wrappers do not decide (a `page:foo` parent
                    // shares its children's context; CORE-66's declared-value
                    // compare regressed 38 tests). Out-of-flow (float /
                    // abspos / fixed) and zero-height boxes never host or
                    // demand a boundary; flex containers are leaves (items
                    // are not page-grouped); atomic interiors (inline-block)
                    // and explicit-writing-mode subtrees suppress the break
                    // entirely (v1 deviations, spec Non-Goals). Bare text
                    // between page-declaring blocks does not carry a context
                    // (fixedpos-010's ref keeps trailing text on the named
                    // page), so Text items are invisible to the comparison.
                    // Suppressed inside atomic interiors and flex containers
                    // (row or column — column items lay through this loop via
                    // layout_box and are NOT page-grouped, flex-001/002 refs),
                    // and inside out-of-flow subtrees: an abspos/floated box
                    // lays monolithic (bottom_limit = MAX), so an internal
                    // page-change break would emit an outgoing token the
                    // out-of-flow branch drops — later letters vanish
                    // (page-name-abspos-002 ref keeps both on one page).
                    if !matches!(
                        style.display,
                        Display::InlineBlock | Display::Flex | Display::InlineFlex
                    ) && matches!(style.position, Position::Static | Position::Relative)
                        && style.float == Float::None
                        && !self.orthogonal_flow(id)
                    {
                        // The comparison runs unless the box is a resume-empty
                        // wrapper whose subtree still holds content. Such a
                        // box finished its children on an EARLIER page (its
                        // last fragment just placed nothing here — a forced
                        // break-after fired before it, and-break-003): the
                        // page-change boundary it would demand ALREADY fired
                        // with the child that ended the earlier page, so
                        // breaking again would add a blank page. A genuinely
                        // contentless box (no leaf anywhere, e.g. a wrapper
                        // holding only `display: none` children) hosts its
                        // own boundary — page-name-display-none-child's page:c
                        // wrapper must keep its page even though it places
                        // nothing (Chromium renders the empty page).
                        let prev_leaf = self.page_context_leaf(*child, true);
                        let genuinely_contentless = prev_leaf.is_none();
                        let resuming_wrapper = res.empty && prev_leaf.is_some();
                        if !resuming_wrapper {
                        let prev_end = prev_leaf
                            // A leaf-less box (replaced element, empty div)
                            // IS its own content leaf — fall back to the box
                            // itself so its own `page` declares the boundary
                            // (page-name-canvas-004).
                            .or(Some(*child))
                            // Class-A applicability: an inline-level placed
                            // box (inline-block) carries no page context of
                            // its own — its declared `page` is inert
                            // (page-name-inline-block-002: the `page:c`
                            // inline-block inherits the default context, so
                            // the following `page:c` block differs and
                            // breaks).
                            .map(|n| self.context_effective_page(n));
                        let mut j = i + 1;
                        let mut target: Option<(usize, Option<&str>)> = None;
                        while j < items.len() {
                            match &items[j] {
                                Item::Atomic(a) => {
                                    // An inline-block sibling hosts a
                                    // boundary of its OWN (the placed side's
                                    // rule, mirror: an atomic interior is
                                    // not page-grouped, but the box's
                                    // inherited context is a boundary
                                    // position). A following page-declaring
                                    // BLOCK past an inert inline-block must
                                    // still compare against the block
                                    // (page-name-inline-block-002), so the
                                    // atomic box is only a transient target:
                                    // scan past it to the next block (its own
                                    // boundary fires when IT is placed).
                                    j += 1;
                                }
                                Item::Text(..) => {
                                    // CORE-158: a bare text run IS in-flow
                                    // content, and it takes its containing
                                    // block's page context (css-page-3 §4.2).
                                    // The itemizer has already dropped
                                    // whitespace-only runs, so a surviving
                                    // `Item::Text` holds real content.
                                    target = Some((j, self.effective_page(id)));
                                    break;
                                }
                                Item::Block(b) => {
                                    let cs = &self.styles[*b];
                                    if cs.float != Float::None
                                        || matches!(
                                            cs.position,
                                            Position::Absolute | Position::Fixed
                                        )
                                        || cs.display == Display::None
                                        || cs.height == Some(Scalar::ZERO)
                                    {
                                        j += 1;
                                        continue;
                                    }
                                    match self.page_context_leaf(*b, false) {
                                        Some(leaf) => {
                                            target = Some((j, self.effective_page(leaf)));
                                            break;
                                        }
                                        None => {
                                            // Leaf-less box: same fallback as
                                            // the placed side (.or(Some(child))
                                            // — canvas-004) — the box's own
                                            // effective page (own decl, or the
                                            // default when undeclared,
                                            // pseudo-first-margin-003's pink
                                            // div) still demands the boundary.
                                            // Wrappers whose context matches
                                            // `prev` produce an equal compare
                                            // and no break, so text-adjacent
                                            // zero boxes stay harmless.
                                            target = Some((j, self.effective_page(*b)));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        // A float-only span between the placed box and the
                        // target means the deferred break would land on a
                        // page holding only deferred floats — an EMPTY page
                        // when every one of them defers again (page-size-
                        // 007/008). The break then fires with nothing
                        // in-flow on either side. Skip the comparison unless
                        // a context-bearing target was found (CORE-145).
                        if let (Some(prev), Some((j, next_ctx))) = (prev_end, target) {
                            if prev != next_ctx {
                                // Defer, don't break: items BETWEEN this box
                                // and the target (out-of-flow floats etc.)
                                // still lay on THIS page in document order
                                // (page-name-float-002 ref: page 1 = 'ab').
                                // The loop breaks only when it REACHES j.
                                deferred_break_at = Some(j);
                            }
                        }
                        }
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

        // BFC float containment (css2 §10.6.3): a flow-root / BFC-establishing
        // box's height encloses its floats. `y` only tracks in-flow content,
        // so extend it to the lowest float bottom that lies inside this box
        // (floats at/above the box's content top were placed by an ancestor
        // and do not count). Applies to every fragment of the box: the
        // background must paint behind float-only continuation bands too
        // (page-size-007/008 ref paints the container color behind the
        // floated second row, CORE-145).
        if !flow.active_floats.is_empty() {
            let top_incl = box_top + padding_top;
            let bottom = flow
                .active_floats
                .iter()
                .filter(|f| f.y.get() >= top_incl.get() - 1e-9)
                .map(|f| f.bottom())
                .fold(Scalar::ZERO, |a, b| if b.get() > a.get() { b } else { a });
            if bottom.get() > y.get() {
                y = bottom;
            }
        }

        let mut box_height = y - box_top;
        // Declared `height` (CORE-66 model now refined): when the box finished
        // on this fragmentainer, an explicit CSS height sizes the BORDER box
        // (css-sizing-3 §5.1; box-sizing honored like the atomic path). The
        // block measure path stays content-based, so pagination is unchanged
        // for auto-height boxes; an explicit height only overrides the paint
        // box (larger of content/declared so overflow text never clips).
        if !broke {
            if let Some(mut declared) = self.resolved_height(style) {
                // Content-box sizing: the declared height excludes padding
                // AND border (css-sizing-3 §5.1) — add both to the target.
                if style.box_sizing != crate::css::StyloBoxSizing::BorderBox {
                    declared = declared
                        + style.padding_top
                        + style.padding_bottom
                        + style.border_top
                        + style.border_bottom;
                }
                let target = declared;
                if target.get() > box_height.get() {
                    box_height = target;
                }
            }
        }

        // Background fill spans the box's border box in this fragmentainer
        // (CORE-126 geometry fix): the box's own x starts AFTER its left
        // margin and its width is the resolved border-box width — previously
        // the fill painted the full containing-block width, so inline
        // `margin-left` shifted only text, never the painted box (WPT refs
        // position colored boxes with margins pervasively).
        //
        // css-break-3 fill-to-edge: a box that CONTINUES past this page (a
        // middle fragment) extends its background paint to the fragmentainer
        // bottom edge — only the LAST fragment ends at the content edge
        // (Chromium-verified, page-size-007 test page 1: the yellow container
        // paints full-bleed while its float rows continue). Paint-box only:
        // `used` stays content-based so pagination never sees the fill.
        let paint_height = if broke && style.background_color.is_some() {
            let fill = bottom_limit - box_top;
            if fill.get() > box_height.get() {
                fill
            } else {
                box_height
            }
        } else {
            box_height
        };
        let origin = Point::new(Self::frag_border_x(style, origin_x), box_top);
        let mut fragment = Fragment::block(origin, (box_border_w, paint_height));
        if let Some(bg) = style.background_color {
            if paint_height.get() > 0.0 {
                fragment.content = FragmentContent::Background(bg);
            }
        }
        // Regular-block border painting (CORE-126): the same attach the table
        // cell path uses, so `border` on any block/inline-block paints. The
        // border draws INSIDE the fragment rect (css-backgrounds-3): with
        // content-box sizing the declared width excludes it, so shrink the
        // content area handed to children by the border widths.
        if style.border_top.get() > 0.0
            || style.border_right.get() > 0.0
            || style.border_bottom.get() > 0.0
            || style.border_left.get() > 0.0
        {
            let color = style.border_color.unwrap_or(crate::css::Color::BLACK);
            let border_box = BorderBox {
                top: style.border_top,
                right: style.border_right,
                bottom: style.border_bottom,
                left: style.border_left,
                color,
            };
            match fragment.content {
                FragmentContent::Background(_) => {
                    // CORE-100 model: keep the background, push the border as
                    // a zero-offset child so it strokes ON TOP of the fill.
                    let mut bf = Fragment::block(
                        Point::new(Scalar::ZERO, Scalar::ZERO),
                        (fragment.size.0, fragment.size.1),
                    );
                    bf.content = FragmentContent::Border(border_box);
                    fragment.children.insert(0, bf);
                }
                _ => {
                    fragment.content = FragmentContent::Border(border_box);
                }
            }
        }
        // Rebase children to be parent-relative: each child's offset (and any
        // text baseline) is stored relative to this fragment's own top-left, so
        // the tree carries LayoutNG-style parent-relative geometry. The PDF
        // walk re-accumulates absolutes from the fragmentainer down.
        for child in &mut children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let has_block_child = children
            .iter()
            .any(|c| matches!(c.kind, crate::frag::FragmentKind::Block));
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
                deferred_once: false,
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
        // css-sizing-3 §5.1: a declared `height` sizes the box's FLOW extent —
        // overflow content paints (the paint box above keeps
        // max(content, declared)) but does not push following content down.
        // The cursor advance therefore uses min(content, declared) when the
        // box finished: a `height:0` div with a text line lets floats start
        // at its own top (page-size-007/008: 8 floats per page need the
        // float row to start at y=0, not below the line box). Boxes that
        // broke keep the content-based extent (declared height applies to
        // the whole box, not a fragment).
        // A box whose overflow comes from a BLOCK child keeps the content extent:
// the block child itself fragments/positions against the box, and shrinking
// the flow extent displaces following siblings (block-002-wm-* regression).
// Only own-inline-content overflow (bare text lines, e.g. a `height:0` div
// with a text line — page-size-007/008) clamps to the declared height.
        let flow_height = if !broke && !has_block_child {
            match self.resolved_height(style) {
                Some(mut declared) => {
                    if style.box_sizing != crate::css::StyloBoxSizing::BorderBox {
                        declared = declared
                            + style.padding_top
                            + style.padding_bottom
                            + style.border_top
                            + style.border_bottom;
                    }
                    if declared.get() < box_height.get() {
                        declared
                    } else {
                        box_height
                    }
                }
                None => box_height,
            }
        } else {
            box_height
        };
        let used = (box_top - top) + flow_height + margin_bottom;

        let empty = children_empty(&fragment) && outgoing.is_none() && box_height.get() <= 0.0;

        // css-position-3 §6.2: a relatively-positioned box paints at its
        // static position plus its insets. The shift lands on the FRAGMENT
        // only — the parent cursor advanced via `used`, so siblings and
        // following content never reflow.
        RelativeInsetShift::resolve(style, self.icb_width, self.icb_height).apply(&mut fragment);

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
                deferred_once: false,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };
        let empty = height.get() <= 0.0 && outgoing.is_none();
        // css-position-3 §6.2: paint-only shift; the parent cursor advanced
        // via `used`, so rows and following content keep their flow spots.
        RelativeInsetShift::resolve(&self.styles[id], self.icb_width, self.icb_height).apply(&mut fragment);
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
        // CORE-100: a group-level background (`thead`/`tbody`/`tfoot`) paints
        // as the group fragment's own fill — under row and cell fills. On a
        // continuation page the fragment's height is the placed-slice height,
        // so the fill covers exactly the rows that landed on that page.
        if let Some(bg) = self.styles[id].background_color {
            if height.get() > 0.0 {
                fragment.content = FragmentContent::Background(bg);
            }
        }
        let outgoing = if !seen_all {
            let consumed = token.consumed_block_size + height;
            let tok = BreakToken {
                consumed_block_size: consumed,
                seen_all_children: seen_all,
                child_tokens: outgoing_children,
                break_before: false,
                consumed_chars: None,
                flex: None,
                deferred_once: false,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };
        let empty = height.get() <= 0.0 && outgoing.is_none();
        // css-position-3 §6.2: paint-only shift; the parent cursor advanced
        // via `used`, so following rows keep their flow spots.
        RelativeInsetShift::resolve(&self.styles[id], self.icb_width, self.icb_height).apply(&mut fragment);
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
        let fresh = token.is_break_before();
        // Cells in placement order. Outgoing tokens are indexed by CELL
        // ORDINAL (position among the element children that are table cells),
        // matching this loop — NOT the raw DOM child index, which includes
        // whitespace text nodes between cells.
        let cells: Vec<NodeId> = self.dom.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|c| {
                matches!(self.dom.nodes[*c].kind, NodeKind::Element(_))
                    && self.styles[*c].display == Display::TableCell
            })
            .collect();
        let row_ids = [id];
        let (row_heights, _) = measure_rows(self.dom, self.styles, &row_ids, columns, avail_width);
        let row_height = row_heights.first().copied().unwrap_or(Scalar::ZERO);

        // An UNSTARTED row that cannot fit here defers whole to the next
        // fragmentainer. Three guards keep that from looping forever:
        // - `deferred_once` (CORE-109): a repeating header/footer eats into
        //   every page, so the second attempt force-places instead.
        // - a row taller than a FULL fragmentainer never fits anywhere; it
        //   takes the monolithic path right away.
        // - CORE-116: a CONTINUATION (!fresh) of an already-split row must
        //   place now. Deferring it would hit the same header wall as
        //   CORE-109, and its remaining height only shrinks page over page.
        if top + row_height > bottom_limit
            && page_has_content
            && fresh
            && row_height.get() <= self.page_height.get()
            && !token.deferred_once
        {
            return BlockResult {
                fragment: Fragment::block(Point::new(origin_x, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: Some(BreakToken::break_before_deferred()),
                empty: true,
            };
        }

        let mut x = origin_x;
        let mut children = Vec::new();
        // Tallest cell placement this fragmentainer (Scalar deliberately has
        // no Ord — explicit compare).
        let mut used_h = Scalar::ZERO;
        // True when some cell broke inside and carries a real continuation.
        let mut broke_inside = false;
        // Per-cell outcome THIS pass, indexed by ordinal. Cells not laid this
        // pass keep `None`; before emitting the token we backfill them from
        // the incoming token so finished-cell markers SURVIVE the round trip
        // (dropping them made every later page restart earlier cells — the
        // `[first]` duplication).
        let mut outcomes: Vec<Option<ChildToken>> = vec![None; cells.len()];
        let mut col = 0usize;
        for (ordinal, &cell) in cells.iter().enumerate() {
            let span = crate::table::cell_colspan(self.dom, cell);
            let mut col_w = Scalar::ZERO;
            for j in 0..span {
                col_w = col_w + columns.widths.get(col + j).copied().unwrap_or(Scalar::ZERO);
            }
            let cell_x = x;
            x += col_w;
            col += span;

            // Resume bookkeeping: a cell with a DONE marker (seen-all, no
            // pending children) finished on an earlier fragmentainer — skip
            // it so it does not render twice (CORE-116). Its marker carries
            // forward via the backfill below. Otherwise start it fresh or
            // resume it with its own continuation token.
            let cell_tok = self.child_incoming(token, ordinal);
            if !cell_tok.is_break_before()
                && cell_tok.seen_all_children
                && cell_tok.child_tokens.is_empty()
            {
                continue;
            }
            let mut res = self.layout_table_cell(
                cell,
                cell_x,
                col_w,
                top,
                bottom_limit,
                page_has_content,
                &cell_tok,
                flow,
            );
            if !res.empty {
                // CORE-119 #4 (css-tables-3 §9.7): every cell fills its ROW's
                // height. A fresh row knows its measured height up front; a
                // single-line cell laid out shorter must STRETCH to it so
                // its bottom border lands on the row edge instead of leaving
                // a gap. Resumed rows keep per-cell heights (the row is split;
                // cells on this fragmentainer hold only their own remainder).
                if fresh {
                    let target = row_height;
                    if res.fragment.size.1 < target {
                        // The border child was sized to the PRE-stretch cell
                        // height (CORE-119 follow-up: layout_table_cell
                        // attaches it before we see the row height). Stretch
                        // it too, or its bottom edge floats above the true
                        // row bottom — the header-row gap in CORE-119's
                        // follow-up screenshot.
                        let delta = target - res.fragment.size.1;
                        for child in &mut res.fragment.children {
                            child.size.1 = child.size.1 + delta;
                        }
                        res.fragment.size.1 = target;
                    }
                    if res.used < target {
                        res.used = target;
                    }
                }
                children.push(res.fragment);
            }
            if res.used.get() > used_h.get() {
                used_h = res.used;
            }
            if let Some(tok) = res.outgoing {
                // The cell continues on the next fragmentainer; the row stops
                // here (cells in a row fragment together — css-tables-3 §3.3).
                broke_inside = true;
                outcomes[ordinal] = Some(ChildToken {
                    index: ordinal,
                    token: tok,
                });
                break;
            }
            // Done marker: the cell finished on THIS fragmentainer. A bare
            // break_before would RESTART it on resume; seen_all_children with
            // no pending children is the engine-wide "finished" encoding.
            outcomes[ordinal] = Some(ChildToken {
                index: ordinal,
                token: BreakToken {
                    seen_all_children: true,
                    ..BreakToken::default()
                },
            });
        }

        // Backfill: a cell this pass did NOT lay out (finished on an earlier
        // page and skipped) keeps its incoming state, so its done marker
        // survives into the outgoing token instead of being lost between
        // passes. Cells before an in-flight one are all done; cells after it
        // have not started (drop their entries entirely).
        let mut outgoing_cells: Vec<ChildToken> = Vec::new();
        if broke_inside {
            for (ordinal, slot) in outcomes.into_iter().enumerate() {
                match slot {
                    Some(ct) => outgoing_cells.push(ct),
                    None => {
                        let inc = self.child_incoming(token, ordinal);
                        if !inc.is_break_before() {
                            outgoing_cells.push(ChildToken {
                                index: ordinal,
                                token: inc,
                            });
                        }
                    }
                }
            }
        }

        // A fresh row claims its measured height (the pre-CORE-116 contract:
        // following rows position below the full row box). A resumed row
        // claims only what its cells actually used on THIS fragmentainer —
        // claiming the full measured height again would strand every
        // following row alone on its own page.
        let used = if fresh { row_height } else { used_h };

        let origin = Point::new(origin_x, top);
        for child in &mut children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let mut fragment = Fragment::block(origin, (avail_width, used));
        fragment.children = children;
        fragment.source = Some(id);
        let outgoing = if broke_inside {
            let tok = BreakToken {
                consumed_block_size: token.consumed_block_size + used,
                seen_all_children: false,
                child_tokens: outgoing_cells,
                break_before: false,
                consumed_chars: None,
                flex: None,
                deferred_once: false,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };
        let empty = used.get() <= 0.0 && outgoing.is_none();
        // CORE-100: a row-level background (`tr { background-color }`) paints
        // as the row fragment's own fill. The pre-order collector pushes a
        // parent's Background before its children's, so the row fill lands
        // UNDER the cell fills and under borders/text (css-tables-3 paint
        // order: row groups < rows < cells). Gate on the placed height (`used`),
        // not the measured height: a resumed row claims only what its cells
        // actually used on this fragmentainer.
        if let Some(bg) = self.styles[id].background_color {
            if used.get() > 0.0 {
                fragment.content = FragmentContent::Background(bg);
            }
        }

        // css-position-3 §6.2: paint-only shift; the parent cursor advanced
        // via `used`, so following rows keep their flow spots.
        RelativeInsetShift::resolve(&self.styles[id], self.icb_width, self.icb_height).apply(&mut fragment);
        BlockResult {
            fragment,
            used,
            outgoing,
            empty,
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
                Item::Atomic(child) | Item::Block(child) => {
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
                    // A monolithic `<img>`/`<svg>` contributes its used box
                    // height (CORE-106/CORE-131) — measured the same way
                    // layout_image places.
                    let child_el = self.dom.nodes[child].kind.element();
                    if child_el.is_some_and(|el| el.tag == "img" || el.tag == "svg") {
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
        // A monolithic `<img>`/`<svg>` measures by its used box size, never
        // shrink-to-fit (CORE-106, CORE-131).
        if self.is_replaced_image(id) {
            let (used_w, used_h, _) = self.image_used_size(id, inner_width, style);
            let w = used_w + style.margin_left + style.margin_right;
            return (
                w,
                style.margin_top + style.padding_top + used_h + style.padding_bottom,
            );
        }
        let content_w = match self.resolved_width(style, inner_width) {
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
        // A declared `height` sizes the float's BORDER box even when the
        // content measures smaller (css-sizing-3 §5.1) — layout_box already
        // paints it that way. Empty fixed-height floats (WPT refs position
        // colored boxes this way pervasively) measured ZERO otherwise: they
        // never stacked, never intruded, and never deferred across pages
        // (page-size-007/008 root cause, CORE-145). Same max() shape as the
        // block path's declared-height override; explicit compares (Scalar
        // has no Ord).
        let inner_measured = h - style.margin_top - style.margin_bottom;
        let inner = match self.resolved_height(style) {
            Some(hh) => {
                let target = if style.box_sizing == crate::css::StyloBoxSizing::BorderBox {
                    hh
                } else {
                    hh + style.padding_top
                        + style.padding_bottom
                        + style.border_top
                        + style.border_bottom
                };
                if target.get() > inner_measured.get() {
                    target
                } else {
                    inner_measured
                }
            }
            None => inner_measured,
        };
        (w, style.margin_top + inner + style.margin_bottom)
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
                } Item::Atomic(child) | Item::Block(child) => {
                    if self.float_is_splittable(child) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// CSS 2 §9.5.1 float rules, constraint 2: a left float's outer LEFT edge
    /// must be left of every other left float that overlaps it vertically
    /// (right floats mirror). Float stacking — two floats that vertically
    /// overlap may NOT share the same x; the newcomer goes to the right of
    /// (or below, if no room) the stacked float. Floats do not intrude on
    /// each other's own x placement beyond this rule.
    fn float_x(
        &self,
        side: Float,
        fw: Scalar,
        inner_left: Scalar,
        inner_width: Scalar,
        y: Scalar,
        fh: Scalar,
        flow: &Flow,
    ) -> Scalar {
        let ideal = match side {
            Float::Right => inner_left + inner_width - fw,
            _ => inner_left,
        };
        let mut x = ideal;
        // Iterate: each overlapping float pushes x sideways; repeat until no
        // push remains (bounded by the float count).
        for _ in 0..flow.active_floats.len() + 1 {
            let mut pushed = false;
            for f in &flow.active_floats {
                let overlaps = f.y.get() < y.get() + fh.get() && f.bottom().get() > y.get();
                if !overlaps {
                    continue;
                }
                match (side, f.side) {
                    (Float::Left, Float::Left) => {
                        // My left edge must be >= this float's right edge.
                        let need = f.x + f.width;
                        if x.get() < need.get() - 1e-9 {
                            x = need;
                            pushed = true;
                        }
                    }
                    (Float::Right, Float::Right) => {
                        // My right edge must be <= this float's left edge.
                        let need = f.x - fw;
                        if x.get() > need.get() + 1e-9 {
                            x = need;
                            pushed = true;
                        }
                    }
                    _ => {}
                }
            }
            if !pushed {
                break;
            }
        }
        // Out of horizontal room: fall back to the ideal x. The fits check
        // (stacking-aware, same vertical-overlap predicate) then defers the
        // float to the next fragmentainer, where the space is empty again.
        let min_x = inner_left;
        let max_x = inner_left + inner_width - fw;
        if x.get() < min_x.get() - 1e-9 || x.get() > max_x.get() + 1e-9 {
            return ideal;
        }
        x
    }

    /// Whether a float of margin-box (fw x fh) fits in the fragmentainer
    /// GIVEN the already-placed floats: css2 §9.5.1 rules 2+5+7 — enough
    /// vertical room AND a stacking-free x lane; when no lane exists beside
    /// the overlapping floats, the float moves DOWN below them until it fits
    /// or leaves the fragmentainer. Returns the (possibly lowered) y where
    /// the float should be placed, so fits and placement always agree.
    fn float_placement_y(
        &self,
        fw: Scalar,
        fh: Scalar,
        inner_width: Scalar,
        y: Scalar,
        bottom_limit: Scalar,
        flow: &Flow,
    ) -> Option<Scalar> {
        let mut probe_y = y;
        loop {
            if probe_y + fh > bottom_limit {
                return None;
            }
            // Sum the widths of floats that overlap this probe span: the
            // newcomer needs a stacking-free lane of its own width (css2
            // §9.5.1 rule 2 — overlapping same-side floats may not share x;
            // width-clamped floats may coexist in one lane).
            let mut total = fw.get();
            let mut lowest: Option<Scalar> = None;
            for f in &flow.active_floats {
                let overlaps =
                    f.y.get() < probe_y.get() + fh.get() && f.bottom().get() > probe_y.get();
                if overlaps {
                    total += f.width.get();
                    let b = f.bottom();
                    if lowest.map(|l| b.get() > l.get()).unwrap_or(true) {
                        lowest = Some(b);
                    }
                }
            }
            if total <= inner_width.get() {
                return Some(probe_y);
            }
            // No lane beside the floats: move down below the lowest
            // overlapping float (css2 §9.5.1 rule 7 — a float that does not
            // fit horizontally moves down until it fits).
            match lowest {
                Some(b) => probe_y = b,
                None => return None,
            }
        }
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
                Item::Atomic(child) | Item::Block(child) => {
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

    /// The effective `page` of a box (css-page-3 §4): its own non-auto `page`
    /// declaration, else the nearest ancestor-or-self with one, else None
    /// (the default page). Sibling page-change breaks compare THIS, never
    /// the declared value (a `page:foo` parent's unnamed children share its
    /// context — comparing declared values would break between them).
    fn effective_page(&self, id: NodeId) -> Option<&str> {
        // Own declaration wins — EXCEPT for an inline-level replaced image.
        // css-page-3 §8.1: the `page` property "Applies to: boxes that create
        // class A break points", and an inline-level box creates none. An
        // `<img>`/inline `<svg>` computes `display: inline` by default, so its
        // own `page` declaration neither starts a page for the image nor
        // demands a boundary at it (Chromium oracle, page-name-img-001/002:
        // the image stays on its ancestor's page; a following `page:b` block
        // is the box that breaks). A block-level replaced box
        // (`display: block`) IS a class-A box and keeps its own declaration
        // (page-name-img-003/004).
        let own_decl_applies = !(self.is_replaced_image(id)
            && matches!(
                self.styles[id].display,
                Display::Inline | Display::InlineBlock | Display::InlineFlex
            ));
        if own_decl_applies {
            if let Some(name) = &self.styles[id].page {
                return Some(name.as_str());
            }
        }
        // css-page-3 §4.2 (canvas-004 oracle): an undeclared box continues the
        // page context of the nearest PRECEDING in-flow sibling that declared
        // one (document-order stickiness), else the nearest ancestor-or-self
        // declaration. Stickiness subsumes ancestor inheritance (a declaring
        // parent precedes its children) and keeps trailing siblings with a
        // declaring canvas/sibling (canvas page:b, unnamed div stays 'b').
        if let Some(parent) = self.dom.nodes[id].parent {
            let mut sticky: Option<&str> = None;
            for &sib in &self.dom.nodes[parent].children {
                if sib == id {
                    break;
                }
                let cs = &self.styles[sib];
                if cs.float != Float::None
                    || matches!(cs.position, Position::Absolute | Position::Fixed)
                    || cs.display == Display::None
                {
                    continue;
                }
                if let Some(name) = &cs.page {
                    // Stickiness propagates only from REPLACED elements
                    // (canvas/img/svg — canvas-004 keeps the following div on
                    // the canvas's page). A declaring normal block does not
                    // stick to its followers (siblings-001: c returns to the
                    // default page after b).
                    if self.is_replaced_image(sib) {
                        sticky = Some(name.as_str());
                    }
                }
            }
            if sticky.is_some() {
                return sticky;
            }
            // No sticky sibling: the context is the parent's own (recursively
            // — a text leaf inherits its element chain). Ancestor `page`
            // declarations resolve through the same walk.
            if parent != id {
                return self.effective_page(parent);
            }
        }
        None
    }

    /// The effective page context of `id`, with class-A applicability applied:
    /// an inline-level box (inline-block or inline replaced image) carries no
    /// page-context of its OWN, so its `page` declaration is inert — the
    /// context is inherited from the ancestor chain (css-page-3 §8.1 "Applies
    /// to: boxes that create class A break points"). Used by the sibling
    /// boundary comparison for the placed side: a `page:c` inline-block under
    /// a default-page body inherits the DEFAULT context, so a following
    /// `page:c` block (also class A — its declaration IS inert against the
    /// default) demands the break (page-name-inline-block-002).
    fn context_effective_page(&self, id: NodeId) -> Option<&str> {
        if matches!(
            self.styles[id].display,
            Display::InlineBlock | Display::Inline | Display::InlineFlex
        ) {
            // Skip the box's own declaration: an inline-level box is not
            // class A. Recurse from the parent so the ancestor chain (and
            // any sticky replaced sibling) resolves normally.
            if let Some(parent) = self.dom.nodes[id].parent {
                return self.effective_page(parent);
            }
            return None;
        }
        self.effective_page(id)
    }

    /// True when an INTERMEDIATE ancestor-or-self of `id` (never the root
    /// element itself) carries an explicit `writing-mode` declaration.
    ///
    /// CORE-127 suppression: the engine paginates every flow horizontally in
    /// v1, so page-change breaks inside an orthogonal-flow SUBTREE cannot
    /// match the harness refs (the refs keep the page-declaring pair on one
    /// page by nesting them in a wrapper that switches writing mode —
    /// orthogonal-writing-001/003/004). A writing-mode declaration on the
    /// ROOT element (html) establishes the PAGE's own flow, not an interior
    /// orthogonal context: the page-change break between root-level siblings
    /// still fires (orthogonal-writing-002's `page:a`/`page:b` body children
    /// under `html[writing-mode: vertical-rl]` render two pages in
    /// Chromium, and its ref's margin overflow produces the same two pages).
    fn orthogonal_flow(&self, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(n) = cur {
            if self.styles[n].writing_mode_declared && !self.is_root_element(n) {
                return true;
            }
            cur = self.dom.nodes[n].parent;
        }
        false
    }

    /// True when `id` is the root ELEMENT (html) — the direct child of the
    /// synthetic document node. The document node itself (`dom.root`) is
    /// never element-styled, so only a real `<html>` element can carry the
    /// root-level writing-mode that defines the page flow.
    fn is_root_element(&self, id: NodeId) -> bool {
        matches!(
            self.dom.nodes[id].kind,
            crate::dom::NodeKind::Element(_)
        ) && self.dom.nodes[id].parent == Some(self.dom.root)
    }

    /// The first (`last = false`) or last (`last = true`) in-flow content
    /// leaf of `id`'s subtree, for page-context resolution: descends through
    /// in-flow block children, skipping out-of-flow (float / abspos / fixed),
    /// display:none and zero-height boxes. A qualifying block with no
    /// qualifying children is itself the leaf; a flex container is a leaf
    /// (flex items are not page-grouped). `None` when the subtree holds no
    /// in-flow content — such a box neither hosts nor demands a boundary.
    fn page_context_leaf(&self, id: NodeId, last: bool) -> Option<NodeId> {
        // A flex container is itself the leaf, row or column: its items are
        // not page-grouped (flex-001/002 refs keep item page-changes on one
        // page; flex-004's required break comes from plain divs INSIDE an
        // item, reached via the normal block path). Checked here, not on the
        // children — items blockify to display:block and would defeat a
        // child-side check.
        if matches!(
            self.styles[id].display,
            Display::Flex | Display::InlineFlex | Display::Grid
        ) {
            return Some(id);
        }
        let children = &self.dom.nodes[id].children;
        let ordered: Vec<NodeId> = if last {
            children.iter().rev().copied().collect()
        } else {
            children.clone()
        };
        for child in ordered {
            let cs = &self.styles[child];
            if cs.float != Float::None
                || matches!(cs.position, Position::Absolute | Position::Fixed)
                || cs.display == Display::None
                || cs.height == Some(Scalar::ZERO)
            {
                continue;
            }
            match &self.dom.nodes[child].kind {
                NodeKind::Text(t) => {
                    if !t.trim().is_empty() {
                        return Some(child);
                    }
                }
                NodeKind::Element(_) => {
                    if let Some(leaf) = self.page_context_leaf(child, last) {
                        return Some(leaf);
                    }
                }
                NodeKind::Root => {}
            }
        }
        None
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

    /// The border-box x of a block: after its left margin (css-sizing-3 —
    /// the margin box occupies the containing block, the border box starts
    /// at margin-left). `margin-left: auto` computes to 0 in stylo's
    /// `lp_or_auto_to_pt`, so no auto-centering resolve is needed in v1
    /// (the WPT auto-centering refs need it — Stage 3 follow-up).
    fn frag_border_x(style: &ComputedStyle, origin_x: Scalar) -> Scalar {
        origin_x + style.margin_left
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
                    // CORE-159: `<br>` folds a forced-break sentinel into the
                    // run; the typography breaker turns it into a forced line
                    // break. (html5ever may also surface it as a `Text` child;
                    // that path is empty and harmless.)
                    if self.dom.nodes[child]
                        .kind
                        .element()
                        .is_some_and(|el| el.tag == "br")
                    {
                        pending.push(crate::typography::FORCED_BREAK_CHAR);
                        continue;
                    }
                    // A `display: none` child generates no box: its text must
                    // not fold into the parent's run (page-name-display-none-
                    // child — the hidden child's text leaked onto the empty
                    // page). `layout_box` also returns an empty fragment for
                    // it, so itemizing it would be dead weight.
                    if self.styles[child].display == Display::None {
                        continue;
                    }
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
                    // An `<img>` or inline `<svg>` is a replaced element laid
                    // out as a monolithic block-level box in v1 (CORE-106
                    // spec Behavior 2, CORE-131); it must reach layout_box as
                    // its own item or it folds into the parent's text run
                    // and vanishes.
                    let is_img = self.is_replaced_image(child);
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
                                | Display::InlineBlock
                                | Display::Grid
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
                        if self.styles[child].display == Display::InlineBlock {
                            items.push(Item::Atomic(child));
                        } else {
                            items.push(Item::Block(child));
                        }
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
                                        node: child,
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
                reserved_width = reserved_width
                    + crate::typography::shape_word_with_features(
                        s,
                        style.font_size,
                        face,
                        &style.ot_features,
                    )
                    .width;
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
                ContentPiece::StringRef(name, _kw) => {
                    // Element generated content reads the CURRENT value for
                    // every keyword (spec §Interfaces deviation note); only
                    // margin boxes get page-scoped keyword semantics.
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
                ContentPiece::CounterRef(_) | ContentPiece::CountersRef { .. } => {
                    // Element generated content reads the document counter,
                    // which is not threaded for `content` here (CORE-140 item
                    // 3 targets margin boxes); render the css initial 0.
                    count_reserved("0", false);
                    push_side(&mut before, &mut after, leader_char, "0");
                }
                ContentPiece::TargetCounter { attr, counter } => {
                    let v = self.resolve_target(id, attr, counter);
                    count_reserved(&v, false);
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::TargetText { attr } => {
                    let v = self.resolve_target_text(id, attr);
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
                    crate::typography::shape_word_with_features(
                        &ch.to_string(),
                        style.font_size,
                        face,
                        &style.ot_features,
                    )
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

    /// Resolve a `target-counter(attr(name), counter)` on element `id` to its
    /// target's value (spec edge case: `?` when the target is missing).
    ///
    /// The attribute (`href`) is read from `id`; a leading `#` is stripped and
    /// the matching element's recorded value is used. Counters:
    /// - `page` → the target's 1-based page number (two-pass map).
    /// - `pages` → the total page count from the previous pass.
    /// - any other name → the target's counter snapshot value (`0` when the
    ///   target declares no such counter — css-counters-3 initial value).
    fn resolve_target(&self, id: NodeId, attr: &str, counter: &str) -> String {
        let Some(target) = self.resolve_target_node(id, attr) else {
            return "?".to_string();
        };
        if counter.eq_ignore_ascii_case("pages") {
            return self.total_pages.to_string();
        }
        let page = match self.target_pages.get(&target) {
            Some(p) => *p,
            // The id exists but its element produced no fragment on any page
            // (suppressed box): nothing to resolve.
            None => return "?".to_string(),
        };
        if counter.eq_ignore_ascii_case("page") {
            return (page + 1).to_string();
        }
        // Named counter: the target element's snapshot, else its nearest
        // earlier snapshot carrying that counter (css-counters-3 §4.3.1
        // inheritance — snapshots exist at every counter-declaring or
        // bookmarked element), else 0. The snapshot Vec is (name, value) in
        // declaration order; a linear find keeps lookup deterministic.
        if let Some(snap) = self.target_counters.get(&target) {
            if let Some((_, v)) = snap.iter().find(|(k, _)| k == counter) {
                return v.to_string();
            }
        }
        for (nid, snap) in self.target_counters.iter().rev() {
            if *nid >= target {
                continue;
            }
            if let Some((_, v)) = snap.iter().find(|(k, _)| k == counter) {
                return v.to_string();
            }
        }
        "0".to_string()
    }

    /// Resolve a `target-text(attr(name))` on element `id` to the target
    /// element's text content (css-gcpm-3 §7.1). Missing target → `?`.
    /// Synchronous: reads the DOM directly, so it needs no second pass — the
    /// multi-pass loop is shared purely for code-path uniformity.
    fn resolve_target_text(&self, id: NodeId, attr: &str) -> String {
        match self
            .resolve_target_node(id, attr)
            .map(|t| self.dom.text_content(t))
        {
            Some(text) if !text.is_empty() => text,
            _ => "?".to_string(),
        }
    }

    /// The NodeId a `target-counter`/`target-text` on element `id` points at:
    /// read the named attribute, strip a leading `#`, and find the first
    /// element (document order) carrying that id.
    fn resolve_target_node(&self, id: NodeId, attr: &str) -> Option<NodeId> {
        let href = match &self.dom.nodes[id].kind {
            NodeKind::Element(e) => e.attr(attr).map(|s| s.to_string()),
            _ => None,
        }?;
        let anchor = href.trim_start_matches('#');
        self.dom.nodes.iter().position(|n| match &n.kind {
            NodeKind::Element(e) => e.id.as_deref() == Some(anchor),
            _ => false,
        })
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
                record_image_info(id, key, store, infos);
            }
            return; // void element — no children to walk
        }
        // Inline `<svg>` (CORE-131): serialize the subtree back to SVG bytes
        // and feed the same rasterizer bridge. The serialized subtree is a
        // pure function of the DOM, so the intern key is content-stable.
        if el.tag == "svg" {
            let bytes = serialize_svg_subtree(dom, id);
            if let Ok(key) = store.intern_bytes(bytes, None) {
                record_image_info(id, key, store, infos);
            }
            return; // the subtree is consumed — no children to walk
        }
    }
    for &child in &dom.nodes[id].children {
        collect_image_sources_rec(dom, child, store, infos, base_url);
    }
}

/// Record an interned image's info for `id` (the broken/loaded branch both
/// collectors share).
fn record_image_info(
    id: NodeId,
    key: [u8; 32],
    store: &crate::images::ImageStore,
    infos: &mut BTreeMap<NodeId, ImageInfo>,
) {
    let (broken, width_px, height_px) = match store.get(&key) {
        Some(crate::images::ImageEntry::Loaded(img)) => (false, img.width_px, img.height_px),
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

/// Serialize an inline `<svg>` subtree back to SVG text (CORE-131). Pure
/// function of the DOM: elements emit as `<tag attr="value">`, text as
/// escaped text content, self-closing for empty non-text elements. Casing
/// survives — html5ever keeps camelCase attributes (`viewBox`) on SVG
/// foreign content — so the round-trip preserves the geometry attributes.
fn serialize_svg_subtree(dom: &Dom, id: NodeId) -> Vec<u8> {
    let mut out = String::new();
    serialize_svg_rec(dom, id, &mut out);
    out.into_bytes()
}

fn serialize_svg_rec(dom: &Dom, id: NodeId, out: &mut String) {
    match &dom.nodes[id].kind {
        NodeKind::Element(el) => {
            out.push('<');
            out.push_str(&el.tag);
            for (k, v) in &el.attrs {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                escape_svg_attr(v, out);
                out.push('"');
            }
            if dom.nodes[id].children.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for &child in &dom.nodes[id].children {
                    serialize_svg_rec(dom, child, out);
                }
                out.push_str("</");
                out.push_str(&el.tag);
                out.push('>');
            }
        }
        NodeKind::Text(t) => escape_svg_text(t, out),
        NodeKind::Root => {}
    }
}

/// Minimal XML escaping — the five characters that must never appear raw
/// in text or attribute content. SVG text is plain UTF-8 otherwise.
fn escape_svg_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn escape_svg_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
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
/// running-string / counter state now in effect and clipped to one line. Each
/// box uses its resolved `text-align`/`vertical-align` (UA default table of
/// css-page-3 §6.2 unless the box declared them) and its resolved font/color.
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
    page_start: &RunningStrings,
    page_local: &[(String, i32)],
) {
    if spec.margin_boxes.is_empty() {
        return;
    }
    let content = geo.content_rect();

    for mb in &spec.margin_boxes {
        let text = render_margin_content(&mb.content, mb, flow, page_local, total_pages, page_start);
        if text.is_empty() {
            continue;
        }
        let face = mb.font_face;
        let font_size = mb.font_size;
        let lh = mb.line_height;
        // Shape the resolved content so non-ASCII (em dash, curly quotes, ·)
        // renders as a real glyph with a ToUnicode mapping — never raw UTF-8
        // bytes (CORE-83). Margin boxes are one line; no microtypography.
        let shaped = crate::typography::shape_word(&text, font_size, face);
        let (slot_x, slot_w, slot_y) =
            margin_box_slot(mb.name, geo, &content, lh, mb.text_align, mb.vertical_align);
        let text_w = shaped.width;
        let x = match mb.text_align {
            // CORE-117: center-aligned boxes center on the CONTENT-box
            // midline ((left+right)/2), not within a fixed third-slot. A wide
            // running head then spills into adjacent slots symmetrically
            // (css-page-3 margin-box geometry, matches Prince 16.2).
            TextAlign::Center => {
                content.x + Scalar((content.width.get() - text_w.get()) * 0.5)
            }
            // End-aligned margin boxes anchor their RIGHT edge at the
            // content-box right edge and grow LEFTWARD into the middle slot
            // when wider than one third (css-page-3 margin-box geometry).
            TextAlign::Right | TextAlign::End => content.x + content.width - text_w,
            _ => slot_x,
        };
        let baseline = slot_y + crate::typography::baseline_offset(font_size, lh, face);
        let run = TextRun {
            text: shaped.text,
            baseline: Point::new(x, baseline),
            font_size,
            color: mb.color,
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

/// The (x, width, y) slot for a margin box in the page margin area, honoring
/// the box's resolved `text-align` (inline axis) and `vertical-align` (block
/// axis).
fn margin_box_slot(
    name: MarginBoxName,
    geo: &PageGeometry,
    content: &crate::geom::Rect,
    lh: Scalar,
    text_align: TextAlign,
    vertical_align: VerticalAlign,
) -> (Scalar, Scalar, Scalar) {
    let third = content.width * (1.0 / 3.0);
    match name.row() {
        MarginRow::Top => {
            let y = vertical_slot(vertical_align, geo.margin_top, lh);
            let (x, w) = horizontal_slot(text_align, content, third);
            (x, w, y)
        }
        MarginRow::Bottom => {
            let band_top = geo.height - geo.margin_bottom;
            let y = band_top + vertical_slot(vertical_align, geo.margin_bottom, lh);
            let (x, w) = horizontal_slot(text_align, content, third);
            (x, w, y)
        }
        MarginRow::Left => {
            let x = Scalar::ZERO;
            let w = geo.margin_left;
            let y = content.y + vertical_slot(vertical_align, content.height, lh);
            (x, w, y)
        }
        MarginRow::Right => {
            let x = geo.width - geo.margin_right;
            let w = geo.margin_right;
            let y = content.y + vertical_slot(vertical_align, content.height, lh);
            (x, w, y)
        }
    }
}

/// Horizontal slot (x, width) for a top/bottom margin box.
fn horizontal_slot(
    text_align: TextAlign,
    content: &crate::geom::Rect,
    third: Scalar,
) -> (Scalar, Scalar) {
    match text_align {
        TextAlign::Left | TextAlign::Start => (content.x, third),
        TextAlign::Center => (content.x + third, third),
        TextAlign::Right | TextAlign::End => (content.x + third + third, third),
        TextAlign::Justify => (content.x, third),
    }
}

/// Vertical offset within a margin band for `top`/`middle`/`bottom` alignment.
fn vertical_slot(vertical_align: VerticalAlign, band_height: Scalar, lh: Scalar) -> Scalar {
    match vertical_align {
        VerticalAlign::Top => Scalar::ZERO,
        VerticalAlign::Middle => Scalar((band_height.get() - lh.get()).max(0.0) * 0.5),
        VerticalAlign::Bottom => Scalar((band_height.get() - lh.get()).max(0.0)),
    }
}

/// Resolve a margin box's content pieces to a single string. Margin boxes do
/// not fill leaders (no line-break context) and have no target-counter in the
/// demos; leader/target pieces are rendered inertly. `counter(pages)` reads
/// the total page count from the previous layout pass.
///
/// `string(name, kw)` resolves with css-gcpm-3 §7 keyword semantics against
/// the CURRENT page's assignment log (`flow.page_string_sets`) and the
/// carried current value (`flow.running`); the semantics were probed against
/// Prince 16.2 (see docs/research/css-gcpm/prince-string-keywords-probe.md
/// and the string-set spec §Behavior 4).
fn resolve_string_value(
    name: &str,
    kw: crate::paged::StringKeyword,
    flow: &Flow,
    page_start: &RunningStrings,
) -> String {
    let page_assignments: Vec<&String> = flow
        .page_string_sets
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, v)| v)
        .collect();
    match kw {
        crate::paged::StringKeyword::First => page_assignments
            .first()
            .cloned()
            .cloned()
            .unwrap_or_else(|| flow.running.get(name).to_string()),
        crate::paged::StringKeyword::Last => page_assignments
            .last()
            .cloned()
            .cloned()
            .unwrap_or_else(|| flow.running.get(name).to_string()),
        // Prince-verified: `start` is the value entering the page, even when
        // an assignment sits at the very top of the page — resolve from the
        // page-start snapshot, never the live map.
        crate::paged::StringKeyword::Start => page_start.get(name).to_string(),
        crate::paged::StringKeyword::FirstExcept => {
            if page_assignments.is_empty() {
                flow.running.get(name).to_string()
            } else {
                String::new()
            }
        }
    }
}

fn render_margin_content(
    pieces: &[ContentPiece],
    mb: &MarginBoxSpec,
    flow: &Flow,
    page_local: &[(String, i32)],
    total_pages: usize,
    page_start: &RunningStrings,
) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            ContentPiece::Literal(s) => out.push_str(s),
            ContentPiece::StringRef(name, kw) => {
                out.push_str(&resolve_string_value(name, *kw, flow, page_start))
            }
            ContentPiece::CounterPage => out.push_str(&flow.page_number().to_string()),
            ContentPiece::CounterPages => out.push_str(&total_pages.to_string()),
            ContentPiece::CounterRef(name) => {
                out.push_str(&margin_counter_value(name, mb, flow, page_local).to_string())
            }
            // `counters()` in a page/margin context resolves to the single
            // innermost value: css-page-3 §8 obscures the outer scopes rather
            // than nesting, so the separator never joins (documented residual).
            ContentPiece::CountersRef { name, .. } => {
                out.push_str(&margin_counter_value(name, mb, flow, page_local).to_string())
            }
            ContentPiece::TargetCounter { .. } => {}
            ContentPiece::TargetText { .. } => {}
            ContentPiece::Leader(_) => {}
        }
    }
    out
}

/// Whether a counter name is one of the css-page-3 built-ins (`page`,
/// `pages`), which the `@page`/margin counter machinery handles separately
/// (and which `pages` cannot be reset/incremented at all, §8).
fn is_builtin_counter(name: &str) -> bool {
    name.eq_ignore_ascii_case("page") || name.eq_ignore_ascii_case("pages")
}

/// Apply the `@page` context's `page` counter reset/increment at page start
/// (css-page-3 §8). The auto-increment defaults to 1 and is replaced by an
/// explicit `counter-increment: page N` (content-008/009); a
/// `counter-reset: page N` sets the value first (reset-then-increment,
/// css2.1 §12.4).
fn apply_page_counter(spec: &PageSpec, flow: &mut Flow) {
    if let CounterValue::List(list) = &spec.counter_reset {
        if let Some((_, n)) = list.iter().rev().find(|(name, _)| name == "page") {
            flow.page_counter = *n;
        }
    }
    let mut incr = 1;
    if let CounterValue::List(list) = &spec.counter_increment {
        let names: Vec<i32> = list
            .iter()
            .filter(|(name, _)| name == "page")
            .map(|(_, v)| *v)
            .collect();
        if !names.is_empty() {
            incr = names.iter().sum();
        }
    }
    flow.page_counter += incr;
}

/// Apply the `@page` context's named counter reset/increment after the body
/// lays out (css-page-3 §8). A reset creates a page-local shadow (returned)
/// that hides the threaded document counter for THIS page only; an increment
/// on a name not reset this page threads onto `flow.counters` (content-010/
/// 012's `counter-increment: foo` on a page with no reset).
fn apply_page_named_counters(spec: &PageSpec, flow: &mut Flow) -> Vec<(String, i32)> {
    let mut page_local: Vec<(String, i32)> = Vec::new();
    if let CounterValue::List(list) = &spec.counter_reset {
        for (name, n) in list {
            if is_builtin_counter(name) {
                continue;
            }
            match page_local.iter_mut().find(|(k, _)| k == name) {
                Some((_, v)) => *v = *n,
                None => page_local.push((name.clone(), *n)),
            }
        }
    }
    if let CounterValue::List(list) = &spec.counter_increment {
        for (name, n) in list {
            if is_builtin_counter(name) {
                continue;
            }
            if let Some((_, v)) = page_local.iter_mut().find(|(k, _)| k == name) {
                *v += *n;
            } else {
                match flow.counters.iter_mut().find(|(k, _)| k == name) {
                    Some((_, v)) => *v += *n,
                    None => flow.counters.push((name.clone(), *n)),
                }
            }
        }
    }
    page_local
}

/// Resolve one named counter in a margin box's scope (css-page-3 §8): the
/// box's own reset > the page-context reset shadow > the threaded document
/// counter, then the box's own increment. `page`/`pages` never reach here
/// (they resolve via `CounterPage`/`CounterPages`).
fn margin_counter_value(
    name: &str,
    mb: &MarginBoxSpec,
    flow: &Flow,
    page_local: &[(String, i32)],
) -> i32 {
    let mut value = page_local
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| *v)
        .or_else(|| flow.counters.iter().find(|(k, _)| k == name).map(|(_, v)| *v))
        .unwrap_or(0);
    if let CounterValue::List(list) = &mb.counter_reset {
        for (n, v) in list {
            if n == name {
                value = *v;
            }
        }
    }
    if let CounterValue::List(list) = &mb.counter_increment {
        for (n, v) in list {
            if n == name {
                value += *v;
            }
        }
    }
    value
}

/// Apply one element's `counter-reset` then `counter-increment` (css2.1 §12.4
/// order) to the threaded document counter state: the `page` counter field and
/// the named `flow.counters` map. Shared between [`Ctx::layout_box`] (every
/// fresh element box) and the `html` root seeding in [`paginate`] (the root is
/// never laid out — layout starts at `body`).
fn apply_document_counters(style: &ComputedStyle, flow: &mut Flow) {
    for (name, n) in &style.counter_reset {
        if name.eq_ignore_ascii_case("page") {
            flow.page_counter = *n;
        } else {
            match flow.counters.iter_mut().find(|(k, _)| k == name) {
                Some((_, v)) => *v = *n,
                None => flow.counters.push((name.clone(), *n)),
            }
        }
    }
    for (name, n) in &style.counter_increment {
        if name.eq_ignore_ascii_case("page") {
            flow.page_counter += *n;
        } else {
            match flow.counters.iter_mut().find(|(k, _)| k == name) {
                Some((_, v)) => *v += *n,
                None => flow.counters.push((name.clone(), *n)),
            }
        }
    }
}

/// Collect bookmark outline entries (CORE-128) in DOM (pre-order / document)
/// order: every element whose computed `bookmark-level` is set. Each entry
/// carries its resolved `bookmark-label`, `bookmark-state`, and the page +
/// y-anchor of the element's first fragment. An element with a bookmark
/// declaration but NO fragment on any page (suppressed box) emits a
/// `bookmark-anchor-unresolved` diagnostic and no entry (spec Behavior 8).
fn build_headings(
    dom: &Dom,
    styles: &[ComputedStyle],
    pages: &[Fragmentainer],
    target_pages: &BTreeMap<NodeId, usize>,
    counter_snaps: &BTreeMap<NodeId, Vec<(String, i32)>>,
) -> Vec<Heading> {
    // Element → (page, y) from the fragment trees: same walk as
    // `record_sources`, plus the accumulated y-offset of the first fragment.
    let mut anchors: BTreeMap<NodeId, (usize, f64)> = BTreeMap::new();
    for page in pages {
        record_anchor(&page.root, page.index, 0.0, &mut anchors);
    }

    let mut out = Vec::new();
    collect_headings(
        dom,
        dom.root,
        styles,
        target_pages,
        &anchors,
        counter_snaps,
        &mut out,
    );
    out
}

/// Accumulate each sourced element's first-fragment page + top y. Fragment
/// offsets are parent-relative, so the walk accumulates down the tree
/// (CORE-121 lesson); the first (document-order) occurrence wins.
fn record_anchor(
    frag: &Fragment,
    page_index: usize,
    parent_y: f64,
    map: &mut BTreeMap<NodeId, (usize, f64)>,
) {
    let abs_y = parent_y + frag.offset.y.get();
    if let Some(src) = frag.source {
        map.entry(src).or_insert((page_index, abs_y));
    }
    for child in &frag.children {
        record_anchor(child, page_index, abs_y, map);
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_headings(
    dom: &Dom,
    id: NodeId,
    styles: &[ComputedStyle],
    target_pages: &BTreeMap<NodeId, usize>,
    anchors: &BTreeMap<NodeId, (usize, f64)>,
    counter_snaps: &BTreeMap<NodeId, Vec<(String, i32)>>,
    out: &mut Vec<Heading>,
) {
    if let NodeKind::Element(_) = &dom.nodes[id].kind {
        let style = &styles[id];
        let level = match style.bookmark_level {
            crate::css::BookmarkLevel::None => None,
            crate::css::BookmarkLevel::Level(n) => Some(n),
        };
        if let Some(level) = level {
            match target_pages.get(&id) {
                Some(&page_index) => {
                    // Resolved label: `bookmark-label` pieces, or the element's
                    // own text when unset (the `contents()` default).
                    let title = if style.bookmark_label.is_empty() {
                        dom.text_content(id)
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {
                        resolve_bookmark_label(style, id, counter_snaps)
                    };
                    let y = anchors.get(&id).map(|(_, y)| *y).unwrap_or(0.0);
                    out.push(Heading {
                        level,
                        title,
                        state_open: !style.bookmark_closed,
                        page_index,
                        y: Some(y),
                    });
                }
                None => {
                    // Spec Behavior 8: a bookmark declaration that resolves to
                    // no destination (the element produced no fragment) is a
                    // warning and no entry — never an invalid destination
                    // (krilla panics on out-of-range page indexes). The
                    // event drains through `--diagnostics` in main.rs.
                    let el = &dom.nodes[id];
                    let desc = match &el.kind {
                        NodeKind::Element(e) => {
                            let mut s = format!("<{}", e.tag);
                            if let Some(id_attr) = &e.id {
                                s.push_str(&format!(" id=\"{id_attr}\""));
                            }
                            if !e.classes.is_empty() {
                                s.push_str(&format!(" class=\"{}\"", e.classes.join(" ")));
                            }
                            s.push('>');
                            s
                        }
                        _ => "element".to_string(),
                    };
                    crate::diagnostics::report_bookmark_anchor_unresolved(format!(
                        "bookmark-anchor-unresolved: {desc} has bookmark-* declarations but resolves to no destination"
                    ));
                }
            }
        }
    }
    for &child in &dom.nodes[id].children {
        collect_headings(dom, child, styles, target_pages, anchors, counter_snaps, out);
    }
}

/// Resolve a `bookmark-label` content list to the outline entry text
/// (CORE-128). Literals pass through; `counter(name)` reads the counter state
/// captured at the element's box start (`counter_snaps`); the other piece
/// kinds (`string()`, `counter(page|pages)`, `target-counter`, `leader()`)
/// contribute nothing — page state is not retained per element (documented
/// residual; literal + counter covers the css-gcpm examples).
fn resolve_bookmark_label(
    style: &ComputedStyle,
    id: NodeId,
    counter_snaps: &BTreeMap<NodeId, Vec<(String, i32)>>,
) -> String {
    let mut out = String::new();
    for piece in &style.bookmark_label {
        match piece {
            crate::paged::ContentPiece::Literal(s) => out.push_str(s),
            crate::paged::ContentPiece::CounterRef(name)
            | crate::paged::ContentPiece::CountersRef { name, .. } => {
                if let Some(snaps) = counter_snaps.get(&id) {
                    let val = snaps
                        .iter()
                        .find(|(k, _)| k == name)
                        .map(|(_, v)| *v)
                        .unwrap_or(0);
                    out.push_str(&val.to_string());
                }
            }
            crate::paged::ContentPiece::StringRef(..) => {}
            crate::paged::ContentPiece::CounterPage => {}
            crate::paged::ContentPiece::CounterPages => {}
            crate::paged::ContentPiece::TargetCounter { .. } => {}
            crate::paged::ContentPiece::TargetText { .. } => {}
            crate::paged::ContentPiece::Leader(_) => {}
        }
    }
    out
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

#[cfg(test)]
mod core150_tests {
    use super::*;
    use crate::css::Stylesheet;
    use crate::geom::PageGeometry;

    /// Walk a fragment tree accumulating parent-relative offsets into
    /// page-absolute coordinates. Mirrors the PDF emitter's walk (pdf.rs):
    /// a Text run's baseline is PARENT-relative — rebased off the parent's
    /// absolute origin, not the fragment's own offset.
    fn collect_text_baselines(frag: &Fragment, px: f64, py: f64, out: &mut Vec<(f64, f64)>) {
        let ax = px + frag.offset.x.get();
        let ay = py + frag.offset.y.get();
        if let FragmentContent::Text(run) = &frag.content {
            out.push((px + run.baseline.x.get(), py + run.baseline.y.get()));
        }
        for child in &frag.children {
            collect_text_baselines(child, ax, ay, out);
        }
    }

    /// CORE-150: a multicol container on a page with `float: footnote`
    /// notes must lay its columns against the footnote band's raised
    /// floor — the lowest column-body baseline may not sit below the
    /// band top. (The old multicol path laid a mid-page container's
    /// balanced set at its full target height, past `bottom_limit`,
    /// painting body text straight through the footnote band.)
    #[test]
    fn multicol_columns_stop_above_footnote_band() {
        let css = r#"
            @page { size: 6in 4in; margin: 0.5in; }
            .cols { column-count: 2; }
            .fn { float: footnote; }
        "#;
        let mut body = String::new();
        for i in 1..=16 {
            body.push_str(&format!(
                "<p>Paragraph {} text.<a class=\"fn\">Note {} text.</a></p>\n",
                i, i
            ));
        }
        let html = format!(
            r#"<html><head><style>{}</style></head><body>
            <h1>Heading</h1>
            <div class="cols">
            {}
            </div>
        </body></html>"#,
            css, body
        );
        let dom = Dom::parse(&html).expect("parse");
        let stylesheet = Stylesheet::parse(css);
        // @page size/margins live in the page rules, not the CLI geometry;
        // the geometry below is only the fallback and the @page rule wins.
        let geometry = PageGeometry {
            width: Scalar(432.0),
            height: Scalar(288.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let laid = layout(&dom, &stylesheet, geometry);
        assert!(!laid.pages.is_empty(), "no pages laid out");
        let page = &laid.pages[0];

        // Gather every painted text baseline (page-absolute, top-left).
        let mut baselines = Vec::new();
        collect_text_baselines(&page.root, 0.0, 0.0, &mut baselines);
        assert!(
            baselines.len() >= 5,
            "expected column + footnote text, got {} baselines",
            baselines.len()
        );

        // Footnote notes render 8.5pt text (CORE-107 hardcoded size) with
        // the note hanging MARKER_HANG=8.5pt left of the content edge
        // (content left = margin 36pt). Notes are the baselines whose x
        // sits left of the content edge.
        let content_left = 36.0;
        let note_ys: Vec<f64> = baselines
            .iter()
            .filter(|(x, _)| *x < content_left)
            .map(|(_, y)| *y)
            .collect();
        assert!(
            !note_ys.is_empty(),
            "no footnote note baselines found on page 0 (notes hang left of the content edge)"
        );

        // Body-column baselines are everything at/inside the content edge.
        let body_ys: Vec<f64> = baselines
            .iter()
            .filter(|(x, _)| *x >= content_left)
            .map(|(_, y)| *y)
            .collect();
        let max_body_y = body_ys
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        // The invariant: columns must stay above the band top. The band
        // floor for this assertion is the first (highest) note baseline
        // minus one note line pitch (attach_footnotes paints notes at
        // 13.45pt pitch); any column baseline below it means the columns
        // ignored the raised floor.
        let first_note_y = note_ys
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let band_floor = first_note_y - 13.45;
        assert!(
            max_body_y <= band_floor,
            "column body baseline at y={:.1} paints below the footnote band \
             floor at y={:.1} (columns ignored the band floor)",
            max_body_y,
            band_floor
        );
    }
}
