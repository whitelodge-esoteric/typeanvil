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

use crate::css::{cascade, ComputedStyle, Display, StringSetValue, Stylesheet};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::frag::{
    BreakInside, BreakToken, ChildToken, Fragment, FragmentContent, Fragmentainer, FragmentKind,
    TextRun,
};
use crate::geom::{PageGeometry, Point, Scalar};
use crate::paged::{
    parse_page_rules, resolve_page_spec, ContentPiece, MarginAlign, MarginBoxName, MarginRow,
    PageRule, PageSpec, RunningStrings,
};

/// Line-height multiple applied to font-size.
const LINE_HEIGHT_FACTOR: f64 = 1.2;
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

/// The full paginated layout. `pages` *are* fragmentainers.
#[derive(Clone, Debug)]
pub struct Layout {
    /// The CLI/default geometry (the fallback when no `@page` rule applies).
    /// Per-page size lives on each fragmentainer's root `size`.
    pub geometry: PageGeometry,
    pub pages: Vec<Fragmentainer>,
    /// Heading outline for PDF bookmarks, in DOM order.
    pub headings: Vec<Heading>,
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
    /// A run of inline text (this block's own text between block children).
    Text(String),
    /// A block-level child element.
    Block(NodeId),
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
}

impl Flow {
    fn page_number(&self) -> i32 {
        self.page_base + self.current_index as i32
    }
}

/// Immutable layout inputs, threaded by shared reference.
struct Ctx<'a> {
    dom: &'a Dom,
    styles: &'a [ComputedStyle],
    /// Content-box left edge and width for the page currently being laid out.
    content_x: Scalar,
    content_width: Scalar,
    /// Content-box height of the page currently being laid out.
    page_height: Scalar,
    /// Resolved target-counter page numbers by element `NodeId` (from the
    /// previous layout pass). Empty on the first pass.
    target_pages: &'a BTreeMap<NodeId, usize>,
}

/// Run the pipeline stage: cascade, then paginate into fragmentainers.
///
/// Public signature is unchanged (spec §14). Internally: parse `@page` rules,
/// resolve each page's spec independently, thread running-string / page-counter
/// state, attach margin boxes, and — when `target-counter` appears — run the
/// bounded two-pass TOC resolution.
pub fn layout(dom: &Dom, stylesheet: &Stylesheet, geometry: PageGeometry) -> Layout {
    let styles = cascade(dom, stylesheet);
    let page_rules = parse_page_rules(stylesheet.source());
    let root = dom.find_tag("body").unwrap_or(dom.root);

    // Does any generated content reference target-counter? If so, run the
    // bounded multi-pass resolution; otherwise a single pass suffices.
    let needs_toc = styles.iter().any(|s| {
        s.content
            .iter()
            .any(|p| matches!(p, ContentPiece::TargetCounter { .. }))
    });
    let passes = if needs_toc { MAX_TOC_PASSES } else { 1 };

    let mut target_pages: BTreeMap<NodeId, usize> = BTreeMap::new();
    let mut pages = Vec::new();
    for _ in 0..passes {
        let (p, map) = paginate(dom, &styles, &page_rules, root, &geometry, &target_pages);
        // Converged when the target map is unchanged between passes: the
        // resolved page numbers are stable, so the next pass would be identical.
        let converged = map == target_pages;
        pages = p;
        target_pages = map;
        if converged {
            break;
        }
    }
    // Build the heading outline from the final pass's element→page map.
    let headings = build_headings(dom, &target_pages);
    Layout {
        geometry,
        pages,
        headings,
    }
}

/// One full pagination pass. Returns the pages and the element→page-index map
/// (first fragmentainer each sourced element appears on, in `NodeId` order).
fn paginate(
    dom: &Dom,
    styles: &[ComputedStyle],
    page_rules: &[PageRule],
    root: NodeId,
    cli: &PageGeometry,
    target_pages: &BTreeMap<NodeId, usize>,
) -> (Vec<Fragmentainer>, BTreeMap<NodeId, usize>) {
    let mut pages: Vec<Fragmentainer> = Vec::new();
    let mut incoming = Some(BreakToken::break_before());
    let mut flow = Flow {
        running: RunningStrings::new(),
        page_base: 1,
        current_index: 0,
    };
    // The named page in effect, carried across pages until an element switches
    // it (spec §4).
    let mut current_name: Option<String> = None;

    while let Some(token) = incoming.take() {
        let page_index = pages.len();
        flow.current_index = page_index;

        // Resolve the named page in effect for this page: a `page:<name>` box
        // that starts fresh at the top of this page switches the context.
        if let Some(name) = active_page_name(dom, styles, root, &token) {
            current_name = Some(name);
        }
        let spec = resolve_page_spec(page_rules, current_name.as_deref(), page_index, cli);
        let geo = spec.geometry();
        let content = geo.content_rect();

        let ctx = Ctx {
            dom,
            styles,
            content_x: content.x,
            content_width: content.width,
            page_height: content.height,
            target_pages,
        };

        let mut fragmentainer = Fragmentainer::new(page_index, spec.size);
        let res = ctx.layout_root(root, content.y, &token, &mut flow);
        if !res.empty {
            fragmentainer.root.children.push(res.fragment);
        }

        // Margin boxes resolve against the running-string / counter state now
        // in effect at the end of this page's flow (spec §6, §7).
        attach_margin_boxes(&mut fragmentainer, &spec, &geo, &flow, styles, dom, target_pages);

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
        attach_margin_boxes(&mut fragmentainer, &spec, &geo, &flow, styles, dom, target_pages);
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
        self.layout_block(
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

    /// Lay out one block into the current fragmentainer.
    ///
    /// - `origin_x`: left edge for this box's border-box (points).
    /// - `avail_width`: width available for this box's border-box.
    /// - `top`: y where this box starts in the fragmentainer (points).
    /// - `bottom_limit`: y beyond which content does not fit (the fragmentainer
    ///   content bottom).
    /// - `token`: incoming continuation (break-before = start fresh).
    fn layout_block(
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

        let fresh = token.is_break_before();

        // Margins/padding adjoining a fragmentainer break truncate to zero
        // (css-break-3). On resume (not fresh) the top margin/padding is gone.
        let margin_top = if fresh { style.margin_top } else { Scalar::ZERO };
        let padding_top = if fresh { style.padding_top } else { Scalar::ZERO };

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

        let line_height = |s: &ComputedStyle| s.font_size * LINE_HEIGHT_FACTOR;

        // Generated content (`content` property, e.g. a TOC entry) is emitted
        // as one line at the start of the box, when the box starts fresh. Its
        // width is the inner width; a `leader('.')` fills to the right content
        // edge, so pagination does not depend on the resolved number glyphs
        // (spec §9, §10).
        if fresh && !style.content.is_empty() {
            let lh = line_height(style);
            if y + lh <= bottom_limit || !placed {
                let text = self.resolve_content(id, &style.content, inner_width, style, flow);
                let baseline = y + style.font_size;
                let run = TextRun {
                    text,
                    baseline: Point::new(inner_left, baseline),
                    font_size: style.font_size,
                    color: style.color,
                    font_family: style.font_family.clone(),
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

        let mut i = start_index;
        while i < items.len() {
            // Forced break-before on a block child starts a new fragmentainer.
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
                Item::Text(text) => {
                    let child_tok = self.child_incoming(token, i);
                    let lh = line_height(style);
                    let lines = self.break_lines(text, inner_width, style);
                    // How many lines already consumed by earlier fragments.
                    let consumed_lines =
                        (child_tok.consumed_block_size.get() / lh.get()).round() as usize;
                    let mut li = consumed_lines;
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
                        let baseline = y + style.font_size;
                        let run = TextRun {
                            text: lines[li].clone(),
                            baseline: Point::new(inner_left, baseline),
                            font_size: style.font_size,
                            color: style.color,
                            font_family: style.font_family.clone(),
                        };
                        children.push(Fragment::line(
                            Point::new(inner_left, y),
                            (inner_width, lh),
                            run,
                        ));
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
                        let consumed = lh * (split as f64);
                        seen_all = false;
                        outgoing_children.push(ChildToken {
                            index: i,
                            token: BreakToken {
                                consumed_block_size: consumed,
                                seen_all_children: false,
                                child_tokens: Vec::new(),
                                break_before: false,
                            },
                        });
                        broke = true;
                        break;
                    }
                }
                Item::Block(child) => {
                    let child_tok = self.child_incoming(token, i);
                    let res = self.layout_block(
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

                    if !res.empty {
                        children.push(res.fragment);
                        y += res.used;
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

        // Padding-bottom / margin-bottom only apply when the box finished.
        let padding_bottom = if broke { Scalar::ZERO } else { style.padding_bottom };
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
                run.baseline =
                    Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
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
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };

        // margin-bottom advances the *parent* cursor, not the box height.
        let margin_bottom = if broke { Scalar::ZERO } else { style.margin_bottom };
        let used = (box_top - top) + box_height + margin_bottom;

        let empty = children_empty(&fragment) && outgoing.is_none() && box_height.get() <= 0.0;

        BlockResult {
            fragment,
            used,
            outgoing,
            empty,
        }
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
        let mut h = style.margin_top + style.padding_top + style.padding_bottom + style.margin_bottom;
        for item in self.collect_items(id) {
            match item {
                Item::Text(text) => {
                    let lines = self.break_lines(&text, inner_width, style);
                    h = h + (style.font_size * LINE_HEIGHT_FACTOR) * (lines.len() as f64);
                }
                Item::Block(child) => {
                    h = h + self.measure_block(child, inner_width);
                }
            }
        }
        h
    }

    /// Collect a block's children as an ordered item list: contiguous inline
    /// text becomes one `Text` item; each block child becomes a `Block` item.
    fn collect_items(&self, id: NodeId) -> Vec<Item> {
        let mut items: Vec<Item> = Vec::new();
        let mut pending = String::new();
        self.collect_items_rec(id, &mut items, &mut pending);
        if !pending.trim().is_empty() {
            items.push(Item::Text(std::mem::take(&mut pending)));
        }
        items
    }

    fn collect_items_rec(&self, id: NodeId, items: &mut Vec<Item>, pending: &mut String) {
        for &child in &self.dom.nodes[id].children {
            match &self.dom.nodes[child].kind {
                NodeKind::Text(t) => pending.push_str(t),
                NodeKind::Element(_) => {
                    if self.styles[child].display == Display::Block {
                        if !pending.trim().is_empty() {
                            items.push(Item::Text(std::mem::take(pending)));
                        } else {
                            pending.clear();
                        }
                        items.push(Item::Block(child));
                    } else {
                        // Inline element: fold its text into the current run.
                        self.collect_items_rec(child, items, pending);
                    }
                }
                NodeKind::Root => {}
            }
        }
    }

    /// Greedy first-fit line breaking. Returns the lines (text only).
    fn break_lines(&self, text: &str, max_width: Scalar, style: &ComputedStyle) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let advance = style.font_size.get() * AVG_ADVANCE_EM;
        let space_w = advance;
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        let mut line_w = 0.0f64;
        for word in words {
            let word_w = word.chars().count() as f64 * advance;
            let added = if line.is_empty() {
                word_w
            } else {
                line_w + space_w + word_w
            };
            if !line.is_empty() && added > max_width.get() {
                lines.push(std::mem::take(&mut line));
                line_w = 0.0;
            }
            if line.is_empty() {
                line.push_str(word);
                line_w = word_w;
            } else {
                line.push(' ');
                line.push_str(word);
                line_w += space_w + word_w;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// Resolve a generated-content piece list to a single line of text.
    ///
    /// `string(name)` and `counter(page)` read the current [`Flow`] state;
    /// `target-counter(attr(N), page)` reads the previous pass's page map
    /// (empty → `?`); `leader(ch)` fills the remaining line width to the right
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
        let advance = style.font_size.get() * AVG_ADVANCE_EM;
        // Resolve every non-leader piece to text; note the leader position.
        let mut before = String::new();
        let mut after = String::new();
        let mut leader_char: Option<char> = None;
        for piece in pieces {
            match piece {
                ContentPiece::Literal(s) => push_side(&mut before, &mut after, leader_char, s),
                ContentPiece::StringRef(name) => {
                    let v = flow.running.get(name).to_string();
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::CounterPage => {
                    let v = flow.page_number().to_string();
                    push_side(&mut before, &mut after, leader_char, &v);
                }
                ContentPiece::CounterRef(_) => {
                    // Named counters are parsed but not tracked yet; render 0.
                    push_side(&mut before, &mut after, leader_char, "0");
                }
                ContentPiece::TargetCounter { attr } => {
                    let v = self.resolve_target(id, attr);
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
                let used = (before.chars().count() + after.chars().count()) as f64 * advance;
                let room = inner_width.get() - used;
                let count = if room > 0.0 {
                    (room / advance).floor() as usize
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

/// The named page in effect entering the page laid out with `token`.
///
/// Descends the incoming break-token tree following the fresh-start (break-
/// before) path from `root`; the deepest `page: <name>` box on that path that
/// starts fresh at the top of this page wins. Returns `None` when no such box
/// switches the context (the caller then carries the previous page's name).
fn active_page_name(
    dom: &Dom,
    styles: &[ComputedStyle],
    root: NodeId,
    token: &BreakToken,
) -> Option<String> {
    let mut found: Option<String> = None;
    let mut id = root;
    let mut tok = token;
    loop {
        // A box switches the page context only when it starts fresh here.
        if tok.is_break_before() {
            if let NodeKind::Element(_) = &dom.nodes[id].kind {
                if let Some(name) = &styles[id].page {
                    found = Some(name.clone());
                }
            }
        }
        // Descend to the first (lowest-index) unfinished child, mapping the
        // child-item index back to a DOM node.
        let Some(child_tok) = tok.child_tokens.first() else {
            break;
        };
        let Some(child_id) = nth_block_child(dom, styles, id, child_tok.index) else {
            break;
        };
        id = child_id;
        tok = &child_tok.token;
    }
    found
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
                if styles[child].display == Display::Block {
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
) {
    if spec.margin_boxes.is_empty() {
        return;
    }
    let content = geo.content_rect();
    // A fixed margin-box font size (points). Margin boxes are one line.
    let font_size = Scalar(10.0);
    let lh = font_size * LINE_HEIGHT_FACTOR;

    for (name, pieces) in &spec.margin_boxes {
        let text = render_margin_content(pieces, flow);
        if text.is_empty() {
            continue;
        }
        let (slot_x, slot_w, slot_y) = margin_box_slot(*name, geo, &content, lh);
        let text_w = text.chars().count() as f64 * font_size.get() * AVG_ADVANCE_EM;
        let x = match name.align() {
            MarginAlign::Start => slot_x,
            MarginAlign::Center => slot_x + Scalar((slot_w.get() - text_w).max(0.0) * 0.5),
            MarginAlign::End => slot_x + Scalar((slot_w.get() - text_w).max(0.0)),
        };
        let baseline = slot_y + font_size;
        let run = TextRun {
            text,
            baseline: Point::new(x, baseline),
            font_size,
            color: crate::css::Color::BLACK,
            font_family: "sans-serif".to_string(),
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
fn horizontal_slot(name: MarginBoxName, content: &crate::geom::Rect, third: Scalar) -> (Scalar, Scalar) {
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
/// demos; leader/target pieces are rendered inertly.
fn render_margin_content(pieces: &[ContentPiece], flow: &Flow) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            ContentPiece::Literal(s) => out.push_str(s),
            ContentPiece::StringRef(name) => out.push_str(flow.running.get(name)),
            ContentPiece::CounterPage => out.push_str(&flow.page_number().to_string()),
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
            let title = dom.text_content(id).split_whitespace().collect::<Vec<_>>().join(" ");
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
