//! CORE-363: MathML Core layout (spec `math-rendering.spec.md`).
//!
//! Tests walk the fragment tree and assert the invariants of acceptance
//! criteria 1-5: raised superscript, stacked fraction with a rule, radical
//! sign plus overbar, inline baseline, tagged `Formula` with alt text.

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(612.0),
        height: Scalar(792.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

/// Absolute (x, baseline y, text) for every text run, and (x, y, w, h) for
/// every background rule, in document order.
#[derive(Default)]
struct Items {
    texts: Vec<(f64, f64, String)>,
    rules: Vec<(f64, f64, f64, f64)>,
}

fn walk(f: &Fragment, px: f64, py: f64, out: &mut Items) {
    let ax = px + f.offset.x.get();
    let ay = py + f.offset.y.get();
    match &f.content {
        FragmentContent::Text(run) => {
            out.texts
                .push((px + run.baseline.x.get(), py + run.baseline.y.get(), run.text.clone()));
        }
        FragmentContent::Background(_) => {
            out.rules.push((ax, ay, f.size.0.get(), f.size.1.get()));
        }
        _ => {}
    }
    for c in &f.children {
        walk(c, ax, ay, out);
    }
}

fn items(body: &str) -> Items {
    let html = format!("<html><body>{body}</body></html>");
    let dom = Dom::parse(&html).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(""), geometry());
    let mut out = Items::default();
    walk(&laid.pages[0].root, 0.0, 0.0, &mut out);
    out
}

fn find<'a>(it: &'a Items, needle: &str) -> &'a (f64, f64, String) {
    it.texts
        .iter()
        .find(|t| t.2 == needle)
        .unwrap_or_else(|| panic!("no run {needle:?} in {:?}", it.texts))
}

#[test]
fn superscript_is_raised() {
    let it = items("<math><mi>c</mi><msup><mi>x</mi><mn>2</mn></msup></math>");
    let base = find(&it, "\u{1D465}"); // math italic x
    let sup = find(&it, "2");
    assert!(
        sup.1 < base.1 - 2.0,
        "superscript baseline {} not above base {}",
        sup.1,
        base.1
    );
}

#[test]
fn fraction_is_stacked_with_rule() {
    let it = items("<math display=\"block\"><mfrac><mn>1</mn><mn>3</mn></mfrac></math>");
    let n = find(&it, "1");
    let d = find(&it, "3");
    assert!(!it.rules.is_empty(), "no fraction rule");
    let rule_y = it.rules[0].1;
    assert!(n.1 < rule_y, "numerator baseline {} not above rule {}", n.1, rule_y);
    assert!(d.1 > rule_y, "denominator baseline {} not below rule {}", d.1, rule_y);
}

#[test]
fn radical_has_sign_and_bar() {
    let it = items("<math><msqrt><mn>2</mn></msqrt></math>");
    assert!(
        it.texts.iter().any(|t| t.2 == "\u{221A}"),
        "no radical sign: {:?}",
        it.texts
    );
    let two = find(&it, "2");
    let bar = it.rules.first().expect("no overbar rule");
    assert!(bar.1 < two.1, "overbar not above content baseline");
    assert!(bar.2 > 0.0);
}

#[test]
fn inline_math_sits_on_baseline() {
    let it = items("<p>Energy <math><mi>E</mi><mo>=</mo><mn>5</mn></math> end</p>");
    let word = find(&it, "Energy");
    let five = find(&it, "5");
    assert!(
        (word.1 - five.1).abs() < 0.5,
        "math baseline {} differs from text baseline {}",
        five.1,
        word.1
    );
}

fn inflated(pdf: &[u8]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = pdf[at..].windows(6).position(|w| w == b"stream") {
        let mut s = at + i + 6;
        if pdf[s..].starts_with(b"\r\n") {
            s += 2;
        } else if pdf[s..].starts_with(b"\n") {
            s += 1;
        }
        let Some(e) = pdf[s..].windows(9).position(|w| w == b"endstream") else {
            break;
        };
        let mut buf = Vec::new();
        if flate2::read::ZlibDecoder::new(&pdf[s..s + e])
            .read_to_end(&mut buf)
            .is_ok()
        {
            out.push_str(&String::from_utf8_lossy(&buf));
        }
        at = s + e + 9;
    }
    out
}

#[test]
fn tagged_formula_has_alt() {
    let html = "<html lang=\"en\"><head><title>T</title></head><body><p>x</p>\
        <math alttext=\"x squared\"><msup><mi>x</mi><mn>2</mn></msup></math></body></html>";
    let dom = Dom::parse(html).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(""), geometry());
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    let pdf = render_with_options(&laid, Some(&dom), &meta, true, false).expect("tagged render");
    let text = String::from_utf8_lossy(&pdf).to_string() + &inflated(&pdf);
    assert!(text.contains("/Formula"), "no Formula structure element");
    assert!(text.contains("x squared"), "alt text missing");
}
