//! CORE-116 regression tests — an oversized table row must not drop content.
//!
//! `layout_table_row` used to return `outgoing: None` unconditionally, so a
//! cell whose content crossed a fragmentainer boundary lost its continuation
//! (and any following rows could vanish when the row took the deferral
//! path). These tests pin the invariant: every line of every cell renders
//! exactly once, and the table resumes after an oversized row.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

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
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, geo)
}

/// Every TextRun fragment text across ALL pages, in order.
fn all_text(l: &Layout) -> String {
    fn rec(f: &Fragment, out: &mut String) {
        if let FragmentContent::Text(run) = &f.content {
            out.push_str(&run.text);
            out.push(' ');
        }
        for c in &f.children {
            rec(c, out);
        }
    }
    let mut out = String::new();
    for p in &l.pages {
        rec(&p.root, &mut out);
    }
    out
}

/// A cell taller than one full fragmentainer (monolithic path): its lines
/// continue on the next page instead of vanishing, and later rows survive.
#[test]
fn oversized_row_cell_content_continues_across_pages() {
    // 3in x 2in with 0.5in margins → 144pt-tall fragmentainer. The giant
    // cell's 40pt-font lines exceed one page of height; the old code dropped
    // everything past the first page inside that cell's row.
    let html = r#"<!DOCTYPE html><html><head><style>
        body { margin: 0; }
        table { border-collapse: collapse; width: 100%; }
        td { padding: 4pt; font-size: 12pt; }
        td.giant { font-size: 40pt; }
    </style></head><body><table>
      <tr><td>row 1 normal</td><td>a</td></tr>
      <tr><td class="giant">GIANT ROW two three four five six seven eight nine ten eleven twelve lines tall here to exceed the fragmentainer height for sure yes indeed quite tall</td><td>b</td></tr>
      <tr><td>row 3 after giant</td><td>c</td></tr>
      <tr><td>row 10 last</td><td>j</td></tr>
    </table></body></html>"#;
    let l = lay(html, geometry(3.0, 2.0, 0.5));

    assert!(l.pages.len() >= 2, "table should span pages");
    let text = all_text(&l);
    // Every word of the giant cell survives somewhere in the render.
    for word in ["GIANT", "ROW", "twelve", "fragmentainer", "tall"] {
        assert!(text.contains(word), "lost giant-cell text: {word:?}");
    }
    // Rows AFTER the oversized row still render, exactly once each.
    assert_eq!(text.matches("row 3 after giant").count(), 1);
    assert_eq!(text.matches("row 10 last").count(), 1);
    // And nothing from the first row duplicated across the resume.
    assert_eq!(text.matches("row 1 normal").count(), 1);
}

/// A resumed row must not re-render cells that already finished on the
/// previous page (the resume pass walks the same row again).
#[test]
fn oversized_row_resume_does_not_duplicate_finished_cells() {
    let html = r#"<!DOCTYPE html><html><head><style>
        body { margin: 0; }
        table { border-collapse: collapse; width: 100%; }
        td { padding: 4pt; font-size: 12pt; }
        td.giant { font-size: 40pt; }
    </style></head><body><table>
      <tr><td>first</td><td class="giant">GIANT two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</td><td>last</td></tr>
    </table></body></html>"#;
    let l = lay(html, geometry(3.0, 2.0, 0.5));
    let text = all_text(&l);
    // The sibling cells render once, not again on the continuation page.
    assert_eq!(
        text.matches("first").count(),
        1,
        "finished sibling cell re-rendered on resume"
    );
    assert_eq!(
        text.matches("last").count(),
        1,
        "finished sibling cell re-rendered on resume"
    );
    // The breaking cell itself starts only once ("GIANT" opens the run).
    assert_eq!(text.matches("GIANT").count(), 1);
}
