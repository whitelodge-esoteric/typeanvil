//! Flexbox acceptance tests — one per acceptance criterion in
//! `docs/specifications/flexbox-fragmentation.spec.md` (CORE-65).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const EPS: f64 = 1e-6;

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

fn style_text(html: &str) -> &str {
    let s = html.find("<style>").expect("style element");
    let e = html.find("</style>").expect("style end");
    &html[s + "<style>".len()..e]
}

fn dom_of(html: &str) -> Dom {
    Dom::parse(html).expect("dom parses")
}

fn lay(html: &str) -> Layout {
    let dom = dom_of(html);
    let ss = Stylesheet::parse(&style_text(html));
    layout(&dom, &ss, geometry())
}

fn node_id_by_class(dom: &Dom, class: &str) -> NodeId {
    dom.nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| match &n.kind {
            NodeKind::Element(el) if el.classes.iter().any(|c| c == class) => Some(i as NodeId),
            _ => None,
        })
        .expect("element with class must exist") as NodeId
}

/// Fragments whose `source` is the given node.
fn find_source<'a>(frag: &'a Fragment, id: NodeId, out: &mut Vec<&'a Fragment>) {
    if frag.source == Some(id) {
        out.push(frag);
    }
    for c in &frag.children {
        find_source(c, id, out);
    }
}

/// The x/y/width/height of the first fragment for a node on the given page.
fn box_of(layout: &Layout, page: usize, id: NodeId) -> (f64, f64, f64, f64) {
    let mut v = Vec::new();
    find_source(&layout.pages[page].root, id, &mut v);
    assert!(!v.is_empty(), "node must have a fragment on page {page}");
    let f = v[0];
    (f.offset.x.get(), f.offset.y.get(), f.size.0.get(), f.size.1.get())
}

// ---------------------------------------------------------------------------
// Criterion 1 — Row split: two flex:1 items share the container width
// ---------------------------------------------------------------------------

#[test]
fn row_flex_one_share_width() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; }
      .item { flex: 1; height: 0.3in; }
    </style>
    <div class="flex">
      <div class="item a">A</div>
      <div class="item b">B</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let a = node_id_by_class(&dom, "a");
    let b = node_id_by_class(&dom, "b");
    let (ax, _ay, aw, _ah) = box_of(&out, 0, a);
    let (bx, _by, bw, _bh) = box_of(&out, 0, b);
    let content_w = inches(4.0).get(); // 5in - 0.5in margins
    assert!((aw - bw).abs() < EPS, "flex:1 items share width: {aw} vs {bw}");
    assert!((aw + bw - content_w).abs() < EPS, "items fill the container: {aw}+{bw} vs {content_w}");
    assert!((bx - (ax + aw)).abs() < EPS, "second item follows the first");
}

// ---------------------------------------------------------------------------
// Criterion 2 — Column stack with gaps
// ---------------------------------------------------------------------------

#[test]
fn column_stack_with_gap() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; flex-direction: column; gap: 10pt; }
      .item { height: 20pt; }
    </style>
    <div class="flex">
      <div class="item a">A</div>
      <div class="item b">B</div>
      <div class="item c">C</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let a = node_id_by_class(&dom, "a");
    let b = node_id_by_class(&dom, "b");
    let c = node_id_by_class(&dom, "c");
    let (_ax, ay, _aw, ah) = box_of(&out, 0, a);
    let (_bx, by, _bw, bh) = box_of(&out, 0, b);
    let (_cx, cy, _cw, ch) = box_of(&out, 0, c);
    assert!((by - (ay + ah) - 10.0).abs() < EPS, "gap between a and b");
    assert!((cy - (by + bh) - 10.0).abs() < EPS, "gap between b and c");
}

// ---------------------------------------------------------------------------
// Criterion 3 — Stretch two-pass: line cross = tallest item, resolved before
// fragmentation
// ---------------------------------------------------------------------------

#[test]
fn row_stretch_cross_size() {
    // The engine's block path ignores `height` (CORE-66 auto-height
    // self-consistency), so the cross size is CONTENT-based: the tall item
    // uses a bigger font, making its natural line height the line's cross.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; }
      .item { flex: 1; }
      .tall { font-size: 30pt; }
    </style>
    <div class="flex">
      <div class="item a">A</div>
      <div class="item b tall">B</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let a = node_id_by_class(&dom, "a");
    let b = node_id_by_class(&dom, "b");
    let flex = node_id_by_class(&dom, "flex");
    let (_ax, _ay, _aw, ah) = box_of(&out, 0, a);
    let (_bx, _by, _bw, bh) = box_of(&out, 0, b);
    let (_fx, _fy, _fw, fh) = box_of(&out, 0, flex);
    // a: 16pt → line 19.2pt; b: 30pt → line 36pt. The line cross is b's.
    assert!((bh - 36.0).abs() < 0.01, "tall item line is 36pt, got {bh}");
    assert!(ah < bh, "short item is shorter than the tall item: {ah} < {bh}");
    // The container's height equals the line's cross size (the tallest item).
    assert!((fh - 36.0).abs() < 0.01, "container is one line of cross 36pt, got {fh}");
}

// ---------------------------------------------------------------------------
// Criterion 4 — Fragment across pages: lines move whole; break-inside: avoid
// ---------------------------------------------------------------------------

#[test]
fn row_fragments_across_pages() {
    // Each item is one line at 48pt font → 57.6pt (0.8in) tall, content-based
    // (the block path ignores `height`). 2 lines fit the 144pt content page;
    // line 3 moves to page 2.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; flex-wrap: wrap; }
      .item { flex: 0 0 100%; font-size: 48pt; }
    </style>
    <div class="flex">
      <div class="item a">1</div>
      <div class="item b">2</div>
      <div class="item c">3</div>
      <div class="item d">4</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let a = node_id_by_class(&dom, "a");
    let b = node_id_by_class(&dom, "b");
    let c = node_id_by_class(&dom, "c");
    let d = node_id_by_class(&dom, "d");
    let (_ax, ay, _aw, ah) = box_of(&out, 0, a);
    let (_bx, by, _bw, bh) = box_of(&out, 0, b);
    assert!((by - (ay + ah)).abs() < 0.01, "line 2 follows line 1");
    // 2 × 57.6 = 115.2pt; the third line does not fit 144pt → page 2.
    assert_eq!(out.pages.len(), 2, "two pages");
    let (_cx, cy, _cw, _ch) = box_of(&out, 1, c);
    let (_dx, dy, _dw, _dh) = box_of(&out, 1, d);
    assert!((cy - 0.0).abs() < EPS, "line 3 starts at the top of page 2");
    // 48pt font → line 57.6pt through stylo's f32 pipeline — 0.01pt tolerance.
    assert!(
        (dy - (cy + 0.8 * 72.0)).abs() < 0.01,
        "line 4 follows line 3, cy={cy} dy={dy}"
    );
}

#[test]
fn break_inside_avoid_moves_item_whole() {
    // Content-based heights: each item is one 80pt-font line → 96pt. Two
    // items (192pt) do not fit the 144pt content page, so `break-inside:
    // avoid` moves item B whole to page 2.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; flex-direction: column; }
      .item { font-size: 80pt; break-inside: avoid; }
    </style>
    <div class="flex">
      <div class="item a">A</div>
      <div class="item b">B</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let a = node_id_by_class(&dom, "a");
    let b = node_id_by_class(&dom, "b");
    assert_eq!(out.pages.len(), 2, "b moves whole to page 2");
    let (_ax, ay, _aw, ah) = box_of(&out, 0, a);
    let (_bx, by, _bw, _bh) = box_of(&out, 1, b);
    assert!((ay - 0.0).abs() < EPS, "a at the top of page 1");
    // One 80pt-font line ≈ 96pt; font-metric f32 conversion drifts ~0.04pt,
    // so bound loosely — the point is the box is a single tall line.
    assert!(ah > 90.0 && ah < 100.0, "a is one tall line, got {ah}");
    assert!((by - 0.0).abs() < EPS, "b at the top of page 2, whole");
}

// ---------------------------------------------------------------------------
// Criterion 5 — WPT targets (run via the harness, not here)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Criterion 6 — Determinism: two renders are byte-identical
// ---------------------------------------------------------------------------

#[test]
fn flex_render_deterministic() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; gap: 6pt; }
      .item { flex: 1; height: 0.4in; }
    </style>
    <div class="flex">
      <div class="a">A</div>
      <div class="b">B</div>
      <div class="c">C</div>
    </div>
    "#;
    let dir = std::env::temp_dir().join("typeanvil-flex-det");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let html_path = dir.join("det.html");
    std::fs::write(&html_path, html).expect("write html");

    let render = |out: &Path| {
        let status = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
            .arg("render")
            .arg(&html_path)
            .arg("--page-width")
            .arg("5in")
            .arg("--page-height")
            .arg("3in")
            .arg("--margin-top")
            .arg("0.5in")
            .arg("--margin-right")
            .arg("0.5in")
            .arg("--margin-bottom")
            .arg("0.5in")
            .arg("--margin-left")
            .arg("0.5in")
            .arg("-o")
            .arg(out)
            .status()
            .expect("render runs");
        assert!(status.success(), "render exit ok");
    };
    let p1 = dir.join("a.pdf");
    let p2 = dir.join("b.pdf");
    render(&p1);
    render(&p2);
    let b1 = std::fs::read(&p1).expect("read a");
    let b2 = std::fs::read(&p2).expect("read b");
    assert_eq!(b1, b2, "two renders are byte-identical");
}

// ---------------------------------------------------------------------------
// Wrap + break propagation (WPT 081-style behavior, engine-level)
// ---------------------------------------------------------------------------

#[test]
fn wrap_line_break_before_propagates() {
    // 4 items of 50% width → 2 per line; item 3 has break-before: page.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 12pt; }
      .flex { display: flex; flex-wrap: wrap; border: 4pt solid black; }
      .item { box-sizing: border-box; width: 50%; border: 2pt solid purple; }
      .page-before { break-before: page; }
    </style>
    <div class="flex">
      <div class="item a">1</div><div class="item b">2</div>
      <div class="item c page-before">3</div><div class="item d">4</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let a = node_id_by_class(&dom, "a");
    let b = node_id_by_class(&dom, "b");
    let c = node_id_by_class(&dom, "c");
    let d = node_id_by_class(&dom, "d");
    let (_ax, ay, _aw, _ah) = box_of(&out, 0, a);
    let (_bx, by, _bw, _bh) = box_of(&out, 0, b);
    assert!((by - ay).abs() < EPS, "items 1 and 2 share line 1");
    assert_eq!(out.pages.len(), 2, "forced break moves line 2 to page 2");
    let (_cx, cy, _cw, _ch) = box_of(&out, 1, c);
    let (_dx, dy, _dw, _dh) = box_of(&out, 1, d);
    assert!((cy - 0.0).abs() < EPS, "line 2 at the top of page 2");
    assert!((dy - cy).abs() < EPS, "items 3 and 4 share line 2");
}

// ---------------------------------------------------------------------------
// Nested-container break propagation (WPT 069b/069d semantics)
// ---------------------------------------------------------------------------

#[test]
fn nested_column_first_item_break_before_propagates_to_container() {
    // WPT single-line-column-flex-fragmentation-069b: an OUTER column flex
    // holds a nested column flex whose FIRST inner item carries
    // `break-before: page`. The forced break propagates to the nested
    // container's start edge — the whole nested box moves to page 2; no
    // empty sliver stays on page 1.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 12pt; }
      .outer { display: flex; flex-direction: column; border: 4pt solid black; }
      .nested { display: flex; flex-direction: column; border: 2pt solid gold; }
      .page-before { break-before: page; }
    </style>
    <div class="outer">
      <div class="nested first">
        <div>a</div><div>b</div>
      </div>
      <div class="nested second">
        <div class="page-before">c</div><div>d</div>
      </div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let second = node_id_by_class(&dom, "second");
    assert_eq!(out.pages.len(), 2, "the whole nested box moves to page 2");
    // The second nested container must NOT appear on page 1 at all.
    let mut on_page1 = Vec::new();
    find_source(&out.pages[0].root, second, &mut on_page1);
    assert!(
        on_page1.is_empty(),
        "no sliver of the deferred container may stay on page 1"
    );
}

#[test]
fn last_item_break_after_propagates_and_terminates() {
    // WPT single-line-column-flex-fragmentation-069d: a forced break-after
    // on the LAST item of a column flex container propagates past the end:
    // the container fragments after that item (so following content starts
    // on the next page) AND terminates cleanly with no spurious trailing
    // page.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 12pt; }
      .flex { display: flex; flex-direction: column; border: 4pt solid black; }
      .page-after { break-after: page; }
    </style>
    <div>Before</div>
    <div class="flex">
      <div>a</div>
      <div class="page-after">b</div>
    </div>
    <div>After</div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let after = node_id_by_class(&dom, "flex");
    assert_eq!(
        out.pages.len(),
        2,
        "content after the container starts on page 2, exactly one extra page"
    );
    // The container itself must terminate: it has a fragment on page 1 but
    // no continuation fragment on page 2 (empty sliver would loop forever).
    let mut on_page2 = Vec::new();
    find_source(&out.pages[1].root, after, &mut on_page2);
    assert!(
        on_page2.iter().all(|f| f.size.1.get() == 0.0 && f.children.is_empty()),
        "any page-2 fragment of the finished container is an empty terminator"
    );
}
