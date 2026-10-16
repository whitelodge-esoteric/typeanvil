//! CORE-356 regression: in auto table layout a wrapping cell with a specified
//! `width` contributes that width as its max-content, so the surplus width is
//! shared among the auto columns (css-tables-3), not given to the fixed one.
//! Chromium widths for the probe: 69 / 69 / 75 / 74 pt.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn xs(html: &str) -> Vec<(String, f64)> {
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
    fn rec(f: &Fragment, x: f64, out: &mut Vec<(String, f64)>) {
        if let FragmentContent::Text(r) = &f.content {
            out.push((r.text.clone(), x + r.baseline.x.get()));
        }
        let x = x + f.offset.x.get();
        for c in &f.children {
            rec(c, x, out);
        }
    }
    let mut out = Vec::new();
    for c in &l.pages[0].root.children {
        rec(c, 0.0, &mut out);
    }
    out
}

fn x_of(r: &[(String, f64)], word: &str) -> f64 {
    r.iter().find(|(t, _)| t.contains(word)).unwrap_or_else(|| panic!("no run {word}")).1
}

fn html(cols: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
body {{ font-family: Arial, sans-serif; font-size: 9pt; margin: 0; }}
table {{ border-collapse: collapse; width: 100%; }}
td {{ border: 0.5pt solid #888; padding: 2pt 4pt; }}
td.w {{ width: 60pt; }}
</style></head><body><table><tr>{cols}</tr></table></body></html>"#
    )
}

/// Offset from layout-space x to PDF text-layer x (already page-absolute).
const MARGIN: f64 = 0.0;

/// One matrix case: fixed-width cells, then auto cells. Marker x positions
/// must match the Chromium print-to-PDF numbers within 2pt (probe matrix in
/// ~/workspace/typeanvil-evidence/CORE-356/matrix).
fn check(fixed: &[&str], auto: &[&str], chromium: &[f64]) {
    let mut cells = String::new();
    for t in fixed {
        cells.push_str(&format!("<td class=\"w\">{t}</td>"));
    }
    for t in auto {
        cells.push_str(&format!("<td>{t}</td>"));
    }
    let r = xs(&html(&cells));
    let words: Vec<&str> = fixed
        .iter()
        .map(|t| t.split_whitespace().next().unwrap())
        .chain(auto.iter().copied())
        .collect();
    assert_eq!(words.len(), chromium.len());
    for (w, c) in words.iter().zip(chromium) {
        let x = x_of(&r, w) + MARGIN;
        assert!((x - c).abs() <= 2.0, "{w}: TA x={x} vs Chromium {c}");
    }
}

const FA: &str = "Fxa wraps across several lines here";
const FB: &str = "Fxb another wrapping long cell text";
const FC: &str = "Fxc yet more wrapping words here";

#[test]
fn one_fixed_three_auto_matches_chromium() {
    check(&[FA], &["Aa", "Bbbbbb", "Ccc"], &[41.5, 109.5, 161.7, 265.2]);
}

#[test]
fn one_fixed_two_and_four_auto_match_chromium() {
    check(&[FA], &["Aa", "Bbbbbb"], &[41.5, 109.5, 182.7]);
    check(&[FA], &["Aa", "Bbbbbb", "Ccc", "Dddddddddd"], &[41.5, 109.5, 140.1, 200.2, 237.2]);
}

#[test]
fn two_fixed_auto_columns_match_chromium() {
    check(&[FA, FB], &["Aa", "Bbbbbb"], &[41.5, 110.2, 178.2, 228.6]);
    check(&[FA, FB], &["Aa", "Bbbbbb", "Ccc"], &[41.5, 110.2, 178.2, 214.2, 285.1]);
    check(&[FA, FB], &["Aa", "Bbbbbb", "Ccc", "Dddddddddd"], &[41.5, 110.2, 178.2, 199.4, 240.6, 266.0]);
}

#[test]
fn zero_fixed_control_matches_chromium() {
    check(&[], &["Aa", "Bbbbbb", "Ccc"], &[40.7, 109.1, 245.3]);
}

#[test]
fn all_fixed_matches_chromium() {
    check(&[FA, FB, FC], &[], &[41.5, 137.2, 233.0]);
}
