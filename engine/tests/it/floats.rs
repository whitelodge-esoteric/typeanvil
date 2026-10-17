//! CSS float acceptance tests — one per acceptance criterion in
//! `docs/specifications/css-floats.spec.md` (CORE-62).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, FragmentKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const EPS: f64 = 1e-6;

// --- helpers ---------------------------------------------------------------

/// Points from inches.
fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

/// A page geometry with uniform margins (inches).
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

/// Lay out an HTML string with the given geometry. Body/paragraph margins are
/// zeroed so fragment offsets are comparable across siblings.
fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = Stylesheet::parse(&style_text(html));
    layout(&dom, &ss, geo)
}

/// Extract the `<style>` text (mirrors the CLI's stylesheet collection).
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

/// (x, y, width) of every line fragment in the tree, depth-first.
fn collect_lines(frag: &Fragment, out: &mut Vec<(Scalar, Scalar, Scalar)>) {
    if matches!(frag.kind, FragmentKind::Line) {
        if matches!(frag.content, FragmentContent::Text(_)) {
            out.push((frag.offset.x, frag.offset.y, frag.size.0));
        }
    }
    for c in &frag.children {
        collect_lines(c, out);
    }
}

/// Fragments whose `source` is the given node (the float's placed boxes).
fn find_source<'a>(frag: &'a Fragment, id: NodeId, out: &mut Vec<&'a Fragment>) {
    if frag.source == Some(id) {
        out.push(frag);
    }
    for c in &frag.children {
        find_source(c, id, out);
    }
}

fn page_lines(layout: &Layout, page: usize) -> Vec<(Scalar, Scalar, Scalar)> {
    let mut out = Vec::new();
    collect_lines(&layout.pages[page].root, &mut out);
    out
}

fn page_float_fragments<'a>(layout: &'a Layout, page: usize, id: NodeId) -> Vec<&'a Fragment> {
    let mut out = Vec::new();
    find_source(&layout.pages[page].root, id, &mut out);
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

// --- 1. Left float wraps text ---------------------------------------------

#[test]
fn left_float_wraps_text() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f { float: left; width: 2in; }
    </style></head><body>
        <div class="f"><p>Float box content.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));

    let floats = page_float_fragments(&layout, 0, fid);
    assert!(!floats.is_empty(), "float must be placed on page 1");
    let ff = floats[0];
    assert_close(ff.size.0, inches(2.0), "float width (explicit 2in)");

    let lines = page_lines(&layout, 0);
    assert!(lines.len() >= 3, "paragraph must produce several lines");
    // A line beside the float starts 2in to the right of the float's edge
    // (find, not [0]: the float's own text line is collected first).
    let beside = lines
        .iter()
        .find(|(x, _y, _w)| (x.get() - ff.offset.x.get() - 144.0).abs() < EPS);
    assert!(beside.is_some(), "a line beside the float is indented by 2in");
    // Some line is below the float's bottom: full width (x == float x).
    let below = lines.iter().find(|(x, _y, _w)| (x.get() - ff.offset.x.get()).abs() < EPS);
    assert!(below.is_some(), "a line below the float returns to full width");
}

// --- 2. Right float shortens lines ----------------------------------------

#[test]
fn right_float_shrinks_lines() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f { float: right; width: 2in; }
    </style></head><body>
        <div class="f"><p>Float box content.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let lines = page_lines(&layout, 0);
    assert!(lines.len() >= 3, "paragraph must produce several lines");
    let full = inches(4.0); // content width 5in - 0.5in*2 margins
    // Line 0 overlaps the right float: its width is reduced by 2in.
    assert_close(lines[0].2, full - inches(2.0), "line beside right float shortened");
    // A later line below the float has the full width.
    let below = lines.iter().find(|(_x, _y, w)| (w.get() - full.get()).abs() < EPS);
    assert!(below.is_some(), "a line below the float returns to full width");
}

// --- 3. Shrink-to-fit width -----------------------------------------------

#[test]
fn shrink_to_fit_width() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f { float: left; }
    </style></head><body>
        <div class="f"><p>Short float text.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let floats = page_float_fragments(&layout, 0, fid);
    assert!(!floats.is_empty(), "float must be placed");
    let w = floats[0].size.0.get();
    assert!(w > 0.0, "shrink-to-fit width must be positive");
    assert!(w < inches(4.0).get(), "shrink-to-fit width must be under the content width");
}

// --- 4. Float does not advance the in-flow cursor --------------------------

#[test]
fn float_does_not_advance_cursor() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f { float: left; width: 1in; }
    </style></head><body>
        <div class="f"><p>F.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let floats = page_float_fragments(&layout, 0, fid);
    assert!(!floats.is_empty(), "float must be placed");
    let ff = floats[0];
    let lines = page_lines(&layout, 0);
    assert!(!lines.is_empty(), "paragraph must produce lines");
    // The first paragraph line starts at the float's top (beside it), not below.
    assert_close(lines[0].1, ff.offset.y, "paragraph starts beside the float, not below it");
}

// --- 5. Float taller than a page resumes (Done criterion) ------------------

#[test]
fn float_taller_than_page_resumes() {
    // The float's content is ~18 lines (~260pt) against a 144pt content box:
    // it must suspend at the page bottom and resume at the top of page 2,
    // while the in-flow paragraph wraps around it on BOTH pages.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f { float: left; width: 1.5in; }
    </style></head><body>
        <div class="f"><p>alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p></div>
        <p>Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur excepteur sint occaecat cupidatat non proident sunt in culpa qui officia deserunt mollit anim id est laborum lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim ad minim veniam.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let fid = node_id_by_class(&dom, "f");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "document must paginate to at least 2 pages");

    // Page 1: the float's first fragment + text wrapping beside it.
    let f1 = page_float_fragments(&layout, 0, fid);
    assert!(!f1.is_empty(), "float first fragment on page 1");
    let lines1 = page_lines(&layout, 0);
    let beside1 = lines1.iter().find(|(x, _y, _w)| x.get() > f1[0].offset.x.get() + 1.0);
    assert!(beside1.is_some(), "page 1 text wraps beside the float");

    // Page 2: the float's continuation at the content top, text still beside it.
    let f2 = page_float_fragments(&layout, 1, fid);
    assert!(!f2.is_empty(), "float must resume on page 2");
    assert_close(f2[0].offset.y, Scalar::ZERO, "float continuation at content top");
    let lines2 = page_lines(&layout, 1);
    let beside2 = lines2.iter().find(|(x, _y, _w)| x.get() > f2[0].offset.x.get() + 1.0);
    assert!(beside2.is_some(), "page 2 text wraps around the resumed float");

    // The float's fragments cover its full measured height across the pages.
    let total: f64 = f1.iter().chain(f2.iter()).map(|f| f.size.1.get()).sum();
    assert!(total > 200.0, "float total height across pages must exceed one page (got {total:.1})");
}

// --- 6. Floats stack simply -------------------------------------------------

#[test]
fn stacks_simply() {
    // Two left floats at the same y: the simplified rule places both at the
    // left edge and text wraps around the combined (widest) intrusion.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f1 { float: left; width: 1in; }
        .f2 { float: left; width: 0.5in; }
    </style></head><body>
        <div class="f1"><p>AAAA.</p></div>
        <div class="f2"><p>BB.</p></div>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron.</p>
    </body></html>"#;
    let dom = dom_of(html);
    let f1id = node_id_by_class(&dom, "f1");
    let f2id = node_id_by_class(&dom, "f2");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(!page_float_fragments(&layout, 0, f1id).is_empty(), "f1 placed");
    assert!(!page_float_fragments(&layout, 0, f2id).is_empty(), "f2 placed");
    // Text wraps around the combined intrusion: lines beside them start at
    // the wider float's width (1in), not the narrower one's.
    let lines = page_lines(&layout, 0);
    let beside = lines
        .iter()
        .find(|(x, _y, _w)| (x.get() - inches(1.0).get()).abs() < EPS);
    assert!(beside.is_some(), "lines beside the floats use the widest intrusion");
}

// --- 7. No float -> unchanged ----------------------------------------------

#[test]
fn no_float_unchanged() {
    // A doc with no float declarations still lays out as a single page of
    // full-width lines; the full existing suite is the stronger guarantee.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p { margin: 0; }
    </style></head><body>
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(layout.pages.len(), 1, "no-float doc renders on one page");
    let lines = page_lines(&layout, 0);
    assert!(!lines.is_empty(), "paragraph renders");
    assert_close(lines[0].2, inches(4.0), "line uses full content width");
}

// --- CORE-91 regression: segment reflow must not duplicate glyphs ------------

/// Concatenate every line's run text across the whole document (pre-order),
/// whitespace-normalized to single spaces. Used to prove that float segment
/// reflow preserves the source text exactly — no duplicated boundary glyph.
fn joined_run_text(layout: &Layout) -> String {
    fn walk(frag: &Fragment, out: &mut Vec<String>) {
        if let FragmentContent::Text(run) = &frag.content {
            out.push(run.text.clone());
        }
        for c in &frag.children {
            walk(c, out);
        }
    }
    let mut runs = Vec::new();
    for page in &layout.pages {
        walk(&page.root, &mut runs);
    }
    runs.join(" ")
}

#[test]
fn segment_reflow_preserves_source_text() {
    // CORE-91: `src_offset += lr.text.len()` undercounted by the collapsed
    // whitespace between source words, so the segment resumed a few bytes
    // early and the next line re-drew the previous line's last glyph
    // ("A float t that is taller" from "A float that is taller"). Every
    // wrapped line's glyph text must now tile the source exactly.
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .f { float: right; width: 2in; }
    </style></head><body>
        <div class="f"><p>Floated sidebar content with several words inside it.</p></div>
        <p>How does an engine place such a box the float is removed from the in-flow
        cursor entirely it does not advance the paragraph below it and the next
        in-flow line begins at the same height where the float began a right float
        sits flush against the right content edge a left float against the left
        edge each offset by its own margin its width comes from the style sheet
        when one is declared or from the widest line of its content when the width
        is left automatic the same measurement the engine uses for a table column
        capped at the inner width of the page.</p>
    </body></html>"#;
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    // The fixture must actually wrap beside the float AND cross its bottom so
    // the segment boundary is exercised (otherwise the test proves nothing).
    let lines = page_lines(&layout, 0);
    assert!(
        lines.iter().any(|(_x, _y, w)| w.get() < inches(4.0).get() - 1.0),
        "some lines must be narrowed by the float"
    );
    // Normalize both sides: collapse every whitespace run to one space.
    let joined = joined_run_text(&layout);
    let joined_norm: String = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    let src: String = "Floated sidebar content with several words inside it. How does an engine place such a box the float is removed from the in-flow cursor entirely it does not advance the paragraph below it and the next in-flow line begins at the same height where the float began a right float sits flush against the right content edge a left float against the left edge each offset by its own margin its width comes from the style sheet when one is declared or from the widest line of its content when the width is left automatic the same measurement the engine uses for a table column capped at the inner width of the page."
        .to_string();
    assert_eq!(
        joined_norm, src,
        "segment reflow must tile the source text exactly (CORE-91)"
    );
}

// --- 8. Determinism ---------------------------------------------------------

const FIXTURE: &str = r#"<html><head><style>
    body { margin: 0; font-size: 14px; line-height: 1.6; }
    p, div { margin: 0; padding: 0; }
    .f { float: right; width: 2in; }
</style></head><body>
    <div class="f"><p>Floated sidebar content with several words inside it.</p></div>
    <p>Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.</p>
</body></html>"#;

#[test]
fn output_is_deterministic_with_floats() {
    let dir = tempfile::tempdir().unwrap();
    let html = dir.path().join("float.html");
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
    assert!(status.success(), "engine exited non-zero: {status:?}");
}

// --- CORE-145: floats paginating across pages ------------------------------

/// css-sizing-3 §5.1: a declared `height` sizes the box's flow extent. A
/// `height:0` div with a text line does NOT push following floats down —
/// the floats start at the div's own top (page-size-007/008: without this,
/// the float row starts below the line box and only half the floats fit
/// per page, inflating the page count 9 vs 6).
#[test]
fn zero_height_div_does_not_push_floats_down() {
    let html = r#"<html><head><style>
        body { margin: 0; }
        .zero { height: 0; }
        .float { float: left; width: 37.5pt; height: 45pt; }
        .container { display: flow-root; }
    </style></head><body>
        <div class="container">
            <div class="zero">first</div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
        </div>
    </body></html>"#;
    let dom = dom_of(html);
    let f0 = node_id_by_class(&dom, "float");
    // 5in x 3in page, 0.5in margins -> 288pt x 144pt content. The float row
    // must start at y=0 (the container's content top), not below the
    // zero-height div's line box (~14.4pt).
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_float_fragments(&layout, 0, f0);
    assert!(!frags.is_empty(), "first float placed on page 1");
    assert_close(
        frags[0].offset.y,
        Scalar(0.0),
        "first float starts at the container's content top",
    );
}

/// css2 §9.5.1 rules 2+7 + css-break-3: a float that has no free x lane
/// beside vertically-overlapping floats moves DOWN below them; when the
/// lowered row no longer fits the fragmentainer, the float defers to the
/// next fragmentainer (page-size-007/008 packing shape, scaled to Letter).
#[test]
fn float_row_wraps_then_overflows_to_next_page() {
    let html = r#"<html><head><style>
        body { margin: 0; }
        .float { float: left; width: 37.5pt; height: 80pt; }
        .container { display: flow-root; }
    </style></head><body>
        <div class="container">
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            second
        </div>
    </body></html>"#;
    // 288pt content width / 37.5pt = 7 per row. A second row (80pt) starts
    // at y=80 but 80+80=160 > 144pt content height, so row-2 floats defer
    // whole to page 2 (css2 §9.5.1 rule 7 lowered them, css-break-3 then
    // moved them to the next fragmentainer).
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    // Float fragments are exactly 37.5pt x 80pt blocks with a 0 y-offset in
    // the container subtree — count them per page by size.
    let mut float_ys: Vec<Vec<f64>> = Vec::new();
    for page in &layout.pages {
        let mut ys = Vec::new();
        fn walk(f: &Fragment, ys: &mut Vec<f64>) {
            // pt->px->pt through stylo drifts ~0.04pt; match loosely.
            if matches!(f.kind, FragmentKind::Block)
                && (f.size.0.get() - 37.5).abs() < 0.01
                && (f.size.1.get() - 80.0).abs() < 0.01
            {
                ys.push(f.offset.y.get());
            }
            for c in &f.children {
                walk(c, ys);
            }
        }
        walk(&page.root, &mut ys);
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        float_ys.push(ys);
    }
    assert_eq!(float_ys[0].len() + float_ys[1].len(), 9, "all nine floats placed");
    assert_eq!(float_ys[0].len(), 7, "one full row packs on page 1");
    assert_eq!(float_ys[1].len(), 2, "row-2 floats overflow to page 2");
    // Row 1 packs at y=0 — no phantom cursor below preceding content.
    assert_eq!(
        float_ys[0].iter().filter(|y| **y < EPS).count(),
        7,
        "row 1 has 7 floats at y=0"
    );
}

/// css-break-3 fill-to-edge: a box that continues past the page extends its
/// background to the fragmentainer bottom edge; only the LAST fragment ends
/// at the content edge (Chromium-verified, page-size-007 test page 1).
#[test]
fn continued_box_background_fills_to_fragmentainer_bottom() {
    let html = r#"<html><head><style>
        body { margin: 0; }
        .float { float: left; width: 37.5pt; height: 80pt; }
        .container { display: flow-root; background: yellow; }
    </style></head><body>
        <div class="container">
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            <div class="float"></div>
            second
        </div>
    </body></html>"#;
    let dom = dom_of(html);
    let cid = node_id_by_class(&dom, "container");
    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() >= 2, "container fragments across pages");
    // Page 1's container fragment is a MIDDLE fragment: its paint box
    // reaches the fragmentainer bottom (144pt content height at 0.5in
    // margins), even though its in-flow content continues.
    let mut found = Vec::new();
    find_source(&layout.pages[0].root, cid, &mut found);
    assert!(!found.is_empty(), "container fragment on page 1");
    let frag = found[0];
    assert_close(
        frag.size.1,
        inches(2.0),
        "middle fragment's background fills to the fragmentainer bottom",
    );
}
