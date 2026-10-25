// SPDX-License-Identifier: AGPL-3.0-only

//! Table layout helpers (CORE-61 tables × page breaks; CORE-89 first-page
//! column freeze).
//!
//! Pure helpers: measure column widths, build row geometry, and map
//! display roles to table-family depth for anonymous-box wrapping.

use crate::css::{ComputedStyle, Display, Hyphens, TextAlign};
use crate::dom::{Dom, Element, Node, NodeId, NodeKind};
use crate::fonts;
use crate::geom::Scalar;
use crate::typography::{break_paragraph, shape_word_with_features};

/// Resolved per-column widths plus the intrinsic basis (CORE-81).
#[derive(Clone, Debug, Default)]
pub struct ColumnWidths {
    pub widths: Vec<Scalar>,
    /// Intrinsic min-content widths (uncapped; widest whitespace word + padding).
    pub min_widths: Vec<Scalar>,
    /// Intrinsic max-content widths (uncapped; whole-text width + padding).
    pub max_widths: Vec<Scalar>,
}

/// A provider of a replaced child's used box (w, h) in points — `<img>` /
/// inline `<svg>` / generated-content images (CORE-210). The table intrinsic
/// measures call it for each DIRECT element child of a cell, and add the box
/// to the column intrinsic width / row height. `None` for non-replaced nodes
/// (the pure table helpers stay image-agnostic; layout.rs supplies the real
/// sizes through `Ctx::image_intrinsic_box`, tests pass a no-op).
pub type ImageSizer<'a> = &'a dyn Fn(NodeId) -> Option<(Scalar, Scalar)>;

/// Which rows contribute to the intrinsic measure (CORE-89 first-page freeze).
///
/// Spec: table-first-page-column-freeze §Behavior #1–#3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeasureScope {
    /// Header + every body row + footer (CORE-81 behavior; single-
    /// fragmentainer tables, and pass 1 of the freeze resolution).
    All,
    /// Header + the first `body_rows` body rows + footer (frozen scope).
    FirstPage { body_rows: usize },
}

/// Resolve a table's used width: the specified `width` (a length, or a
/// percentage of the containing block) when present, else `None` (auto —
/// resolved inside [`measure_columns`]).
///
/// Spec: auto-table-layout §Behavior #4.
pub fn table_used_width(
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
) -> Option<Scalar> {
    let s = &styles[table_id];
    s.width
        .or_else(|| s.width_percent.map(|p| avail_width * p))
}

/// The column span of a table cell (the HTML `colspan` attribute; default 1,
/// clamped to ≥ 1). CORE-96: spanned cells contribute their intrinsic to every
/// spanned column (equal share) and occupy every spanned slot in layout.
/// `rowspan` is NOT supported (auto-table-layout spec Non-Goals).
pub fn cell_colspan(dom: &Dom, cell: NodeId) -> usize {
    match &dom.nodes[cell].kind {
        NodeKind::Element(el) => el
            .attr("colspan")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(1),
        _ => 1,
    }
}

/// Measure each column's intrinsic min/max content width over its scoped
/// cells. Public for unit tests; no available-width argument — intrinsics
/// are uncapped.
///
/// Spec: auto-table-layout §Behavior #1–#3; table-first-page-column-freeze
/// §Behavior #1 (scope).
pub fn intrinsic_column_widths(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    scope: MeasureScope,
    img_sizes: ImageSizer<'_>,
) -> (Vec<Scalar>, Vec<Scalar>) {
    let rows = scoped_table_rows(dom, styles, table_id, &scope);
    let column_count = rows
        .iter()
        .map(|r| {
            collect_cells(dom, styles, *r)
                .iter()
                .map(|c| cell_colspan(dom, *c))
                .sum()
        })
        .max()
        .unwrap_or(0);
    if column_count == 0 {
        return (Vec::new(), Vec::new());
    }
    let mut min_widths = vec![Scalar::ZERO; column_count];
    let mut max_widths = vec![Scalar::ZERO; column_count];
    for row in rows {
        let cells = collect_cells(dom, styles, row);
        let mut col = 0usize;
        for cell in cells {
            let span = cell_colspan(dom, cell);
            let style = &styles[cell];
            // CORE-359: measure the transformed text (cell-level approximation).
            let text = crate::layout::apply_text_transform(
                &dom.text_content(cell),
                style.text_transform,
                None,
            );
            let (mut cell_min, mut cell_max) = measure_intrinsics(&text, style);
            // CORE-210: a replaced child (`<img>`/inline `<svg>`) is ATOMIC —
            // its min-content == max-content == its used box. Without this
            // contribution an image-only cell measures 0 and its column
            // collapses (the image then painted at a negative rect and was
            // skipped). Stacked images are the widest one (block flow).
            for &child in &dom.nodes[cell].children {
                if let Some((w, _h)) = img_sizes(child) {
                    let cs = &styles[child];
                    let box_w = w + cs.margin_left + cs.margin_right;
                    // CORE-521: a replaced child with a percentage `width`
                    // is compressible (css-sizing-3 §5.2.1): its min-content
                    // contribution is zero and its max-content is the
                    // natural box. Using the box as min made three
                    // `width:100%` images force every column to the full
                    // image width and overflow the table.
                    if cs.width_percent.is_none() && box_w.get() > cell_min.get() {
                        cell_min = box_w;
                    }
                    if box_w.get() > cell_max.get() {
                        cell_max = box_w;
                    }
                }
            }
            // CORE-368: a descendant block with a DECLARED width is as wide as
            // that width at both min- and max-content (css-sizing-3 §5: a
            // definite `width` is the box's min/max-content contribution).
            // Without it a cell whose only content is a sized `<div>`
            // measured 0, its column collapsed, and nothing in it painted
            // (WPT repeated-section/*: `width:25px` green columns).
            let declared = declared_descendant_width(dom, styles, cell);
            if declared.get() > cell_min.get() {
                cell_min = declared;
            }
            if declared.get() > cell_max.get() {
                cell_max = declared;
            }
            let cell_min = cell_min + style.padding_left + style.padding_right;
            let mut cell_max = cell_max + style.padding_left + style.padding_right;
            // CORE-356: a cell with a definite `width` has that width as its
            // max-content contribution (css-tables-3 §3.5.1/§4: specified
            // width replaces max-content, floored at min-content). Without
            // it a wrapping fixed-width cell keeps its whole-text max and
            // takes all the surplus width in proportion to (max - min).
            if let (Some(w), None) = (style.width, style.width_percent) {
                let chrome = if style.box_sizing == crate::css::StyloBoxSizing::BorderBox {
                    Scalar::ZERO
                } else {
                    style.padding_left + style.padding_right + style.border_left + style.border_right
                };
                let specified = w + chrome;
                cell_max = if specified.get() > cell_min.get() { specified } else { cell_min };
            }
            // CORE-96: a spanning cell contributes an EQUAL SHARE of its
            // intrinsic to each spanned column (css-tables-3 §10.4.3
            // simplification, Prince-matching: the tfoot's colspan=4 total
            // no longer inflates a single column to its whole-text width).
            let share_min = Scalar(cell_min.get() / span as f64);
            let share_max = Scalar(cell_max.get() / span as f64);
            for j in 0..span {
                let target = col + j;
                if target >= min_widths.len() {
                    break;
                }
                if share_min.get() > min_widths[target].get() {
                    min_widths[target] = share_min;
                }
                if share_max.get() > max_widths[target].get() {
                    max_widths[target] = share_max;
                }
            }
            col += span;
        }
    }
    (min_widths, max_widths)
}

/// Per column: true when any single-span scoped cell in it has a definite
/// (non-percentage) `width` (CORE-356).
fn fixed_columns(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    scope: &MeasureScope,
    column_count: usize,
) -> Vec<bool> {
    let mut fixed = vec![false; column_count];
    for row in scoped_table_rows(dom, styles, table_id, scope) {
        let mut col = 0usize;
        for cell in collect_cells(dom, styles, row) {
            let span = cell_colspan(dom, cell);
            let s = &styles[cell];
            if span == 1 && s.width.is_some() && s.width_percent.is_none() && col < column_count {
                fixed[col] = true;
            }
            col += span;
        }
    }
    fixed
}

/// The widest margin box among `id`'s in-flow element descendants that
/// declare a definite (non-percentage) `width` (CORE-368). A content-box
/// width adds its padding + border; margins always add. Out-of-flow and
/// `display: none` subtrees contribute nothing.
fn declared_descendant_width(dom: &Dom, styles: &[ComputedStyle], id: NodeId) -> Scalar {
    let mut widest = Scalar::ZERO;
    for &child in &dom.nodes[id].children {
        if !matches!(dom.nodes[child].kind, NodeKind::Element(_)) {
            continue;
        }
        let cs = &styles[child];
        if cs.display == Display::None
            || matches!(cs.position, crate::css::Position::Absolute | crate::css::Position::Fixed)
        {
            continue;
        }
        if let (Some(w), None) = (cs.width, cs.width_percent) {
            let chrome = if cs.box_sizing == crate::css::StyloBoxSizing::BorderBox {
                Scalar::ZERO
            } else {
                cs.padding_left + cs.padding_right + cs.border_left + cs.border_right
            };
            let box_w = w + chrome + cs.margin_left + cs.margin_right;
            if box_w.get() > widest.get() {
                widest = box_w;
            }
        }
        let nested = declared_descendant_width(dom, styles, child) + cs.padding_left + cs.padding_right;
        if nested.get() > widest.get() {
            widest = nested;
        }
    }
    widest
}

/// The rows of a table restricted to a [`MeasureScope`] (CORE-89).
///
/// `All` returns every row. `FirstPage { body_rows: k }` returns the header
/// rows, the first `k` body rows, and the footer rows — the rows whose top
/// edge lies in the table's first fragmentainer (footer is included in the
/// measure per spec §Behavior #1 even though it sits at the table's end).
fn scoped_table_rows(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    scope: &MeasureScope,
) -> Vec<NodeId> {
    let (header, body, footer) = collect_table_groups(dom, styles, table_id);
    let header_rows: Vec<NodeId> = header.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    let mut body_rows: Vec<NodeId> = body.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    let footer_rows: Vec<NodeId> = footer.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    match scope {
        MeasureScope::All => {
            let mut rows = header_rows;
            rows.extend(body_rows);
            rows.extend(footer_rows);
            rows
        }
        MeasureScope::FirstPage { body_rows: k } => {
            body_rows.truncate(*k);
            let mut rows = header_rows;
            rows.extend(body_rows);
            rows.extend(footer_rows);
            rows
        }
    }
}

/// The body rows of a table in document order (used by the freeze resolver).
fn body_rows(dom: &Dom, styles: &[ComputedStyle], table_id: NodeId) -> Vec<NodeId> {
    let (_, body, _) = collect_table_groups(dom, styles, table_id);
    body.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect()
}

/// Intrinsic min/max content width of one cell's text, uncapped.
///
/// min = widest whitespace-delimited word. UAX #14 soft breaks inside words
/// are deliberately NOT taken: `/` (class SY) and hyphens stay glued, so
/// "Disappearing/reappearing" is one token (≈112pt at 9pt Arial) — the
/// measured width of Prince's Description column on table-stress (verified
/// 2026-08-20).
/// max = the whole text on one line (strict CSS max-content), spaces included.
fn measure_intrinsics(text: &str, style: &ComputedStyle) -> (Scalar, Scalar) {
    if text.trim().is_empty() {
        return (Scalar::ZERO, Scalar::ZERO);
    }
        let face = style.font_face;
        let font_size = style.font_size;
        let mut min_w = Scalar::ZERO;
        for word in text.split_whitespace() {
            let w = shape_word_with_features(word, font_size, face, &style.ot_features).width;
            if w.get() > min_w.get() {
                min_w = w;
            }
        }
        let max_w = shape_word_with_features(text, font_size, face, &style.ot_features).width;
        (min_w, max_w)
    }

/// Pure css-tables-3 width distribution. Unit-testable with hand-built
/// min/max vectors.
///
/// Spec: auto-table-layout §Behavior #5.
pub fn distribute_column_widths(
    min_widths: &[Scalar],
    max_widths: &[Scalar],
    used_width: Scalar,
) -> Vec<Scalar> {
    let n = min_widths.len();
    if n == 0 {
        return Vec::new();
    }
    let sum_min: f64 = min_widths.iter().map(|w| w.get()).sum();
    let sum_max: f64 = max_widths.iter().map(|w| w.get()).sum();
    let used = used_width.get();

    if used <= sum_min {
        // Overflow: every column keeps its min-content width; the table
        // overflows the available width (no panic).
        return min_widths.to_vec();
    }
    if used >= sum_max {
        // Room to spare: max-content, then the extra ∝ max-content.
        if sum_max <= 0.0 {
            let share = used / n as f64;
            return (0..n).map(|_| Scalar(share)).collect();
        }
        let extra = used - sum_max;
        return max_widths
            .iter()
            .map(|w| Scalar(w.get() + extra * w.get() / sum_max))
            .collect();
    }
    // Middle branch (CSS2.1 §17.5.2.2 / css-tables-3 §10.4.2): min-content
    // floor, then the extra ∝ (max − min). Because used < sum(max), the extra
    // is always below the total (max − min) slack, so proportional shares
    // never exceed a column's slack — no capping pass is reachable.
    let extra = used - sum_min;
    let total_slack = sum_max - sum_min;
    min_widths
        .iter()
        .zip(max_widths.iter())
        .map(|(mn, mx)| Scalar(mn.get() + extra * (mx.get() - mn.get()) / total_slack))
        .collect()
}

/// Measure each column's min/max content width over its cells and resolve
/// widths for the given available content width.
///
/// Spec: auto-table-layout (replaces the tables-fragmentation §Behavior #3
/// content-scaling heuristic).
pub fn measure_columns(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
    used_width: Option<Scalar>,
    img_sizes: ImageSizer<'_>,
) -> ColumnWidths {
    measure_columns_scoped(
        dom,
        styles,
        table_id,
        avail_width,
        used_width,
        MeasureScope::All,
        img_sizes,
    )
}

/// [`measure_columns`] with an explicit measure scope (CORE-89).
///
/// Spec: table-first-page-column-freeze §Behavior #5 (distribution unchanged;
/// only the measure set differs).
pub fn measure_columns_scoped(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
    used_width: Option<Scalar>,
    scope: MeasureScope,
    img_sizes: ImageSizer<'_>,
) -> ColumnWidths {
    let frozen = matches!(scope, MeasureScope::FirstPage { .. });
    let (mut min_widths, mut max_widths) =
        intrinsic_column_widths(dom, styles, table_id, scope.clone(), img_sizes);
    if frozen {
        // CORE-353: the first-page freeze (CORE-89) governs max-content
        // distribution only. A column is never narrower than its min-content
        // over ALL rows (css-tables-3 §3), so floor each column there; the
        // max is raised to keep max >= min.
        let (all_min, _) =
            intrinsic_column_widths(dom, styles, table_id, MeasureScope::All, img_sizes);
        for (i, mn) in min_widths.iter_mut().enumerate() {
            if let Some(all) = all_min.get(i) {
                if all.get() > mn.get() {
                    *mn = *all;
                }
            }
            if mn.get() > max_widths[i].get() {
                max_widths[i] = *mn;
            }
        }
    }
    if min_widths.is_empty() {
        return ColumnWidths {
            widths: Vec::new(),
            min_widths,
            max_widths,
        };
    }
    let sum_max: Scalar = max_widths.iter().fold(Scalar::ZERO, |a, w| a + *w);
    let mut used = match used_width {
        Some(w) => w,
        None => {
            // `width: auto`: the smaller of the available width and the sum
            // of the columns' max-content widths.
            if sum_max.get() < avail_width.get() {
                sum_max
            } else {
                avail_width
            }
        }
    };
    // CORE-235: in the SEPARATE model (border-collapse: separate, the CSS
    // initial) the table's used width is its BORDER-BOX width. The column
    // grid lives INSIDE it, minus the table's own border, padding, and the
    // border-spacing gaps — one gap between the table border/padding and the
    // outermost columns plus one between adjacent columns (n+1 total for n
    // columns). css-tables-3 §17.5.2: border-spacing separates adjacent
    // cells AND the outermost cells from the table's border.
    // In the COLLAPSE model the borders merge onto shared edges and spacing
    // is zero, so the full used width reaches the columns (existing behavior).
    let style = &styles[table_id];
    if style.border_collapse == crate::css::BorderCollapse::Separate {
        let n = max_widths.len() as f64;
        let chrome = style.border_left
            + style.border_right
            + style.padding_left
            + style.padding_right
            + Scalar(style.border_spacing_h.get() * (n + 1.0));
        used = used - chrome;
        if used.get() < 0.0 {
            used = Scalar::ZERO;
        }
    } else {
        // CORE-545: the table's `width` is its border box in the collapse
        // model too, and the row grid is inset by the table's own border and
        // padding (layout_table_grid's `grid_x`). The columns must fit inside
        // that inset or the last column overflows the table box
        // (weasyprint-invoice `table#total`: 18cm wide, 3cm borders).
        used = used
            - (style.border_left + style.border_right + style.padding_left + style.padding_right);
        if used.get() < 0.0 {
            used = Scalar::ZERO;
        }
    }
    let mut widths = distribute_column_widths(&min_widths, &max_widths, used);
    // CORE-356: when the table has room beyond every column's max-content,
    // columns with a specified width stay at it and the surplus goes to the
    // auto columns ∝ their max-content (css-tables-3 §distributing excess
    // width; Chromium parity). Only when both kinds exist.
    let sum_max_f: f64 = max_widths.iter().map(|w| w.get()).sum();
    if used.get() > sum_max_f {
        let fixed = fixed_columns(dom, styles, table_id, &scope, min_widths.len());
        let auto_max: f64 = max_widths
            .iter()
            .zip(fixed.iter())
            .filter(|(_, f)| !**f)
            .map(|(w, _)| w.get())
            .sum();
        if fixed.iter().any(|f| *f) && fixed.iter().any(|f| !*f) && auto_max > 0.0 {
            let extra = used.get() - sum_max_f;
            widths = max_widths
                .iter()
                .zip(fixed.iter())
                .map(|(w, f)| {
                    if *f {
                        *w
                    } else {
                        Scalar(w.get() + extra * w.get() / auto_max)
                    }
                })
                .collect();
        }
    }
    // CORE-355: over-constrained — every column declares a definite width and
    // their sum exceeds the used width. Keep the declared widths; the table
    // grows to the sum (CSS 2.1 §17.5.2.1: table width = max(width, sum of
    // columns); Prince parity). Shrinking only the low-slack column would end
    // short of the declared widths in every engine.
    if used.get() < sum_max_f {
        let fixed = fixed_columns(dom, styles, table_id, &scope, min_widths.len());
        if fixed.iter().all(|f| *f) {
            widths = max_widths.clone();
        }
    }
    ColumnWidths {
        widths,
        min_widths,
        max_widths,
    }
}

/// Resolve the frozen measure scope at the table's first layout (CORE-89).
///
/// Bounded fixed point over the first fragmentainer, spec
/// table-first-page-column-freeze §Behavior #3: pass 1 measures with scope
/// `All`, distributes, and counts the body rows whose top edge lies within
/// `first_fragmentainer_height`; subsequent passes re-measure with
/// `FirstPage { body_rows: k }` and re-count until `k` stabilizes. Hard cap 3
/// passes; on the cap the last computed scope freezes. Returns `All` when the
/// whole table fits one fragmentainer (spec §Behavior #2 — the no-op guard).
///
/// Pure over (dom, styles, geometry): identical input → identical scope →
/// identical frozen widths on every fragmentainer.
pub fn resolve_freeze_scope(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
    used_width: Option<Scalar>,
    first_fragmentainer_height: Scalar,
    img_sizes: ImageSizer<'_>,
) -> MeasureScope {
    let body = body_rows(dom, styles, table_id);
    if body.is_empty() {
        return MeasureScope::All;
    }
    let mut scope = MeasureScope::All;
    for _ in 0..3 {
        let columns = measure_columns_scoped(
            dom,
            styles,
            table_id,
            avail_width,
            used_width,
            scope.clone(),
            img_sizes,
        );
        let k = count_first_page_rows(
            dom,
            styles,
            table_id,
            &columns,
            avail_width,
            first_fragmentainer_height,
            img_sizes,
        );
        if k >= body.len() {
            return MeasureScope::All;
        }
        let next = MeasureScope::FirstPage { body_rows: k };
        if next == scope {
            return scope;
        }
        scope = next;
    }
    scope
}

/// Count the body rows whose top edge lies within the first fragmentainer's
/// content area, given resolved column widths (spec §Behavior #1).
///
/// Walks header rows (consuming their heights), then body rows: a body row is
/// counted while its top edge (the running y after prior rows) is strictly
/// inside the content area — a row whose top is at or past the bottom edge is
/// NOT counted. A straddling row (top inside, bottom past) IS counted.
fn count_first_page_rows(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    columns: &ColumnWidths,
    avail_width: Scalar,
    first_fragmentainer_height: Scalar,
    img_sizes: ImageSizer<'_>,
) -> usize {
    let (header, body, _footer) = collect_table_groups(dom, styles, table_id);
    let header_rows: Vec<NodeId> = header.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    let mut body_rows: Vec<NodeId> = body.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    if body_rows.is_empty() {
        return 0;
    }
    // Header heights consume the top of the fragmentainer.
    let (header_heights, _) = measure_rows(dom, styles, &header_rows, columns, avail_width, img_sizes);
    let mut y: f64 = header_heights.iter().map(|h| h.get()).sum();
    let height = first_fragmentainer_height.get();
    let mut k = 0usize;
    while let Some(row) = body_rows.first().copied() {
        body_rows.remove(0);
        let (heights, _) = measure_rows(dom, styles, &[row], columns, avail_width, img_sizes);
        let row_h = heights.first().copied().unwrap_or(Scalar::ZERO).get();
        if y >= height {
            break;
        }
        k += 1;
        y += row_h;
    }
    k
}

/// Table-family depth for anonymous table box wrapping.
///
/// 0 = table, 1 = row-group, 2 = row, 3 = cell. Unknowns return 255.
pub fn table_family_depth(display: Display) -> u8 {
    match display {
        Display::Table => 0,
        Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup => 1,
        Display::TableRow => 2,
        Display::TableCell => 3,
        _ => 255,
    }
}

/// Compute row heights given column widths.
///
/// Returns (row_heights, cell_heights_per_row).
pub fn measure_rows(
    dom: &Dom,
    styles: &[ComputedStyle],
    row_ids: &[NodeId],
    column_widths: &ColumnWidths,
    _avail_width: Scalar,
    img_sizes: ImageSizer<'_>,
) -> (Vec<Scalar>, Vec<Vec<Scalar>>) {
    let mut row_heights = Vec::with_capacity(row_ids.len());
    let mut cell_heights = Vec::with_capacity(row_ids.len());

    for &row in row_ids {
        let cells = collect_cells(dom, styles, row);
        let mut heights = Vec::with_capacity(cells.len());
        let mut row_h = Scalar::ZERO;
        let mut col = 0usize;
        for cell in cells {
            let span = cell_colspan(dom, cell);
            let style = &styles[cell];
            let mut col_w = Scalar::ZERO;
            for j in 0..span {
                col_w = col_w
                    + column_widths
                        .widths
                        .get(col + j)
                        .copied()
                        .unwrap_or(Scalar::ZERO);
            }
            let inner_w = col_w - style.padding_left - style.padding_right;
            let inner_w = if inner_w.get() < 0.0 { Scalar::ZERO } else { inner_w };
            let text = crate::layout::apply_text_transform(
                &dom.text_content(cell),
                style.text_transform,
                None,
            );
            let mut content_h = measure_text_height(&text, style, inner_w);
// CORE-235: a cell whose height comes from a BLOCK child — a div,
            // an image, a nested box with a declared block-size — is not
            // measured by the text pass. Sum the direct block children's
            // resolved heights (absolute lengths and px runtime, not
            // percentages — those need the containing-block chain, resolved
            // at layout) so `measure_rows` reports the REAL row height. The
            // row-deferral guard (layout.rs) relies on this: a row taller
            // than a full fragmentainer must force-place monolithically, and
            // with only the text measure it looked short enough to defer —
            // which deferred forever (table-fragmentation-003a/b runaway to
            // 100000 pages under the separate model).
            let mut block_h = Scalar::ZERO;
            for &child in &dom.nodes[cell].children {
                if !matches!(dom.nodes[child].kind, NodeKind::Element(_)) {
                    continue;
                }
                let cs = &styles[child];
                // `style.height` is the resolved absolute length.
                // Viewport/percentage heights stay out (they need the
                // containing-block chain, resolved at layout — never reached
                // for the WPT fixtures). Auto stays out (zero).
                if let Some(h) = cs.height {
                    if h.get() > block_h.get() {
                        block_h = h;
                    }
                }
            }
            if block_h.get() > content_h.get() {
                content_h = block_h;
            }
            // CORE-368: the cell's own declared `height` is a minimum, as
            // `layout_box` applies it when the cell is laid (content box
            // unless `box-sizing: border-box`). Without it a footer of
            // `td { height: 0.25in }` measured 0, the footer reservation was
            // one cell short, and the repeated footer no longer fit under the
            // rows (table-fragmentation-003a-d).
            if let Some(h) = style.height {
                let declared = if style.box_sizing == crate::css::StyloBoxSizing::BorderBox {
                    h - style.padding_top - style.padding_bottom - style.border_top - style.border_bottom
                } else {
                    h
                };
                if declared.get() > content_h.get() {
                    content_h = declared;
                }
            }
            // CORE-210: a replaced child contributes its used box height
            // (mirrors `measure_block`'s image handling in layout.rs) — an
            // image-only cell otherwise measures 0 and its row collapses
            // under the image, so following rows would paint OVER it.
            for &child in &dom.nodes[cell].children {
                if let Some((iw, mut h)) = img_sizes(child) {
                    let cs = &styles[child];
                    // CORE-521: the sizer resolves a percentage width
                    // against the image's own natural cap, so its height is
                    // too tall for a narrower column. Re-derive the height
                    // from the aspect ratio at the percentage of the cell's
                    // real inner width (height stays auto only).
                    if let Some(p) = cs.width_percent {
                        if cs.height.is_none() && iw.get() > 0.0 {
                            let used_w = inner_w.get() * p;
                            h = Scalar(used_w * h.get() / iw.get());
                        }
                    }
                    content_h = content_h
                        + cs.margin_top
                        + cs.padding_top
                        + h
                        + cs.padding_bottom
                        + cs.margin_top
                        + cs.margin_bottom;
                }
            }
            // CORE-96: a row's height includes its collapsed row-start border
            // (border-collapse: collapse — the row-start border is drawn on
            // the fragment that starts the row, spec rule 9, and Prince
            // accounts for it: measured 26.1 vs 25.6 for a 2-line row).
            //
            // CORE-235: in the SEPARATE model every cell keeps all four of
            // its borders (collapse_cell_borders is skipped), so a row's
            // height is content + padding + border_top + border_bottom.
            // `border-spacing` is applied OUTSIDE the row box (between rows
            // and around the grid) by the table-block layout, not here.
            let border_bottom_h = match style.border_collapse {
                crate::css::BorderCollapse::Collapse => Scalar::ZERO,
                crate::css::BorderCollapse::Separate => style.border_bottom,
            };
            let total_h =
                content_h + style.padding_top + style.padding_bottom + style.border_top + border_bottom_h;
            heights.push(total_h);
            if total_h.get() > row_h.get() {
                row_h = total_h;
            }
            col += span;
        }
        row_heights.push(row_h);
        cell_heights.push(heights);
    }
    (row_heights, cell_heights)
}

fn collect_table_groups(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
) -> (Vec<NodeId>, Vec<NodeId>, Vec<NodeId>) {
    let mut header = Vec::new();
    let mut body = Vec::new();
    let mut footer = Vec::new();
    for &child in &dom.nodes[table_id].children {
        if let NodeKind::Element(_) = &dom.nodes[child].kind {
            match styles[child].display {
                Display::TableHeaderGroup => header.push(child),
                Display::TableFooterGroup => footer.push(child),
                Display::TableRowGroup => body.push(child),
                Display::TableRow => body.push(child),
                _ => {}
            }
        }
    }
    (header, body, footer)
}

fn collect_rows(dom: &Dom, styles: &[ComputedStyle], group_id: NodeId) -> Vec<NodeId> {
    let mut rows = Vec::new();
    if styles[group_id].display == Display::TableRow {
        rows.push(group_id);
        return rows;
    }
    for &child in &dom.nodes[group_id].children {
        if let NodeKind::Element(_) = &dom.nodes[child].kind {
            if styles[child].display == Display::TableRow {
                rows.push(child);
            }
        }
    }
    rows
}

fn collect_cells(dom: &Dom, styles: &[ComputedStyle], row_id: NodeId) -> Vec<NodeId> {
    let mut cells = Vec::new();
    for &child in &dom.nodes[row_id].children {
        if let NodeKind::Element(_) = &dom.nodes[child].kind {
            if styles[child].display == Display::TableCell {
                cells.push(child);
            }
        }
    }
    cells
}

/// Collapse adjacent cell borders (CORE-119 #5, border-collapse: collapse).
///
/// With `border-collapse: collapse`, the shared edge between two adjacent
/// cells must render ONCE. The layout draws each cell's four sides from its
/// own ComputedStyle, so two neighboring cells that both declare `border`
/// stroke the shared edge twice — interior lines read double-width against a
/// single-width table frame (the CORE-119 screenshot defect). Walk every
/// table's cell grid and zero the duplicate side on the LATER cell of each
// adjacent pair (right neighbor loses `border-left`; the row below loses
/// `border-top`). The surviving side keeps its full width, so the shared
/// edge renders once at declared thickness.
pub fn collapse_cell_borders(dom: &Dom, styles: &mut [ComputedStyle]) {
    // Every table element in document order. Only applies to tables with
    // `border-collapse: collapse`; the separate model keeps each cell's full
    // border and relies on border-spacing (css-tables-3 §13, CORE-235).
    let tables: Vec<NodeId> = (0..dom.nodes.len())
        .filter(|&id| {
            matches!(dom.nodes[id].kind, NodeKind::Element(_))
                && styles[id].display == Display::Table
                && styles[id].border_collapse == crate::css::BorderCollapse::Collapse
        })
        .collect();
    for table_id in tables {
        // Rows in order across all groups (thead/tbody/tfoot + bare tr).
        let mut rows: Vec<NodeId> = Vec::new();
        for &child in &dom.nodes[table_id].children {
            if !matches!(dom.nodes[child].kind, NodeKind::Element(_)) {
                continue;
            }
            match styles[child].display {
                Display::TableRowGroup
                | Display::TableHeaderGroup
                | Display::TableFooterGroup => {
                    rows.extend(collect_rows(dom, styles, child));
                }
                Display::TableRow => rows.push(child),
                _ => {}
            }
        }
        let grid: Vec<Vec<NodeId>> = rows.iter().map(|&r| collect_cells(dom, styles, r)).collect();
        for (ri, row) in grid.iter().enumerate() {
            for ci in 0..row.len() {
                // Horizontal adjacency: this cell's left border duplicates the
                // previous cell's right border — keep the previous one.
                if ci > 0 {
                    styles[row[ci]].border_left = Scalar::ZERO;
                }
                // Vertical adjacency: this row's top border duplicates the
                // row above's bottom border — keep the one above.
                if ri > 0 && !grid[ri - 1].is_empty() {
                    styles[row[ci]].border_top = Scalar::ZERO;
                }
            }
            // Table frame: the first row's cells drop top borders duplicating
            // the table's own top; first column drops left similarly.
            if ri == 0 {
                // handled by vertical rule above when ri>0 only; frame edges
                // stay as declared (outer frame renders once).
            }
        }
    }
}

fn measure_text_width(text: &str, style: &ComputedStyle, max_width: Scalar) -> Scalar {
    if text.trim().is_empty() {
        return Scalar::ZERO;
    }
    let hyphenate = style.hyphens == Hyphens::Auto;
    let justify = style.text_align == TextAlign::Justify;
    let lines = break_paragraph(text, max_width, style, hyphenate, justify);
    let mut width = Scalar::ZERO;
    for line in lines {
        let w = line.drawn_width();
        if w.get() > width.get() {
            width = w;
        }
    }
    if width.get() > max_width.get() { max_width } else { width }
}

fn measure_text_height(text: &str, style: &ComputedStyle, max_width: Scalar) -> Scalar {
    if text.trim().is_empty() {
        return Scalar::ZERO;
    }
    let hyphenate = style.hyphens == Hyphens::Auto;
    let justify = style.text_align == TextAlign::Justify;
    let lines = break_paragraph(text, max_width, style, hyphenate, justify);
    let line_height = style.line_height;
    line_height * (lines.len() as f64)
}

/// Tag of the anonymous table wrappers (CORE-368). It names no HTML element,
/// so no tag-keyed path (`img`, `td` colspan, `br`, ...) ever matches one.
pub const ANONYMOUS_TABLE_TAG: &str = "-typeanvil-anonymous";

/// The wrapper role a table-family parent gives a run of children that are
/// not proper table children (css-tables-3 §3.4.2 "generate missing child
/// wrappers"), or `None` when `parent` is not a table-family container.
fn child_wrapper_role(display: Display) -> Option<Display> {
    match display {
        Display::Table
        | Display::TableRowGroup
        | Display::TableHeaderGroup
        | Display::TableFooterGroup => Some(Display::TableRow),
        Display::TableRow => Some(Display::TableCell),
        _ => None,
    }
}

/// True when `child` is already a proper child of a `parent_display` box,
/// or renders nothing and so needs no wrapper.
fn is_proper_table_child(nodes: &[Node], styles: &[ComputedStyle], parent_display: Display, child: NodeId) -> bool {
    let NodeKind::Element(el) = &nodes[child].kind else {
        return false;
    };
    let cs = &styles[child];
    if cs.display == Display::None {
        return true;
    }
    // Column boxes are unsupported (they compute to Block, css.rs); they hold
    // no content and must not grow an anonymous row.
    if el.tag.eq_ignore_ascii_case("col") || el.tag.eq_ignore_ascii_case("colgroup") {
        return true;
    }
    match parent_display {
        Display::Table => {
            cs.table_caption.is_some()
                || matches!(
                    cs.display,
                    Display::TableRow
                        | Display::TableRowGroup
                        | Display::TableHeaderGroup
                        | Display::TableFooterGroup
                )
        }
        Display::TableRow => cs.display == Display::TableCell,
        _ => cs.display == Display::TableRow,
    }
}

/// A run is worth a wrapper when it holds an element or non-white-space
/// text; a white-space-only run between table boxes is dropped (§3.4.2).
fn run_is_significant(nodes: &[Node], run: &[NodeId]) -> bool {
    run.iter().any(|&c| match &nodes[c].kind {
        NodeKind::Text(t) => !t.trim().is_empty(),
        NodeKind::Element(_) => true,
        NodeKind::Root => false,
    })
}

/// True for an internal table box that needs a table ancestor (css-tables-3
/// §3.4.3 "generate missing parents"). Captions outside a table stay blocks
/// (tables-fragmentation Behavior 19).
fn is_internal_table_box(nodes: &[Node], styles: &[ComputedStyle], id: NodeId) -> bool {
    matches!(nodes[id].kind, NodeKind::Element(_))
        && matches!(
            styles[id].display,
            Display::TableRowGroup
                | Display::TableHeaderGroup
                | Display::TableFooterGroup
                | Display::TableRow
                | Display::TableCell
        )
}

/// Maximal runs of internal table boxes among the children of a NON-table
/// box `id`; each run gets one anonymous table. White space between two
/// internal boxes joins the run; a white-space run at either end does not.
fn orphan_runs(nodes: &[Node], styles: &[ComputedStyle], id: NodeId) -> Vec<Vec<NodeId>> {
    let mut runs = Vec::new();
    let mut cur: Vec<NodeId> = Vec::new();
    let mut pending_ws: Vec<NodeId> = Vec::new();
    for &c in &nodes[id].children {
        if is_internal_table_box(nodes, styles, c) {
            if !cur.is_empty() {
                cur.append(&mut pending_ws);
            }
            pending_ws.clear();
            cur.push(c);
        } else if matches!(&nodes[c].kind, NodeKind::Text(t) if t.trim().is_empty()) {
            pending_ws.push(c);
        } else {
            if !cur.is_empty() {
                runs.push(std::mem::take(&mut cur));
            }
            pending_ws.clear();
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

/// The wrapper role and the runs it wraps for node `id`: child-side runs
/// inside a table-family box, parent-side (anonymous table) runs elsewhere.
fn wrapper_runs(nodes: &[Node], styles: &[ComputedStyle], id: NodeId) -> Option<(Display, Vec<Vec<NodeId>>)> {
    if matches!(nodes[id].kind, NodeKind::Text(_)) {
        return None;
    }
    let (role, runs) = match child_wrapper_role(styles[id].display) {
        Some(role) => (role, improper_runs(nodes, styles, id)),
        // A `display: none` box renders nothing and needs no fixup. A cell's
        // content is ordinary block flow, so a row inside a cell DOES get an
        // anonymous table (the arm below).
        None if styles[id].display == Display::None => return None,
        None => (Display::Table, orphan_runs(nodes, styles, id)),
    };
    (!runs.is_empty()).then_some((role, runs))
}

/// Maximal runs of consecutive improper children of `id` that need a wrapper.
fn improper_runs(nodes: &[Node], styles: &[ComputedStyle], id: NodeId) -> Vec<Vec<NodeId>> {
    if child_wrapper_role(styles[id].display).is_none() {
        return Vec::new();
    }
    if !matches!(nodes[id].kind, NodeKind::Element(_)) {
        return Vec::new();
    }
    let pd = styles[id].display;
    let mut runs = Vec::new();
    let mut cur: Vec<NodeId> = Vec::new();
    for &c in &nodes[id].children {
        if is_proper_table_child(nodes, styles, pd, c) {
            if run_is_significant(nodes, &cur) {
                runs.push(std::mem::take(&mut cur));
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if run_is_significant(nodes, &cur) {
        runs.push(cur);
    }
    runs
}

/// Generate the anonymous table boxes CSS requires (CORE-368; css-tables-3
/// §3.4, CSS2.1 §17.2.1):
///
/// - child side (§3.4.2): a run of non-row children of a table / row group
///   is wrapped in an anonymous `table-row`, and a run of non-cell children
///   of a row in an anonymous `table-cell`;
/// - parent side (§3.4.3): a run of rows / row groups / cells whose parent
///   is not a table-family box is wrapped in an anonymous `table` (a stray
///   cell then gets its anonymous row from the child side).
///
/// Without it the table loop skipped that content and it vanished
/// (`<div style="display:table">text</div>` painted nothing).
///
/// Runs AFTER the cascade, so selectors never see the wrappers (anonymous
/// boxes are not elements). Wrappers are appended to the arena, so every
/// existing `NodeId` stays valid; their styles inherit from the parent
/// ([`ComputedStyle::anonymous_child`]). Returns `None` (no copy) when no
/// table needs fixing.
pub fn fixup_anonymous_table_boxes(dom: &Dom, styles: &mut Vec<ComputedStyle>) -> Option<Dom> {
    if !(0..dom.nodes.len()).any(|id| wrapper_runs(&dom.nodes, styles, id).is_some()) {
        return None;
    }
    let mut nodes = dom.nodes.clone();
    // Index order; wrappers are appended, so a new anonymous row is itself
    // visited and gets its anonymous cell.
    let mut id = 0;
    while id < nodes.len() {
        if let Some((role, runs)) = wrapper_runs(&nodes, styles, id) {
            for run in runs {
                let anon = nodes.len();
                nodes.push(Node {
                    kind: NodeKind::Element(Element {
                        tag: ANONYMOUS_TABLE_TAG.to_string(),
                        id: None,
                        classes: Vec::new(),
                        attrs: Vec::new(),
                    }),
                    parent: Some(id),
                    children: run.clone(),
                });
                let st = styles[id].anonymous_child(role);
                styles.push(st);
                for &c in &run {
                    nodes[c].parent = Some(anon);
                }
                let first = nodes[id].children.iter().position(|&c| c == run[0]).expect("run child");
                nodes[id].children.splice(first..first + run.len(), [anon]);
            }
        }
        id += 1;
    }
    Some(Dom {
        nodes,
        root: dom.root,
        source_lines: dom.source_lines.clone(),
    })
}
