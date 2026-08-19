//! Multi-column layout (css-multicol-1) — CORE-63.
//!
//! A box with `column-count: > 1` (or `column-width` set) lays its items into
//! N balanced column fragmentainers that progress inline within the box's
//! content box (the fragmentainer generalization from fragmentation-core
//! goal #3: a column is a fragmentainer with no source box).
//!
//! **Balancing.** The box's items are measured once at the column width
//! (bounded: one measure pass per set), and each set's column height is
//! `ceil(total / n)` rounded up to a line box. Spanners segment the items:
//! content before a `column-span: all` element fills one balanced set, the
//! spanner spans the full content width, content after starts a new set.
//!
//! **Multicol under print (sequential sets).** When the container's balanced
//! extent does not fit the remaining page, the page fills as many sets (and
//! columns) as fit; the container breaks with the in-flight items' tokens,
//! and the next page re-balances the remaining content as a fresh set — a
//! column never slices a line.
//!
//! Non-goals this pass (see the spec): `column-rule-*` painting, floats
//! inside columns (laid as plain blocks), `column-fill: auto` (the servo
//! build does not compile the longhand).

use super::*;
use crate::css::{ColumnSpan, Position};
use crate::dom::NodeId;
use crate::frag::{Fragment, FragmentContent, FragmentKind};
use crate::geom::{Point, Scalar};

impl<'a> Ctx<'a> {
    /// Resolve the css-multicol auto algorithm. Returns `(n, col_w, gap)` in
    /// points when the box actually engages multicol (`n >= 2`), else `None`
    /// (plain block layout).
    pub(super) fn multicol_geometry(
        &self,
        style: &ComputedStyle,
        inner_width: Scalar,
    ) -> Option<(u32, Scalar, Scalar)> {
        let gap = style.column_gap;
        let w = inner_width.get();
        if w <= 0.0 {
            return None;
        }
        let g = gap.get();
        let width = style.column_width; // Option<Scalar>
        let count = style.column_count; // Option<u32>
        let n = match (width, count) {
            (None, Some(c)) => c.max(1),
            (Some(cw), None) => (((w + g) / (cw.get() + g)).floor() as u32).max(1),
            (Some(cw), Some(c)) => c
                .max(1)
                .min((((w + g) / (cw.get() + g)).floor() as u32).max(1)),
            (None, None) => 1,
        };
        if n < 2 {
            return None;
        }
        let col_w = ((w - (n as f64 - 1.0) * g) / n as f64).max(0.0);
        Some((n, Scalar(col_w), gap))
    }

    /// Lay out a multicol container and return its block result.
    pub(super) fn layout_multicol_container(
        &self,
        id: NodeId,
        inner_left: Scalar,
        inner_width: Scalar,
        top: Scalar,
        bottom_limit: Scalar,
        placed: bool,
        token: &BreakToken,
        flow: &mut Flow,
        style: &ComputedStyle,
        (n, col_w, gap): (u32, Scalar, Scalar),
    ) -> BlockResult {
        let fresh = token.is_break_before();
        let margin_top = if fresh { style.margin_top } else { Scalar::ZERO };
        let box_top = top + margin_top;
        let content_top = box_top + style.padding_top;

        let items = self.collect_items(id);
        // Where to resume (after a page break inside the container).
        let start_index = if fresh {
            0
        } else {
            token
                .child_tokens
                .first()
                .map(|c| c.index)
                .unwrap_or(items.len())
        };

        // ---- Segment items into balanced sets separated by spanners ----
        // A "set" is a maximal run of non-spanner items; spanners sit between
        // sets and span the full content width.
        let mut sets: Vec<(usize, usize, Scalar)> = Vec::new(); // (start, end, target_h)
        let mut spanners: Vec<(usize, Scalar)> = Vec::new(); // (index, measured h)
        let mut seg_start = start_index;
        let mut i = start_index;
        while i < items.len() {
            if let Item::Block(child) = &items[i] {
                let cs = &self.styles[*child];
                if cs.column_span == ColumnSpan::All {
                    if i > seg_start {
                        let h = self.measure_items(&items[seg_start..i], col_w, style);
                        sets.push((seg_start, i, self.balanced_target(h, n, style)));
                    }
                    let sh = self.measure_block(*child, inner_width);
                    spanners.push((i, sh));
                    seg_start = i + 1;
                }
            }
            i += 1;
        }
        if seg_start < items.len() {
            let h = self.measure_items(&items[seg_start..], col_w, style);
            sets.push((seg_start, items.len(), self.balanced_target(h, n, style)));
        }

        // ---- Lay sets + spanners sequentially ----
        let mut y = content_top;
        let mut children: Vec<Fragment> = Vec::new();
        let mut outgoing: Option<BreakToken> = None;
        let mut done = true;

        // Consumed child tokens accumulate across the container (for the
        // resume token when the container fragments across pages).
        let mut outgoing_children: Vec<ChildToken> = Vec::new();
        let mut set_i = 0usize;
        let mut span_i = 0usize;
        let mut in_flight = start_index;

        while set_i < sets.len() || span_i < spanners.len() {
            // Lay the next set (if any) before the next spanner.
            let set_before_span = if span_i < spanners.len() {
                sets.get(set_i).map(|s| s.1 <= spanners[span_i].0)
            } else {
                None
            };
            let lay_set = set_i < sets.len()
                && (span_i >= spanners.len() || sets[set_i].0 < spanners[span_i].0);
            let lay_span = span_i < spanners.len()
                && (set_i >= sets.len() || spanners[span_i].0 < sets[set_i].0);
            debug_assert!(lay_set != lay_span || (sets.is_empty() && spanners.is_empty()));
            let _ = set_before_span;

            if lay_set {
                let (s0, s1, target) = sets[set_i];
                let fits = y + target <= bottom_limit;
                if !fits && !(placed && y == content_top) {
                    // The set does not fit the page: fill the remaining page
                    // with as many columns as fit — anchored at the
                    // container's current cursor (anchoring at the page
                    // bottom gave columns zero usable height: the first item
                    // was force-placed at `bottom_limit` and could never
                    // progress, the CORE-78 infinite pagination) — then
                    // fragment the container; the next page re-balances the
                    // rest as a fresh set. A set whose balanced estimate
                    // over-ran but whose content actually finished inside the
                    // page's columns is DONE: do not fragment (fragmenting
                    // with an empty child token would re-lay the same set
                    // with no cursor advance).
                    let set_top = y;
                    let (cols, consumed_upto, tok) = self.fill_columns_partial(
                        id,
                        &items,
                        s0,
                        inner_left,
                        col_w,
                        gap,
                        n,
                        set_top,
                        bottom_limit,
                        placed,
                        token,
                        flow,
                        style,
                    );
                    for c in &cols {
                        children.push(c.clone());
                    }
                    in_flight = consumed_upto;
                    if let Some(t) = tok {
                        outgoing_children.push(t);
                    }
                    if consumed_upto < items.len() {
                        // Content remains after the page's columns: the
                        // container fragments and the rest resumes next page.
                        done = false;
                        y = bottom_limit;
                        break;
                    }
                    // The set finished inside the page's columns: advance the
                    // cursor past the laid columns and continue with the next
                    // set.
                    let laid = cols.iter().map(|c| c.size.1).fold(Scalar::ZERO, |a, b| {
                        if b.get() > a.get() {
                            b
                        } else {
                            a
                        }
                    });
                    y = y + laid;
                    set_i += 1;
                }
                // The set fits: fill all n columns at the balanced target.
                let (cols, consumed_upto, tok) = self.fill_columns(
                    id,
                    &items,
                    s0,
                    s1,
                    inner_left,
                    col_w,
                    gap,
                    n,
                    target,
                    content_top,
                    y,
                    placed,
                    token,
                    flow,
                    style,
                );
                for c in cols {
                    children.push(c);
                }
                in_flight = consumed_upto;
                if let Some(t) = tok {
                    // The set itself broke mid-way (shouldn't happen when the
                    // target fits, but stay honest): fragment the container.
                    outgoing_children.push(t);
                    done = false;
                    y = y + target;
                    break;
                }
                y = y + target;
                set_i += 1;
            } else if lay_span {
                let (si, _sh) = spanners[span_i];
                let child = match items[si] {
                    Item::Block(c) => c,
                    _ => unreachable!("spanner is a block item"),
                };
                let child_tok = self.child_incoming(token, si);
                let fits = y + self.measure_block(child, inner_width) <= bottom_limit;
                if !fits && !(placed && y == content_top) {
                    // The spanner does not fit: fragment before it.
                    done = false;
                    in_flight = si;
                    break;
                }
                let res = self.layout_box(
                    child,
                    inner_left,
                    inner_width,
                    y,
                    bottom_limit,
                    placed,
                    &child_tok,
                    flow,
                );
                children.push(res.fragment);
                if res.outgoing.is_some() {
                    outgoing_children.push(ChildToken {
                        index: si,
                        token: res.outgoing.unwrap(),
                    });
                    done = false;
                    break;
                }
                y = y + res.used;
                in_flight = si + 1;
                span_i += 1;
            } else {
                break;
            }
        }

        let box_height = if done {
            // The container finished: its height is the laid extent.
            let h = y - box_top;
            if h.get() > 0.0 {
                h
            } else {
                Scalar::ZERO
            }
        } else {
            // Fragmented: the container fills the page.
            let h = bottom_limit - box_top;
            if h.get() > 0.0 {
                h
            } else {
                Scalar::ZERO
            }
        };
        let used = (box_top - top) + box_height + style.margin_bottom;

        // Offsets become relative to the container fragment.
        let origin = Point::new(inner_left, top);
        for child in &mut children {
            child.offset = Point::new(
                child.offset.x - origin.x,
                child.offset.y - origin.y,
            );
        }

        let seen_all = done && set_i >= sets.len() && span_i >= spanners.len();
        let outgoing = if done {
            None
        } else {
            // Resume token: consumed children's tokens + in-flight child.
            let mut ct = outgoing_children;
            ct.sort_by_key(|c| c.index);
            Some(BreakToken {
                consumed_block_size: Scalar::ZERO,
                seen_all_children: seen_all,
                child_tokens: ct,
                break_before: false,
                consumed_chars: None,
            })
        };

        let mut fragment = Fragment::block(Point::new(inner_left, top), (inner_width, box_height));
        fragment.children = children;
        fragment.source = Some(id);
        let empty = children_empty(&fragment) && outgoing.is_none() && box_height.get() <= 0.0;

        BlockResult {
            fragment,
            used,
            outgoing,
            empty,
        }
    }

    /// One measure pass: the items' in-flow height at the column width.
    fn measure_items(
        &self,
        items: &[Item],
        col_w: Scalar,
        style: &ComputedStyle,
    ) -> Scalar {
        let mut h = Scalar::ZERO;
        for item in items {
            match item {
                Item::Text(text) => {
                    let lines = self.break_paragraph(text, col_w, style);
                    h = h + style.line_height * lines.len() as f64;
                }
                Item::Block(child) => {
                    let cs = &self.styles[*child];
                    if cs.column_span == ColumnSpan::All || cs.float != crate::css::Float::None
                        || matches!(cs.position, Position::Absolute | Position::Fixed)
                    {
                        continue;
                    }
                    h = h + self.measure_block(*child, col_w);
                }
            }
        }
        h
    }

    /// The balanced column height for a set: `ceil(total / n)` rounded up to
    /// a whole line box, at least one line. An empty set needs no columns.
    fn balanced_target(&self, total: Scalar, n: u32, style: &ComputedStyle) -> Scalar {
        if total.get() <= 0.0 {
            return Scalar::ZERO;
        }
        let lh = style.line_height;
        let lines = (total.get() / lh.get() / n as f64).ceil().max(1.0);
        lh * lines
    }

    /// Fill one balanced set: `n` columns of height `target` with the items
    /// `items[s0..s1]`. Returns the column fragments (offsets absolute), the
    /// item index consumed up to (exclusive), and the in-flight child token
    /// if the set did not finish.
    #[allow(clippy::too_many_arguments)]
    fn fill_columns(
        &self,
        id: NodeId,
        items: &[Item],
        s0: usize,
        s1: usize,
        col_x: Scalar,
        col_w: Scalar,
        gap: Scalar,
        n: u32,
        target: Scalar,
        content_top: Scalar,
        set_top: Scalar,
        placed: bool,
        token: &BreakToken,
        flow: &mut Flow,
        style: &ComputedStyle,
    ) -> (Vec<Fragment>, usize, Option<ChildToken>) {
        let mut cols = Vec::with_capacity(n as usize);
        let mut i = s0;
        let mut col = 0usize;
        let mut in_flight: Option<ChildToken> = None;
        let mut resume: Option<(usize, BreakToken)> = None;
        while col < n as usize && i < s1 {
            // Only the CURRENT column's in-flight token matters: a token from
            // an earlier column is stale once its item finished.
            in_flight = None;
            let x = col_x + Scalar((col as f64) * (col_w + gap).get());
            let col_bottom = set_top + target;
            let (col_frag, consumed, tok) = self.fill_one_column(
                id,
                items,
                i,
                s1,
                x,
                col_w,
                col_bottom,
                set_top,
                placed,
                token,
                resume,
                flow,
                style,
            );
            cols.push(col_frag);
            i = consumed;
            resume = match &tok {
                Some(t) => Some((i, t.token.clone())),
                None => None,
            };
            if let Some(t) = tok {
                in_flight = Some(t);
            }
            if i >= s1 {
                break;
            }
            col += 1;
        }
        let _ = content_top;
        // If content remains after the last column, the set is in-flight.
        if i < s1 {
            (cols, i, in_flight)
        } else {
            (cols, i, None)
        }
    }

    /// Fill columns against the page bottom (a set that does not fit the
    /// page): columns span `set_top..bottom_limit`; overflow resumes on
    /// the next page. Returns (columns, consumed_upto, in-flight token).
    #[allow(clippy::too_many_arguments)]
    fn fill_columns_partial(
        &self,
        id: NodeId,
        items: &[Item],
        s0: usize,
        col_x: Scalar,
        col_w: Scalar,
        gap: Scalar,
        n: u32,
        set_top: Scalar,
        bottom_limit: Scalar,
        placed: bool,
        token: &BreakToken,
        flow: &mut Flow,
        style: &ComputedStyle,
    ) -> (Vec<Fragment>, usize, Option<ChildToken>) {
        let mut cols = Vec::with_capacity(n as usize);
        let mut i = s0;
        let mut col = 0usize;
        let mut in_flight: Option<ChildToken> = None;
        let mut resume: Option<(usize, BreakToken)> = None;
        while col < n as usize {
            in_flight = None;
            let x = col_x + Scalar((col as f64) * (col_w + gap).get());
            let (col_frag, consumed, tok) = self.fill_one_column(
                id,
                items,
                i,
                items.len(),
                x,
                col_w,
                bottom_limit,
                set_top,
                placed,
                token,
                resume,
                flow,
                style,
            );
            cols.push(col_frag);
            i = consumed;
            resume = match &tok {
                Some(t) => Some((i, t.token.clone())),
                None => None,
            };
            if let Some(t) = tok {
                in_flight = Some(t);
            }
            if i >= items.len() {
                break;
            }
            col += 1;
        }
        (cols, i, in_flight)
    }

    /// Fill one column fragment with items starting at `start`, breaking at
    /// `col_bottom`. Returns (column fragment, consumed_upto, in-flight
    /// child token when the content did not finish in this column).
    #[allow(clippy::too_many_arguments)]
    fn fill_one_column(
        &self,
        id: NodeId,
        items: &[Item],
        start: usize,
        end: usize,
        x: Scalar,
        col_w: Scalar,
        col_bottom: Scalar,
        set_top: Scalar,
        placed: bool,
        token: &BreakToken,
        resume: Option<(usize, BreakToken)>,
        flow: &mut Flow,
        style: &ComputedStyle,
    ) -> (Fragment, usize, Option<ChildToken>) {
        let mut children: Vec<Fragment> = Vec::new();
        let mut y = set_top;
        let mut i = start;
        let mut page_placed = placed;
        let mut in_flight: Option<ChildToken> = None;
        let mut broke = false;

        while i < end {
            let fallback = self.child_incoming(token, i);
            let child_tok: &BreakToken = match &resume {
                Some((ri, t)) if *ri == i => t,
                _ => &fallback,
            };
            match &items[i] {
                Item::Text(text) => {
                    let lh = style.line_height;
                    let resume_offset = child_tok.consumed_chars;
                    let mut src_offset = resume_offset.unwrap_or(0).min(text.len());
                    let mut consumed_lines = if resume_offset.is_some() {
                        0
                    } else {
                        (child_tok.consumed_block_size.get() / lh.get()).round() as usize
                    };
                    let lines = self.break_paragraph(&text[src_offset..], col_w, style);
                    if lines.is_empty() {
                        i += 1;
                        continue;
                    }
                    let mut li = consumed_lines;
                    let mut run_broke = false;
                    while li < lines.len() {
                        let fits = y + lh <= col_bottom;
                        // Last resort: a genuinely empty column places at
                        // least one line even when taller than the column.
                        let last_resort = !page_placed && li == consumed_lines;
                        if !fits && !last_resort {
                            break;
                        }
                        let baseline = y + style.font_size;
                        let lr = &lines[li];
                        let lx = self.aligned_x(x, col_w, lr.drawn_width(), style);
                        let run = TextRun {
                            text: lr.text.clone(),
                            baseline: Point::new(lx, baseline),
                            font_size: style.font_size,
                            color: style.color,
                            font_face: crate::fonts::face_for(style.font_weight, style.font_style),
                            glyphs: lr.glyphs.clone(),
                            expansion: lr.expansion,
                            protrude_left: lr.protrude_left,
                            protrude_right: lr.protrude_right,
                        };
                        children.push(Fragment::line(Point::new(x, y), (col_w, lh), run));
                        y += lh;
                        li += 1;
                        src_offset += lr.text.len();
                        page_placed = true;
                        if last_resort && y > col_bottom {
                            run_broke = true;
                            break;
                        }
                    }
                    if li < lines.len() || run_broke {
                        // The run did not finish in this column.
                        in_flight = Some(ChildToken {
                            index: i,
                            token: BreakToken {
                                consumed_block_size: Scalar::ZERO,
                                seen_all_children: false,
                                child_tokens: Vec::new(),
                                break_before: false,
                                consumed_chars: Some(src_offset),
                            },
                        });
                        broke = true;
                        break;
                    }
                    i += 1;
                }
                Item::Block(child) => {
                    let cs = &self.styles[*child];
                    if cs.column_span == ColumnSpan::All {
                        // A spanner ends the set (defensive; segmentation
                        // normally prevents this).
                        in_flight = None;
                        broke = true;
                        break;
                    }
                    if matches!(cs.position, Position::Absolute | Position::Fixed) {
                        // Abspos inside a column: attach to the page (the
                        // flow machinery reuses the same path), do not
                        // advance the cursor.
                        let cb = flow
                            .abspos_cb
                            .unwrap_or((Point::new(self.content_x, set_top), self.content_width));
                        let (fw, fh) = self.measure_float(*child, cb.1);
                        let ax = match cs.inset_left {
                            Some(l) => cb.0.x + l,
                            None => match cs.inset_right {
                                Some(r) => cb.0.x + cb.1 - fw - r,
                                None => cb.0.x,
                            },
                        };
                        let ay = match cs.inset_top {
                            Some(t) => cb.0.y + t,
                            None => match cs.inset_bottom {
                                Some(b) => cb.0.y + self.page_height - fh - b,
                                None => cb.0.y,
                            },
                        };
                        let saved = std::mem::take(&mut flow.active_floats);
                        let res = self.layout_box(
                            *child,
                            ax,
                            fw,
                            ay,
                            Scalar(f64::MAX),
                            true,
                            &child_tok,
                            flow,
                        );
                        flow.active_floats = saved;
                        flow.abspos.push((cs.z_index, res.fragment));
                        i += 1;
                        continue;
                    }
                    // Forced break before: start a new column (or page, when
                    // this is the last column — the break_before token makes
                    // the container fragment and the item resumes fresh).
                    // Only when the CONTAINER is resuming: a fresh container
                    // hands every item a default break_before token.
                    if !token.is_break_before() && child_tok.is_break_before() && !broke && y > set_top {
                        in_flight = Some(ChildToken {
                            index: i,
                            token: BreakToken::break_before(),
                        });
                        broke = true;
                        break;
                    }
                    let h = self.measure_block(*child, col_w);
                    let fits = y + h <= col_bottom;
                    let last_resort = !page_placed && y == set_top;
                    if !fits && !last_resort {
                        in_flight = Some(ChildToken {
                            index: i,
                            token: BreakToken::break_before(),
                        });
                        broke = true;
                        break;
                    }
                    let res = self.layout_box(
                        *child,
                        x,
                        col_w,
                        y,
                        col_bottom,
                        page_placed,
                        &child_tok,
                        flow,
                    );
                    children.push(res.fragment);
                    y = y + res.used;
                    page_placed = true;
                    if res.outgoing.is_some() {
                        in_flight = Some(ChildToken {
                            index: i,
                            token: res.outgoing.unwrap(),
                        });
                        broke = true;
                        break;
                    }
                    i += 1;
                }
            }
        }
        let _ = id;

        let origin = Point::new(x, set_top);
        let h = y - set_top;
        let height = if h.get() > 0.0 { h } else { Scalar::ZERO };
        for child in &mut children {
            child.offset = Point::new(
                child.offset.x - origin.x,
                child.offset.y - origin.y,
            );
        }
        let mut frag = Fragment::block(origin, (col_w, height));
        frag.children = children;
        (frag, i, in_flight)
    }
}
