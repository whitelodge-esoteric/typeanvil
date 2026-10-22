//! CORE-543: an inline element's own `background-color` paints behind its
//! text. Before the fix the glyphs carried no background and nothing drew it.

use typeanvil::css::{Color, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(6.0 * 72.0),
        height: Scalar(3.0 * 72.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
    }
}

/// (word, background) for every non-space glyph cluster, per word.
fn words(html: &str) -> Vec<(String, Option<Color>)> {
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
    let laid = layout(&dom, &sheet, geometry());
    let mut out = Vec::new();
    collect(&laid.pages[0].root, &mut out);
    out
}

fn collect(f: &Fragment, out: &mut Vec<(String, Option<Color>)>) {
    if let FragmentContent::Text(run) = &f.content {
        let mut cur: Option<(String, Option<Color>)> = None;
        for g in &run.glyphs {
            let s = &run.text[g.range.clone()];
            if s.trim().is_empty() {
                if let Some(w) = cur.take() {
                    out.push(w);
                }
                continue;
            }
            match &mut cur {
                Some(w) => w.0.push_str(s),
                None => cur = Some((s.to_string(), g.bg)),
            }
        }
        if let Some(w) = cur.take() {
            out.push(w);
        }
    }
    for c in &f.children {
        collect(c, out);
    }
}

const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };

#[test]
fn span_background_reaches_its_glyphs_only() {
    let ws = words(
        "<style>span{background-color:#000}</style>\
         <p>plain <span>boxed text</span> after</p>",
    );
    let bg = |w: &str| ws.iter().find(|x| x.0 == w).unwrap_or_else(|| panic!("{w} in {ws:?}")).1;
    assert_eq!(bg("plain"), None);
    assert_eq!(bg("boxed"), Some(BLACK));
    assert_eq!(bg("text"), Some(BLACK));
    assert_eq!(bg("after"), None);
}

#[test]
fn display_inline_paragraphs_paint_background() {
    let ws = words(
        "<style>#t p{background-color:#000;display:inline}</style>\
         <section id=\"t\"><p>e-sport live</p><p>tournoi</p></section>",
    );
    for (w, bg) in &ws {
        assert_eq!(*bg, Some(BLACK), "word {w} in {ws:?}");
    }
    assert!(!ws.is_empty());
}
