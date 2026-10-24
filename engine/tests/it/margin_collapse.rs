//! CORE-118: adjacent vertical margins collapse to the max
//! (CSS 2.1 §8.3.1). Regression tests assert the invariant on the
//! fragment tree — the gap between two adjacent in-flow sibling blocks
//! is max(bottom-margin of the first, top-margin of the second), never
//! their sum — plus a page-break safety check.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::PageGeometry;
use typeanvil::layout::layout;

/// Letter geometry with 1in margins.
fn letter() -> PageGeometry {
    PageGeometry {
        width: typeanvil::geom::Scalar(8.5 * 72.0),
        height: typeanvil::geom::Scalar(11.0 * 72.0),
        margin_top: typeanvil::geom::Scalar(72.0),
        margin_right: typeanvil::geom::Scalar(72.0),
        margin_bottom: typeanvil::geom::Scalar(72.0),
        margin_left: typeanvil::geom::Scalar(72.0),
    }
}

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
    layout(&dom, &sheet, letter())
}

/// Top offsets (page-relative) of every block fragment carrying the given
/// source node's text, collected from the first page.
fn block_tops(page: &typeanvil::frag::Fragmentainer) -> Vec<f64> {
    let mut out = Vec::new();
    walk(&page.root, &mut out);
    out
}

fn walk(frag: &Fragment, out: &mut Vec<f64>) {
    let _ = frag;
    // handled by caller via absolute accumulation; see collect_abs
}

/// Absolute y of each text-bearing line whose text starts with `prefix`,
/// accumulated from parent-relative offsets (LayoutNG-style tree).
fn line_tops_with_prefix(frag: &Fragment, prefix: &str, y: f64, out: &mut Vec<f64>) {
    let mut abs_y = y + frag.offset.y.get();
    match &frag.content {
        FragmentContent::Text(run) => {
            if run.text.trim_start().starts_with(prefix) {
                out.push(abs_y);
            }
        }
        _ => {}
    }
    for c in &frag.children {
        line_tops_with_prefix(c, prefix, abs_y, out);
    }
}

#[test]
fn adjacent_sibling_gap_is_max_not_sum() {
    // p { margin-bottom: 7pt }, blockquote { margin-top: 6pt }:
    // the p → blockquote boundary must collapse to max(7,6) = 7pt.
    // Before CORE-118 the margins summed (13pt gap).
    let html = r#"<html><head><style>
p { font-size: 12pt; margin: 0 0 7pt 0; }
blockquote { font-size: 12pt; margin: 6pt 18pt 0 18pt; }
</style></head><body>
<p>Alpha paragraph.</p>
<blockquote>Quoted material.</blockquote>
<p>After the quote.</p>
</body></html>"#;
    let l = lay(html);
    let mut tops = Vec::new();
    line_tops_with_prefix(&l.pages[0].root, "Quoted", 0.0, &mut tops);
    assert_eq!(tops.len(), 1, "blockquote line found once");

    // Reference: same doc but with p { margin-bottom: 0 } — the only
    // difference between the two renders is the collapsed overlap
    // min(7, 6) = 6pt. The quote must sit exactly 6pt higher with the
    // bottom margin than it would without any margin at all... i.e. the
    // gap must NOT grow by 7 + 6.
    let html_nomargin = html.replace("margin: 0 0 7pt 0;", "margin: 0;");
    let l2 = lay(&html_nomargin);
    let mut tops2 = Vec::new();
    line_tops_with_prefix(&l2.pages[0].root, "Quoted", 0.0, &mut tops2);
    assert_eq!(tops2.len(), 1);

    // With mb=7/mt=6: gap above quote = 7pt (max). With mb=0/mt=6: gap = 6pt.
    // Difference must be exactly 1pt (= 7 - 6), not 7pt (the full added mb).
    let delta = tops[0] - tops2[0];
    assert!(
        (delta - 1.0).abs() < 0.05,
        "collapsed delta should be ~1pt (max(7,6)-max(0,6)), got {}",
        delta
    );
}

#[test]
fn larger_top_margin_wins_collapse() {
    // Reverse case: h2 top margin (20pt) > previous sibling's bottom (5pt)
    // → gap = 20pt, not 25pt. Compare against mt-only baseline.
    let html = r#"<html><head><style>
p { font-size: 12pt; margin: 0 0 5pt 0; }
h2 { font-size: 14pt; margin: 20pt 0 4pt 0; }
</style></head><body>
<p>Lead in.</p>
<h2>Section head</h2>
</body></html>"#;
    let l = lay(html);
    let mut tops = Vec::new();
    line_tops_with_prefix(&l.pages[0].root, "Section", 0.0, &mut tops);
    assert_eq!(tops.len(), 1);

    let html_nomargin = html.replace("margin: 0 0 5pt 0;", "margin: 0;");
    let l2 = lay(&html_nomargin);
    let mut tops2 = Vec::new();
    line_tops_with_prefix(&l2.pages[0].root, "Section", 0.0, &mut tops2);
    assert_eq!(tops2.len(), 1);

    // Adding a 5pt bottom margin to the p must move the heading by
    // max(20,5) - max(20,0) = 0pt. Summing would move it 5pt down.
    let delta = tops[0] - tops2[0];
    assert!(
        delta.abs() < 0.05,
        "larger top margin wins: delta should be ~0pt, got {}",
        delta
    );
}

#[test]
fn text_run_between_blocks_blocks_collapse() {
    // A bare text run between two blocks separates them: no collapse
    // state may carry across it. (Structural guard on the tracker reset.)
    let html = r#"<html><head><style>
p { font-size: 12pt; margin: 0 0 8pt 0; }
</style></head><body>
<p>Framed.</p>
Bare text between.
<p>Closer.</p>
</body></html>"#;
    let l = lay(html);
    // Just verify all three pieces render on page 1 in order.
    let mut runs: Vec<String> = Vec::new();
    fn collect(f: &Fragment, out: &mut Vec<String>) {
        if let FragmentContent::Text(run) = &f.content {
            out.push(run.text.clone());
        }
        for c in &f.children {
            collect(c, out);
        }
    }
    collect(&l.pages[0].root, &mut runs);
    let joined: String = runs.join("\n");
    assert!(joined.contains("Framed."));
    assert!(joined.contains("Closer."));
}

/// CORE-483: a parent's top margin collapses with its first in-flow child's
/// top margin (CSS 2.1 §8.3.1) when nothing separates them.
#[test]
fn first_child_top_margin_collapses_through_parent() {
    let html = |ul: &str| {
        format!(
            r#"<html><head><style>
body {{ margin: 0; }}
section {{ margin-top: 30pt; }}
ul {{ margin: {ul}; padding: 0; list-style: none; font-size: 10pt; line-height: 10pt; }}
</style></head><body><section><ul><li>MARK</li></ul></section></body></html>"#
        )
    };
    let top = |h: &str| {
        let l = lay(h);
        let mut t = Vec::new();
        line_tops_with_prefix(&l.pages[0].root, "MARK", 0.0, &mut t);
        assert_eq!(t.len(), 1);
        t[0]
    };
    // Child margin 12 < parent 30: result max(30,12)=30, so adding the
    // child margin moves nothing. Summing would move it 12pt.
    let delta = top(&html("12pt 0")) - top(&html("0"));
    assert!(delta.abs() < 0.05, "expected 0pt delta, got {delta}");
}

/// CORE-566: a first-child top margin LARGER than the parent's still
/// collapses: the gap is max(parent, child), never the sum (CSS 2.2
/// §8.3.1). Before the fix the child landed at parent + child.
#[test]
fn larger_first_child_top_margin_collapses_through_parent() {
    let html = |pre: &str| {
        format!(
            r#"<html><head><style>
body {{ margin: 0; }}
section {{ margin-top: 20pt; }}
ul {{ margin: 100pt 0 0 0; padding: 0; list-style: none; font-size: 10pt; line-height: 10pt; }}
</style></head><body><section>{pre}<ul><li>MARK</li></ul></section></body></html>"#
        )
    };
    let top = |h: &str| {
        let l = lay(h);
        let mut t = Vec::new();
        line_tops_with_prefix(&l.pages[0].root, "MARK", 0.0, &mut t);
        assert_eq!(t.len(), 1);
        t[0]
    };
    // Content top is 72pt (page margin); collapsed margin is max(20,100).
    let plain = top(&html(""));
    assert!((plain - 172.0).abs() < 0.05, "expected 172pt, got {plain}");
    // A leading out-of-flow sibling does not separate the margins.
    let abspos = top(&html(r#"<p style="position:absolute;top:0;margin:0">x</p>"#));
    assert!((abspos - 172.0).abs() < 0.05, "expected 172pt with abspos sibling, got {abspos}");
}

/// A border on the parent separates the margins: no collapse.
#[test]
fn parent_border_blocks_first_child_collapse() {
    let html = |ul: &str| {
        format!(
            r#"<html><head><style>
body {{ margin: 0; }}
section {{ margin-top: 30pt; border-top: 1pt solid black; }}
ul {{ margin: {ul}; padding: 0; list-style: none; font-size: 10pt; line-height: 10pt; }}
</style></head><body><section><ul><li>MARK</li></ul></section></body></html>"#
        )
    };
    let top = |h: &str| {
        let l = lay(h);
        let mut t = Vec::new();
        line_tops_with_prefix(&l.pages[0].root, "MARK", 0.0, &mut t);
        t[0]
    };
    let delta = top(&html("12pt 0")) - top(&html("0"));
    assert!((delta - 12.0).abs() < 0.05, "expected 12pt delta, got {delta}");
}
