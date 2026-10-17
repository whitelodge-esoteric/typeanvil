//! CORE-454: text after an inline-block continues the box's line
//! (css2 §9.4.2) instead of starting a fresh line below it.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

/// (text, absolute baseline y) of every text run.
fn runs(f: &Fragment, oy: f64, out: &mut Vec<(String, f64)>) {
    let y = oy + f.offset.y.get();
    if let FragmentContent::Text(run) = &f.content {
        out.push((run.text.clone(), oy + run.baseline.y.get()));
    }
    for c in &f.children {
        runs(c, y, out);
    }
}

fn lay_runs(html: &str) -> Vec<(String, f64)> {
    let dom = Dom::parse(html).expect("parse html");
    let sheet = Stylesheet::parse("");
    let l = layout(&dom, &sheet, geo());
    let mut v = Vec::new();
    runs(&l.pages[0].root, 0.0, &mut v);
    v
}

/// A tall inline-block that spans pages, then `<br>`, then text: the text is
/// drawn exactly once (monolithic-overflow-015 shape), not beside the box on
/// every page it occupies.
#[test]
fn text_after_tall_inline_block_and_br_appears_once() {
    let html = "<!DOCTYPE html>\n\
                <style>:root{print-color-adjust:exact}body{margin:0}</style>\n\
                <div style=\"display:inline-block; vertical-align:top; width:50px; \
                height:350vh; background:hotpink;\"></div><br>\n\
                MARKERTEXT should be at the middle of the fourth page.";
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let typeanvil::dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    let l = layout(&dom, &sheet, geo());
    let mut hits = Vec::new();
    for (pi, p) in l.pages.iter().enumerate() {
        let mut v = Vec::new();
        runs(&p.root, 0.0, &mut v);
        for (t, _) in &v {
            if t.contains("MARKERTEXT") {
                hits.push(pi);
            }
        }
    }
    assert!(l.pages.len() > 1, "box must span pages");
    assert_eq!(
        hits,
        vec![l.pages.len() - 1],
        "text after <br> must be drawn once, on the page where the box ends"
    );
}

#[test]
fn text_after_inline_block_shares_its_line() {
    let v = lay_runs(
        "<!DOCTYPE html><p>Before <span style=\"display:inline-block;width:20px;\
         height:10px;background:#c00\"></span> AFTERWORD continues here.</p>",
    );
    eprintln!("runs: {v:?}");
    let before = v.iter().find(|(t, _)| t.contains("Before")).expect("Before");
    let after = v.iter().find(|(t, _)| t.contains("AFTERWORD")).expect("AFTERWORD");
    assert!(
        (before.1 - after.1).abs() < 0.5,
        "AFTERWORD baseline {} must equal Before baseline {}",
        after.1,
        before.1
    );
}
