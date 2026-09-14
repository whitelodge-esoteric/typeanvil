//! CORE-176: grid/flex references rendered extra pages — one empty, one
//! duplicated.
//!
//! Three defects, three invariants:
//!
//! 1. `measure_block` ADDED a declared extent to content (a `height: 9em`
//!    flex child with three 1em text lines measured 9em + 3 lines). The used
//!    height is the GREATER of the two (css-sizing-3 §4) — content lives
//!    INSIDE a declared box. Invariant: a declared-height box with small
//!    content measures exactly its declared extent (plus chrome).
//! 2. Grid rows past a definite-height container's box fragmented instead
//!    of painting as ink overflow on the same page (css-break-3: a box that
//!    fits the fragmentainer does not fragment; overflow is ink).
//! 3. The trailing-blank-page drop was dead code. Invariant: no page in the
//!    output paints nothing UNLESS a forced break demanded it (Chromium
//!    keeps a demanded final blank page for `break-after: page`).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::PageGeometry;
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    // 5in x 3in page, no margins — the harness PageSpec for print reftests
    // minus the 0.5in margins (these fixtures declare @page margin: 0).
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar::ZERO,
        margin_right: Scalar::ZERO,
        margin_bottom: Scalar::ZERO,
        margin_left: Scalar::ZERO,
    }
}

use typeanvil::geom::Scalar;

fn lay(html: &str) -> typeanvil::layout::Layout {
    let dom = Dom::parse(html).expect("parse html");
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
    layout(&dom, &sheet, geo())
}

/// The issue's minimal repro: a definite-height grid whose auto row holds a
/// 6in child. Must render ONE page (ink overflow, no second page).
#[test]
fn overflowing_grid_does_not_emit_a_blank_trailing_page() {
    let html = r#"<style>
        body { margin: 0 }
        .g { display: grid; grid-template-rows: 1em auto 1em; height: 216pt }
        .g > div { background: lime }
    </style>
    <div class="g"><div></div><div style="height: 432pt; background: hotpink"></div><div></div></div>"#;
    let laid = lay(html);
    assert_eq!(
        laid.pages.len(),
        1,
        "a definite-height grid whose content overflows must paint the overflow on ONE page, got {}",
        laid.pages.len()
    );
}

/// The margin-boxes refs' shape (dimensions-004-print-ref): a 100vh grid
/// body with fixed + auto rows sized to fill the page exactly. The auto row
/// holds a flex column of DECLARED-height children with text lines inside.
/// With the measure bug the row inflated and the body fragmented; now the
/// page count is 1.
#[test]
fn grid_body_with_declared_height_flex_children_fits_one_page() {
    let html = r#"<style>
        body { display: grid; grid-template-rows: 48pt auto 96pt; height: 216pt; margin: 0 }
        .edge { display: flex; flex-flow: column }
    </style>
    <div class="edge" style="height: 48pt"></div>
    <div class="edge">
      <div style="height: 108pt; background: yellow">x<br>x<br>x</div>
      <div style="height: 72pt; background: hotpink">x<br>x</div>
    </div>
    <div class="edge" style="height: 96pt"></div>"#;
    let laid = lay(html);
    assert_eq!(
        laid.pages.len(),
        1,
        "rows sized from declared-height flex children must fill exactly one page, got {}",
        laid.pages.len()
    );
}

/// Fix 1's invariant directly: a declared-height box's measure equals its
/// declared extent even when it contains content lines (content competes,
/// never adds).
#[test]
fn declared_height_measures_extent_not_extent_plus_content() {
    // The grid auto row takes the max item height. The item declares
    // height: 100pt and holds two 12pt text lines: the row must measure
    // 100pt, not 124pt. If it measured 124pt, rows would overflow the
    // 216pt container (100 + 124 > 216) and fragment into 2 pages.
    // `body { margin: 0 }` pins the container's fit against the UA margin
    // (CORE-153): with the default 8px the 216pt container itself would
    // not fit the page and fragmenting would be CORRECT.
    let html = r#"<style>
        body { margin: 0 }
        .g { display: grid; grid-template-rows: auto auto; height: 216pt }
    </style>
    <div class="g">
      <div style="height: 100pt">line one<br>line two</div>
      <div style="height: 100pt"></div>
    </div>"#;
    let laid = lay(html);
    assert_eq!(
        laid.pages.len(), 1,
        "declared-height content must measure its extent (100pt + 100pt fits 216pt), got {} pages",
        laid.pages.len()
    );
}

/// A TRAILING forced break is absorbed: a single div with `break-after:
/// page` renders ONE page (Chromium oracle — the used value of a forced
/// break at the end of the document produces no page; basic-pagination-001
/// codifies this). The blank-page drop must not be defeated by a forced
/// break that demanded nothing.
#[test]
fn trailing_forced_break_is_absorbed() {
    let html = r#"<div style="break-after: page">one</div>"#;
    let laid = lay(html);
    assert_eq!(
        laid.pages.len(), 1,
        "a trailing forced break produces no page (Chromium), got {}",
        laid.pages.len()
    );
}

/// A final page that actually paints is never dropped: text on the last
/// page survives.
#[test]
fn a_final_page_that_paints_is_kept() {
    let html = r#"<p>page one</p><div style="break-before: page">page two content</div>"#;
    let laid = lay(html);
    assert_eq!(laid.pages.len(), 2);
    let page2 = &laid.pages[1];
    let mut texts = Vec::new();
    collect_texts(&page2.root, &mut texts);
    assert!(
        texts.iter().any(|t| t.contains("page two")),
        "the final page's text must survive the blank-page drop"
    );
}

fn collect_texts(f: &typeanvil::frag::Fragment, out: &mut Vec<String>) {
    if let typeanvil::frag::FragmentContent::Text(run) = &f.content {
        out.push(run.text.clone());
    }
    for c in &f.children {
        collect_texts(c, out);
    }
}

/// An empty document still renders exactly one page (the fallback below the
/// page loop must not interact with the drop).
#[test]
fn an_empty_document_still_has_one_page() {
    let laid = lay("<p></p>");
    assert_eq!(laid.pages.len(), 1);
}
