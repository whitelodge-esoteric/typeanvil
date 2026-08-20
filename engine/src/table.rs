//! Table layout helpers (CORE-61 tables × page breaks).
//!
//! Pure helpers: measure column widths, build row geometry, and map
//! display roles to table-family depth for anonymous-box wrapping.

use crate::css::{ComputedStyle, Display, Hyphens, TextAlign};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::fonts;
use crate::geom::Scalar;
use crate::typography::{break_paragraph, shape_word};

/// Resolved per-column widths plus the intrinsic basis (CORE-81).
#[derive(Clone, Debug, Default)]
pub struct ColumnWidths {
    pub widths: Vec<Scalar>,
    /// Intrinsic min-content widths (uncapped; widest whitespace word + padding).
    pub min_widths: Vec<Scalar>,
    /// Intrinsic max-content widths (uncapped; whole-text width + padding).
    pub max_widths: Vec<Scalar>,
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

/// Measure each column's intrinsic min/max content width over its cells.
/// Public for unit tests; no available-width argument — intrinsics are
/// uncapped.
///
/// Spec: auto-table-layout §Behavior #1–#3.
pub fn intrinsic_column_widths(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
) -> (Vec<Scalar>, Vec<Scalar>) {
    let rows = table_rows(dom, styles, table_id);
    let column_count = rows
        .iter()
        .map(|r| collect_cells(dom, styles, *r).len())
        .max()
        .unwrap_or(0);
    if column_count == 0 {
        return (Vec::new(), Vec::new());
    }
    let mut min_widths = vec![Scalar::ZERO; column_count];
    let mut max_widths = vec![Scalar::ZERO; column_count];
    for row in rows {
        let cells = collect_cells(dom, styles, row);
        for (col, cell) in cells.iter().enumerate() {
            let style = &styles[*cell];
            let text = dom.text_content(*cell);
            let (cell_min, cell_max) = measure_intrinsics(&text, style);
            let cell_min = cell_min + style.padding_left + style.padding_right;
            let cell_max = cell_max + style.padding_left + style.padding_right;
            if cell_min.get() > min_widths[col].get() {
                min_widths[col] = cell_min;
            }
            if cell_max.get() > max_widths[col].get() {
                max_widths[col] = cell_max;
            }
        }
    }
    (min_widths, max_widths)
}

/// The rows of a table in document order: header groups, then body groups,
/// then footer groups (matching the old `measure_columns` iteration).
fn table_rows(dom: &Dom, styles: &[ComputedStyle], table_id: NodeId) -> Vec<NodeId> {
    let (header, body, footer) = collect_table_groups(dom, styles, table_id);
    let mut rows: Vec<NodeId> = Vec::new();
    rows.extend(header.iter().flat_map(|g| collect_rows(dom, styles, *g)));
    rows.extend(body.iter().flat_map(|g| collect_rows(dom, styles, *g)));
    rows.extend(footer.iter().flat_map(|g| collect_rows(dom, styles, *g)));
    rows
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
    let face = fonts::face_for(style.font_weight, style.font_style);
    let font_size = style.font_size;
    let mut min_w = Scalar::ZERO;
    for word in text.split_whitespace() {
        let w = shape_word(word, font_size, face).width;
        if w.get() > min_w.get() {
            min_w = w;
        }
    }
    let max_w = shape_word(text, font_size, face).width;
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
    let (min_widths, max_widths) = intrinsic_column_widths(dom, styles, table_id);
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
        for (col, cell) in cells.iter().enumerate() {
            let style = &styles[*cell];
            let col_w = column_widths
                .widths
                .get(col)
                .copied()
                .unwrap_or(Scalar::ZERO);
            let inner_w = col_w - style.padding_left - style.padding_right;
            let inner_w = if inner_w.get() < 0.0 { Scalar::ZERO } else { inner_w };
            let text = dom.text_content(*cell);
            let content_h = measure_text_height(&text, style, inner_w);
            let total_h = content_h + style.padding_top + style.padding_bottom;
            heights.push(total_h);
            if total_h.get() > row_h.get() {
                row_h = total_h;
            }
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
