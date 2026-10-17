//! CORE-358: the first word of a footnote body must not be shifted by the
//! marker width via a single glyph's x_offset (that moves only one glyph and
//! overprints the second word). The marker's own advance must carry the pen.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, TextRun};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn runs(f: &Fragment, out: &mut Vec<TextRun>) {
    if let FragmentContent::Text(run) = &f.content {
        out.push(run.clone());
    }
    for c in &f.children {
        runs(c, out);
    }
}

#[test]
fn footnote_body_first_glyph_has_no_x_offset() {
    let html = r#"<!DOCTYPE html><html><head><style>
@page { size: 5in 3in; margin: 0.5in; }
body { font-family: serif; font-size: 10pt; }
.fn { float: footnote; font-size: 9pt; }
</style></head><body><p>Body text.<span class="fn">A monitoring rule fires.</span></p></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let inch = |v: f64| Scalar(v * 72.0);
    let geometry = PageGeometry {
        width: inch(5.0),
        height: inch(3.0),
        margin_top: inch(0.5),
        margin_right: inch(0.5),
        margin_bottom: inch(0.5),
        margin_left: inch(0.5),
    };
    let l = layout(&dom, &Stylesheet::parse(&css), geometry);
    let mut all = Vec::new();
    runs(&l.pages[0].root, &mut all);
    let note = all
        .iter()
        .find(|r| r.text.contains("monitoring"))
        .expect("note run");
    for g in &note.glyphs {
        assert!(
            g.x_offset.get().abs() < 1e-6,
            "no glyph in the note run may carry an x_offset (got {})",
            g.x_offset.get()
        );
    }
    // Marker "1. " is the text prefix; its glyph advances sum to the hang.
    let marker_len = "1. ".len();
    let marker_w: f64 = note
        .glyphs
        .iter()
        .filter(|g| g.range.end <= marker_len)
        .map(|g| g.x_advance.get())
        .sum();
    assert!(
        (marker_w - 8.5).abs() < 0.01,
        "marker must advance exactly the hang width, got {marker_w}"
    );
}
