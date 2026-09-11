use typeanvil::css::{cascade, ComputedStyle, Stylesheet};
use typeanvil::dom::Dom;
use typeanvil::geom::PageGeometry;
use typeanvil::layout::layout;
use typeanvil::typography::break_paragraph;

fn geometry(w: f64, h: f64, m: f64) -> PageGeometry {
    PageGeometry {
        width: typeanvil::geom::Scalar(w),
        height: typeanvil::geom::Scalar(h),
        margin_top: typeanvil::geom::Scalar(m),
        margin_right: typeanvil::geom::Scalar(m),
        margin_bottom: typeanvil::geom::Scalar(m),
        margin_left: typeanvil::geom::Scalar(m),
    }
}

#[test]
fn br_resume_wraps_full_width() {
    // Direct breaker probe: text after the sentinel must wrap at full width.
    let dom = Dom::parse("<html><body><p>x</p></body></html>").unwrap();
    let ss = Stylesheet::parse("");
    let styles = cascade(&dom, &ss, &geometry(5.0, 3.0, 0.5));
    let style = styles[dom.find_tag("p").unwrap()].clone();
    let text = "3rd page\u{0000}Also 3rd page";
    let lines = break_paragraph(text, typeanvil::geom::Scalar(288.0), &style, false, false);
    for (i, l) in lines.iter().enumerate() {
        println!("line {}: {:?} consumed={}", i, l.text, l.consumed);
    }
    assert_eq!(lines.len(), 2, "breaker must produce 2 lines, got {}", lines.len());
}
