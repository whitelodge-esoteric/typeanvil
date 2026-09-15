//! Out-of-flow positioning acceptance tests — one per acceptance criterion in
//! `docs/specifications/out-of-flow-positioning.spec.md` (CORE-64).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

// Float accumulation across layout passes lands within 1e-4; the abspos y
// delta here carries ~1.2e-6 noise (61.0000012 vs 61).
const EPS: f64 = 1e-4;

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
        .position(
            |n| matches!(&n.kind, NodeKind::Element(el) if el.classes.iter().any(|c| c == class)),
        )
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
fn collect_lines(frag: &Fragment, abs: (f64, f64), out: &mut Vec<(Scalar, Scalar)>) {
    let here = (abs.0 + frag.offset.x.get(), abs.1 + frag.offset.y.get());
    if let typeanvil::frag::FragmentContent::Text(_) = &frag.content {
        // Page-absolute line origin (offsets are parent-relative).
        out.push((Scalar(here.0), Scalar(here.1)));
    }
    for c in &frag.children {
        collect_lines(c, here, out);
    }
}

fn page_lines(layout: &Layout, page: usize) -> Vec<(Scalar, Scalar)> {
    let mut out = Vec::new();
    collect_lines(&layout.pages[page].root, (0.0, 0.0), &mut out);
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
    assert!(
        layout.pages.len() >= 2,
        "document must paginate to at least 2 pages"
    );

    // Page 1: the filler's page — no abspos fragment.
    assert!(
        page_abspos_fragments(&layout, 0, aid).is_empty(),
        "no abspos fragment on page 1"
    );
    // Page 2: the containing block starts at the content top; the abspos sits
    // 10pt right / 20pt down from its padding-box origin. Offsets are
    // page-absolute (CORE-127 slice b): the page content origin is the
    // 36pt margin, so cb origin = (36, 36).
    let frags = page_abspos_fragments(&layout, 1, aid);
    assert!(
        !frags.is_empty(),
        "abspos fragment lands on the containing block's page"
    );
    assert_close(frags[0].offset.x, Scalar(46.0), "x = 36 + cb.x + left (10pt)");
    assert_close(frags[0].offset.y, Scalar(56.0), "y = 36 + cb.y + top (20pt)");
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
    // cb padding-box origin = content origin + padding (12pt, 0pt); offsets
    // are page-absolute (CORE-127 slice b), content origin = 36pt margin.
    assert_close(
        frags[0].offset.x,
        Scalar(48.0),
        "x = 36 + cb padding-box origin (padding-left 12pt)",
    );
    assert_close(frags[0].offset.y, Scalar(36.0), "y = 36 + cb padding-box top");
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
    // Offsets are page-absolute (CORE-127 slice b); content origin = 36pt.
    assert_close(
        frags[0].offset.x,
        Scalar(44.0),
        "x = 36 + inner padding-box origin (8pt)",
    );
    assert_close(
        frags[0].offset.y,
        Scalar(36.0),
        "y = 36 + inner padding-box top",
    );
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
    // Initial containing block = page content box, page-absolute origin
    // (36, 36), size (288, 144) — CORE-127 slice b convention.
    let r = page_abspos_fragments(&layout, 0, rid);
    assert!(!r.is_empty(), "right-inset fragment placed");
    assert_close(
        r[0].offset.x,
        Scalar(36.0 + 288.0 - 72.0),
        "right:0 flushes to the CB's right edge",
    );
    assert_close(r[0].offset.y, Scalar(36.0), "top:0 keeps the CB's top");
    let b = page_abspos_fragments(&layout, 0, bid);
    assert!(!b.is_empty(), "bottom-inset fragment placed");
    // fh = one 12pt line at line-height 1.2 = 14.4pt; bottom:0 flushes to the
    // fragmentainer content bottom (the CB-height approximation).
    assert_close(b[0].offset.x, Scalar(36.0), "left:0 keeps the CB's left");
    assert_close(
        b[0].offset.y,
        Scalar(36.0 + 144.0 - 14.4),
        "bottom:0 flushes to the content bottom",
    );
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
    // Offsets are page-absolute (CORE-127 slice b): the CB is the page
    // content box, origin (36, 36).
    assert_close(frags[0].offset.x, Scalar(36.0), "static x = CB origin (36)");
    assert_close(frags[0].offset.y, Scalar(36.0), "static y = CB origin (36)");
    // The paragraph still starts at the very top of the content area
    // (content top = 36pt margin). Out-of-flow boxes paint after in-flow
    // ones, so the FIRST line belongs to the paragraph.
    let lines = page_lines(&layout, 0);
    assert!(lines.len() >= 2, "paragraph produced lines");
    assert_close(
        lines[0].1,
        Scalar(36.0),
        "following text starts at the content top (no cursor advance)",
    );
}

// --- 6. Fixed anchors to the page box and repeats on every page --------------

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
    // content box origin, page-absolute (CORE-127 slice b), is (36, 36).
    assert_close(
        frags[0].offset.x,
        Scalar(36.0),
        "fixed x = page content origin, not the CB",
    );
    assert_close(
        frags[0].offset.y,
        Scalar(36.0),
        "fixed y = page content origin",
    );
}

/// css-position-3 §fixed in paged media: a fixed box repeats on EVERY page.
/// It is laid out once against the initial containing block (page 0's content
/// box) and its fragment clones onto each page with the page's content-origin
/// offset (CORE-127 slice b).
#[test]
fn fixed_repeats_on_every_page() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .f { position: fixed; top: 10pt; left: 10pt; }
        .tall { height: 130pt; break-after: page; }
    </style></head>
    <body>
        <div class="f">F</div>
        <div class="tall">Page one body.</div>
        <div>Page two body.</div>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "document paginates to 2+ pages");
    for page in 0..layout.pages.len() {
        let frags = page_abspos_fragments(&layout, page, fid);
        assert!(
            !frags.is_empty(),
            "fixed fragment repeats on page {}",
            page + 1
        );
        // Same page-absolute anchor on every page: ICB origin (36, 36) plus
        // the insets (10, 10).
        assert_close(
            frags[0].offset.x,
            Scalar(46.0),
            "fixed x on every page = content origin + left",
        );
        assert_close(
            frags[0].offset.y,
            Scalar(46.0),
            "fixed y on every page = content origin + top",
        );
    }
}

/// `bottom` on a fixed box resolves against the initial containing block's
/// height (the page content box), NOT the containing block or the body (CORE-127
/// slice b).
#[test]
fn fixed_bottom_resolves_against_icb_height() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; }
        .f { position: fixed; bottom: 0; left: 0; }
    </style></head>
    <body>
        <div class="f">F</div>
        <p>Body.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_abspos_fragments(&layout, 0, fid);
    assert!(!frags.is_empty(), "fixed fragment placed");
    // ICB = (36, 36) size (288, 144); fh = one 12pt line at 1.2 = 14.4pt.
    // bottom:0 flushes the margin box to the content-box bottom edge.
    assert_close(
        frags[0].offset.y,
        Scalar(36.0 + 144.0 - 14.4),
        "bottom:0 flushes to ICB bottom",
    );
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

// --- 9. Relative inset offsets: paint-time shift, no sibling reflow --------

/// css-position-3 §6.2: a relatively-positioned box paints at its static
/// position plus its insets; siblings keep their flow positions.
#[test]
fn relative_top_shifts_fragment_not_sibling() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        div { margin: 0; padding: 0; height: 20pt; }
        .rel { position: relative; top: 20pt; }
    </style></head>
    <body>
        <div class="rel">A</div>
        <div>B</div>
    </body></html>"#;
    let dom = dom_of(html);
    let rel_id = node_id_by_class(&dom, "rel");
    // The unshifted reference: identical document without the relative box.
    let base = lay(
        r#"<html><head><style>
            body { margin: 0; font-size: 12pt; line-height: 1.2; }
            div { margin: 0; padding: 0; height: 20pt; }
        </style></head>
        <body>
            <div>A</div>
            <div>B</div>
        </body></html>"#,
        geometry(5.0, 3.0, 0.5),
    );

    let layout = lay(html, geometry(5.0, 3.0, 0.5));

    // Sibling B: collect line positions per page for the second block.
    let shifted_lines = page_lines(&layout, 0);
    let static_lines = page_lines(&base, 0);
    assert_eq!(shifted_lines.len(), static_lines.len(), "same line count");

    // First line belongs to A (shifted by top: 20pt); the rest to B (static).
    assert_close(
        shifted_lines[0].1,
        static_lines[0].1 + Scalar(20.0),
        "A y += top (20pt)",
    );
    for k in 1..shifted_lines.len() {
        assert_close(
            shifted_lines[k].1,
            static_lines[k].1,
            "B keeps its static y",
        );
    }
}

#[test]
fn relative_left_shifts_x_only() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        div { margin: 0; padding: 0; height: 20pt; }
        .rel { position: relative; left: 15pt; }
    </style></head>
    <body>
        <div class="rel">A</div>
        <div>B</div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let lines = page_lines(&layout, 0);
    assert!(lines.len() >= 2, "two blocks render");
    // A shifts right 15pt; B's x is the content-box left edge (36pt margin).
    assert_close(lines[0].0, lines[1].0 + Scalar(15.0), "A x += left (15pt)");
    assert_close(lines[1].0, Scalar(36.0), "B x unchanged at content left");
}

#[test]
fn relative_right_negative_and_over_constrained() {
    // right: 10pt mirrors to dx = -10pt.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        div { margin: 0; padding: 0; height: 20pt; }
        .neg { position: relative; right: 10pt; }
        .over { position: relative; left: 15pt; right: 99pt; top: 5pt; bottom: 77pt; }
    </style></head>
    <body>
        <div class="neg">N</div>
        <div class="over">O</div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let lines = page_lines(&layout, 0);
    assert!(lines.len() >= 2, "two blocks render");
    // N: right-only pair resolves dx = -10pt (mirrored), dy = 0.
    assert_close(
        lines[0].0,
        Scalar(36.0) - Scalar(10.0),
        "N x -= right (10pt)",
    );
    // O: over-constrained — left/top win over right/bottom (LTR, §6.2).
    assert_close(
        lines[1].0,
        Scalar(36.0) + Scalar(15.0),
        "O x += left (left wins)",
    );
    // Box pitch between the two blocks is 20pt (each `height: 20pt` — with
    // CORE-126 the declared height sizes the border box; previously height
    // was ignored and the pitch was the 14.4pt line box);
    // `top: 5pt` adds on top of it.
    assert_close(
        lines[1].1,
        lines[0].1 + Scalar(20.0) + Scalar(5.0),
        "O y += top (top wins)",
    );
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

/// CORE-154: a SKIPPED fixed box must not count as page content for the
/// leading forced-break guard. A fixed box is laid out once and cloned onto
/// every page (CORE-127 slice b), so it is never "placed" in body layout. The
/// old guard used `i > start_index` as a proxy for "this page already has
/// content"; the fixed skip advanced `i`, so the first `break-before: page`
/// sibling fired a LEADING break — an empty page 1 plus one extra page
/// (3 pages where the reference needs 2).
#[test]
fn skipped_fixed_box_does_not_trigger_a_leading_forced_break() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        .fixed { position: fixed; top: 0; left: 0; width: 20pt; height: 20pt; }
        .brk { break-before: page; }
    </style></head><body>
        <div class="fixed"></div>
        <div class="brk">A</div>
        <div class="brk">B</div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "a leading fixed box is cloned onto the page, not content placed on it: \
         the first break-before must be suppressed (page 1 = fixed + A, page 2 = B)"
    );
}

/// CORE-154 companion: a genuinely PLACED out-of-flow box (abspos) DOES count,
/// so a following `break-before: page` still fires. This is the distinction the
/// fix must preserve — the abspos paints once on this page, unlike a cloned
/// fixed box.
#[test]
fn placed_abspos_still_counts_for_a_leading_forced_break() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        .abs { position: absolute; top: 0; left: 0; width: 20pt; height: 20pt; }
        .brk { break-before: page; }
    </style></head><body>
        <div class="abs"></div>
        <div class="brk">A</div>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(
        layout.pages.len(),
        2,
        "a placed abspos is content on this page, so the following forced break fires"
    );
}

// --- CORE-169: page-anchored abspos fragments across pages ------------------

/// A page-anchored abspos container (auto insets, no positioned ancestor)
/// taller than the remaining fragmentainer space defers to the next page and
/// fragments there (css-break-3 §2.3 class A), instead of painting
/// monolithically over this page's content. Its declared-height children
/// slice at the fragmentainer edge (CORE-167 machinery).
#[test]
fn page_anchored_abspos_fragments_across_pages() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        body { margin: 0; }
        .fill { height: 150pt; background: #111; }
        .abs { position: absolute; }
        .abs > div { box-sizing: border-box; height: 150pt; width: 100pt; }
    </style></head><body>
        <div class="fill"></div>
        <div class="abs"><div style="background: #f00"></div><div style="background: #0f0"></div></div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "abs");
    let layout = lay(html, geometry(5.0, 3.0, 0.0));
    // Page 1: the 150pt filler; the abspos container (2x150pt = 300pt) does
    // not fit the remaining 66pt → defers whole. Page 2: first 150pt child
    // fills the page (216pt content height) and fragments; page 3: the rest.
    assert!(
        layout.pages.len() >= 3,
        "deferral must create a fresh page for the abspos box, got {} pages",
        layout.pages.len()
    );
    // Page 1 must NOT hold the abspos container's fragments.
    assert!(
        page_abspos_fragments(&layout, 0, aid).is_empty(),
        "deferred abspos box paints nothing on the deferral page"
    );
    // Page 2 holds the box's first fragment, at the content top.
    let frags = page_abspos_fragments(&layout, 1, aid);
    assert!(
        !frags.is_empty(),
        "abspos box starts on the page after the deferral"
    );
    assert_close(
        frags[0].offset.y,
        Scalar(0.0),
        "class-A resume anchors at the fragmentainer top",
    );
}

// --- CORE-185: a PINNED abspos whose inset lands past page 1 -------------

#[test]
fn pinned_abspos_past_page_one_lands_on_offset_page() {
    // Chromium oracle: `top:500px` at a 216pt content height (5x3in page,
    // @page margin 0) puts the box on page 2 at local y = 375-216 = 159pt
    // and grows the document to 2 pages. Our engine used to paint it at
    // y=375pt on page 1 — outside the page box, invisible, no page added.
    let html = r#"<html><head><style>
        @page { margin: 0; }
        body { margin: 0; }
        .a { position: absolute; top: 500px; }
    </style></head><body>
        <div class="a">hello</div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.0));
    assert_eq!(
        layout.pages.len(),
        2,
        "document must grow to include the box's page, got {} pages",
        layout.pages.len()
    );
    // Page 1 stays paint-free for this box.
    assert!(
        page_abspos_fragments(&layout, 0, aid).is_empty(),
        "pinned box past page 1 paints nothing on page 1"
    );
    // Page 2 holds it at the offset's local y: 500px = 375pt, minus the
    // 216pt page 1 = 159pt.
    let frags = page_abspos_fragments(&layout, 1, aid);
    assert!(!frags.is_empty(), "box paints on the page containing its offset");
    assert_close(
        frags[0].offset.y,
        Scalar(159.0),
        "pinned box starts at the inset's local y on the target page",
    );
}

#[test]
fn pinned_abspos_past_page_one_fragments_tall_box() {
    // Chromium oracle: `top:500px; height:1000px` at a 216pt page renders 6
    // pages — the box starts on page 2 at local 159pt and the 750pt-tall
    // box fragments across pages 2-6. Our engine painted nothing (1 page).
    let html = r#"<html><head><style>
        @page { margin: 0; }
        body { margin: 0; }
        .a { position: absolute; top: 500px; height: 1000px; }
    </style></head><body>
        <div class="a">hello</div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.0));
    assert_eq!(
        layout.pages.len(),
        6,
        "tall pinned box fragments across pages 2-6 (Chromium-verified), got {} pages",
        layout.pages.len()
    );
    let frags = page_abspos_fragments(&layout, 1, aid);
    assert!(!frags.is_empty(), "tall box starts on page 2");
    assert_close(
        frags[0].offset.y,
        Scalar(159.0),
        "tall box starts at the inset's local y on page 2",
    );
    // Every subsequent page holds a continuation fragment.
    for p in 2..6 {
        assert!(
            !page_abspos_fragments(&layout, p, aid).is_empty(),
            "page {} must hold a continuation fragment of the tall box",
            p + 1
        );
    }
}

#[test]
fn pinned_abspos_inside_page_keeps_monolithic() {
    // A pinned box that STARTS inside the page keeps the monolithic model
    // (spec Behavior 9): 1 page, offset unchanged. CORE-185 only defers a
    // box whose used inset lands at/past the fragmentainer bottom.
    let html = r#"<html><head><style>
        @page { margin: 0; }
        body { margin: 0; }
        .a { position: absolute; top: 200px; height: 1000px; }
    </style></head><body>
        <div class="a">hello</div>
    </body></html>"#;
    let dom = dom_of(html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(html, geometry(5.0, 3.0, 0.0));
    assert_eq!(layout.pages.len(), 1, "monolithic box adds no page");
    let frags = page_abspos_fragments(&layout, 0, aid);
    assert!(!frags.is_empty(), "box paints on page 1");
    assert_close(
        frags[0].offset.y,
        Scalar(150.0),
        "200px = 150pt, unpaged",
    );
}

#[test]
fn pinned_replaced_image_past_page_one_terminates() {
    // A replaced image (`content: url()`) lays MONOLITHICALLY and IGNORES
    // break tokens: `layout_image` returns an empty fragment plus a
    // `break_before` token whenever the box does not fit. Deferring such a
    // box past page 1 therefore regenerated that same token forever —
    // measured at the MAX_PAGES cap of 100_000 pages (and it crashed the
    // WPT harness worker on `firefox-bug-2026295-print`). The drain's
    // non-progress guard must place it as a LAST RESORT instead, so
    // pagination stays bounded AND the image still paints.
    const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
    // Never hand-type a base64 constant (CORE-186): a single wrong
    // character silently corrupts the payload and the failure points at the
    // engine, not the fixture.
    assert_eq!(PNG_B64.len(), 92, "PNG constant must not be corrupted");
    let html = format!(
        r#"<html><head><style>
        @page {{ margin: 0; }}
        body {{ margin: 0; }}
        .a {{ position: absolute; top: 500px; height: 1000px; content: url(data:image/png;base64,{PNG_B64}); }}
    </style></head><body>
        <div class="a"></div>
    </body></html>"#
    );
    let dom = dom_of(&html);
    let aid = node_id_by_class(&dom, "a");
    let layout = lay(&html, geometry(5.0, 3.0, 0.0));
    assert!(
        layout.pages.len() >= 2,
        "the box's offset page is kept, got {} page(s)",
        layout.pages.len()
    );
    assert!(
        layout.pages.len() < 10,
        "non-progress guard must bound pagination (was 100_000), got {} pages",
        layout.pages.len()
    );
    // The replaced-element path does NOT tag its fragment with `source`, so
    // assert on CONTENT: the offset's page must hold an Image fragment.
    fn has_image(frag: &Fragment) -> bool {
        matches!(frag.content, typeanvil::frag::FragmentContent::Image(_))
            || frag.children.iter().any(has_image)
    }
    assert!(
        has_image(&layout.pages[1].root),
        "the image paints on the page containing its offset (node {aid})"
    );
}
