//! Footnote band sizing (CORE-366, footnotes.spec.md req. 4/6): the band
//! holds the notes whose calls land on the page, and the body keeps every
//! line that fits above its own notes.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

/// Page-absolute (x, baseline y) of every text run (the PDF emitter's walk:
/// a run's baseline is relative to its PARENT's absolute origin).
fn baselines(frag: &Fragment, px: f64, py: f64, out: &mut Vec<(f64, f64)>) {
    let ax = px + frag.offset.x.get();
    let ay = py + frag.offset.y.get();
    if let FragmentContent::Text(run) = &frag.content {
        out.push((px + run.baseline.x.get(), py + run.baseline.y.get()));
    }
    for c in &frag.children {
        baselines(c, ax, ay, out);
    }
}

#[test]
fn footnote_band_does_not_starve_the_body() {
    // 6x4in page, 0.5in margins: content 36..252pt (216pt tall). Each
    // paragraph is one 14pt line with one one-line 14pt note, so a page
    // holds k paragraphs while 28k <= 216: k = 7. The full-height attempt
    // reaches 15 calls; reserving all 15 notes (210pt) left the body one
    // forced line and floated the notes mid-page.
    let css = r#"
        @page { size: 6in 4in; margin: 0.5in; }
        body { margin: 0; font-size: 10pt; line-height: 14pt; }
        p { margin: 0; }
        .fn { float: footnote; }
    "#;
    let body: String = (1..=20)
        .map(|i| format!("<p>Para {i}.<span class=\"fn\">Note {i}.</span></p>"))
        .collect();
    let html = format!("<html><head><style>{css}</style></head><body>{body}</body></html>");
    let dom = Dom::parse(&html).expect("parse");
    let ss = Stylesheet::parse(css);
    let geo = PageGeometry {
        width: Scalar(432.0),
        height: Scalar(288.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    };
    let laid = layout(&dom, &ss, geo);
    let mut b = Vec::new();
    baselines(&laid.pages[0].root, 0.0, 0.0, &mut b);
    // Notes hang 8.5pt left of the content edge (x = 36).
    let body_lines = b.iter().filter(|(x, _)| *x >= 36.0 - 1e-6).count();
    let notes: Vec<f64> = b.iter().filter(|(x, _)| *x < 36.0 - 1e-6).map(|p| p.1).collect();
    assert_eq!(body_lines, 7, "page 1 holds the 7 paragraphs that fit beside their notes");
    assert_eq!(notes.len(), 7, "one note per placed call");
    let last = notes.iter().cloned().fold(f64::MIN, f64::max);
    assert!(
        last > 252.0 - 14.0,
        "the band sits on the bottom margin edge: last note baseline {last}"
    );
}
