//! Table layout helpers (CORE-61 tables × page breaks).
//!
//! Pure helpers: measure column widths, build row geometry, and map
//! display roles to table-family depth for anonymous-box wrapping.

use crate::css::{ComputedStyle, Display, Hyphens, TextAlign};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::geom::Scalar;
use crate::typography::break_paragraph;

/// Resolved per-column widths.
#[derive(Clone, Debug, Default)]
pub struct ColumnWidths {
    pub widths: Vec<Scalar>,
}

/// Measure each column's min/max content width over its cells and resolve
/// widths for the given available content width.
///
/// Spec: tables-fragmentation §Behavior #3.
pub fn measure_columns(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
) -> ColumnWidths {
    let (header, body, footer) = collect_table_groups(dom, styles, table_id);
    let mut rows: Vec<NodeId> = Vec::new();
    rows.extend(header.iter().flat_map(|g| collect_rows(dom, styles, *g)));
    rows.extend(body.iter().flat_map(|g| collect_rows(dom, styles, *g)));
    rows.extend(footer.iter().flat_map(|g| collect_rows(dom, styles, *g)));

    let column_count = rows
        .iter()
        .map(|r| collect_cells(dom, styles, *r).len())
        .max()
        .unwrap_or(0);
    if column_count == 0 {
        return ColumnWidths { widths: Vec::new() };
    }

    let mut max_widths = vec![Scalar::ZERO; column_count];
    for row in rows {
        let cells = collect_cells(dom, styles, row);
        for (col, cell) in cells.iter().enumerate() {
            let style = &styles[*cell];
            let text = dom.text_content(*cell);
            let content_w = measure_text_width(&text, style, avail_width);
            let cell_w = content_w + style.padding_left + style.padding_right;
            if cell_w.get() > max_widths[col].get() {
                max_widths[col] = cell_w;
            }
        }
    }

    let total: Scalar = max_widths.iter().fold(Scalar::ZERO, |acc, w| acc + *w);
    if total.get() <= 0.0 {
        let equal = avail_width * (1.0 / column_count as f64);
        return ColumnWidths {
            widths: vec![equal; column_count],
        };
    }
    let scale = (avail_width.get() / total.get()).min(1.0);
    let widths = max_widths
        .iter()
        .map(|w| Scalar(w.get() * scale))
        .collect();
    ColumnWidths { widths }
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
    let line_height = style.font_size * 1.2;
    line_height * (lines.len() as f64)
}
