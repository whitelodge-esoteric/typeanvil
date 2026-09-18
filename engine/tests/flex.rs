//! Flexbox acceptance tests — one per acceptance criterion in
//! `docs/specifications/flexbox-fragmentation.spec.md` (CORE-65).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, FragmentKind};
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
    // css-flexbox-1 §9.4 step 4: items with auto cross sizes and the
    // default align-self:stretch GROW to the line's cross size (the
    // tallest item's natural height). The engine's block path ignores
    // `height` (CORE-66 auto-height self-consistency), so the line cross
    // is CONTENT-based: the tall item uses a bigger font, making its
    // natural line height the line's cross; the short item then stretches
    // to match it.
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
    // a: 16pt → line 19.2pt; b: 30pt → line 36pt. The line cross is b's;
    // a then stretches to the same 36pt (margin box fills the line).
    assert!((bh - 36.0).abs() < 0.01, "tall item line is 36pt, got {bh}");
    assert!((ah - 36.0).abs() < 0.01, "short item stretches to 36pt, got {ah}");
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

// ---------------------------------------------------------------------------
// CORE-122 — trailing margin/height handling across a forced break: the
// following sibling's position must be identical with and without the forced
// break inside the container (css-break-3 §5; WPT 046).
// ---------------------------------------------------------------------------

#[test]
fn forced_break_inside_row_container_keeps_following_sibling_position() {
    // The invariant: the container's resumed page-2 fragment has the SAME
    // height whether the container is display:block or display:flex, so the
    // following sibling lands at the same y in both (css-break-3 §5; WPT
    // single-line-row-flex-fragmentation-046). Pre-fix, the resumed row
    // flex path measured its line at the item's FULL block size instead of
    // the un-consumed remainder, and the sibling landed one line box short.
    let html = |disp: &str| {
        format!(
            r#"
    <style>
      @page {{ size: 5in 3in; margin: 0.5in; }}
      body {{ margin: 0; }}
      .c {{ display: {disp}; border: 0.25in solid black; }}
    </style>
    <div>Before</div>
    <div class="c"><div><div>1</div><div style="break-after: page">2</div><div>3</div><div>4</div></div></div>
    <div class="after">After</div>
    "#
        )
    };
    let dom_b = dom_of(&html("block"));
    let out_b = lay(&html("block"));
    let dom_f = dom_of(&html("flex"));
    let out_f = lay(&html("flex"));
    assert_eq!(out_b.pages.len(), 2, "block variant: two pages");
    assert_eq!(out_f.pages.len(), 2, "flex variant: two pages");

    // The container's page-2 fragment height must match between variants.
    let c_b = node_id_by_class(&dom_b, "c");
    let c_f = node_id_by_class(&dom_f, "c");
    let (_, _, _, h_b) = box_of(&out_b, 1, c_b);
    let (_, _, _, h_f) = box_of(&out_f, 1, c_f);
    assert!(
        (h_b - h_f).abs() < EPS,
        "resumed container height must match the block reference: flex {h_f} vs block {h_b}"
    );

    // And therefore the following sibling starts at the same y in both.
    let a_b = node_id_by_class(&dom_b, "after");
    let a_f = node_id_by_class(&dom_f, "after");
    let (_, y_b, _, _) = box_of(&out_b, 1, a_b);
    let (_, y_f, _, _) = box_of(&out_f, 1, a_f);
    assert!(
        (y_b - y_f).abs() < EPS,
        "following sibling position must not depend on display:flex inside the container: flex {y_f} vs block {y_b}"
    );
}
// ---------------------------------------------------------------------------
// CORE-207 — column flex grow/shrink and negative margin fixes
// ---------------------------------------------------------------------------

/// Test that flex:1 items in a column container with declared height grow to fill remaining space
#[test]
fn column_flex1_grows_to_fill_remaining_height() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 12pt; }
      .flex { display: flex; flex-direction: column; height: 120pt; border: 1pt solid black; }
      .item-fixed { height: 20pt; flex: none; background: red; }
      .item-flex { flex: 1; background: blue; }
    </style>
    <div class="flex">
      <div class="item-fixed">Fixed</div>
      <div class="item-flex">Flex 1</div>
      <div class="item-flex">Flex 2</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let fixed = node_id_by_class(&dom, "item-fixed");
    let flex1 = node_id_by_class(&dom, "item-flex");
    
    // Get positions and heights
    let (_fx, _fy, _fw, fh) = box_of(&out, 0, fixed);
    let (_f1x, _f1y, _f1w, f1h) = box_of(&out, 0, flex1);
    
    // Fixed item should have height of 20pt
    assert!((fh - 20.0).abs() < EPS, "Fixed item should have height of 20pt");
    
    // Flex items should have height > 20pt each (they should grow)
    assert!(f1h > 20.0, "Flex items should be taller than 20pt");
}

/// Test that negative margins cause items to overlap in column flex containers
#[test]
fn column_negative_margin_overlaps_previous_item() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 12pt; }
      .flex { display: flex; flex-direction: column; border: 4pt solid black; }
      .item-first { height: 30pt; background: red; }
      .item-second { height: 30pt; margin-top: -10pt; background: blue; }
    </style>
    <div class="flex">
      <div class="item-first">First</div>
      <div class="item-second">Second</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let first = node_id_by_class(&dom, "item-first");
    let second = node_id_by_class(&dom, "item-second");
    
    let (_fx, fy, _fw, fh) = box_of(&out, 0, first);
    let (_sx, sy, _sw, _sh) = box_of(&out, 0, second);
    
    // Second item should overlap first item by approximately 10pt
    let overlap = (fy + fh) - sy;
    assert!((overlap - 10.0).abs() < EPS, "Second item should overlap first by ~10pt");
}

/// Test that auto-height flex items with negative margins render with positive height
/// when the container has a DEFINITE main size (css-flexbox-1 §9.8: grow only
/// distributes free space against a definite container main size — without it,
/// an empty flex:1 item legitimately measures 0 and is dropped).
#[test]
fn column_flex1_auto_height_item_renders_nonzero() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 12pt; }
      .flex { display: flex; flex-direction: column; height: 120pt; border: 1pt solid black; }
      .item-first { height: 30pt; flex: 1; margin: -10pt 0; background: red; }
      .item-second { height: 30pt; flex: none; background: blue; }
      .item-third { flex: 1; background: green; } /* auto height, empty */
    </style>
    <div class="flex">
      <div class="item-first">First</div>
      <div class="item-second">Second</div>
      <div class="item-third"></div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let first = node_id_by_class(&dom, "item-first");
    let third = node_id_by_class(&dom, "item-third");

    let (_fx, _fy, _fw, fh) = box_of(&out, 0, first);
    let (_tx, _ty, _tw, th) = box_of(&out, 0, third);

    // Both items should have positive height
    assert!(fh > 0.0, "First item should have positive height");
    assert!(th > 0.0, "Third item should have positive height (not dropped)");
}

// ---------------------------------------------------------------------------
// CORE-234 — a flex container with a border paints the border and offsets its
// items inside the border box, matching the block path (the flex path
// previously dropped the border and placed items at the content edge).
// ---------------------------------------------------------------------------

#[test]
fn flex_container_border_paints_and_offsets_items() {
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; flex-direction: column; border: 0.25in solid black; }
    </style>
    <div class="flex">
      <div class="a">1</div>
      <div class="b">2</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let flex = node_id_by_class(&dom, "flex");
    let a = node_id_by_class(&dom, "a");
    // The container fragment carries a Border (not just a background).
    let mut v = Vec::new();
    find_source(&out.pages[0].root, flex, &mut v);
    assert!(!v.is_empty(), "flex container must have a fragment on page 1");
    assert!(
        matches!(v[0].content, FragmentContent::Border(_)),
        "flex container with a border must paint a Border, got {:?}",
        v[0].content
    );
    // The first item sits inside the border box: its x is the container's
    // left edge + border_left + padding_left.
    let (fx, _fy, _fw, _fh) = box_of(&out, 0, flex);
    let (ax, _ay, _aw, _ah) = box_of(&out, 0, a);
    let border = inches(0.25).get();
    assert!(
        (ax - (fx + border)).abs() < EPS,
        "item must be offset inside the border box: item x {ax} vs container x {fx} + border {border}"
    );
}

// ---------------------------------------------------------------------------
// CORE-234 residuals — row flex line fragmentation (046) and single-item line
// fragmentation (080)
// ---------------------------------------------------------------------------

#[test]
fn row_flex_broken_line_keeps_container_height() {
    // 046: a row flex container whose single item breaks inside the line
    // (break-after:page) must keep a box that spans the placed content, not
    // collapse to a thin border bar.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; }
      .flex { display: flex; border: 0.25in solid black; }
      .text { block-size: 0.25in; }
    </style>
    <div class="text">Before Flexbox</div>
    <div class="flex">
      <div>
        <div>1</div>
        <div style="break-after: page">2</div>
        <div>3</div>
        <div>4</div>
      </div>
    </div>
    <div class="text">After Flexbox</div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let flex = node_id_by_class(&dom, "flex");
    let (_fx, _fy, _fw, fh) = box_of(&out, 0, flex);
    // Border top + placed content + border bottom: definitely more than the
    // two borders alone (36pt), which is what a collapsed thin bar would be.
    assert!(
        fh > 2.0 * inches(0.25).get(),
        "row flex container must keep its placed height on a broken line, got {fh}"
    );
}

#[test]
fn row_flex_single_item_fragments_across_pages() {
    // 080: a single-item row line that does not fit the remaining
    // fragmentainer fragments the item's content across pages instead of
    // deferring the whole line.
    let html = r#"
    <style>
      @page { size: 5in 3in; margin: 0.5in; }
      body { margin: 0; font-size: 0.25in; }
      .flex { display: flex; flex-flow: row wrap; border: 0.25in solid black; }
      .item { width: 100%; }
    </style>
    <div style="height: 1in; background: gray;"></div>
    <div class="flex">
      <div class="item">1<br>2<br>3<br>4</div>
      <div class="item">5</div>
    </div>
    "#;
    let out = lay(html);
    let dom = dom_of(html);
    let item = node_id_by_class(&dom, "item");
    let mut p1 = Vec::new();
    find_source(&out.pages[0].root, item, &mut p1);
    let mut p2 = Vec::new();
    find_source(&out.pages[1].root, item, &mut p2);
    assert!(!p1.is_empty(), "item must start on page 1");
    assert!(!p2.is_empty(), "item must continue on page 2");
}

