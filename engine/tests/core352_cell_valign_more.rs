//! CORE-352 follow-up: bottom alignment lands content on the row bottom, and
//! `baseline` aligns first-line baselines across cells (CSS 2.1 §17.5.4).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn runs(html: &str) -> Vec<(String, f64)> {
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    let i = |v: f64| Scalar(v * 72.0);
    let l = layout(
        &dom,
        &sheet,
        PageGeometry {
            width: i(5.0),
            height: i(3.0),
            margin_top: i(0.5),
            margin_right: i(0.5),
            margin_bottom: i(0.5),
            margin_left: i(0.5),
        },
    );
    fn rec(f: &Fragment, y: f64, out: &mut Vec<(String, f64)>) {
        // Text baselines are parent-relative (pdf.rs): exclude the run's own offset.
        if let FragmentContent::Text(r) = &f.content {
            out.push((r.text.clone(), y + r.baseline.y.get()));
        }
        let y = y + f.offset.y.get();
        for c in &f.children {
            rec(c, y, out);
        }
    }
    let mut out = Vec::new();
    for c in &l.pages[0].root.children {
        rec(c, 0.0, &mut out);
    }
    out
}

fn y_of(r: &[(String, f64)], word: &str) -> f64 {
    r.iter().find(|(t, _)| t.contains(word)).unwrap_or_else(|| panic!("no run {word}")).1
}

#[test]
fn bottom_cell_baseline_equals_last_line_baseline_of_tall_cell() {
    // Same font size: a bottom-aligned one-line cell shares the baseline of
    // the tall cell's LAST line, i.e. its content bottom sits on the row bottom.
    let html = r#"<!DOCTYPE html><html><head><style>
table { border-collapse: collapse; width: 100%; }
td { padding: 0; font-size: 10pt; line-height: 12pt; }
</style></head><body><table><tr>
<td>alpha<br>bravo<br>charlie</td>
<td style="vertical-align: bottom">BOT</td>
</tr></table></body></html>"#;
    let r = runs(html);
    let last = y_of(&r, "charlie");
    let bot = y_of(&r, "BOT");
    assert!((bot - last).abs() < 0.1, "bottom content must end on the row bottom: {bot} vs {last}");
}

#[test]
fn baseline_cells_share_first_line_baseline() {
    let html = r#"<!DOCTYPE html><html><head><style>
table { border-collapse: collapse; width: 100%; }
td { padding: 0; vertical-align: baseline; line-height: normal; }
</style></head><body><table><tr>
<td style="font-size: 24pt">BIG</td>
<td style="font-size: 10pt">small</td>
</tr></table></body></html>"#;
    let r = runs(html);
    let big = y_of(&r, "BIG");
    let small = y_of(&r, "small");
    assert!((big - small).abs() < 0.1, "baselines must align: {big} vs {small}");
}
