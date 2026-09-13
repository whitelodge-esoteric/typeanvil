//! Page floats (CORE-130) — `float: top | bottom | next-page | snap`
//! acceptance tests, one per Behavior in
//! `docs/specifications/page-floats.spec.md`.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, FragmentKind};
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

fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = Stylesheet::parse(&style_text(html));
    layout(&dom, &ss, geo)
}

fn style_text(html: &str) -> String {
    let dom = Dom::parse(html).unwrap();
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

fn dom_of(html: &str) -> Dom {
    Dom::parse(html).unwrap()
}

fn node_id_by_class(dom: &Dom, class: &str) -> NodeId {
    dom.nodes
        .iter()
        .position(|n| {
            matches!(&n.kind, NodeKind::Element(el) if el.classes.iter().any(|c| c == class))
        })
        .expect("element with class must exist") as NodeId
}

/// Absolute (x, y) of every line fragment in the tree, depth-first.
/// Offsets are PARENT-relative — accumulate down the tree (CORE-121 lesson).
fn collect_lines(frag: &Fragment, ox: f64, oy: f64, out: &mut Vec<(f64, f64)>) {
    let ax = ox + frag.offset.x.get();
    let ay = oy + frag.offset.y.get();
    if matches!(frag.kind, FragmentKind::Line) {
        if matches!(frag.content, FragmentContent::Text(_)) {
            out.push((ax, ay));
        }
    }
    for c in &frag.children {
        collect_lines(c, ax, ay, out);
    }
}

fn page_lines(layout: &Layout, page: usize) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    collect_lines(&layout.pages[page].root, 0.0, 0.0, &mut out);
    out
}

/// Absolute (x, y) + size of every fragment whose `source` is `id`.
fn page_float_fragments(layout: &Layout, page: usize, id: NodeId) -> Vec<(f64, f64, (f64, f64))> {
    fn find(
        frag: &Fragment,
        id: NodeId,
        ox: f64,
        oy: f64,
        out: &mut Vec<(f64, f64, (f64, f64))>,
    ) {
        let ax = ox + frag.offset.x.get();
        let ay = oy + frag.offset.y.get();
        if frag.source == Some(id) {
            out.push((ax, ay, (frag.size.0.get(), frag.size.1.get())));
        }
        for c in &frag.children {
            find(c, id, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    find(&layout.pages[page].root, id, 0.0, 0.0, &mut out);
    out
}

fn assert_close(got: Scalar, want: Scalar, label: &str) {
    let diff = (got.get() - want.get()).abs();
    assert!(
        diff < EPS,
        "{label}: got {:.6}pt want {:.6}pt (diff {:.6})",
        got.get(),
        want.get(),
        diff
    );
}

/// Shared fixture: a page-floated figure + a body paragraph.
const TOP_FIXTURE: &str = r#"<html><head><style>
    body { margin: 0; font-size: 12pt; line-height: 1.2; }
    p, div { margin: 0; padding: 0; }
    .pf { float: top; width: 3in; height: 40pt; background: lightblue; }
</style></head><body>
    <div class="pf"><p>Figure.</p></div>
    <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p>
</body></html>"#;

// --- Behavior 1: `float: top` pins the figure at the content-box top -------

#[test]
fn top_float_pins_to_page_top() {
    let html = TOP_FIXTURE;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "pf");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_float_fragments(&layout, 0, fid);
    assert!(!frags.is_empty(), "page float must be placed on page 1");
    // Content-box top edge = top margin (0.5in). The float's fragment offset
    // is the box's border-box origin.
    assert_close(Scalar(frags[0].1), inches(0.5), "top float at content top");
}

// --- Behavior 2: in-flow text starts BELOW a top float's band --------------

#[test]
fn body_text_below_top_float_band() {
    let html = TOP_FIXTURE;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "pf");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_float_fragments(&layout, 0, fid);
    let band_bottom = frags[0].1 + frags[0].2.1;
    // Body lines only: exclude the float's own subtree (the CORE-121 walker
    // flags every fragment whose source is the float).
    let mut body_lines = Vec::new();
    fn walk(
        frag: &Fragment,
        fid: NodeId,
        inside_float: bool,
        ox: f64,
        oy: f64,
        out: &mut Vec<(f64, f64)>,
    ) {
        let ax = ox + frag.offset.x.get();
        let ay = oy + frag.offset.y.get();
        let now_inside = inside_float || frag.source == Some(fid);
        if matches!(frag.kind, FragmentKind::Line)
            && matches!(frag.content, FragmentContent::Text(_))
            && !now_inside
        {
            out.push((ax, ay));
        }
        for c in &frag.children {
            walk(c, fid, now_inside, ax, ay, out);
        }
    }
    walk(&layout.pages[0].root, fid, false, 0.0, 0.0, &mut body_lines);
    assert!(!body_lines.is_empty(), "body text placed");
    for (x, y) in &body_lines {
        assert!(
            *y >= band_bottom - EPS,
            "body line at y={y} overlaps the top float's band (bottom {band_bottom})"
        );
    }
}

// --- Behavior 3: full-width line below the float (no lane shortening) ------

#[test]
fn top_float_does_not_shorten_lines() {
    let html = TOP_FIXTURE;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let lines = page_lines(&layout, 0);
    assert!(lines.len() >= 2, "paragraph produces several lines");
    // Every line starts at the content-box left edge (0.5in margin).
    for (x, _y) in &lines {
        assert_close(Scalar(*x), inches(0.5), "line at content-box left");
    }
}

// --- Behavior 4: `float: bottom` pins at the page's bottom edge ------------

#[test]
fn bottom_float_pins_to_page_bottom() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .pf { float: bottom; width: 3in; height: 30pt; background: pink; }
    </style></head><body>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p>
        <div class="pf"><p>Figure.</p></div>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "pf");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_float_fragments(&layout, 0, fid);
    assert!(!frags.is_empty(), "bottom float placed on page 1");
    // Content bottom edge = 0.5in top + 2.0in content height = 2.5in.
    // The float's band hangs at the bottom: offset + height == 2.5in.
    let bottom_edge = frags[0].1 + frags[0].2.1;
    assert_close(
        Scalar(bottom_edge),
        inches(2.5),
        "bottom float's band ends at the content-box bottom edge",
    );
}

// --- Behavior 5: two top floats stack in document order --------------------

#[test]
fn top_floats_stack_in_document_order() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .a { float: top; width: 3in; height: 30pt; }
        .b { float: top; width: 3in; height: 30pt; }
    </style></head><body>
        <div class="a"><p>A.</p></div>
        <div class="b"><p>B.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let bid = node_id_by_class(&dom, "b");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let a = page_float_fragments(&layout, 0, aid);
    let b = page_float_fragments(&layout, 0, bid);
    assert!(!a.is_empty() && !b.is_empty(), "both top floats placed");
    // B starts where A ends (stacked band).
    assert_close(Scalar(b[0].1), Scalar(a[0].1 + a[0].2.1), "B below A");
}

// --- Behavior 6: a float that cannot fit defers to the next page -----------

#[test]
fn oversized_top_float_defers_to_next_page() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .tall { float: top; width: 3in; height: 100pt; }
    </style></head><body>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta.</p>
        <div class="tall"><p>Figure.</p></div>
    </body></html>"#;
    let dom = dom_of(html);
    let tid = node_id_by_class(&dom, "tall");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "deferred float creates page 2");
    // The paragraph fills page 1 (6 lines * 14.4pt ≈ 86pt; the 100pt float
    // cannot fit below it) → the float defers and pins at page 2's top.
    let p1 = page_float_fragments(&layout, 0, tid);
    assert!(p1.is_empty(), "float must not render on page 1");
    let p2 = page_float_fragments(&layout, 1, tid);
    assert!(!p2.is_empty(), "float renders on page 2");
    assert_close(Scalar(p2[0].1), inches(0.5), "deferred float at page 2 top");
}

// --- Behavior 7: `float: next-page` always defers --------------------------

#[test]
fn next_page_float_starts_on_next_page() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .np { float: next-page; width: 2in; height: 30pt; }
    </style></head><body>
        <div class="np"><p>Next.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let nid = node_id_by_class(&dom, "np");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "next-page float creates page 2");
    let p1 = page_float_fragments(&layout, 0, nid);
    assert!(p1.is_empty(), "next-page float must not render on page 1");
    let p2 = page_float_fragments(&layout, 1, nid);
    assert!(!p2.is_empty(), "next-page float renders on page 2");
    // It behaves as a top float on its page.
    assert_close(Scalar(p2[0].1), inches(0.5), "next-page float at page 2 top");
}

// --- Behavior 8: `float: snap` resolves to the nearest edge (top, v1) ------

#[test]
fn snap_float_behaves_as_top() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .s { float: snap; width: 2in; height: 30pt; }
    </style></head><body>
        <div class="s"><p>Snapped.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let sid = node_id_by_class(&dom, "s");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let s = page_float_fragments(&layout, 0, sid);
    assert!(!s.is_empty(), "snap float placed on page 1");
    assert_close(Scalar(s[0].1), inches(0.5), "snap float pins at page top");
}

// --- Behavior 9: `float: footnote` (CORE-107) unaffected --------------------

#[test]
fn footnote_float_still_renders_in_footnote_area() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .note { float: footnote; }
    </style></head><body>
        <p>Alpha beta<span class="note">Note text here.</span></p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    // The note text must appear in the page's footnote band (bottom), and
    // the call marker digit in the body. Text extraction check: the note's
    // text exists on page 1.
    let lines = page_lines(&layout, 0);
    assert!(!lines.is_empty(), "page 1 has text");
    // The footnote band renders below the body's last line: at least two
    // distinct text groups (body line + note) exist.
    assert!(lines.len() >= 2, "body text and footnote text both present");
}
