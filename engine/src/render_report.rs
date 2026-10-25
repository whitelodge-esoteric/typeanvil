// SPDX-License-Identifier: AGPL-3.0-only

//! Render report (CORE-571): layout-outcome diagnostics mapped to source.
//!
//! Spec: `docs/specifications/render-report.spec.md`. The report is a pure
//! read of the FINAL layout (`Layout`) plus the DOM and its computed styles.
//! It runs only when `--diagnostics` is passed, after the PDF is written, so
//! a plain render pays nothing and its bytes never change.
//!
//! Determinism: every walk is in page order, then fragment pre-order, then
//! DOM order. No hash-map iteration feeds the output.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::css::{ComputedStyle, Display, Stylesheet};
use crate::diagnostics::{json_string, Diagnostic};
use crate::dom::{Dom, NodeId, NodeKind};
use crate::frag::{Fragment, FragmentContent, FragPart};
use crate::layout::Layout;
use crate::paged::ContentPiece;

/// Report schema version shared with the CSS diagnostics document.
pub const SCHEMA_VERSION: u32 = 2;

/// Tolerance (pt) before a box counts as overflowing.
const EPS: f64 = 0.5;

/// One layout-outcome event.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutEvent {
    /// Stable code, e.g. `"layout-overflow"`.
    pub code: &'static str,
    pub message: String,
    /// 1-based page number; `0` when the element produced no box.
    pub page: usize,
    /// Page-absolute border-box rect `[x, y, width, height]` in points
    /// (origin = page top-left). `None` when the element produced no box.
    pub rect: Option<[f64; 4]>,
    /// 1-based HTML source line / column of the element's start tag
    /// (`0, 0` when unknown).
    pub line: u32,
    pub column: u32,
    /// A CSS selector that matches the responsible element
    /// (`tag#id.class…`).
    pub element: String,
    /// Machine-readable fix, only where unambiguous.
    pub hint: Option<String>,
}

/// Summary of the render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub pages: usize,
    pub passes: usize,
    pub font_bundle: String,
}

/// The whole layout report.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderReport {
    pub events: Vec<LayoutEvent>,
    pub summary: Summary,
}

/// One fragment's absolute placement, collected by the walk.
struct Placed<'a> {
    frag: &'a Fragment,
    x: f64,
    y: f64,
    /// Nearest sourced ancestor-or-self.
    owner: Option<NodeId>,
}

/// Build the report from a finished layout.
pub fn analyze_layout(dom: &Dom, stylesheet: &Stylesheet, layout: &Layout) -> RenderReport {
    let styles = crate::css::cascade(dom, stylesheet, &layout.geometry);
    let mut events: Vec<LayoutEvent> = Vec::new();
    // Element → first (page, rect), from the fragment walk.
    let mut first_box: BTreeMap<NodeId, (usize, [f64; 4])> = BTreeMap::new();
    let mut pages_with_content: Vec<bool> = Vec::new();
    let split = split_sources(layout);

    for (pi, page) in layout.pages.iter().enumerate() {
        let mut placed = Vec::new();
        collect(&page.root, 0.0, 0.0, None, &mut placed);
        for p in &placed {
            if let Some(src) = p.frag.source {
                first_box
                    .entry(src)
                    .or_insert((pi + 1, rect_of(p)));
            }
        }
        pages_with_content.push(placed.iter().any(|p| paints(p.frag)));

        // layout-overflow: the deepest sourced box whose border box exceeds
        // the page content area in either axis.
        let cw = page.content_size.0.get();
        let ch = page.content_size.1.get();
        let mut seen = Vec::new();
        overflow_walk(&page.root, 0.0, 0.0, cw, ch, pi + 1, dom, &mut seen, &mut events);

        // table-header-only / table-single-row-fragment.
        table_events(dom, &styles, &placed, pi + 1, &split, &mut events);
    }

    // Page-level events need every page's content flag.
    let n = layout.pages.len();
    for pi in 0..n {
        if !pages_with_content[pi] {
            events.push(LayoutEvent {
                code: "empty-page",
                message: format!("page {} has no painted content", pi + 1),
                page: pi + 1,
                rect: None,
                line: 0,
                column: 0,
                element: String::new(),
                hint: None,
            });
        }
        if pi + 1 < n && pages_with_content[pi + 1] {
            if let Some(h) = orphaned_heading(dom, &layout.pages[pi].root) {
                let (line, column) = dom.source_pos(h);
                let tag = tag_of(dom, h);
                events.push(LayoutEvent {
                    code: "orphaned-heading",
                    message: format!(
                        "heading <{tag}> is the last content on page {} and its section continues on page {}",
                        pi + 1,
                        pi + 2
                    ),
                    page: pi + 1,
                    rect: first_box.get(&h).map(|b| b.1),
                    line,
                    column,
                    element: selector_of(dom, h),
                    hint: Some(format!("add `break-after: avoid` to {}", selector_of(dom, h))),
                });
            }
        }
    }

    // Element-scoped events in DOM order.
    let mut fallback_seen: Vec<String> = Vec::new();
    for id in 0..dom.nodes.len() {
        let Some(el) = dom.nodes[id].kind.element() else {
            continue;
        };
        let at = |id: NodeId| first_box.get(&id).copied();
        let mk = |code: &'static str, message: String, hint: Option<String>| {
            let (line, column) = dom.source_pos(id);
            let b = at(id);
            LayoutEvent {
                code,
                message,
                page: b.map(|b| b.0).unwrap_or(0),
                rect: b.map(|b| b.1),
                line,
                column,
                element: selector_of(dom, id),
                hint,
            }
        };
        // An inline box (image, link, generated content) carries no source of
        // its own: report the page of the nearest ancestor box, with no rect.
        let ancestor_page = |e: &mut LayoutEvent| {
            if e.page != 0 {
                return;
            }
            let mut a = dom.nodes[id].parent;
            while let Some(p) = a {
                if let Some(b) = first_box.get(&p) {
                    e.page = b.0;
                    return;
                }
                a = dom.nodes[p].parent;
            }
        };
        let style = styles.get(id);
        let displayed = style.is_some_and(|s| s.display != Display::None);

        if el.tag == "img" && displayed && layout.broken_images.contains(&id) {
            let src = el.attr("src").unwrap_or("");
            let mut e = mk(
                "image-missing",
                format!("image `{src}` could not be loaded; a placeholder was drawn"),
                None,
            );
            ancestor_page(&mut e);
            events.push(e);
        }

        if el.tag == "a" {
            if let Some(href) = el.attr("href") {
                if let Some(anchor) = href.strip_prefix('#') {
                    if !anchor.is_empty() && !id_exists(dom, anchor) {
                        let mut e = mk(
                            "link-target-missing",
                            format!("link target `#{anchor}` does not exist; the link was dropped"),
                            None,
                        );
                        if displayed {
                            ancestor_page(&mut e);
                        }
                        events.push(e);
                    }
                }
            }
        }

        if let Some(st) = style {
            for piece in &st.content {
                let attr = match piece {
                    ContentPiece::TargetCounter { attr, .. } => attr,
                    ContentPiece::TargetText { attr } => attr,
                    _ => continue,
                };
                let target = el.attr(attr).map(|v| v.trim_start_matches('#').to_string());
                let ok = target.as_deref().is_some_and(|t| !t.is_empty() && id_exists(dom, t));
                if !ok {
                    let shown = target.unwrap_or_default();
                    let mut e = mk(
                        "target-unresolved",
                        format!(
                            "`target-counter`/`target-text` target `#{shown}` (from attr({attr})) does not exist; `?` was drawn"
                        ),
                        None,
                    );
                    if displayed {
                        ancestor_page(&mut e);
                    }
                    events.push(e);
                }
            }

            if displayed && has_text_child(dom, id) {
                if let Some(family) = first_named_family(st) {
                    let key = family.to_ascii_lowercase();
                    if !fallback_seen.contains(&key) && !crate::fonts::family_available(&family) {
                        fallback_seen.push(key);
                        let resolved = crate::fonts::face_postscript_name(st.font_face);
                        events.push(mk(
                            "font-fallback",
                            format!(
                                "font family `{family}` is not available; text uses `{resolved}`"
                            ),
                            Some(format!(
                                "add an `@font-face` rule for `{family}` or install the font"
                            )),
                        ));
                    }
                }
            }
        }
    }

    // Stable sort by page (unknown page last); within a page the order of
    // the walks above is kept.
    events.sort_by_key(|e| if e.page == 0 { usize::MAX } else { e.page });

    RenderReport {
        events,
        summary: Summary {
            pages: layout.pages.len(),
            passes: layout.passes,
            font_bundle: crate::fonts::bundle_fingerprint(),
        },
    }
}

fn rect_of(p: &Placed) -> [f64; 4] {
    [
        round2(p.x),
        round2(p.y),
        round2(p.frag.size.0.get()),
        round2(p.frag.size.1.get()),
    ]
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn collect<'a>(f: &'a Fragment, px: f64, py: f64, owner: Option<NodeId>, out: &mut Vec<Placed<'a>>) {
    let x = px + f.offset.x.get();
    let y = py + f.offset.y.get();
    let owner = f.source.or(owner);
    out.push(Placed { frag: f, x, y, owner });
    for c in &f.children {
        collect(c, x, y, owner, out);
    }
}

fn paints(f: &Fragment) -> bool {
    !matches!(f.content, FragmentContent::None)
}

/// Report the deepest overflowing sourced box per subtree. Returns true when
/// this subtree reported an event.
#[allow(clippy::too_many_arguments)]
fn overflow_walk(
    f: &Fragment,
    px: f64,
    py: f64,
    cw: f64,
    ch: f64,
    page: usize,
    dom: &Dom,
    seen: &mut Vec<NodeId>,
    events: &mut Vec<LayoutEvent>,
) -> bool {
    let x = px + f.offset.x.get();
    let y = py + f.offset.y.get();
    let mut reported = false;
    for c in &f.children {
        reported |= overflow_walk(c, x, y, cw, ch, page, dom, seen, events);
    }
    if reported {
        return true;
    }
    let Some(src) = f.source else {
        return false;
    };
    if f.repeated || dom.nodes.get(src).and_then(|n| n.kind.element()).is_none() {
        return false;
    }
    let (w, h) = (f.size.0.get(), f.size.1.get());
    let wide = w > cw + EPS;
    let tall = h > ch + EPS;
    if !(wide || tall) || seen.contains(&src) {
        return false;
    }
    seen.push(src);
    let (line, column) = dom.source_pos(src);
    let axis = match (wide, tall) {
        (true, true) => format!("{w:.2}x{h:.2}pt exceeds the {cw:.2}x{ch:.2}pt content area"),
        (true, false) => format!("width {w:.2}pt exceeds the {cw:.2}pt content width"),
        _ => format!("height {h:.2}pt exceeds the {ch:.2}pt content height"),
    };
    events.push(LayoutEvent {
        code: "layout-overflow",
        message: format!("<{}> overflows page {page}: {axis}", tag_of(dom, src)),
        page,
        rect: Some([round2(x), round2(y), round2(w), round2(h)]),
        line,
        column,
        element: selector_of(dom, src),
        hint: None,
    });
    true
}

/// Table split checks for one page.
fn table_events(
    dom: &Dom,
    styles: &[ComputedStyle],
    placed: &[Placed],
    page: usize,
    split: &[NodeId],
    events: &mut Vec<LayoutEvent>,
) {
    let mut tables: Vec<NodeId> = Vec::new();
    for p in placed {
        if let Some(src) = p.frag.source {
            if styles.get(src).is_some_and(|s| s.display == Display::Table) && !tables.contains(&src) {
                tables.push(src);
            }
        }
    }
    for t in tables {
        // Rows of this table on this page: (row id, repeated chrome?).
        let mut body_rows: Vec<NodeId> = Vec::new();
        let mut repeated_rows = 0usize;
        let mut rect = None;
        for p in placed {
            if p.frag.source == Some(t) && rect.is_none() {
                rect = Some(rect_of(p));
            }
            let Some(src) = p.frag.source else { continue };
            if !styles.get(src).is_some_and(|s| s.display == Display::TableRow) {
                continue;
            }
            if nearest_table(dom, styles, src) != Some(t) {
                continue;
            }
            if p.frag.repeated || under_repeated(placed, p) {
                repeated_rows += 1;
            } else if !in_header_or_footer(dom, styles, src) && !body_rows.contains(&src) {
                body_rows.push(src);
            }
        }
        let on_pages = split.contains(&t);
        let (line, column) = dom.source_pos(t);
        let sel = selector_of(dom, t);
        if repeated_rows > 0 && body_rows.is_empty() {
            events.push(LayoutEvent {
                code: "table-header-only",
                message: format!("table header repeats on page {page} with no body rows"),
                page,
                rect,
                line,
                column,
                element: sel.clone(),
                hint: None,
            });
        } else if body_rows.len() == 1 && on_pages {
            events.push(LayoutEvent {
                code: "table-single-row-fragment",
                message: format!("table fragment on page {page} holds a single body row"),
                page,
                rect,
                line,
                column,
                element: sel,
                hint: None,
            });
        }
    }
}

fn under_repeated(placed: &[Placed], p: &Placed) -> bool {
    // A row inside a repeated header group: its group fragment is repeated.
    // `Placed` keeps pre-order, so find the nearest earlier fragment that
    // contains this one geometrically and is repeated — cheaper: walk the
    // group via pointer identity.
    placed.iter().any(|q| q.frag.repeated && contains_ptr(q.frag, p.frag))
}

fn contains_ptr(root: &Fragment, target: &Fragment) -> bool {
    if std::ptr::eq(root, target) {
        return true;
    }
    root.children.iter().any(|c| contains_ptr(c, target))
}

fn nearest_table(dom: &Dom, styles: &[ComputedStyle], mut id: NodeId) -> Option<NodeId> {
    while let Some(p) = dom.nodes[id].parent {
        if styles.get(p).is_some_and(|s| s.display == Display::Table) {
            return Some(p);
        }
        id = p;
    }
    None
}

fn in_header_or_footer(dom: &Dom, styles: &[ComputedStyle], id: NodeId) -> bool {
    dom.nodes[id].parent.is_some_and(|p| {
        styles.get(p).is_some_and(|s| {
            matches!(s.display, Display::TableHeaderGroup | Display::TableFooterGroup)
        })
    })
}

/// The heading that owns the LAST painted content on a page, if the last
/// content belongs to an `h1`–`h6` element. Footnote bodies, list markers
/// and repeated table chrome are skipped.
fn orphaned_heading(dom: &Dom, root: &Fragment) -> Option<NodeId> {
    let mut placed = Vec::new();
    collect(root, 0.0, 0.0, None, &mut placed);
    let last = placed.iter().rev().find(|p| {
        paints(p.frag) && p.frag.part == FragPart::Content && !p.frag.repeated
    })?;
    let mut id = last.owner?;
    loop {
        if let Some(el) = dom.nodes[id].kind.element() {
            if matches!(el.tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                return Some(id);
            }
        }
        id = dom.nodes[id].parent?;
    }
}

fn id_exists(dom: &Dom, anchor: &str) -> bool {
    dom.nodes.iter().any(|n| n.kind.element().is_some_and(|e| e.id.as_deref() == Some(anchor)))
}

fn has_text_child(dom: &Dom, id: NodeId) -> bool {
    dom.nodes[id]
        .children
        .iter()
        .any(|&c| matches!(&dom.nodes[c].kind, NodeKind::Text(t) if !t.trim().is_empty()))
}

/// The first family of the computed stack when it is a NAME (generic
/// families always resolve).
fn first_named_family(st: &ComputedStyle) -> Option<String> {
    let f = st.font_family.trim();
    let generic = matches!(
        f.to_ascii_lowercase().as_str(),
        "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" | "system-ui" | ""
    );
    if generic {
        None
    } else {
        Some(f.to_string())
    }
}

fn tag_of(dom: &Dom, id: NodeId) -> String {
    dom.nodes[id].kind.element().map(|e| e.tag.clone()).unwrap_or_default()
}

/// `tag#id.class1.class2` — a selector that matches the element.
pub fn selector_of(dom: &Dom, id: NodeId) -> String {
    let Some(el) = dom.nodes[id].kind.element() else {
        return String::new();
    };
    let mut s = el.tag.clone();
    if let Some(i) = &el.id {
        s.push('#');
        s.push_str(i);
    }
    for c in &el.classes {
        s.push('.');
        s.push_str(c);
    }
    s
}

/// Source nodes whose boxes appear on more than one page, ascending.
fn split_sources(layout: &Layout) -> Vec<NodeId> {
    let mut count: BTreeMap<NodeId, usize> = BTreeMap::new();
    for page in &layout.pages {
        let mut ids = Vec::new();
        sources(&page.root, &mut ids);
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            *count.entry(id).or_insert(0) += 1;
        }
    }
    count.into_iter().filter(|(_, n)| *n > 1).map(|(id, _)| id).collect()
}

fn sources(f: &Fragment, out: &mut Vec<NodeId>) {
    if let Some(s) = f.source {
        out.push(s);
    }
    for c in &f.children {
        sources(c, out);
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

/// One JSON object line for a layout event (schema 2).
fn event_json(e: &LayoutEvent) -> String {
    let mut s = format!(
        "{{\"code\": {}, \"severity\": \"warning\", \"message\": {}, \"line\": {}, \"column\": {}, \"source\": \"layout\", \"page\": {}, \"rect\": ",
        json_string(e.code),
        json_string(&e.message),
        e.line,
        e.column,
        e.page
    );
    match e.rect {
        Some(r) => {
            let _ = write!(
                s,
                "[{}, {}, {}, {}]",
                fmt_num(r[0]),
                fmt_num(r[1]),
                fmt_num(r[2]),
                fmt_num(r[3])
            );
        }
        None => s.push_str("null"),
    }
    let _ = write!(s, ", \"element\": {}", json_string(&e.element));
    if let Some(h) = &e.hint {
        let _ = write!(s, ", \"hint\": {}", json_string(h));
    }
    s.push('}');
    s
}

/// The full schema-2 document: stylesheet/markdown events first (exactly as
/// [`crate::diagnostics::to_json`] prints them), then layout events, then the
/// summary.
pub fn to_json(events: &[Diagnostic], report: &RenderReport) -> String {
    to_json_with(events, report, None)
}

/// [`to_json`] with an optional extra top-level member (CORE-573: the
/// `"assertions"` block, already indented, no trailing comma) placed after
/// `counts` and before `summary`.
pub fn to_json_with(events: &[Diagnostic], report: &RenderReport, extra: Option<&str>) -> String {
    let mut lines: Vec<String> = events.iter().map(crate::diagnostics::event_json).collect();
    lines.extend(report.events.iter().map(event_json));
    let mut out = String::new();
    let _ = write!(out, "{{\n  \"schema\": {SCHEMA_VERSION},\n  \"diagnostics\": [");
    if lines.is_empty() {
        out.push_str("],\n");
    } else {
        out.push('\n');
        let n = lines.len();
        for (i, l) in lines.iter().enumerate() {
            out.push_str("    ");
            out.push_str(l);
            out.push_str(if i + 1 == n { "\n" } else { ",\n" });
        }
        out.push_str("  ],\n");
    }
    let _ = writeln!(out, "  \"counts\": {{\"warnings\": {}}},", lines.len());
    if let Some(extra) = extra {
        out.push_str(extra);
        out.push_str(",\n");
    }
    let _ = writeln!(
        out,
        "  \"summary\": {{\"pages\": {}, \"passes\": {}, \"font_bundle\": {}}}",
        report.summary.pages,
        report.summary.passes,
        json_string(&report.summary.font_bundle)
    );
    out.push('}');
    out
}

/// Text-mode line for a layout event.
pub fn to_text(e: &LayoutEvent) -> String {
    format!(
        "{}:{} warning [{}]: {} (page {}, {})",
        e.line, e.column, e.code, e.message, e.page, e.element
    )
}
