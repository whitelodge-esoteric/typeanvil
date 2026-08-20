//! Fragmentation-core acceptance tests — one per acceptance criterion in
//! `docs/specifications/fragmentation-core.spec.md`.
//!
//! These drive the library directly (`typeanvil::layout::layout`) and assert on
//! the fragment tree, plus one end-to-end determinism test through the CLI that
//! mirrors the existing smoke pattern.

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent, Fragmentainer};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

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

/// Lay out an HTML string with the given geometry.
fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = Dom::parse(html).expect("parse html");
    // Pull inline <style> text into the author sheet, mirroring the CLI.
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let typeanvil::dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, geo)
}

/// Collect all text-run strings on one fragmentainer, in pre-order (document
/// order), so reassembling equals the source reading order.
fn page_lines(page: &Fragmentainer) -> Vec<String> {
    let mut out = Vec::new();
    collect_text(&page.root, &mut out);
    out
}

fn collect_text(frag: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &frag.content {
        out.push(run.text.clone());
    }
    for c in &frag.children {
        collect_text(c, out);
    }
}

/// All text runs across every page, in page-then-preorder order.
fn all_lines(layout: &Layout) -> Vec<String> {
    layout.pages.iter().flat_map(|p| page_lines(p)).collect()
}

// --- 1. Forced break -------------------------------------------------------

#[test]
fn forced_break_page() {
    let html = r#"<html><head><style>
        #two { break-before: page; }
    </style></head><body>
        <section id="one"><p>Section one alpha.</p></section>
        <section id="two"><p>Section two beta.</p></section>
    </body></html>"#;

    let layout = lay(html, geometry(5.0, 3.0, 0.5));
    assert_eq!(layout.pages.len(), 2, "expected exactly two pages");

    let p1 = page_lines(&layout.pages[0]).join(" ");
    let p2 = page_lines(&layout.pages[1]).join(" ");

    assert!(
        p1.contains("Section one"),
        "page 1 should hold section one, got: {p1:?}"
    );
    assert!(
        !p1.contains("Section two"),
        "page 1 must NOT hold section two, got: {p1:?}"
    );
    assert!(
        p2.contains("Section two"),
        "page 2 should hold section two, got: {p2:?}"
    );
    assert!(
        !p2.contains("Section one"),
        "page 2 must NOT hold section one, got: {p2:?}"
    );
}

// --- 2. Unforced overflow: split preserves content -------------------------

#[test]
fn unforced_split_content_preserved() {
    // A paragraph of many short words, far taller than one small page.
    let words: Vec<String> = (0..400).map(|i| format!("w{i}")).collect();
    let source = words.join(" ");
    let html = format!(
        "<html><body><p>{source}</p></body></html>",
        source = source
    );

    let layout = lay(&html, geometry(5.0, 3.0, 0.5));
    assert!(
        layout.pages.len() > 1,
        "paragraph should split across pages, saw {}",
        layout.pages.len()
    );

    // Reassemble every laid-out line, in order, and compare word multisets to
    // the source: no line clipped, no word lost or duplicated.
    let reassembled = all_lines(&layout).join(" ");
    let got: Vec<&str> = reassembled.split_whitespace().collect();
    let want: Vec<&str> = source.split_whitespace().collect();
    assert_eq!(
        got, want,
        "reassembled fragment text must equal source with no loss/duplication"
    );
}

// --- 3. break-inside: avoid moves the whole block --------------------------

#[test]
fn avoid_moves_block() {
    // A keep-block that would otherwise split across the page boundary: with
    // `break-inside: avoid` the whole block must move to the next page, even
    // though it fits a full (fresh) page. We prove it by contrast: the same
    // markup WITHOUT avoid splits onto page 1; WITH avoid it does not.
    //
    // CORE-92: `body { margin: 0 }` is pinned so the fixture does not depend
    // on the UA body-margin default (which changed 8px → 0 to match Prince);
    // page height 1.5in makes the keep block genuinely overflow page 1.
    let ktext = (0..8).map(|i| format!("k{i}")).collect::<Vec<_>>().join(" ");
    let no_avoid = format!(
        r#"<html><head><style>body{{margin:0;}}</style></head><body><p>Filler.</p><div><p>{ktext}</p></div></body></html>"#
    );
    let with_avoid = format!(
        r#"<html><head><style>body{{margin:0;}} .keep{{break-inside:avoid;}}</style></head>
           <body><p>Filler.</p><div class="keep"><p>{ktext}</p></div></body></html>"#
    );

    // Narrow, short page: the keep paragraph wraps to several lines and cannot
    // fit beside the filler, but does fit alone on a fresh page.
    let geo = geometry(1.0, 1.5, 0.15);

    // Baseline: without avoid, the block splits — page 1 holds some keep lines.
    let base = lay(&no_avoid, geo);
    let base_p1 = page_lines(&base.pages[0]).join(" ");
    assert!(
        base_p1.contains('k'),
        "sanity: without avoid the block should split onto page 1, got: {base_p1:?}"
    );

    // With avoid: the whole block moves off page 1.
    let layout = lay(&with_avoid, geo);
    assert!(
        layout.pages.len() >= 2,
        "expected the avoid block to move to a new page"
    );
    let p1 = page_lines(&layout.pages[0]).join(" ");
    assert!(p1.contains("Filler"), "page 1 should keep the filler, got: {p1:?}");
    assert!(
        !p1.contains('k'),
        "avoid block must not appear on page 1 (moved whole), got: {p1:?}"
    );

    // The whole block lands together on one later page (all 8 tokens present).
    let landed = layout
        .pages
        .iter()
        .find(|pg| page_lines(pg).iter().any(|l| l.contains("k0")))
        .expect("keep block should appear on some page");
    let joined = page_lines(landed).join(" ");
    for i in 0..8 {
        assert!(
            joined.contains(&format!("k{i}")),
            "the whole avoid block must be on one page; missing k{i} in {joined:?}"
        );
    }
}

// --- 4. Orphans / widows ---------------------------------------------------

#[test]
fn orphans_widows() {
    // orphans: 2; widows: 2 on a paragraph that splits across pages: no page
    // may end or begin with a single line of that paragraph.
    let words: Vec<String> = (0..120).map(|i| format!("z{i}")).collect();
    let source = words.join(" ");
    let html = format!(
        r#"<html><head><style>
            p {{ orphans: 2; widows: 2; }}
        </style></head><body><p>{source}</p></body></html>"#,
        source = source
    );

    let layout = lay(&html, geometry(5.0, 3.0, 0.5));
    assert!(layout.pages.len() > 1, "paragraph must split for the test");

    // Count paragraph lines per page. Every page that holds part of the split
    // paragraph must have >= 2 lines (no single-line orphan/widow).
    for (i, page) in layout.pages.iter().enumerate() {
        let n = page_lines(page).len();
        if n > 0 {
            assert!(
                n >= 2,
                "page {i} has {n} line(s); orphans/widows require >= 2"
            );
        }
    }
}

// --- 5. Monolithic overflow ------------------------------------------------

#[test]
fn monolithic_overflow_no_slice() {
    // A single line whose text is far wider than the page: it is one line box
    // (monolithic). It must be placed (overflowing) and layout must terminate.
    // We give it one very long unbreakable-ish run of words but the key is the
    // line height dwarfs the page: use a large font on a tiny page.
    let html = r#"<html><head><style>
        p { font-size: 200px; }
    </style></head><body><p>Giant</p></body></html>"#;

    // Tiny page: content box height is smaller than one 200px line.
    let layout = lay(html, geometry(1.0, 1.0, 0.1));
    // Terminates (did not hang) and produced at least one page holding the line.
    assert_eq!(layout.pages.len(), 1, "monolithic content should not spawn extra pages");
    let lines = page_lines(&layout.pages[0]);
    assert_eq!(lines.len(), 1, "the giant line is placed once, not sliced");
    assert!(lines[0].contains("Giant"));

    // The line box overflows the content box: assert its height exceeds the
    // page content height (proof it was not clipped/sliced to fit).
    let content_h = layout.pages[0].root.size.1;
    let line_h = find_line_height(&layout.pages[0].root);
    assert!(
        line_h.get() > 0.0 && line_h.get() > content_h.get() * 0.5,
        "line height {line_h:?} should overflow small page {content_h:?}"
    );
}

fn find_line_height(frag: &Fragment) -> Scalar {
    if matches!(frag.content, FragmentContent::Text(_)) {
        return frag.size.1;
    }
    for c in &frag.children {
        let h = find_line_height(c);
        if h.get() > 0.0 {
            return h;
        }
    }
    Scalar::ZERO
}

// --- 6. O(n) pagination ----------------------------------------------------

/// Build a synthetic doc of `sections` forced-break sections.
fn synthetic(sections: usize) -> String {
    let mut html = String::from(
        "<html><head><style>section { break-before: page; }</style></head><body>",
    );
    for i in 0..sections {
        html.push_str(&format!("<section><p>Section {i} content line.</p></section>"));
    }
    html.push_str("</body></html>");
    html
}

#[test]
fn pagination_linear_1000() {
    use std::time::Instant;

    let geo = geometry(5.0, 3.0, 0.5);

    // First section has no break-before effect (nothing precedes it), so N
    // sections => N pages.
    let t100 = {
        let html = synthetic(100);
        let start = Instant::now();
        let layout = lay(&html, geo);
        let dt = start.elapsed();
        assert_eq!(layout.pages.len(), 100, "100 sections => 100 pages");
        dt
    };

    let t1000 = {
        let html = synthetic(1000);
        let start = Instant::now();
        let layout = lay(&html, geo);
        let dt = start.elapsed();
        assert_eq!(layout.pages.len(), 1000, "1000 sections => 1000 pages");
        dt
    };

    // Loose linear bound: 10x the pages should be well under 30x the time.
    // Guard against a zero baseline on very fast machines.
    let base = t100.as_nanos().max(1);
    let ratio = t1000.as_nanos() as f64 / base as f64;
    assert!(
        ratio < 30.0,
        "pagination scaled super-linearly: 100-page {t100:?}, 1000-page {t1000:?} (ratio {ratio:.1}x)"
    );
}

// --- 7. Determinism (multi-page, through the CLI) --------------------------

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typeanvil")
}

fn render_cli(html: &Path, out: &Path) {
    let status = Command::new(bin())
        .args([
            "render",
            html.to_str().unwrap(),
            "--page-width",
            "5in",
            "--page-height",
            "3in",
            "--margin-top",
            "0.5in",
            "--margin-right",
            "0.5in",
            "--margin-bottom",
            "0.5in",
            "--margin-left",
            "0.5in",
            "--base-url",
            "http://127.0.0.1:9/",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("failed to spawn typeanvil");
    assert!(status.success(), "engine exited non-zero: {status:?}");
}

#[test]
fn determinism_multi_page() {
    // A multi-page document (forced breaks) rendered twice must be byte-equal.
    let html = synthetic(5);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("multi.html");
    std::fs::write(&src, &html).unwrap();

    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render_cli(&src, &a);
    render_cli(&src, &b);

    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert_eq!(ba, bb, "multi-page PDF output is not byte-identical across runs");
    // Sanity: it really is multi-page.
    let hay = String::from_utf8_lossy(&ba);
    let pages = hay.matches("/Type /Page\n").count() + hay.matches("/Type/Page/").count();
    assert!(pages == 0 || pages >= 2, "expected multi-page PDF, saw {pages} markers");
}
