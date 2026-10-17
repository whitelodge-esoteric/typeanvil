//! CORE-352 regression: table cells honor `vertical-align` (CSS 2.1 §17.5.4).
//! UA default is `middle`; `top` keeps content at the cell top; `bottom`
//! pushes it to the row's bottom. Assertions are relative to the `top` cell
//! in the same row, so font metrics do not matter.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

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
    let l = layout(
        &dom,
        &sheet,
        PageGeometry {
            width: inches(5.0),
            height: inches(3.0),
            margin_top: inches(0.5),
            margin_right: inches(0.5),
            margin_bottom: inches(0.5),
            margin_left: inches(0.5),
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

const HTML: &str = r#"<!DOCTYPE html><html><head><style>
table { border-collapse: collapse; width: 100%; }
td { padding: 0; font-size: 10pt; }
</style></head><body><table><tr>
<td>TALL one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen</td>
<td>MID</td>
<td style="vertical-align: bottom">BOT</td>
<td style="vertical-align: top">TOP</td>
</tr></table></body></html>"#;

#[test]
fn cells_honor_vertical_align() {
    let r = runs(HTML);
    let top = y_of(&r, "TOP");
    let mid = y_of(&r, "MID");
    let bot = y_of(&r, "BOT");
    assert!(mid - top > 5.0, "default (middle) must sit below top: mid={mid} top={top}");
    assert!(bot - mid > 5.0, "bottom must sit below middle: bot={bot} mid={mid}");
    let first = y_of(&r, "TALL");
    assert!((top - first).abs() < 0.5, "top cell shares the first line: {top} vs {first}");
}

#[test]
fn row_level_vertical_align_is_inherited_by_cells() {
    let html = HTML.replace("<td>MID</td>", "<td>MID</td>").replace(
        "<tr>",
        "<tr style=\"vertical-align: top\">",
    );
    let r = runs(&html);
    let top = y_of(&r, "TOP");
    let mid = y_of(&r, "MID");
    assert!((mid - top).abs() < 0.5, "row top must apply to MID: {mid} vs {top}");
}
