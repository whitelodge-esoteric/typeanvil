//! CORE-128 acceptance tests: PDF outlines and bookmarks.
//!
//! Covers: UA defaults (h1-h6 -> levels 1-6, label contents()), explicit
//! bookmark-level/label/state, destination y-offsets, and broken-anchor
//! diagnostics. Run with `cargo test -p typeanvil --test bookmarks`.

use typeanvil::css::{BookmarkLevel, ComputedStyle, Stylesheet};
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::geom::PageGeometry;
use typeanvil::layout::{layout, Heading};

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    let i = 72.0;
    PageGeometry {
        width: typeanvil::geom::Scalar(w_in * i),
        height: typeanvil::geom::Scalar(h_in * i),
        margin_top: typeanvil::geom::Scalar(margin_in * i),
        margin_right: typeanvil::geom::Scalar(margin_in * i),
        margin_bottom: typeanvil::geom::Scalar(margin_in * i),
        margin_left: typeanvil::geom::Scalar(margin_in * i),
    }
}

fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
}

fn computed_bookmark_level(dom: &Dom, id: NodeId, styles: &[ComputedStyle]) -> BookmarkLevel {
    styles[id].bookmark_level
}

fn titles(headings: &[Heading]) -> Vec<&str> {
    headings.iter().map(|h| h.title.as_str()).collect()
}

fn levels(headings: &[Heading]) -> Vec<u8> {
    headings.iter().map(|h| h.level).collect()
}

fn lay(html: &str) -> typeanvil::layout::Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geometry(5.0, 3.0, 0.4))
}

const NESTED: &str = r#"<html><head><style>
    .ch { break-before: page; }
    h1 { font-size: 16px; }
    h2 { font-size: 13px; }
    p { font-size: 12px; }
</style></head><body>
    <div class="ch"><h1>Alpha</h1><h2>Alpha One</h2><h2>Alpha Two</h2></div>
    <div class="ch"><h1>Beta</h1><h2>Beta One</h2></div>
</body></html>"#;

/// UA defaults: h1/h2 produce bookmark entries at levels 1/2 in document
/// order with contents() labels; non-headings produce nothing.
#[test]
fn ua_defaults_levels_and_labels() {
    let l = lay(NESTED);
    assert_eq!(levels(&l.headings), vec![1, 2, 2, 1, 2]);
    assert_eq!(
        titles(&l.headings),
        vec!["Alpha", "Alpha One", "Alpha Two", "Beta", "Beta One"]
    );
    // All entries resolved (y anchors present).
    assert!(l.headings.iter().all(|h| h.y.is_some()));
    // Default state open.
    assert!(l.headings.iter().all(|h| h.state_open));
}

/// Explicit `bookmark-level: none` suppresses an entry (h2 keeps its level
/// on siblings).
#[test]
fn bookmark_level_none_suppresses() {
    let html = r#"<html><head><style>
        h1 { font-size: 16px; }
        h2 { font-size: 13px; }
        h2.skip { bookmark-level: none; }
    </style></head><body>
        <h1>Alpha</h1><h2 class="skip">Skipped</h2><h2>Kept</h2>
    </body></html>"#;
    let l = lay(html);
    assert_eq!(titles(&l.headings), vec!["Alpha", "Kept"]);
    assert_eq!(levels(&l.headings), vec![1, 2]);
}

/// A non-heading element with an explicit bookmark-level gets an entry;
/// an explicit level overrides the UA heading default.
#[test]
fn explicit_level_overrides() {
    let html = r#"<html><head><style>
        h1 { font-size: 16px; }
        h2 { font-size: 13px; bookmark-level: 1; }
        .extra { bookmark-level: 2; }
    </style></head><body>
        <h1>Alpha</h1><h2>Sub as level 1</h2><div class="extra">Extra</div>
    </body></html>"#;
    let l = lay(html);
    assert_eq!(levels(&l.headings), vec![1, 1, 2]);
    assert_eq!(titles(&l.headings), vec!["Alpha", "Sub as level 1", "Extra"]);
}

/// bookmark-label: literal + counter(name) resolves against the counter
/// state at the element's box start. One reset before both h2s: increments
/// accumulate 1, 2 across the document.
#[test]
fn label_literal_and_counter() {
    let html = r#"<html><head><style>
        h1 { font-size: 16px; }
        h2 { font-size: 13px; bookmark-label: "Chapter " counter(chapter); counter-increment: chapter; }
        .first { counter-reset: chapter 0; }
    </style></head><body>
        <div class="first"><h1>Alpha</h1><h2>One</h2></div>
        <div><h1>Beta</h1><h2>Two</h2></div>
    </body></html>"#;
    let l = lay(html);
    assert_eq!(
        titles(&l.headings),
        vec!["Alpha", "Chapter 1", "Beta", "Chapter 2"]
    );
}

/// bookmark-state: closed on a parent, open on another — the Heading
/// carries it so the PDF emitter writes negative/positive /Count.
#[test]
fn state_open_closed() {
    let html = r#"<html><head><style>
        h1 { font-size: 16px; }
        h2 { font-size: 13px; }
        h1.c { bookmark-state: closed; }
    </style></head><body>
        <h1 class="c">Closed</h1><h2>Hidden</h2>
        <h1>Open</h1><h2>Shown</h2>
    </body></html>"#;
    let l = lay(html);
    assert_eq!(levels(&l.headings), vec![1, 2, 1, 2]);
    assert_eq!(
        l.headings.iter().map(|h| h.state_open).collect::<Vec<_>>(),
        vec![false, true, true, true]
    );
}

/// An element with a bookmark declaration that produces no fragment is
/// dropped from the outline (spec Behavior 8: no invalid destination ever).
#[test]
fn bookmark_anchor_unresolved_drops_entry() {
    let html = r#"<html><head><style>
        h1 { font-size: 16px; }
        h2 { font-size: 13px; }
        h2.gone { bookmark-level: 2; }
        h2.gone { display: none; }
    </style></head><body>
        <h1>Alpha</h1><h2 class="gone">Nowhere</h2><h2>Kept</h2>
    </body></html>"#;
    let l = lay(html);
    assert_eq!(titles(&l.headings), vec!["Alpha", "Kept"]);
}

/// The computed bookmark-level flows through the cascade: UA defaults give
/// h1 level 1; an author rule wins over the UA rule.
#[test]
fn cascade_author_beats_ua() {
    let html = r#"<html><head><style>
        h1 { bookmark-level: 3; }
        h2 { font-size: 13px; }
    </style></head><body><h1>Deep</h1></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    let styles = typeanvil::css::cascade(&dom, &ss, &geometry(5.0, 3.0, 0.4));
    let h1 = dom
        .nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::Element(e) if e.tag == "h1"))
        .unwrap();
    assert_eq!(
        computed_bookmark_level(&dom, h1, &styles),
        BookmarkLevel::Level(3)
    );
}

/// Entries after a h1 on a later page carry correct page indexes (multi-page).
#[test]
fn multipage_page_indexes() {
    let l = lay(NESTED);
    // Beta (index 3) starts its own page via break-before.
    assert!(l.headings[3].page_index > l.headings[0].page_index);
    assert!(l.headings[0].page_index == 0);
}

/// A single-level document still emits entries (level 1 roots only).
#[test]
fn flat_document() {
    let html = r#"<html><head><style>h1 { font-size: 16px; }</style></head><body>
        <h1>One</h1><p>x</p><h1>Two</h1><p>y</p>
    </body></html>"#;
    let l = lay(html);
    assert_eq!(levels(&l.headings), vec![1, 1]);
}
