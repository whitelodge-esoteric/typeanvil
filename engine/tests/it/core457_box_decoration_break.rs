//! `box-decoration-break` (CORE-457, css-break-3 §5.4).
//!
//! `slice` (initial): the top border paints on a box's first fragment and the
//! bottom border on its last. `clone`: every fragment paints both, and the
//! bottom chrome is reserved out of the fragment's limit. Tables follow the
//! same rule. Logical `border-block-start/end-width` map to top/bottom.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn lay(css: &str, body: &str) -> Layout {
    let html =
        format!(r#"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"#);
    let dom = Dom::parse(&html).expect("parse html");
    let mut sheet_css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                sheet_css.push_str(&dom.text_content(id));
                sheet_css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&sheet_css);
    let geometry = PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    };
    layout(&dom, &sheet, geometry)
}

/// (top, bottom) border widths of the first BorderBox with a non-zero side
/// on each page; `None` when the page paints no border.
fn page_borders(layout: &Layout) -> Vec<Option<(f64, f64)>> {
    fn walk(f: &Fragment) -> Option<(f64, f64)> {
        if let FragmentContent::Border(b) = &f.content {
            return Some((b.top.get(), b.bottom.get()));
        }
        f.children.iter().find_map(walk)
    }
    layout.pages.iter().map(|p| walk(&p.root)).collect()
}

const BODY: &str = r#"<div class="box"><div class="r"></div><div class="r"></div><div class="r"></div><div class="r"></div><div class="r"></div><div class="r"></div></div>"#;

fn block_css(extra: &str) -> String {
    format!(
        ".box {{ border: 4pt solid black; padding: 4pt; {extra} }} .r {{ height: 60pt; background: gold; }}"
    )
}

#[test]
fn slice_block_paints_top_on_first_and_bottom_on_last_only() {
    let l = lay(&block_css(""), BODY);
    let b = page_borders(&l);
    assert!(b.len() >= 3, "fixture must fragment, got {} pages", b.len());
    let first = b[0].unwrap();
    let last = b[b.len() - 1].unwrap();
    assert!(first.0 > 0.0 && first.1 == 0.0, "first: {first:?}");
    assert!(last.0 == 0.0 && last.1 > 0.0, "last: {last:?}");
}

#[test]
fn clone_block_paints_top_and_bottom_on_every_fragment() {
    let l = lay(&block_css("box-decoration-break: clone;"), BODY);
    let b = page_borders(&l);
    assert!(b.len() >= 3, "fixture must fragment, got {} pages", b.len());
    for (i, p) in b.iter().enumerate() {
        let (t, bt) = p.unwrap_or_else(|| panic!("page {i} has no border"));
        assert!(t > 0.0 && bt > 0.0, "page {i}: top {t} bottom {bt}");
    }
}

#[test]
fn clone_block_reserves_bottom_chrome_inside_the_page() {
    // The cloned bottom chrome (4pt border + 4pt padding) must fit inside
    // the content box on every page.
    let l = lay(&block_css("box-decoration-break: clone;"), BODY);
    for (i, page) in l.pages.iter().enumerate() {
        fn max_bottom(f: &Fragment, base: f64, out: &mut f64) {
            let y = base + f.offset.y.get();
            *out = out.max(y + f.size.1.get());
            for c in &f.children {
                max_bottom(c, y, out);
            }
        }
        let mut m = 0.0;
        for c in &page.root.children {
            max_bottom(c, 0.0, &mut m);
        }
        // Page coordinates: 36pt margin + 144pt content box = 180pt.
        assert!(m <= 180.0 + 0.5, "page {i} content reaches {m}pt (> 180)");
    }
}

#[test]
fn clone_table_paints_top_and_bottom_on_every_fragment() {
    let css = "table { border-collapse: separate; border-spacing: 0; border: 4pt solid black; \
               padding: 4pt; box-decoration-break: clone; } td { padding: 0; height: 60pt; }";
    let body = "<table><tbody><tr><td></td></tr><tr><td></td></tr><tr><td></td></tr>\
                <tr><td></td></tr><tr><td></td></tr><tr><td></td></tr></tbody></table>";
    let l = lay(css, body);
    let b = page_borders(&l);
    assert!(b.len() >= 3, "fixture must fragment, got {} pages", b.len());
    for (i, p) in b.iter().enumerate() {
        let (t, bt) = p.unwrap_or_else(|| panic!("page {i} has no border"));
        assert!(t > 0.0 && bt > 0.0, "page {i}: top {t} bottom {bt}");
    }
}

#[test]
fn slice_table_keeps_top_first_bottom_last() {
    let css = "table { border-collapse: separate; border-spacing: 0; border: 4pt solid black; \
               padding: 4pt; } td { padding: 0; height: 60pt; }";
    let body = "<table><tbody><tr><td></td></tr><tr><td></td></tr><tr><td></td></tr>\
                <tr><td></td></tr><tr><td></td></tr><tr><td></td></tr></tbody></table>";
    let b = page_borders(&lay(css, body));
    assert!(b.len() >= 3);
    assert!(b[0].unwrap().0 > 0.0 && b[0].unwrap().1 == 0.0);
    let last = b[b.len() - 1].unwrap();
    assert!(last.0 == 0.0 && last.1 > 0.0);
}

#[test]
fn logical_border_block_widths_map_to_top_and_bottom() {
    let l = lay(
        ".box { border: 0 solid black; border-block-start-width: 5pt; border-block-end-width: 7pt; } .r { height: 20pt; }",
        r#"<div class="box"><div class="r"></div></div>"#,
    );
    let b = page_borders(&l);
    assert_eq!(b.len(), 1);
    let (t, bt) = b[0].expect("border painted");
    assert!((t - 5.0).abs() < 0.01 && (bt - 7.0).abs() < 0.01, "got {t} {bt}");
}
