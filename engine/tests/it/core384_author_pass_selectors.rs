//! Break / viewport-unit / border / cell-valign author passes use the shared
//! `selectors` matcher (CORE-384). A combinator rule (`#wrap p { … }`) must
//! reach the element exactly as it does in the cascade; the old hand-rolled
//! simple-selector parser dropped every such rule.

use typeanvil::css::{cascade, BreakBetween, BreakInside, CellVAlign, ComputedStyle, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

fn style_of(html: &str, want: &str) -> ComputedStyle {
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
    let styles = cascade(&dom, &Stylesheet::parse(&css), &geometry());
    let id = dom
        .nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::Element(el) if el.id.as_deref() == Some(want)))
        .unwrap_or_else(|| panic!("no element #{want}"));
    styles[id].clone()
}

fn doc(css: &str) -> String {
    format!(
        "<html><head><style>{css}</style></head><body>\
         <section id=\"wrap\"><p id=\"t\">x</p><div><p id=\"deep\">y</p></div></section>\
         <p id=\"out\">z</p></body></html>"
    )
}

#[test]
fn break_rules_behind_descendant_combinator() {
    let css = "#wrap p { break-before: page; break-inside: avoid; orphans: 4; widows: 5 }";
    let t = style_of(&doc(css), "t");
    assert_eq!(t.break_before, BreakBetween::Page);
    assert_eq!(t.break_inside, BreakInside::Avoid);
    assert_eq!(t.orphans, 4);
    assert_eq!(t.widows, 5);
    // Descendant combinator reaches nested paragraphs; outside stays default.
    assert_eq!(style_of(&doc(css), "deep").break_before, BreakBetween::Page);
    assert_ne!(style_of(&doc(css), "out").break_before, BreakBetween::Page);
}

#[test]
fn break_rules_behind_child_combinator() {
    let css = "section > p { page-break-after: always }";
    assert_eq!(style_of(&doc(css), "t").break_after, BreakBetween::Page);
    assert_ne!(style_of(&doc(css), "deep").break_after, BreakBetween::Page);
}

#[test]
fn viewport_height_behind_combinator() {
    let t = style_of(&doc("#wrap p { height: 50vh }"), "t");
    let v = t.height_viewport.expect("vh fraction recorded");
    assert!((v.frac - 0.5).abs() < 1e-9, "frac = {}", v.frac);
    assert!(style_of(&doc("#wrap p { height: 50vh }"), "out").height_viewport.is_none());
}

#[test]
fn border_behind_combinator() {
    let t = style_of(&doc("#wrap p { border-top: 4px solid #f00 }"), "t");
    assert!((t.border_top.0 - 3.0).abs() < 0.05, "border_top = {}", t.border_top.0);
    assert_eq!(style_of(&doc("#wrap p { border-top: 4px solid #f00 }"), "out").border_top.0, 0.0);
}

/// Border colors come from stylo alone, and the width pass follows the
/// cascade (specificity, then `!important`). Before CORE-384 the pass
/// applied the weaker simple-selector `p` rule over stylo's correct
/// `#wrap p` value, and it ignored `!important`.
#[test]
fn border_pass_does_not_clobber_stylo() {
    // Pre-fix: the simple-selector `p` rule overwrote stylo's 3pt with 0.75pt.
    let t = style_of(&doc("#wrap p { border-top: 4px solid #f00 } p { border-top: 1px solid #00f }"), "t");
    assert!((t.border_top.0 - 3.0).abs() < 0.05, "border_top = {}", t.border_top.0);
    let c = t.border_top_color.expect("color");
    assert_eq!((c.r, c.g, c.b), (255, 0, 0));
    // `!important` on the weaker rule wins (css-cascade-4 §6.4).
    let t = style_of(&doc("p { border-top: 1px solid #00f !important } #wrap p { border-top: 4px solid #f00 }"), "t");
    assert!((t.border_top.0 - 0.75).abs() < 0.05, "border_top = {}", t.border_top.0);
    let c = t.border_top_color.expect("color");
    assert_eq!((c.r, c.g, c.b), (0, 0, 255));
}

/// The width pass keeps declared (unsnapped) widths behind a combinator:
/// 1-4 value `border-width` expansion (page-box-007), `border: solid` =
/// medium, and a fractional 5pt width that stylo would snap to 4.5pt.
#[test]
fn border_width_expansion_behind_combinator() {
    let s = |css: &str| {
        let t = style_of(&doc(css), "t");
        [t.border_top.0, t.border_right.0, t.border_bottom.0, t.border_left.0]
    };
    let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.05);
    assert!(near(s("#wrap p { border: solid; border-width: 40px 80px 120px 160px }"), [30.0, 60.0, 90.0, 120.0]));
    assert!(near(s("#wrap p { border: solid; border-width: 10px 20px 30px }"), [7.5, 15.0, 22.5, 15.0]));
    assert!(near(s("#wrap p { border: solid hotpink }"), [2.25; 4]));
    assert!(near(s("#wrap p { border: 5pt solid }"), [5.0; 4]));
}

#[test]
fn cell_valign_behind_combinator() {
    let html = "<html><head><style>#wrap td { vertical-align: bottom }</style></head><body>\
        <div id=\"wrap\"><table><tr><td id=\"t\">a</td></tr></table></div>\
        <table><tr><td id=\"out\">b</td></tr></table></body></html>";
    assert_eq!(style_of(html, "t").cell_valign, CellVAlign::Bottom);
    assert_eq!(style_of(html, "out").cell_valign, CellVAlign::Middle);
}

#[test]
fn specificity_decides_not_source_order() {
    // #wrap p (1,0,1) beats p.late (0,1,1) even though it comes first.
    let css = "#wrap p { orphans: 7 } p.late { orphans: 2 }";
    let html = format!(
        "<html><head><style>{css}</style></head><body><section id=\"wrap\">\
         <p id=\"t\" class=\"late\">x</p></section></body></html>"
    );
    assert_eq!(style_of(&html, "t").orphans, 7);
}
