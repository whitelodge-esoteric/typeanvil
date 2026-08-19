//! Out-of-flow positioning acceptance tests — one per acceptance criterion in
//! `docs/specifications/out-of-flow-positioning.spec.md` (CORE-64).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const EPS: f64 = 1e-6;

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

fn style_text(html: &str) -> String {
    html.split("<style>")
        .nth(1)
        .and_then(|s| s.split("</style>").next())
        .unwrap_or("")
        .to_string()
}

fn dom_of(html: &str) -> Dom {
    Dom::parse(html).expect("valid html")
}

fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = dom_of(html);
    let ss = Stylesheet::parse(&style_text(html));
    layout(&dom, &ss, geo)
}

fn node_id_by_class(dom: &Dom, class: &str) -> NodeId {
    dom.nodes
        .iter()
        .position(|n| {
            matches!(&n.kind, NodeKind::Element(el) if el.classes.iter().any(|c| c == class))
        })
        .expect("element with class must exist") as NodeId
}

/// Fragments whose `source` is the given node (the abspos boxes).
fn find_source<'a>(frag: &'a Fragment, id: NodeId, out: &mut Vec<&'a Fragment>) {
    if frag.source == Some(id) {
        out.push(frag);
    }
    for c in &frag.children {
        find_source(c, id, out);
    }
}

fn page_abspos_fragments<'a>(layout: &'a Layout, page: usize, id: NodeId) -> Vec<&'a Fragment> {
    let mut out = Vec::new();
    find_source(&layout.pages[page].root, id, &mut out);
    out
}

/// Collect (x, y) of all line fragments in a fragment tree.
fn collect_lines(frag: &Fragment, out: &mut Vec<(Scalar, Scalar)>) {
    if let typeanvil::frag::FragmentContent::Text(_) = &frag.content {
        out.push((frag.offset.x, frag.offset.y));
    }
    for c in &frag.children {
        collect_lines(c, out);
    }
}

fn page_lines(layout: &Layout, page: usize) -> Vec<(Scalar, Scalar)> {
    let mut out = Vec::new();
    collect_lines(&layout.pages[page].root, &mut out);
    out
}

fn assert_close(a: Scalar, b: Scalar, msg: &str) {
    assert!(
        (a.get() - b.get()).abs() < EPS,
        "{msg}: got {} want {}",
        a.get(),
        b.get()
    );
}

// --- 1. Done criterion: abspos lands on its containing block's page --------

#[test]
fn absolute_lands_on_containing_block_page() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .cb { position: relative; break-before: page; }
        .a { position: absolute; top: 20pt; left: 10pt; }
    </style></head>
    <body>
        <p>Filler filler filler filler filler filler filler filler filler filler
        filler filler filler filler filler filler filler filler filler filler
        filler filler filler filler filler filler filler filler filler filler.</p>
        <div class="cb"><div class="a">ABS</div></div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "document must paginate to at least 2 pages");

    // Page 1: the filler's page — no abspos fragment.
    assert!(
        page_abspos_fragments(&layout, 0, aid).is_empty(),
        "no abspos fragment on page 1"
    );
    // Page 2: the containing block starts at the content top; the abspos sits
    // 10pt right / 20pt down from its padding-box origin.
    let frags = page_abspos_fragments(&layout, 1, aid);
    assert!(
        !frags.is_empty(),
        "abspos fragment lands on the containing block's page"
    );
    assert_close(frags[0].offset.x, Scalar(10.0), "x = cb.x + left (10pt)");
    assert_close(frags[0].offset.y, Scalar(20.0), "y = cb.y + top (20pt)");
}

// --- 2. Offsets resolve from the containing block's padding edge -----------

#[test]
fn offsets_from_cb_padding_edge() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .cb { position: relative; padding: 12pt; }
        .a { position: absolute; top: 0; left: 0; }
    </style></head>
    <body>
        <div class="cb"><div class="a">ABS</div><p>Body text.</p></div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_abspos_fragments(&layout, 0, aid);
    assert!(!frags.is_empty(), "abspos fragment placed");
    // cb padding-box origin = content origin + padding (12pt, 0pt).
    assert_close(frags[0].offset.x, Scalar(12.0), "x = cb padding-box origin (padding-left 12pt)");
    assert_close(frags[0].offset.y, Scalar(0.0), "y = cb padding-box top");
}

// --- 3. Nearest positioned ancestor wins ------------------------------------

#[test]
fn nearest_positioned_ancestor_wins() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .outer { position: relative; }
        .inner { position: relative; padding: 8pt; }
        .a { position: absolute; top: 0; left: 0; }
    </style></head>
    <body>
        <div class="outer"><div class="inner"><div class="a">ABS</div></div></div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_abspos_fragments(&layout, 0, aid);
    assert!(!frags.is_empty(), "abspos fragment placed");
    // The inner (not outer) is the containing block: offset = inner's padding.
    assert_close(frags[0].offset.x, Scalar(8.0), "x = inner padding-box origin (8pt)");
    assert_close(frags[0].offset.y, Scalar(0.0), "y = inner padding-box top");
}

// --- 4. right/bottom insets resolve against the containing block -----------

#[test]
fn right_and_bottom_offsets() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .r { position: absolute; top: 0; right: 0; width: 1in; }
        .b { position: absolute; left: 0; bottom: 0; }
    </style></head>
    <body>
        <div class="r">R</div>
        <div class="b">B</div>
        <p>Body text that should not be pushed.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let rid = node_id_by_class(&dom, "r");
    let bid = node_id_by_class(&dom, "b");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    // Initial containing block = page content box (36,36) size (288,144).
    let r = page_abspos_fragments(&layout, 0, rid);
    assert!(!r.is_empty(), "right-inset fragment placed");
    assert_close(r[0].offset.x, Scalar(288.0 - 72.0), "right:0 flushes to the CB's right edge");
    assert_close(r[0].offset.y, Scalar(0.0), "top:0 keeps the CB's top");
    let b = page_abspos_fragments(&layout, 0, bid);
    assert!(!b.is_empty(), "bottom-inset fragment placed");
    // fh = one 12pt line at line-height 1.2 = 14.4pt; bottom:0 flushes to the
    // fragmentainer content bottom (the CB-height approximation).
    assert_close(b[0].offset.x, Scalar(0.0), "left:0 keeps the CB's left");
    assert_close(b[0].offset.y, Scalar(144.0 - 14.4), "bottom:0 flushes to the content bottom");
}

// --- 5. No cursor advance, no in-flow height ---------------------------------

#[test]
fn out_of_flow_does_not_advance_cursor() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .a { position: absolute; top: 0; left: 0; }
    </style></head>
    <body>
        <div class="a">ABS</div>
        <p>Body text starts at the top of the page.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_abspos_fragments(&layout, 0, aid);
    assert!(!frags.is_empty(), "abspos fragment placed");
    assert_close(frags[0].offset.x, Scalar(0.0), "static x = CB origin");
    assert_close(frags[0].offset.y, Scalar(0.0), "static y = CB origin");
    // The paragraph still starts at the very top of the content area.
    let lines = page_lines(&layout, 0);
    assert!(!lines.is_empty(), "paragraph produced lines");
    assert_close(lines[0].1, Scalar(0.0), "following text starts at the same y (no cursor advance)");
}

// --- 6. Fixed anchors to the page box ---------------------------------------

#[test]
fn fixed_anchors_to_page() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .cb { position: relative; margin-left: 1in; }
        .f { position: fixed; top: 0; left: 0; }
    </style></head>
    <body>
        <div class="cb"><div class="f">F</div><p>Body.</p></div>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_abspos_fragments(&layout, 0, fid);
    assert!(!frags.is_empty(), "fixed fragment placed");
    // Fixed ignores the relative containing block (1in margin) — the page
    // content box origin is (0,0) relative to the body.
    assert_close(frags[0].offset.x, Scalar(0.0), "fixed x = page content origin, not the CB");
    assert_close(frags[0].offset.y, Scalar(0.0), "fixed y = page content origin");
}

// --- 7. No position -> unchanged ---------------------------------------------

#[test]
fn no_position_unchanged() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
    </style></head>
    <body>
        <p>Just a paragraph, no positioning at all.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(layout.pages.len(), 1, "single page");
}

// --- 8. Determinism -----------------------------------------------------------

const FIXTURE: &str = r#"<html><head><style>
    body { margin: 0; font-size: 12pt; line-height: 1.2; }
    p, div { margin: 0; }
    .cb { position: relative; }
    .a { position: absolute; top: 10pt; right: 0; width: 1in; }
</style></head>
<body>
    <div class="cb"><div class="a">ABS</div></div>
    <p>Filler filler filler filler filler filler filler filler filler filler
    filler filler filler filler filler filler filler filler filler filler.</p>
</body></html>"#;

#[test]
fn output_is_deterministic_with_abspos() {
    let dir = tempfile::tempdir().unwrap();
    let html = dir.path().join("abspos.html");
    std::fs::write(&html, FIXTURE).unwrap();

    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render_cli(&html, &a, "5in", "3in");
    render_cli(&html, &b, "5in", "3in");

    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert_eq!(ba, bb, "PDF output is not byte-identical across runs");
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typeanvil")
}

fn render_cli(html: &Path, out: &Path, w: &str, h: &str) {
    let status = Command::new(bin())
        .args([
            "render",
            html.to_str().unwrap(),
            "--page-width",
            w,
            "--page-height",
            h,
            "--margin-top",
            "0.5in",
            "--margin-right",
            "0.5in",
            "--margin-bottom",
            "0.5in",
            "--margin-left",
            "0.5in",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("failed to spawn typeanvil");
    assert!(status.success(), "typeanvil render failed");
}
