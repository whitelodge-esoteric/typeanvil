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

use crate::css::{cascade, ComputedStyle, Display, Stylesheet};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::frag::{
    BreakInside, BreakToken, ChildToken, Fragment, FragmentContent, Fragmentainer, TextRun,
};
use crate::geom::{PageGeometry, Point, Scalar};

/// Line-height multiple applied to font-size.
const LINE_HEIGHT_FACTOR: f64 = 1.2;
/// Approximate average glyph advance as a fraction of the em (font-size).
const AVG_ADVANCE_EM: f64 = 0.5;
/// Safety cap: a runaway that emits more pages than this is a bug, not a
/// document. Sized generously above the O(n) test (1,000 pages).
const MAX_PAGES: usize = 100_000;

/// The full paginated layout. `pages` *are* fragmentainers.
#[derive(Clone, Debug)]
pub struct Layout {
    pub geometry: PageGeometry,
    pub pages: Vec<Fragmentainer>,
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

/// Immutable layout inputs, threaded by shared reference (no mutable state).
struct Ctx<'a> {
    dom: &'a Dom,
    styles: &'a [ComputedStyle],
    /// Content-box width available to top-level blocks (page minus page margins).
    content_x: Scalar,
    content_width: Scalar,
    /// Total content-box height of a fragmentainer.
    page_height: Scalar,
}

/// Run the pipeline stage: cascade, then paginate into fragmentainers.
pub fn layout(dom: &Dom, stylesheet: &Stylesheet, geometry: PageGeometry) -> Layout {
    let styles = cascade(dom, stylesheet);
    let content = geometry.content_rect();
    let ctx = Ctx {
        dom,
        styles: &styles,
        content_x: content.x,
        content_width: content.width,
        page_height: content.height,
    };

    // The fragmentation root: <body>, or the document root if absent.
    let root = dom.find_tag("body").unwrap_or(dom.root);

    let page_size = (geometry.width, geometry.height);
    let mut pages: Vec<Fragmentainer> = Vec::new();
    // Start fresh (IsBreakBefore): the root box has not started yet.
    let mut incoming = Some(BreakToken::break_before());

    while let Some(token) = incoming.take() {
        let page_index = pages.len();
        let mut fragmentainer = Fragmentainer::new(page_index, page_size);

        // Lay the root box into this page's content box.
        let res = ctx.layout_root(root, content.y, &token);

        if !res.empty {
            fragmentainer.root.children.push(res.fragment);
        }
        pages.push(fragmentainer);

        incoming = res.outgoing;

        if pages.len() >= MAX_PAGES {
            // Deterministic hard stop; `seen_all_children` should prevent this.
            break;
        }
    }

    // Edge case: an empty document still yields exactly one blank page.
    if pages.is_empty() {
        pages.push(Fragmentainer::new(0, page_size));
    }

    Layout { geometry, pages }
}

impl<'a> Ctx<'a> {
    /// Lay the root/body box directly into the page content box. The root box
    /// itself carries no page margin; its children flow from `content_top`.
    fn layout_root(&self, id: NodeId, content_top: Scalar, token: &BreakToken) -> BlockResult {
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
        let items = self.collect_items(id);

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
