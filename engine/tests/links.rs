//! Hyperlink annotation acceptance tests — one per acceptance criterion in
//! `docs/specifications/hyperlinks.spec.md` (CORE-104).
//!
//! Rect/annotation assertions read `Layout.links` (the same records pdf.rs
//! emits as krilla annotations); determinism compares rendered PDF bytes.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, LinkTarget};

// --- helpers (mirror tests/fonts.rs) ----------------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    Stylesheet::parse(&css)
}

fn parse_html(html: &str) -> Dom {
    Dom::parse(html).expect("parse")
}

fn render_bytes(dom: &Dom, geo: PageGeometry) -> Vec<u8> {
    let sheet = stylesheet_of(dom);
    let lay = layout(dom, &sheet, geo);
    typeanvil::pdf::render(&lay).expect("render")
}

const BASE_CSS: &str = "body { margin: 0 } p { margin: 0 }";

// --- AC 1: external link emits a URI annotation ------------------------------

#[test]
fn external_link_emits_uri_annotation() {
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <p>See <a href=\"https://example.com/\">Example</a> here.</p></body></html>"
    );
    let dom = parse_html(&html);
    let dom = &dom;
    let sheet = stylesheet_of(dom);
    let lay = layout(dom, &sheet, geometry(6.5, 9.0, 1.0));
    assert_eq!(lay.links.len(), 1, "exactly one link rect");
    let pl = &lay.links[0];
    match &pl.target {
        LinkTarget::Url(u) => assert_eq!(u, "https://example.com/"),
        LinkTarget::Page(_) => panic!("external href must not resolve to a page target"),
    }
    // The rect covers only the link glyphs: narrower than the full text width,
    // positive area, inside the content box.
    assert!(pl.w.get() > 10.0 && pl.w.get() < 300.0, "rect w={:?}", pl.w);
    assert!(pl.h.get() > 0.0);
    assert_eq!(pl.page_index, 0);
}

// --- AC 2: internal cross-page link resolves to the target page --------------

#[test]
fn internal_link_targets_correct_page() {
    let html = format!(
        "<html><head><style>{BASE_CSS} .sec {{ break-before: page }}</style></head><body>\
         <p><a href=\"#sec\">Jump</a></p>\
         <h2 id=\"sec\" class=\"sec\">Section</h2></body></html>"
    );
    let dom = parse_html(&html);
    let dom = &dom;
    let sheet = stylesheet_of(dom);
    let lay = layout(dom, &sheet, geometry(6.5, 4.0, 0.75));
    assert!(!lay.links.is_empty(), "internal link rect present");
    for pl in &lay.links {
        match &pl.target {
            LinkTarget::Page(p) => assert_eq!(*p, 1, "target lands on page 2 (index 1)"),
            LinkTarget::Url(_) => panic!("#fragment must resolve to a page target"),
        }
    }
}

// --- AC 3: multi-line link produces one rect per line ------------------------

#[test]
fn multi_line_link_has_rect_per_line() {
    // Long text in a narrow page (3in − 1.5in margins = 108pt column) wraps
    // the link across multiple lines.
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <p><a href=\"https://example.com/wrap\">the quick brown fox jumps over \
         the lazy dog again and again many many words</a></p></body></html>"
    );
    let dom = parse_html(&html);
    let dom = &dom;
    let sheet = stylesheet_of(dom);
    let lay = layout(dom, &sheet, geometry(3.0, 9.0, 0.75));
    assert!(
        lay.links.len() >= 2,
        "expected one rect per wrapped line, got {}",
        lay.links.len()
    );
    // All rects share the same target and have disjoint y ranges.
    for pl in &lay.links {
        match &pl.target {
            LinkTarget::Url(u) => assert_eq!(u, "https://example.com/wrap"),
            LinkTarget::Page(_) => panic!("wrong target kind"),
        }
    }
    let mut ys: Vec<f64> = lay.links.iter().map(|l| l.y.get()).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for pair in ys.windows(2) {
        assert!(pair[1] - pair[0] > 1.0, "rects on distinct lines");
    }
}

// --- AC 4: link across a page break yields rects on both pages ---------------

#[test]
fn link_across_page_break_rects_on_both_pages() {
    // Many linked lines forced to break by a short page.
    let words: Vec<&str> = vec!["lorem"; 400];
    let joined = words.join(" ");
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <p><a href=\"https://example.com/\">{joined}</a></p></body></html>"
    );
    let dom = parse_html(&html);
    let dom = &dom;
    let sheet = stylesheet_of(dom);
    let lay = layout(dom, &sheet, geometry(6.5, 2.0, 0.5));
    let pages_with_links: std::collections::BTreeSet<usize> =
        lay.links.iter().map(|l| l.page_index).collect();
    assert!(
        pages_with_links.len() >= 2,
        "link spans multiple pages, got {pages_with_links:?}"
    );
    for pl in &lay.links {
        match &pl.target {
            LinkTarget::Url(u) => assert_eq!(u, "https://example.com/"),
            other => panic!("wrong target {other:?}"),
        }
    }
    let _ = html;
}

// --- AC 5: dangling fragment emits nothing -----------------------------------

#[test]
fn dangling_fragment_emits_no_annotation() {
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <p><a href=\"#missing\">Nowhere</a></p></body></html>"
    );
    let dom = parse_html(&html);
    let dom = &dom;
    let sheet = stylesheet_of(dom);
    let lay = layout(dom, &sheet, geometry(6.5, 9.0, 1.0));
    assert!(lay.links.is_empty(), "dangling fragment drops the annotation");
}

// --- AC 6: determinism -------------------------------------------------------

#[test]
fn link_render_is_deterministic() {
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <p><a href=\"https://example.com/\">Example</a> \
         <a href=\"#toc\">TOC</a></p>\
         <h1 id=\"toc\" style=\"break-before:page\">Contents</h1></body></html>"
    );
    let dom = parse_html(&html);
    let a = render_bytes(&dom, geometry(6.5, 3.0, 0.75));
    let b = render_bytes(&dom, geometry(6.5, 3.0, 0.75));
    assert_eq!(a, b, "identical input must render byte-identical PDFs");
}
