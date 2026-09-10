// SPDX-License-Identifier: AGPL-3.0-only

//! Flex layout (css-flexbox-1) — CORE-65.
//!
//! A box with `display: flex` / `inline-flex` lays its in-flow children as
//! flex items along a main axis (`flex-direction`), resolving their main
//! sizes with the grow/shrink/basis algorithm, then fragments like a block:
//!
//! - **Row containers** lay out *lines* down the page. `flex-wrap: nowrap`
//!   (default) produces one line; `wrap` packs items greedily left-to-right,
//!   starting a new line when the next item does not fit the remaining main
//!   size (the only wrap form modeled — content-based wrapping is a spec
//!   non-goal). Each line is monolithic across the cross axis: when a line
//!   does not fit the fragmentainer it moves to the next page, and a forced
//!   `break-before`/`break-after` on any item propagates to its line
//!   (css-break-3 §6.2). A single item taller than the page breaks *within*
//!   the line: its continuation resumes at the top of the next fragmentainer
//!   and the line's remaining items follow it (the [`FlexToken`] state).
//! - **Column containers** stack items along the block axis like block
//!   children (each item fragments independently); `column-reverse` reverses
//!   the item order.
//!
//! Items are laid out through [`Ctx::layout_box`], so a flex item that is
//! itself a flex container (nested flex), a table, or a block with floats
//! works unchanged. `order` reordering, `flex-grow` on a definite-height
//! column container, `wrap-reverse` line ordering, and anonymous-item
//! wrapping of text children are non-goals (see the spec).
//!
//! Two-pass rule (research brief): the line's cross size is measured before
//! the line is placed, so `align-items/align-self: stretch` (the default)
//! has a resolved target without relayout across pages.

use super::*;
use crate::css::{AlignSelf, BreakInside, Display, FlexBasis, FlexDirection, FlexWrap, Position};
use crate::dom::NodeId;
use crate::frag::{BreakToken, ChildToken, FlexToken, Fragment, FragmentContent};
use crate::geom::{Point, Scalar};

/// One flex item, in document order.
struct FlexItem {
    /// The DOM node.
    id: NodeId,
    /// Position among the container's *block* children (`collect_items`
    /// indices) — the index used for `child_tokens` resume.
    block_index: usize,
    /// Hypothetical main size (points), resolved from basis/width/content.
    main_size: Scalar,
    /// Natural cross size (points): measured content height (row) / content
    /// width (column), before any stretch.
    cross_size: Scalar,
}

/// One flex line (row containers): a horizontal run of items sharing a
/// cross size. Column containers are treated as a single implicit line.
struct FlexLine {
    /// Indices into the container's `FlexItem` list, in main-axis order.
    items: Vec<usize>,
    /// Main-axis offset of each item from the line's start (points).
    offsets: Vec<Scalar>,
    /// The line's cross size (points): the largest item cross size.
    cross: Scalar,
}

impl FlexLine {
    fn new() -> FlexLine {
        FlexLine {
            items: Vec::new(),
            offsets: Vec::new(),
            cross: Scalar::ZERO,
        }
    }
}

impl<'a> Ctx<'a> {
    /// Lay out a flex container and return its block result.
    ///
    /// Mirrors the `layout_box` contract: same box-accounting (margins
    /// truncate at fragmentainer breaks, padding-top only when fresh), same
    /// break-token resume. The container's own fragment is an ordinary
    /// `Block`; its children are the item fragments (lines are implicit).
    pub(super) fn layout_flex_container(
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
        let fresh = token.is_break_before();

        let margin_top = if fresh { style.margin_top } else { Scalar::ZERO };
        let box_top = top + margin_top;
        let inner_left = origin_x + style.margin_left + style.padding_left;
        let inner_width = avail_width
            - style.margin_left
            - style.margin_right
            - style.padding_left
            - style.padding_right;
        let content_top = box_top + style.padding_top;

        // A positioned flex container is the containing block for its abspos
        // descendants; save/restore so siblings resolve against the OUTER
        // block (the block path's pattern).
        let saved_abspos_cb = flow.abspos_cb;
        if matches!(
            style.position,
            Position::Relative | Position::Absolute | Position::Fixed
        ) {
            // A relative container's abspos descendants resolve against the
            // SHIFTED origin (css-position-3 §6.2); absolute/fixed resolve
            // via their own insets, untouched by this shift.
            flow.abspos_cb = Some((
                RelativeInsetShift::resolve(style).point(Point::new(inner_left, box_top)),
                inner_width,
            ));
        }

        let items = self.collect_flex_items(id);

        let result = match style.flex_direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.layout_flex_row(
                id,
                style,
                items,
                inner_left,
                inner_width,
                avail_width,
                content_top,
                bottom_limit,
                page_has_content,
                token,
                flow,
                box_top,
                top,
            ),
            FlexDirection::Column | FlexDirection::ColumnReverse => self.layout_flex_column(
                id,
                style,
                items,
                inner_left,
                inner_width,
                avail_width,
                content_top,
                bottom_limit,
                page_has_content,
                token,
                flow,
                box_top,
                top,
            ),
        };
        // css-position-3 §6.2: the container's own paint shift lands on its
        // fragment; the parent cursor advanced via `used`, so siblings never
        // reflow.
        let mut result = result;
        RelativeInsetShift::resolve(style).apply(&mut result.fragment);
        flow.abspos_cb = saved_abspos_cb;
        result
    }

    /// Collect a flex container's in-flow element children as flex items,
    /// keeping each item's position among the container's *block* children
    /// (so `child_tokens` resume positionally like the block path).
    pub(super) fn collect_flex_items(&self, id: NodeId) -> Vec<FlexItem> {
        let mut out = Vec::new();
        for (i, item) in self.collect_items(id).into_iter().enumerate() {
            if let Item::Atomic(child) | Item::Block(child) = item {
                let cs = &self.styles[child];
                if cs.display == Display::None {
                    continue;
                }
                if matches!(cs.position, Position::Absolute | Position::Fixed) {
                    continue;
                }
                out.push(FlexItem {
                    id: child,
                    block_index: i,
                    main_size: Scalar::ZERO,
                    cross_size: Scalar::ZERO,
                });
            }
        }
        out
    }

    /// Resolve an item's hypothetical main size for a ROW container
    /// (css-flexbox-1 §9.2): `flex-basis` if definite, else `width`, else
    /// the content-based (shrink-to-fit) size. Percentages resolve against
    /// the container's inner main size.
    fn row_item_main_size(&self, item_id: NodeId, inner_width: Scalar) -> Scalar {
        let s = &self.styles[item_id];
        match s.flex_basis {
            FlexBasis::Size { length, percent } => {
                if let Some(l) = length {
                    l
                } else if let Some(p) = percent {
                    inner_width * p
                } else {
                    // A zero basis (`flex: 1` → `flex-basis: 0%`).
                    Scalar::ZERO
                }
            }
            FlexBasis::Content => self.shrink_to_fit(item_id, inner_width),
            FlexBasis::Auto => {
                if let Some(w) = s.width {
                    w
                } else if let Some(p) = s.width_percent {
                    inner_width * p
                } else {
                    self.shrink_to_fit(item_id, inner_width)
                }
            }
        }
    }

    /// Row container layout: resolve main sizes, pack lines, place them
    /// down the page with fragmentation.
    #[allow(clippy::too_many_arguments)]
    fn layout_flex_row(
        &self,
        id: NodeId,
        style: &ComputedStyle,
        mut items: Vec<FlexItem>,
        inner_left: Scalar,
        inner_width: Scalar,
        avail_width: Scalar,
        content_top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        token: &BreakToken,
        flow: &mut Flow,
        box_top: Scalar,
        top: Scalar,
    ) -> BlockResult {
        let wrap = matches!(style.flex_wrap, FlexWrap::Wrap | FlexWrap::WrapReverse);
        let reverse = matches!(style.flex_direction, FlexDirection::RowReverse);
        let gap = style.flex_column_gap;
        let row_gap = style.row_gap;

        // --- Phase A (pure): resolve main sizes + build lines. ------------
        for item in &mut items {
            item.main_size = self.row_item_main_size(item.id, inner_width);
            // Natural cross size: the item's content height at its main size.
            // Deliberately mirrors the BLOCK path, which ignores `height`
            // (CORE-66 auto-height self-consistency) — WPT references render
            // through the same engine, so flex must match, or every
            // height-authored ref diverges (the CORE-66 trap).
            item.cross_size = {
                let natural = self.measure_block(item.id, item.main_size);
                // CORE-122: a resumed item (child token break_before:false)
                // already consumed part of its block size on an earlier
                // fragmentainer. Only the remainder occupies THIS
                // fragmentainer; counting the full measure inflates the
                // line's cross size and shifts following siblings (WPT 046).
                let child_tok = self.child_incoming(token, item.block_index);
                if child_tok.is_break_before() {
                    natural
                } else {
                    Scalar((natural.get() - child_tok.consumed_block_size.get()).max(0.0))
                }
            };
        }

        // Grow/shrink resolution (nowrap only; wrap packs raw bases): free
        // space is distributed ∝ flex-grow; overflow is absorbed ∝
        // flex-shrink × base, floored at zero (min-content floors deferred).
        let total_main: f64 = items.iter().map(|i| i.main_size.get()).sum();
        let gaps_total = gap.get() * (items.len().saturating_sub(1) as f64);
        let free = inner_width.get() - total_main - gaps_total;
        if !wrap && free > 0.0 {
            let grow_sum: f64 = items.iter().map(|i| self.styles[i.id].flex_grow).sum();
            if grow_sum > 0.0 {
                for item in &mut items {
                    let g = self.styles[item.id].flex_grow;
                    if g > 0.0 {
                        item.main_size = item.main_size + Scalar(free * g / grow_sum);
                    }
                }
            }
        } else if !wrap && free < 0.0 {
            let shrink_sum: f64 = items
                .iter()
                .map(|i| self.styles[i.id].flex_shrink * i.main_size.get())
                .sum();
            if shrink_sum > 0.0 {
                for item in &mut items {
                    let s = self.styles[item.id].flex_shrink;
                    if s > 0.0 {
                        let cut = Scalar(-free * (s * item.main_size.get()) / shrink_sum);
                        let floor = item.main_size.get().min(0.0);
                        item.main_size = Scalar((item.main_size - cut).get().max(floor));
                    }
                }
            }
        }

        // Pack items into lines. Greedy wrap: an item that does not fit the
        // remaining main size starts a new line; a lone item always fits
        // (overflow is placed, never dropped).
        let mut lines: Vec<FlexLine> = Vec::new();
        if !items.is_empty() {
            let mut cur = FlexLine::new();
            let mut cursor = 0.0f64;
            for (i, item) in items.iter().enumerate() {
                let size = item.main_size.get();
                if wrap && !cur.items.is_empty() && cursor + gap.get() + size > inner_width.get() {
                    lines.push(cur);
                    cur = FlexLine::new();
                    cursor = 0.0;
                }
                cur.items.push(i);
                cur.offsets.push(Scalar(cursor));
                if item.cross_size.get() > cur.cross.get() {
                    cur.cross = item.cross_size;
                }
                cursor += size + gap.get();
            }
            lines.push(cur);
        }

        // Justify-content distributes slack inside each line.
        let justify = style.justify_content;
        for line in &mut lines {
            let used: f64 = line
                .items
                .iter()
                .enumerate()
                .map(|(k, &i)| {
                    items[i].main_size.get() + if k + 1 < line.items.len() { gap.get() } else { 0.0 }
                })
                .sum();
            let slack = (inner_width.get() - used).max(0.0);
            match justify {
                crate::css::JustifyContent::FlexEnd => {
                    for off in &mut line.offsets {
                        *off = *off + Scalar(slack);
                    }
                }
                crate::css::JustifyContent::Center => {
                    for off in &mut line.offsets {
                        *off = *off + Scalar(slack / 2.0);
                    }
                }
                crate::css::JustifyContent::SpaceBetween if line.items.len() > 1 => {
                    // Gap between adjacent items grows; ends stay flush.
                    let extra = slack / (line.items.len() as f64 - 1.0);
                    for k in 1..line.items.len() {
                        line.offsets[k] = line.offsets[k] + Scalar(extra * k as f64);
                    }
                }
                _ => {}
            }
        }

        // --- Phase B (fragmentation): place lines down the page. ----------
        let mut y = content_top;
        let mut children: Vec<Fragment> = Vec::new();
        let mut outgoing_children: Vec<ChildToken> = Vec::new();
        let mut broke = false;
        let mut seen_all = true;
        let mut placed = page_has_content;

        // Resume state from the container's flex token.
        let (start_line, start_item, mid_line) = match &token.flex {
            Some(f) => (f.line, f.next_item, f.mid_line),
            None => (0, 0, false),
        };
        // The flex continuation for the NEXT fragmentainer, set at each
        // break site. Defaults to a fresh start; overwritten when we break.
        let mut flex_state = FlexToken {
            next_item: 0,
            line: 0,
            mid_line: false,
        };

        let mut li = start_line;
        while li < lines.len() {
            let line_items = lines[li].items.clone();
            // Item cursor within this line: 0 for a fresh line; for the
            // mid-line resume the token names the item to continue.
            let mut k = if li == start_line && mid_line {
                line_items
                    .iter()
                    .position(|&i| i == start_item)
                    .unwrap_or(0)
            } else {
                0
            };
            let line_started = li == start_line && mid_line;

            // CORE-122: a child token with break_before:false marks a child
            // that RESUMES mid-box (it placed content on an earlier
            // fragmentainer). That child's line is already in progress —
            // the line must not defer or re-place from scratch.
            let line_started = line_started
                || lines[li].items.iter().any(|&i| {
                    let bi = items[i].block_index;
                    token
                        .child_tokens
                        .iter()
                        .any(|c| c.index == bi && !c.token.is_break_before())
                });

            if !line_started {
                // Forced break-before on any item propagates to the line:
                // defer the whole line (ignored at the very top of a page).
                let forced = line_items
                    .iter()
                    .any(|&i| self.styles[items[i].id].break_before.is_forced());
                if forced && placed {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: items[line_items[0]].block_index,
                        token: BreakToken::break_before(),
                    });
                    flex_state = FlexToken {
                        next_item: line_items[0],
                        line: li,
                        mid_line: false,
                    };
                    broke = true;
                    break;
                }
                // The line is monolithic across the cross axis: if it does
                // not fit the remaining fragmentainer, move it whole to the
                // next page (last resort only on an empty page).
                if y.get() + lines[li].cross.get() > bottom_limit.get() && placed {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: items[line_items[0]].block_index,
                        token: BreakToken::break_before(),
                    });
                    flex_state = FlexToken {
                        next_item: line_items[0],
                        line: li,
                        mid_line: false,
                    };
                    broke = true;
                    break;
                }
            }

            let line_top = y;
            let line_cross = lines[li].cross;
            while k < line_items.len() {
                let item_idx = line_items[k];
                let item = &items[item_idx];
                let child_tok = self.child_incoming(token, item.block_index);

                // Cross-axis alignment within the line (align-self, default
                // auto → container align-items).
                let align = match self.styles[item.id].align_self {
                    AlignSelf::Auto => match style.align_items {
                        crate::css::AlignItems::Stretch => AlignSelf::Stretch,
                        crate::css::AlignItems::FlexStart => AlignSelf::FlexStart,
                        crate::css::AlignItems::FlexEnd => AlignSelf::FlexEnd,
                        crate::css::AlignItems::Center => AlignSelf::Center,
                    },
                    other => other,
                };
                let cross_offset = match align {
                    AlignSelf::FlexEnd => {
                        let d = line_cross - item.cross_size;
                        if d.get() > 0.0 { d } else { Scalar::ZERO }
                    }
                    AlignSelf::Center => (line_cross - item.cross_size) * 0.5,
                    _ => Scalar::ZERO,
                };
                let item_top = line_top + cross_offset;

                // Main-axis x: line offsets are measured from the line start;
                // row-reverse mirrors the line about the container's right edge.
                let item_x = if reverse {
                    inner_left + inner_width - lines[li].offsets[k] - item.main_size
                } else {
                    inner_left + lines[li].offsets[k]
                };

                let res = self.layout_box(
                    item.id,
                    item_x,
                    item.main_size,
                    item_top,
                    bottom_limit,
                    placed,
                    &child_tok,
                    flow,
                );
                if !res.empty {
                    children.push(res.fragment);
                    placed = true;
                }
                if let Some(tok) = res.outgoing {
                    // The item breaks inside the line: the line fragments.
                    // The item's continuation resumes at the top of the next
                    // fragmentainer; the line's remaining items follow it.
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: item.block_index,
                        token: tok,
                    });
                    flex_state = FlexToken {
                        next_item: item_idx,
                        line: li,
                        mid_line: true,
                    };
                    broke = true;
                    break;
                }
                k += 1;
            }
            if broke {
                break;
            }

            // The line completed on this page.
            y = y + line_cross;
            placed = true;

            // Forced break-after on any item propagates to the line: the
            // next line moves to the next page.
            let next_line = li + 1;
            if next_line < lines.len() {
                let forced_after = line_items
                    .iter()
                    .any(|&i| self.styles[items[i].id].break_after.is_forced());
                if forced_after {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: items[lines[next_line].items[0]].block_index,
                        token: BreakToken::break_before(),
                    });
                    flex_state = FlexToken {
                        next_item: lines[next_line].items[0],
                        line: next_line,
                        mid_line: false,
                    };
                    broke = true;
                    break;
                }
                y = y + row_gap;
            }
            li = next_line;
        }

        // --- Finish like the block path. ---------------------------------
        let padding_bottom = if broke { Scalar::ZERO } else { style.padding_bottom };
        y = y + padding_bottom;
        let box_height = y - box_top;
        let origin = Point::new(inner_left - style.padding_left - style.margin_left, box_top);
        for child in &mut children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let mut fragment = Fragment::block(origin, (avail_width, box_height));
        if let Some(bg) = style.background_color {
            if box_height.get() > 0.0 {
                fragment.content = FragmentContent::Background(bg);
            }
        }
        fragment.children = children;
        fragment.source = Some(id);

        let outgoing = if broke {
            let consumed = token.consumed_block_size + box_height;
            let tok = BreakToken {
                consumed_block_size: consumed,
                seen_all_children: seen_all,
                child_tokens: outgoing_children,
                break_before: false,
                consumed_chars: None,
                flex: Some(flex_state),
                deferred_once: false,
            };
            fragment.break_token = Some(tok.clone());
            Some(tok)
        } else {
            None
        };

        let margin_bottom = if broke { Scalar::ZERO } else { style.margin_bottom };
        let used = (box_top - top) + box_height + margin_bottom;
        let empty = fragment.children.is_empty() && outgoing.is_none() && box_height.get() <= 0.0;
        BlockResult {
            fragment,
            used,
            outgoing,
            empty,
        }
    }

    /// Column container layout: items stack along the block axis (reversed
    /// for `column-reverse`), each fragmenting like a block child.
    #[allow(clippy::too_many_arguments)]
    fn layout_flex_column(
        &self,
        id: NodeId,
        style: &ComputedStyle,
        items: Vec<FlexItem>,
        inner_left: Scalar,
        inner_width: Scalar,
        avail_width: Scalar,
        content_top: Scalar,
        bottom_limit: Scalar,
        page_has_content: bool,
        token: &BreakToken,
        flow: &mut Flow,
        box_top: Scalar,
        top: Scalar,
    ) -> BlockResult {
        let fresh = token.is_break_before();
        let reverse = matches!(style.flex_direction, FlexDirection::ColumnReverse);
        let gap = style.row_gap;

        // Placement order: document order, or reversed for column-reverse.
        let mut order: Vec<usize> = (0..items.len()).collect();
        if reverse {
            order.reverse();
        }

        // Resume position in placement order: the item named by the first
        // child token (the lowest unfinished block-child index in forward
        // order; in reversed order we locate it explicitly).
        let start_block = if fresh {
            None
        } else {
            token.child_tokens.first().map(|c| c.index)
        };
        let start_pos = match start_block {
            Some(bi) if bi >= items.len() => {
                // Past-the-end resume marker (trailing forced break-after,
                // CORE-116 encoding): every item finished on an earlier
                // fragmentainer; this one gets an empty continuation.
                return BlockResult {
                    fragment: Fragment::block(
                        Point::new(inner_left, top),
                        (avail_width, Scalar::ZERO),
                    ),
                    used: Scalar::ZERO,
                    outgoing: None,
                    empty: true,
                };
            }
            Some(bi) => items
                .iter()
                .position(|it| it.block_index == bi)
                .and_then(|item_pos| order.iter().position(|&i| i == item_pos))
                .unwrap_or(0),
            None => 0,
        };

        let mut y = content_top;
        let mut children: Vec<Fragment> = Vec::new();
        let mut outgoing_children: Vec<ChildToken> = Vec::new();
        let mut broke = false;
        let mut seen_all = true;
        let mut placed = page_has_content;

        // A container that finished every child on an earlier fragmentainer
        // terminates rather than emit a spurious trailing page.
        if !fresh && token.seen_all_children && token.child_tokens.is_empty() {
            return BlockResult {
                fragment: Fragment::block(Point::new(inner_left, top), (avail_width, Scalar::ZERO)),
                used: Scalar::ZERO,
                outgoing: None,
                empty: true,
            };
        }

        let mut oi = start_pos;
        while oi < order.len() {
            let item_idx = order[oi];
            let item = &items[item_idx];
            let block_index = item.block_index;
            let child_tok = self.child_incoming(token, block_index);
            let cstyle = &self.styles[item.id];
            let child_fresh = child_tok.is_break_before();

            // Forced break-before on the item (block path semantics). The
            // first-item exemption holds ONLY at the top of a fragmentainer
            // (`!placed`): mid-page, a forced break on the first item
            // propagates to the container (css-flexbox-1 #pagination;
            // WPT single-line-column-flex-fragmentation-069b/d).
            if child_fresh
                && cstyle.break_before.is_forced()
                && (!children.is_empty() || broke || oi > start_pos || placed)
            {
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: block_index,
                    token: BreakToken::break_before(),
                });
                broke = true;
                break;
            }

            // Item width: `stretch` (default) fills the container; an
            // explicit width/percentage is honored instead.
            let item_width = if let Some(w) = cstyle.width {
                w
            } else if let Some(p) = cstyle.width_percent {
                inner_width * p
            } else {
                inner_width
            };

            let res = self.layout_box(
                item.id,
                inner_left,
                item_width,
                y,
                bottom_limit,
                placed,
                &child_tok,
                flow,
            );

            // Propagation (css-flexbox-1 #pagination): a forced break
            // demanded by the item's own content BEFORE the item placed
            // anything moves the WHOLE item to the next fragmentainer
            // (WPT single-line-column-flex-fragmentation-069b: a
            // break-before on a nested container's first item propagates
            // to that container's start edge — the box moves intact, no
            // sliver stays behind). Guarded by `placed` so a fresh page
            // cannot defer forever.
            let item_placed_nothing = res.fragment.children.is_empty()
                && !matches!(res.fragment.content, FragmentContent::Text(_));
            if placed
                && child_fresh
                && item_placed_nothing
                && res.outgoing.is_some()
            {
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: block_index,
                    token: BreakToken::break_before(),
                });
                broke = true;
                break;
            }

            // break-inside: avoid — move the whole item to the next page
            // when it would fit fresh (bounded abort-and-defer).
            if cstyle.break_inside == BreakInside::Avoid
                && res.outgoing.is_some()
                && child_tok.is_break_before()
                && placed
            {
                if self.block_fits_fresh(item.id, item_width) {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: block_index,
                        token: BreakToken::break_before(),
                    });
                    broke = true;
                    break;
                }
            }

            if !res.empty {
                children.push(res.fragment);
                y = y + res.used;
                placed = true;
            }
            if let Some(tok) = res.outgoing {
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: block_index,
                    token: tok,
                });
                broke = true;
                break;
            }
            // Forced break-after: the NEXT item in placement order goes to
            // the next page. On the LAST item the forced break propagates
            // to the container's end edge (css-flexbox-1 #pagination;
            // WPT single-line-column-flex-fragmentation-069d): emit a
            // past-the-end child token so the container's continuation
            // fragment terminates cleanly (CORE-116 encoding).
            if cstyle.break_after.is_forced() {
                let next_block = if oi + 1 < order.len() {
                    items[order[oi + 1]].block_index
                } else {
                    items.len()
                };
                seen_all = false;
                outgoing_children.push(ChildToken {
                    index: next_block,
                    token: BreakToken::break_before(),
                });
                broke = true;
                break;
            }
            y = y + gap;
            oi += 1;
        }

        let padding_bottom = if broke { Scalar::ZERO } else { style.padding_bottom };
        y = y + padding_bottom;
        let box_height = y - box_top;
        let origin = Point::new(inner_left - style.padding_left - style.margin_left, box_top);
        for child in &mut children {
            child.offset = Point::new(child.offset.x - origin.x, child.offset.y - origin.y);
            if let FragmentContent::Text(run) = &mut child.content {
                run.baseline = Point::new(run.baseline.x - origin.x, run.baseline.y - origin.y);
            }
        }
        let mut fragment = Fragment::block(origin, (avail_width, box_height));
        if let Some(bg) = style.background_color {
            if box_height.get() > 0.0 {
                fragment.content = FragmentContent::Background(bg);
            }
        }
        fragment.children = children;
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

        let margin_bottom = if broke { Scalar::ZERO } else { style.margin_bottom };
        let used = (box_top - top) + box_height + margin_bottom;
        let empty = fragment.children.is_empty() && outgoing.is_none() && box_height.get() <= 0.0;
        BlockResult {
            fragment,
            used,
            outgoing,
            empty,
        }
    }
}
