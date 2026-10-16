//! UA heading break defaults (CORE-370) — Prince 16.2 `html.css` declares
//! `h1..h6 { break-inside: avoid; break-after: avoid }`. See
//! `docs/specifications/ua-print-defaults.spec.md`, Behavior rule 10.
//!
//! Geometry: 5in × 3in page, 0.5in margins → 288pt × 144pt content box.
//! 12pt lines, zero margins, 132pt filler: exactly one line left on page 1.
//! No author break properties appear in any fixture except where stated.

use typeanvil::css::{cascade, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{BreakBetween, BreakInside, Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

fn style_text(dom: &Dom) -> String {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    css
}

fn lay(html: &str) -> Layout {
    let dom = Dom::parse(html).expect("parse html");
    let sheet = Stylesheet::parse(&style_text(&dom));
    layout(&dom, &sheet, geometry())
}

fn collect_text(frag: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &frag.content {
        out.push(run.text.clone());
    }
    for c in &frag.children {
        collect_text(c, out);
    }
}

fn pages(layout: &Layout) -> Vec<String> {
    layout
        .pages
        .iter()
        .map(|p| {
            let mut out = Vec::new();
            collect_text(&p.root, &mut out);
            out.join(" ")
        })
        .collect()
}

/// Zero margins and 12pt lines, but NO break properties and NO heading
/// font-size reset beyond what keeps the line grid: the heading rule under
/// test comes from the UA sheet alone.
const BASE_CSS: &str = "html, body { margin: 0; } \
    body { font-size: 12pt; line-height: 12pt; } \
    h1, h2, h3, h4, h5, h6, p, div { margin: 0; padding: 0; font-size: 12pt; line-height: 12pt; } \
    .fill { height: 132pt; }";

fn doc(css: &str, body: &str) -> String {
    format!(
        "<html><head><style>{BASE_CSS} {css}</style></head><body>\
         <div class=\"fill\">FILL</div>{body}</body></html>"
    )
}

fn assert_heading_moved(got: &[String], what: &str) {
    assert_eq!(got.len(), 2, "{what}: expected two pages, got {got:?}");
    assert!(
        !got[0].contains("HEADING"),
        "{what}: heading must not strand on page 1: {got:?}"
    );
    assert!(
        got[1].contains("HEADING") && got[1].contains("NEXT"),
        "{what}: page 2 holds heading and next sibling: {got:?}"
    );
}

#[test]
fn ua_heading_computes_break_avoid() {
    let dom = Dom::parse("<html><body><h1>a</h1><h3>b</h3><h6>c</h6><p>d</p></body></html>")
        .expect("parse html");
    let sheet = Stylesheet::parse("");
    let styles = cascade(&dom, &sheet, &geometry());
    for tag in ["h1", "h3", "h6"] {
        let id = dom.find_tag(tag).expect(tag);
        assert_eq!(styles[id].break_after, BreakBetween::Avoid, "{tag} break-after");
        assert_eq!(styles[id].break_inside, BreakInside::Avoid, "{tag} break-inside");
    }
    let p = dom.find_tag("p").expect("p");
    assert_eq!(styles[p].break_after, BreakBetween::Auto, "p is unaffected");
}

#[test]
fn unstyled_heading_moves_with_paragraph() {
    let got = pages(&lay(&doc("", "<h2>HEADING</h2><p>NEXT para.</p>")));
    assert_heading_moved(&got, "unstyled h2 + p");
}

#[test]
fn unstyled_heading_moves_with_table() {
    let css = "table { border-collapse: collapse; } td { padding: 0; font-size: 12pt; line-height: 12pt; }";
    let got = pages(&lay(&doc(
        css,
        "<h2>HEADING</h2><table><tbody><tr><td>NEXT</td></tr></tbody></table>",
    )));
    assert_heading_moved(&got, "unstyled h2 + table");
}

#[test]
fn author_break_after_auto_overrides_ua_rule() {
    let got = pages(&lay(&doc(
        "h2 { break-after: auto; }",
        "<h2>HEADING</h2><p>NEXT para.</p>",
    )));
    assert_eq!(got.len(), 2, "two pages, got {got:?}");
    assert!(
        got[0].contains("HEADING") && !got[0].contains("NEXT"),
        "author auto restores the strand: {got:?}"
    );
}

#[test]
fn non_heading_is_unaffected() {
    let got = pages(&lay(&doc("", "<div>HEADING</div><p>NEXT para.</p>")));
    assert_eq!(got.len(), 2, "two pages, got {got:?}");
    assert!(got[0].contains("HEADING"), "a div does not avoid: {got:?}");
}
