//! CORE-530: an inline element's own `color` and `font-weight`/`font-style`
//! apply to its text. Before the fix every glyph of a text run painted in the
//! parent block's colour and face.

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

/// (word, glyph-face-differs-from-run, colour override) for every
/// whitespace-delimited word in the first page's text runs.
fn words(html: &str) -> Vec<(String, bool, Option<Color>)> {
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

fn collect(f: &Fragment, out: &mut Vec<(String, bool, Option<Color>)>) {
    if let FragmentContent::Text(run) = &f.content {
        let mut cur: Option<(String, bool, Option<Color>)> = None;
        for g in &run.glyphs {
            let s = &run.text[g.range.clone()];
            if s.trim().is_empty() {
                if let Some(w) = cur.take() {
                    out.push(w);
                }
                continue;
            }
            let differs = g.face.is_some_and(|f| f != run.font_face);
            match &mut cur {
                Some(w) => {
                    w.0.push_str(s);
                    w.1 |= differs;
                    if w.2.is_none() {
                        w.2 = g.color;
                    }
                }
                None => cur = Some((s.to_string(), differs, g.color)),
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

fn find<'a>(
    ws: &'a [(String, bool, Option<Color>)],
    w: &str,
) -> &'a (String, bool, Option<Color>) {
    ws.iter().find(|x| x.0 == w).unwrap_or_else(|| panic!("{w} in {ws:?}"))
}

const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };

#[test]
fn span_color_and_weight_apply() {
    let ws = words(
        "<style>body{margin:0;font-family:Arial;font-size:20pt}\
         .hot{color:#ff0000;font-weight:bold}</style>\
         <p>plain <span class=\"hot\">spanredbold</span> tail</p>",
    );
    let w = find(&ws, "spanredbold");
    assert!(w.1, "span text must use the bold face: {ws:?}");
    assert_eq!(w.2, Some(RED), "span text must be red: {ws:?}");
    let p = find(&ws, "plain");
    assert!(!p.1 && p.2.is_none(), "parent text unchanged: {ws:?}");
    let t = find(&ws, "tail");
    assert!(!t.1 && t.2.is_none(), "text after the span unchanged: {ws:?}");
}

#[test]
fn inline_style_attribute_and_descendant_selector() {
    let ws = words(
        "<style>body{margin:0;font-family:Arial;font-size:20pt}\
         .box .x{color:#ff0000}</style>\
         <div class=\"box\"><p>a <span class=\"x\">descend</span> \
         <span style=\"color:#ff0000;font-weight:bold\">attr</span></p></div>",
    );
    assert_eq!(find(&ws, "descend").2, Some(RED), "{ws:?}");
    let a = find(&ws, "attr");
    assert_eq!(a.2, Some(RED), "{ws:?}");
    assert!(a.1, "inline style weight: {ws:?}");
}

#[test]
fn b_and_strong_are_bold_and_em_is_italic() {
    let ws = words(
        "<style>body{margin:0;font-family:Arial;font-size:20pt}</style>\
         <p>x <b>btag</b> <strong>stag</strong> <em>etag</em> y</p>",
    );
    assert!(find(&ws, "btag").1, "{ws:?}");
    assert!(find(&ws, "stag").1, "{ws:?}");
    assert!(find(&ws, "etag").1, "{ws:?}");
    assert!(!find(&ws, "y").1, "{ws:?}");
}

#[test]
fn mid_word_inline_and_nested_inline() {
    let ws = words(
        "<style>body{margin:0;font-family:Arial;font-size:20pt}\
         .r{color:#ff0000}</style>\
         <p>un<b>bold</b>ed <span class=\"r\">out <b>both</b></span></p>",
    );
    // The glyph-level scan joins a word across the style change.
    let w = find(&ws, "unbolded");
    assert!(w.1, "the bold middle of the word uses the bold face: {ws:?}");
    assert_eq!(find(&ws, "out").2, Some(RED), "{ws:?}");
    let both = find(&ws, "both");
    assert!(both.1 && both.2 == Some(RED), "nested b inherits red: {ws:?}");
}
