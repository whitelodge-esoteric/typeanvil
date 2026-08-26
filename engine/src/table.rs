// SPDX-License-Identifier: AGPL-3.0-only

//! Table layout helpers (CORE-61 tables × page breaks; CORE-89 first-page
//! column freeze).
//!
//! Pure helpers: measure column widths, build row geometry, and map
//! display roles to table-family depth for anonymous-box wrapping.

use crate::css::{ComputedStyle, Display, Hyphens, TextAlign};
use crate::dom::{Dom, NodeId, NodeKind};
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
            let text = dom.text_content(cell);
            let (cell_min, cell_max) = measure_intrinsics(&text, style);
            let cell_min = cell_min + style.padding_left + style.padding_right;
            let cell_max = cell_max + style.padding_left + style.padding_right;
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
) -> ColumnWidths {
    measure_columns_scoped(dom, styles, table_id, avail_width, used_width, MeasureScope::All)
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
) -> ColumnWidths {
    let (min_widths, max_widths) = intrinsic_column_widths(dom, styles, table_id, scope);
    if min_widths.is_empty() {
        return ColumnWidths {
            widths: Vec::new(),
            min_widths,
            max_widths,
        };
    }
    let sum_max: Scalar = max_widths.iter().fold(Scalar::ZERO, |a, w| a + *w);
    let used = match used_width {
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
    let widths = distribute_column_widths(&min_widths, &max_widths, used);
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
) -> MeasureScope {
    let body = body_rows(dom, styles, table_id);
    if body.is_empty() {
        return MeasureScope::All;
    }
    let mut scope = MeasureScope::All;
    for _ in 0..3 {
        let columns = measure_columns_scoped(dom, styles, table_id, avail_width, used_width, scope.clone());
        let k = count_first_page_rows(dom, styles, table_id, &columns, avail_width, first_fragmentainer_height);
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
) -> usize {
    let (header, body, _footer) = collect_table_groups(dom, styles, table_id);
    let header_rows: Vec<NodeId> = header.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    let mut body_rows: Vec<NodeId> = body.iter().flat_map(|g| collect_rows(dom, styles, *g)).collect();
    if body_rows.is_empty() {
        return 0;
    }
    // Header heights consume the top of the fragmentainer.
    let (header_heights, _) = measure_rows(dom, styles, &header_rows, columns, avail_width);
    let mut y: f64 = header_heights.iter().map(|h| h.get()).sum();
    let height = first_fragmentainer_height.get();
    let mut k = 0usize;
    while let Some(row) = body_rows.first().copied() {
        body_rows.remove(0);
        let (heights, _) = measure_rows(dom, styles, &[row], columns, avail_width);
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
            let text = dom.text_content(cell);
            let content_h = measure_text_height(&text, style, inner_w);
            // CORE-96: a row's height includes its collapsed row-start border
            // (border-collapse: collapse — the row-start border is drawn on
            // the fragment that starts the row, spec rule 9, and Prince
            // accounts for it: measured 26.1 vs 25.6 for a 2-line row).
            let total_h = content_h + style.padding_top + style.padding_bottom + style.border_top;
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
    // Every table element in document order.
    let tables: Vec<NodeId> = (0..dom.nodes.len())
        .filter(|&id| {
            matches!(dom.nodes[id].kind, NodeKind::Element(_))
                && styles[id].display == Display::Table
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
