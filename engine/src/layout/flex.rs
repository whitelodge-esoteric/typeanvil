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

/// Tolerance for comparing a flex-resolved main size against a natural
/// content measure (points). A difference within this bound means the item
/// should keep its natural size (no cross_override handed down).
const EPS: f64 = 1e-6;

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
        let inner_left = origin_x + style.margin_left + style.border_left + style.padding_left;
        let inner_width = avail_width
            - style.margin_left
            - style.margin_right
            - style.border_left
            - style.border_right
            - style.padding_left
            - style.padding_right;
        let content_top = box_top
            + (if fresh { style.border_top } else { Scalar::ZERO })
            + style.padding_top;

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
                RelativeInsetShift::resolve(style, self.icb_width, self.icb_height).point(Point::new(inner_left, box_top)),
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
        RelativeInsetShift::resolve(style, self.icb_width, self.icb_height).apply(&mut result.fragment);
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
                if let Some(w) = self.resolved_width(s, inner_width) {
                    w
                } else {
                    self.shrink_to_fit(item_id, inner_width)
                }
            }
        }
    }
    /// Resolve the hypothetical main size of a column flex item.
    ///
    /// Mirror `row_item_main_size` but for the block axis:
    /// - `FlexBasis::Size { length, percent }` -> length, else percent of the
    ///   container's definite main size, else 0 (flex:1 -> basis 0%).
    /// - `FlexBasis::Content` -> the item's content height at the item's width
    ///   (measure_block is the block-path content measure).
    /// - `FlexBasis::Auto` -> the item's declared height (`specified_extent`)
    ///   if present, else the content measure as above.
    fn column_item_main_size(
        &self,
        item_id: NodeId,
        definite_main: Option<Scalar>,
        item_width: Scalar,
    ) -> Scalar {
        let s = &self.styles[item_id];
        match s.flex_basis {
            FlexBasis::Size { length, percent } => {
                if let Some(l) = length {
                    l
                } else if let Some(p) = percent {
                    definite_main.unwrap_or(Scalar::ZERO) * p
                } else {
                    // A zero basis (`flex: 1` → `flex-basis: 0%`).
                    Scalar::ZERO
                }
            }
            FlexBasis::Content => self.measure_block(item_id, item_width),
            FlexBasis::Auto => {
                if let Some(h) = self.resolved_height(s) {
                    h
                } else {
                    self.measure_block(item_id, item_width)
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
                let placed = if child_tok.is_break_before() {
                    natural
                } else {
                    Scalar((natural.get() - child_tok.consumed_block_size.get()).max(0.0))
                };
                // css-flexbox-1 §4.3: a DECLARED cross size (height on a row)
                // sizes the item before align-self/stretch — an empty styled
                // div (background box) measures 0 naturally but paints at its
                // declared height. Take the max so content never shrinks.
                let declared = self.resolved_height(&self.styles[item.id]);
                match declared {
                    Some(d) if d.get() > placed.get() => d,
                    _ => placed,
                }
            };
        }

        // Grow/shrink resolution (nowrap only; wrap packs raw bases): free
        // space is distributed ∝ flex-grow; overflow is absorbed ∝
        // flex-shrink × base, floored at zero (min-content floors deferred).
        // The free space counts flex BASE sizes only (css-flexbox-1 §9.7):
        // the block path applies each item's margins internally (an
        // auto-width box fills container-minus-margins and offsets by
        // margin-left), so including margins here would shrink the grown
        // box twice (CORE-202: page-margin-auto-and-non-zero's ref needs
        // the single-application form).
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
        // (overflow is placed, never dropped). The line cursor advances by
        // each item's OUTER size (main size + main-axis margins): a
        // negative margin overlaps the previous item, which the refs'
        // edges rely on (CORE-202). Placement hands the block path the
        // OUTER start; the block path applies the item's own margins
        // internally.
        let outer_main = |it: &FlexItem| {
            let st = &self.styles[it.id];
            it.main_size.get() + st.margin_left.get() + st.margin_right.get()
        };
        let mut lines: Vec<FlexLine> = Vec::new();
        if !items.is_empty() {
            let mut cur = FlexLine::new();
            let mut cursor = 0.0f64;
            for (i, item) in items.iter().enumerate() {
                let size = outer_main(item);
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
            // css-flexbox-1 §9.4 §3: a SINGLE-line flex container with a
            // definite cross size stretches the line to the container's
            // inner cross size (auto cross-margins and align-self resolve
            // against it). Multi-line (wrap) lines keep their content cross.
            if !wrap && lines.len() == 1 {
                let definite_cross = style
                    .height
                    .map(|h| {
                        h - style.padding_top
                            - style.padding_bottom
                            - style.border_top
                            - style.border_bottom
                    })
                    .filter(|h| h.get() > lines[0].cross.get());
                if let Some(h) = definite_cross {
                    lines[0].cross = h;
                }
            }
        }

        // css-flexbox-1 §9.4 step 4: items whose align-self resolves to
        // `stretch` (the default) and whose cross size is AUTO grow to fill
        // the line's cross size, minus their vertical margins. Auto cross
        // margins take precedence over stretch (§8.1), and a declared
        // height keeps its specified size. Stretched targets are recorded
        // (block_index → target) so the placement pass can hand them to the
        // paint path via the child token.
        let mut stretched: std::collections::HashMap<usize, Scalar> =
            std::collections::HashMap::new();
        for line in &mut lines {
            let stretch_ids: Vec<usize> = line
                .items
                .iter()
                .copied()
                .filter(|&i| {
                    let st = &self.styles[items[i].id];
                    if st.margin_top_auto || st.margin_bottom_auto {
                        return false;
                    }
                    if self.resolved_height(st).is_some() {
                        return false;
                    }
                    match st.align_self {
                        AlignSelf::Auto => {
                            matches!(style.align_items, crate::css::AlignItems::Stretch)
                        }
                        AlignSelf::Stretch => true,
                        _ => false,
                    }
                })
                .collect();
            for i in stretch_ids {
                let st = &self.styles[items[i].id];
                // The item's MARGIN box fills the line (css-flexbox-1 §9.4
                // step 4); cross_size is margin-inclusive, so the target is
                // the line cross itself and the paint border-box subtracts
                // the margins.
                let target = line.cross;
                let paint_target = target - st.margin_top - st.margin_bottom;
                if target.get() > items[i].cross_size.get() {
                    items[i].cross_size = target;
                    stretched.insert(items[i].block_index, paint_target);
                }
            }
        }

        // Justify-content distributes slack inside each line. css-flexbox-1
        // §8.1: auto margins on a flex item absorb free space BEFORE
        // justify-content, and their presence disables justify-content for
        // the line. Free space splits equally among the line's auto margins.
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
            let auto_margin_count: usize = line
                .items
                .iter()
                .map(|&i| {
                    let st = &self.styles[items[i].id];
                    (st.margin_left_auto as usize) + (st.margin_right_auto as usize)
                })
                .sum();
            if auto_margin_count > 0 {
                let per = Scalar(slack / auto_margin_count as f64);
                // Walk the line left-to-right: each item's left auto margin
                // consumes `per` BEFORE the item, its right auto margin
                // AFTER it (offsets accumulate).
                let mut acc = Scalar::ZERO;
                for (k, &i) in line.items.iter().enumerate() {
                    let st = &self.styles[items[i].id];
                    if st.margin_left_auto {
                        acc = acc + per;
                    }
                    line.offsets[k] = line.offsets[k] + acc;
                    if st.margin_right_auto {
                        acc = acc + per;
                    }
                }
                continue;
            }
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
                // next page (last resort only on an empty page). A single-
                // item line defers to the item's own fragmentation instead:
                // a block item's content breaks across pages like a block
                // (multi-line-row-080: item "1<br>2<br>3<br>4" renders
                // 12 on page 1, 345 on page 2).
                if y.get() + lines[li].cross.get() > bottom_limit.get()
                    && placed
                    && line_items.len() > 1
                {
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
            let mut placed_bottom = line_top;
            while k < line_items.len() {
                let item_idx = line_items[k];
                let item = &items[item_idx];

                // css-flexbox-1 §8.1: auto margins on a flex item absorb
                // free space and take precedence over align-self. Both cross
                // margins auto → the item centers in the line; a LONE auto
                // margin absorbs ALL the free space on its side (a zero
                // margin on the opposite side does not disable it).
                let cstyle = &self.styles[item.id];
                let cross_auto =
                    cstyle.margin_top_auto && cstyle.margin_bottom_auto;
                let cross_lone_top =
                    cstyle.margin_top_auto && !cstyle.margin_bottom_auto;
                let cross_lone_bottom =
                    !cstyle.margin_top_auto && cstyle.margin_bottom_auto;
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
                let cross_free = line_cross - item.cross_size;
                let cross_offset = if cross_auto {
                    cross_free * 0.5
                } else if cross_lone_top {
                    // All free space goes above the item → packed to the end.
                    if cross_free.get() > 0.0 { cross_free } else { Scalar::ZERO }
                } else if cross_lone_bottom {
                    // All free space goes below → packed to the start.
                    Scalar::ZERO
                } else {
                    match align {
                        AlignSelf::FlexEnd => {
                            let d = line_cross - item.cross_size;
                            if d.get() > 0.0 { d } else { Scalar::ZERO }
                        }
                        AlignSelf::Center => (line_cross - item.cross_size) * 0.5,
                        _ => Scalar::ZERO,
                    }
                };
                let item_top = line_top + cross_offset;

                // Main-axis x: line offsets are measured from the line start;
                // row-reverse mirrors the line about the container's right
                // edge. Each offset is the item's OUTER start (the block
                // path applies the item's own margins internally).
                let item_x = if reverse {
                    inner_left + inner_width - lines[li].offsets[k] - item.main_size
                } else {
                    inner_left + lines[li].offsets[k]
                };

                let mut child_tok = self.child_incoming(token, item.block_index);
                // A stretched item (§9.4 step 4, applied in Phase A) carries
                // its resolved cross size to the paint path: the block path
                // grows the paint box to it like a declared height.
                if let Some(target) = stretched.get(&item.block_index) {
                    if child_tok.is_break_before() {
                        child_tok.cross_override = Some(*target);
                    }
                }

                let mut res = self.layout_box(
                    item.id,
                    item_x,
                    item.main_size,
                    item_top,
                    bottom_limit,
                    placed,
                    &child_tok,
                    flow,
                );
                if let Some(z) = self.styles[item.id].z_index {
                    res.fragment.z_index = Some(z);
                }
                if !res.empty {
                    children.push(res.fragment);
                    placed = true;
                    let ib = item_top + res.used;
                    if ib.get() > placed_bottom.get() {
                        placed_bottom = ib;
                    }
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
                // The line broke inside: the container still occupies its
                // placed content on this fragmentainer (css-break-3 §5.1).
                if placed_bottom.get() > y.get() {
                    y = placed_bottom;
                }
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
        // CORE-235: the box's own bottom border is part of its border-box
        // size, exactly like padding_bottom (matches the layout_box block
        // path). Without it the resumed flex container reports a height one
        // border-width short of the equivalent display:block box, so a
        // following sibling lands at a different y (CORE-122 invariant).
        if !broke {
            y = y + style.border_bottom;
        }
        let box_height = y - box_top;
        let origin = Point::new(inner_left - style.border_left - style.padding_left, box_top);
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
        // Regular-block border painting (CORE-126): attach the border so
        // `border` on a flex container paints, matching the block path. The
        // border draws INSIDE the fragment rect (css-backgrounds-3).
        if style.border_top.get() > 0.0
            || style.border_right.get() > 0.0
            || style.border_bottom.get() > 0.0
            || style.border_left.get() > 0.0
        {
            let border_box = BorderBox {
                top: style.border_top,
                right: style.border_right,
                bottom: style.border_bottom,
                left: style.border_left,
                top_color: style.border_top_color.unwrap_or(crate::css::Color::BLACK),
                right_color: style.border_right_color.unwrap_or(crate::css::Color::BLACK),
                bottom_color: style.border_bottom_color.unwrap_or(crate::css::Color::BLACK),
                left_color: style.border_left_color.unwrap_or(crate::css::Color::BLACK),
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
                cross_override: None,
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
        let fresh = token.is_break_before();
        let reverse = matches!(style.flex_direction, FlexDirection::ColumnReverse);
        let gap = style.row_gap;

        // --- Phase A (pure): resolve main sizes + build lines. ------------
        // The container's definite main size (its HEIGHT) when one exists:
        // a grid-stretched flex container carries it as `cross_override`
        // (the refs' .horizontal-edge cells), else a declared `height`.
        // css-flexbox-1 §9.8: without a definite main size there is no free
        // space, so grow/shrink does nothing and items keep their bases.
        let definite_main = if let Some(cross) = token.cross_override {
            Some(cross)
        } else {
            self.specified_extent(id, style)
        };

        // Resolve each item's hypothetical MAIN size (its HEIGHT in a column)
        // at the item's own width (cross size).
        for item in &mut items {
            let item_width = self
                .resolved_width(&self.styles[item.id], inner_width)
                .unwrap_or(inner_width);
            item.main_size = self.column_item_main_size(item.id, definite_main, item_width);
        }

        if let Some(definite) = definite_main {
            // css-flexbox-1 §9.7: free space = the container's inner main size
            // minus the sum of the items' OUTER hypothetical main sizes. The
            // block path applies margins SEPARATELY for the block axis (a
            // column item's resolved height is a border-box target handed via
            // cross_override; `used` then adds margin_top/bottom), so margins
            // belong in the free-space math here — unlike the ROW path, where
            // an auto-width box fills container-minus-margins internally and
            // counting them would double-apply (CORE-202 lesson). Negative
            // block-axis margins thus INCREASE the free space, which is what
            // makes a `flex: 1` item next to a negative-margin sibling grow to
            // fill the leftover (paint-order-001/002 refs; measured vs
            // Chromium: yellow .third grows to ~100pt with margins counted vs
            // ~45pt with bases only).
            let outer_main = |it: &FlexItem| {
                let st = &self.styles[it.id];
                it.main_size.get() + st.margin_top.get() + st.margin_bottom.get()
            };
            let total_main: f64 = items.iter().map(outer_main).sum();
            let gaps_total = gap.get() * (items.len().saturating_sub(1) as f64);
            let free = definite.get() - total_main - gaps_total;
            
            if free > 0.0 {
                // Distribute proportional to flex-grow (css-flexbox-1 §9.7.1)
                let grow_sum: f64 = items.iter().map(|i| self.styles[i.id].flex_grow).sum();
                if grow_sum > 0.0 {
                    for item in &mut items {
                        let g = self.styles[item.id].flex_grow;
                        if g > 0.0 {
                            item.main_size = item.main_size + Scalar(free * g / grow_sum);
                        }
                    }
                }
            } else if free < 0.0 {
                // Absorb proportional to flex-shrink * base, floored at zero (css-flexbox-1 §9.7.2)
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
        }
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
            let item_width = if let Some(w) = self.resolved_width(cstyle, inner_width) {
                w
            } else {
                inner_width
            };

            // Hand each item its resolved height via cross_override when needed
            let mut child_tok_clone = child_tok.clone();
            if definite_main.is_some() {
                // Only set cross_override when the resolved height differs from natural content height
                // and is greater than 0
                let resolved_height = item.main_size;
                let natural_height = self.measure_block(item.id, item_width);
                if resolved_height.get() > 0.0
                    && (resolved_height.get() - natural_height.get()).abs() > EPS
                {
                    child_tok_clone.cross_override = Some(resolved_height);
                }
            }
            
            let mut res = self.layout_box(
                item.id,
                inner_left,
                item_width,
                y,
                bottom_limit,
                placed,
                &child_tok_clone,
                flow,
            );
            if let Some(z) = cstyle.z_index {
                res.fragment.z_index = Some(z);
            }

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
        // CORE-235: the box's own bottom border is part of its border-box
        // size, exactly like padding_bottom (matches the layout_box block
        // path).
        if !broke {
            y = y + style.border_bottom;
        }
        // Container height must stay non-negative (CORE-207 fix)
        let mut box_height = y - box_top;
        box_height = Scalar(box_height.get().max(0.0));
        // If a definite main size exists and is larger, use it
        if let Some(definite) = definite_main {
            if definite.get() > box_height.get() {
                box_height = definite;
            }
        }
        let origin = Point::new(inner_left - style.border_left - style.padding_left, box_top);
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
        // Regular-block border painting (CORE-126): attach the border so
        // `border` on a flex container paints, matching the block path. The
        // border draws INSIDE the fragment rect (css-backgrounds-3).
        if style.border_top.get() > 0.0
            || style.border_right.get() > 0.0
            || style.border_bottom.get() > 0.0
            || style.border_left.get() > 0.0
        {
            let border_box = BorderBox {
                top: style.border_top,
                right: style.border_right,
                bottom: style.border_bottom,
                left: style.border_left,
                top_color: style.border_top_color.unwrap_or(crate::css::Color::BLACK),
                right_color: style.border_right_color.unwrap_or(crate::css::Color::BLACK),
                bottom_color: style.border_bottom_color.unwrap_or(crate::css::Color::BLACK),
                left_color: style.border_left_color.unwrap_or(crate::css::Color::BLACK),
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
                cross_override: None,
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
