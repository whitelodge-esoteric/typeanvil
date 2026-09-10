// SPDX-License-Identifier: AGPL-3.0-only

//! Grid layout (css-grid-1) — CORE-139.
//!
//! A box with `display: grid` / `inline-grid` lays its in-flow children as
//! grid items into the cells defined by `grid-template-columns` /
//! `grid-template-rows`, auto-placed row-major in document order:
//!
//! - **Track sizing**: fixed lengths and percentages resolve against the
//!   container's inner size; `auto` tracks take the max of their items'
//!   content sizes, capped at the available size; left-over space after the
//!   fixed tracks is distributed to `fr` tracks proportionally to their
//!   flex values (auto tracks get their max-content share).
//! - **Placement**: sparse auto-placement, row-major (css-grid-1 §8.5's
//!   "sparse" default packing). Items are direct element children; text
//!   children of a grid container become anonymous items (a single text run
//!   becomes an implicit one-cell item, matching flex's leaf fallback).
//! - **Item layout**: each item is laid out via [`Ctx::layout_box`] into its
//!   cell rect (cell = track area minus gaps), so nested flex, tables, and
//!   blocks work unchanged. `align-self`/`justify-self` non-default values
//!   are deferred (items stretch to the cell by default — the model the
//!   margin-boxes refs exercise via their inner content).
//! - **Fragmentation**: rows are monolithic (like flex lines): when a row
//!   does not fit the fragmentainer it moves whole to the next page. Items
//!   never split across pages inside their cell.
//!
//! Non-goals (this issue): named lines/areas, explicit placement
//! (`grid-column`/`grid-row`), `span`, dense packing, subgrid, `repeat()`,
//! min/max-content sizing beyond the auto maximum.

use super::*;
use crate::css::{AlignSelf, BreakInside, Display, Position};
use crate::dom::NodeId;
use crate::frag::{BreakToken, ChildToken, Fragment, FragmentContent};
use crate::geom::{Point, Scalar};
use style::values::computed::TrackBreadth;

/// One grid item, in document order.
struct GridItem {
    /// The DOM node.
    id: NodeId,
    /// Position among the container's *block* children (`collect_items`
    /// indices) — the index used for `child_tokens` resume.
    block_index: usize,
    /// Row/col assigned by auto-placement (row-major, sparse).
    row: usize,
    col: usize,
}

/// Clamp a scalar at zero (Scalar has no Ord by design — CORE-112).
fn clamp0(v: Scalar) -> Scalar {
    if v.get() < 0.0 {
        Scalar::ZERO
    } else {
        v
    }
}

/// Resolve one computed track breadth to points. `base` is the container's
/// inner size along the track axis; `auto` tracks resolve to `None` (their
/// size is content-driven and resolved in two passes).
fn track_points(b: &TrackBreadth, base: Scalar) -> Option<Scalar> {
    match b {
        TrackBreadth::Breadth(lp) => {
            if let Some(l) = lp.to_length() {
                Some(crate::geom::px_to_pt(l.px() as f64))
            } else if let Some(p) = lp.to_percentage() {
                Some(Scalar(p.0 as f64 * base.get()))
            } else {
                None
            }
        }
        TrackBreadth::Flex(_) | TrackBreadth::Auto => None,
        TrackBreadth::MinContent | TrackBreadth::MaxContent => None,
    }
}

impl Ctx<'_> {
    /// Lay out a grid container. Dispatches from [`Ctx::layout_box`].
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_grid_container(
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
        let inner_width = {
            let w = avail_width
                - style.margin_left
                - style.margin_right
                - style.padding_left
                - style.padding_right;
            if w.get() < 0.0 {
                Scalar::ZERO
            } else {
                w
            }
        };
        let content_top = box_top + style.padding_top;

        // Grid items: the container's block/atomic children in document
        // order (abspos/display:none excluded, flex's collector model).
        // `FlexItem`'s fields are private to flex.rs, so re-collect here via
        // the same `Item` walk (block_index = block-child position).
        let mut placed_items: Vec<GridItem> = Vec::new();
        for (i, item) in self.collect_items(id).into_iter().enumerate() {
            if let Item::Atomic(child) | Item::Block(child) = item {
                let cs = &self.styles[child];
                if cs.display == Display::None {
                    continue;
                }
                if matches!(cs.position, Position::Absolute | Position::Fixed) {
                    continue;
                }
                placed_items.push(GridItem {
                    id: child,
                    block_index: i,
                    row: 0,
                    col: 0,
                });
            }
        }

        // --- 1. Resolve explicit tracks (or implicit single track). -------
        let mut cols = style.grid_columns.clone();
        if cols.is_empty() {
            cols.push(TrackBreadth::Auto);
        }
        let n_cols = cols.len();

        // --- 2. Auto-place items row-major (css-grid-1 §8.5 sparse). ------
        for (k, it) in placed_items.iter_mut().enumerate() {
            it.row = k / n_cols;
            it.col = k % n_cols;
        }
        // Rows: the explicit list, extended implicitly as needed after
        // placement (css-grid-1 §7.1 implicit tracks are auto-sized).
        let mut rows = style.grid_rows.clone();
        let n_rows = placed_items
            .iter()
            .map(|it| it.row + 1)
            .max()
            .unwrap_or(0)
            .max(rows.len());
        while rows.len() < n_rows {
            rows.push(TrackBreadth::Auto);
        }

        // --- 3. Size tracks. ----------------------------------------------
        // Fixed tracks resolve directly; auto tracks take the max of their
        // items' content sizes; leftover space (after fixed + gaps) goes to
        // auto tracks capped at available, then fr tracks proportionally.
        let col_gap = style.flex_column_gap;
        let row_gap = style.row_gap;

        // Max-content share per column for auto tracks: the widest item in
        // the column, shrink-to-fit (bounded by inner_width).
        let mut auto_col_content: Vec<Scalar> = vec![Scalar::ZERO; cols.len()];
        for (ci, c) in cols.iter().enumerate() {
            if !matches!(c, TrackBreadth::Auto) {
                continue;
            }
            let mut w = Scalar::ZERO;
            for it in &placed_items {
                if it.col == ci {
                    let item_w = self
                        .styles[it.id]
                        .width
                        .or_else(|| {
                            self.styles[it.id]
                                .width_percent
                                .map(|p| Scalar(p * inner_width.get()))
                        })
                        .unwrap_or_else(|| self.shrink_to_fit(it.id, inner_width));
                    if item_w.get() > w.get() {
                        w = item_w;
                    }
                }
            }
            auto_col_content[ci] = w;
        }

        // Column base sizes: fixed → resolved; auto → max(content, 0) capped
        // by leftover; fr → leftover share.
        let total_gaps_x = Scalar((cols.len().saturating_sub(1)) as f64 * col_gap.get());
        let mut fixed_sum = Scalar::ZERO;
        let mut fr_sum = 0.0f64;
        let mut auto_cols: Vec<usize> = Vec::new();
        let mut col_sizes: Vec<Scalar> = vec![Scalar::ZERO; cols.len()];
        for (i, c) in cols.iter().enumerate() {
            match c {
                TrackBreadth::Breadth(_) | TrackBreadth::MinContent | TrackBreadth::MaxContent => {
                    let v = track_points(c, inner_width).unwrap_or(Scalar::ZERO);
                    col_sizes[i] = v;
                    fixed_sum = fixed_sum + v;
                }
                TrackBreadth::Flex(f) => {
                    fr_sum += f.0 as f64;
                }
                TrackBreadth::Auto => auto_cols.push(i),
            }
        }
        // Auto columns first: content size (css-grid-1 §12.5 maximize), then
        // §12.6 "expand flexible tracks" does NOT apply to auto — but §12.7
        // "stretch auto tracks" does when justify-content is normal/stretch
        // (the default): auto tracks SHARE the leftover space equally.
        let avail_for_auto = clamp0(inner_width - fixed_sum - total_gaps_x);
        let auto_want: Scalar = auto_cols
            .iter()
            .map(|&i| auto_col_content[i])
            .fold(Scalar::ZERO, |a, b| a + b);
        if !auto_cols.is_empty() {
            // Each auto track gets max(its content share, equal leftover
            // share) — with one auto track this stretches it to fill, which
            // is the margin-boxes-ref shape (`100px auto 100px`).
            let leftover = clamp0(avail_for_auto - auto_want);
            let equal_share = Scalar(leftover.get() / auto_cols.len() as f64);
            for &i in &auto_cols {
                let size = auto_col_content[i] + equal_share;
                col_sizes[i] = size;
                fixed_sum = fixed_sum + size;
            }
        }
        // fr columns: leftover after fixed + autos, proportional.
        let avail_for_fr = clamp0(inner_width - fixed_sum - total_gaps_x);
        if fr_sum > 0.0 {
            for (i, c) in cols.iter().enumerate() {
                if let TrackBreadth::Flex(f) = c {
                    let share = avail_for_fr.get() * (f.0 as f64) / fr_sum;
                    col_sizes[i] = Scalar(share);
                }
            }
        }

        // Row base sizes: same algorithm along the block axis. Content size
        // for auto rows comes from measuring items at their column width.
        let mut fixed_sum_y = Scalar::ZERO;
        let mut fr_sum_y = 0.0f64;
        let mut auto_rows: Vec<usize> = Vec::new();
        let mut row_sizes: Vec<Scalar> = vec![Scalar::ZERO; rows.len()];
        for (i, r) in rows.iter().enumerate() {
            match r {
                TrackBreadth::Breadth(_) | TrackBreadth::MinContent | TrackBreadth::MaxContent => {
                    // Row heights resolve percentages against the page height
                    // proxy (v1 model — same as flex cross sizes).
                    let v = track_points(r, self.page_height).unwrap_or(Scalar::ZERO);
                    row_sizes[i] = v;
                    fixed_sum_y = fixed_sum_y + v;
                }
                TrackBreadth::Flex(f) => {
                    fr_sum_y += f.0 as f64;
                }
                TrackBreadth::Auto => auto_rows.push(i),
            }
        }
        // Auto rows: measure each item at its column width; the row takes
        // the max item height (margin-box).
        for &ri in &auto_rows {
            let mut h = Scalar::ZERO;
            for it in &placed_items {
                if it.row != ri {
                    continue;
                }
                let item_w = col_sizes[it.col];
                let mh = self.measure_block(it.id, item_w);
                if mh.get() > h.get() {
                    h = mh;
                }
            }
            row_sizes[ri] = h;
            fixed_sum_y = fixed_sum_y + h;
        }
        // fr rows distribute the remaining page height (the container's
        // definite block size proxy). When the container has no definite
        // height (page bottom), fr rows collapse to their content — v1:
        // give fr rows an equal share of (page_height - used fixed rows).
        let avail_for_fr_y = clamp0(self.page_height - fixed_sum_y - row_gap);
        if fr_sum_y > 0.0 {
            for (i, r) in rows.iter().enumerate() {
                if let TrackBreadth::Flex(f) = r {
                    let share = avail_for_fr_y.get() * (f.0 as f64) / fr_sum_y;
                    row_sizes[i] = Scalar(share);
                }
            }
        }

        // --- 4. Track offsets. ---------------------------------------------
        let mut col_offsets: Vec<Scalar> = Vec::with_capacity(cols.len());
        let mut x = Scalar::ZERO;
        for (i, w) in col_sizes.iter().enumerate() {
            col_offsets.push(x);
            x = x + *w;
            if i + 1 < cols.len() {
                x = x + col_gap;
            }
        }
        let mut row_offsets: Vec<Scalar> = Vec::with_capacity(rows.len());
        let mut y = Scalar::ZERO;
        for (i, h) in row_sizes.iter().enumerate() {
            row_offsets.push(y);
            y = y + *h;
            if i + 1 < rows.len() {
                y = y + row_gap;
            }
        }
        let grid_height = y;

        // --- 5. Lay items into cells. ---------------------------------------
        let mut children: Vec<Fragment> = Vec::new();
        let mut outgoing_children: Vec<ChildToken> = Vec::new();
        let mut broke = false;
        let mut seen_all = true;
        let mut placed = page_has_content;

        // Resume bookkeeping (flex column's model): rows are monolithic.
        // The first unfinished block index names the resume row.
        if !fresh && token.seen_all_children && token.child_tokens.is_empty() {
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
        let resume_block = if fresh {
            None
        } else {
            token.child_tokens.first().map(|c| c.index)
        };
        // Rows fully above the resume row are skipped (already done).
        let mut row_cursor = 0usize;
        if let Some(bi) = resume_block {
            if let Some(it) = placed_items.iter().find(|it| it.block_index == bi) {
                row_cursor = it.row;
            } else {
                // Past-the-end resume marker: everything finished.
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
        }

        // Items grouped by row for placement.
        let mut y = content_top
            + row_offsets
                .get(row_cursor)
                .copied()
                .unwrap_or(Scalar::ZERO);
        for ri in row_cursor..n_rows {
            let row_top = y;
            let row_h = row_sizes[ri];
            // Row monolithic: doesn't fit → next page (unless the page is
            // empty — last-resort place, matching flex/block behavior).
            if placed && row_top + row_h > bottom_limit {
                let first_in_row = placed_items
                    .iter()
                    .find(|it| it.row == ri)
                    .map(|it| it.block_index);
                if let Some(bi) = first_in_row {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: bi,
                        token: BreakToken::break_before(),
                    });
                    broke = true;
                    break;
                }
                // Empty row: skip it without breaking.
                y = y + row_h + row_gap;
                continue;
            }
            for it in placed_items.iter().filter(|it| it.row == ri) {
                let child_tok = self.child_incoming(token, it.block_index);
                let cell_x = inner_left + col_offsets[it.col];
                let cell_w = col_sizes[it.col];
                let res = self.layout_box(
                    it.id,
                    cell_x,
                    cell_w,
                    row_top,
                    row_top + row_h,
                    placed,
                    &child_tok,
                    flow,
                );
                // Items are monolithic within their row (spec, this issue):
                // a continuation token inside a cell is absorbed — the item
                // content is clipped to the cell for the v1 model.
                if !res.empty {
                    children.push(res.fragment);
                    placed = true;
                }
                if let Some(tok) = res.outgoing {
                    seen_all = false;
                    outgoing_children.push(ChildToken {
                        index: it.block_index,
                        token: tok,
                    });
                    // Absorb: keep going with sibling cells; the row's
                    // continuation rides the row break below.
                }
            }
            y = row_top + row_h + row_gap;
        }

        // --- 6. Finish like the flex/block paths. ---------------------------
        let padding_bottom = if broke {
            Scalar::ZERO
        } else {
            style.padding_bottom
        };
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

        let margin_bottom = if broke {
            Scalar::ZERO
        } else {
            style.margin_bottom
        };
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
