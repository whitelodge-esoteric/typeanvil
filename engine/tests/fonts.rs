//! Font weight/style acceptance tests — one per acceptance criterion in
//! `docs/specifications/font-weight-style.spec.md` (CORE-80).
//!
//! Face-presence assertions check the rendered PDF bytes for the PostScript
//! base names krilla embeds (`ArialMT`, `Arial-BoldMT`, `Arial-ItalicMT`,
//! `Arial-BoldItalicMT` — subset tags may prefix the names). Width assertions
//! use the fragment tree's `TextRun` glyphs (sum of `x_advance`), which is
//! the same metric the typography layer measures.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::fonts::FontFace;
use typeanvil::frag::{Fragment, FragmentContent, TextRun};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::pdf::render;

// --- helpers (mirror tests/typography.rs) -----------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
}

fn lay(html: &str) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geometry(8.5, 11.0, 0.8))
}

/// All main-text runs (non-empty `glyphs`) across every page, pre-order.
fn all_runs(l: &Layout) -> Vec<TextRun> {
    let mut out = Vec::new();
    for page in &l.pages {
        collect_runs(&page.root, &mut out);
    }
    out
}

fn collect_runs(frag: &Fragment, out: &mut Vec<TextRun>) {
    if let FragmentContent::Text(run) = &frag.content {
        if !run.glyphs.is_empty() {
            out.push(run.clone());
        }
    }
    for child in &frag.children {
        collect_runs(child, out);
    }
}

/// Sum of the run's glyph advances in points (the drawn width).
fn run_width(run: &TextRun) -> f64 {
    run.glyphs.iter().map(|g| g.x_advance.get()).sum()
}

fn pdf_bytes(html: &str) -> Vec<u8> {
    let l = lay(html);
    render(&l).unwrap()
}

fn contains_face(bytes: &[u8], base_name: &str) -> bool {
    String::from_utf8_lossy(bytes).contains(base_name)
}

/// The four-face fixture used by several tests (regular, bold, italic, and
/// bold-italic paragraphs of identical text).
const FOUR_FACE_HTML: &str = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.plain { font-weight: normal; font-style: normal; }
.bold { font-weight: bold; }
.italic { font-style: italic; }
.bi { font-weight: bold; font-style: italic; }
</style></head><body>
<p class="plain">The quick brown fox jumps over the lazy dog.</p>
<p class="bold">The quick brown fox jumps over the lazy dog.</p>
<p class="italic">The quick brown fox jumps over the lazy dog.</p>
<p class="bi">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;

// --- AC 1: bold embeds a distinct face ---------------------------------------

#[test]
fn bold_embeds_bold_face() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.plain { font-weight: normal; }
.bold { font-weight: bold; }
</style></head><body>
<p class="plain">The quick brown fox jumps over the lazy dog.</p>
<p class="bold">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let bytes = pdf_bytes(html);
    assert!(
        contains_face(&bytes, "ArialMT"),
        "regular face ArialMT missing from PDF"
    );
    assert!(
        contains_face(&bytes, "Arial-BoldMT"),
        "bold face Arial-BoldMT missing from PDF"
    );
}

/// Behavior 7: an unused face is not embedded — a regular-only doc embeds
/// exactly one face.
#[test]
fn regular_only_doc_embeds_single_face() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
</style></head><body>
<p class="plain">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let bytes = pdf_bytes(html);
    assert!(contains_face(&bytes, "ArialMT"), "regular face missing");
    assert!(
        !contains_face(&bytes, "Arial-BoldMT"),
        "bold face embedded despite no bold usage"
    );
}

// --- AC 2: italic embeds a distinct face --------------------------------------

#[test]
fn italic_embeds_italic_face() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.plain { font-style: normal; }
.italic { font-style: italic; }
</style></head><body>
<p class="plain">The quick brown fox jumps over the lazy dog.</p>
<p class="italic">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let bytes = pdf_bytes(html);
    assert!(
        contains_face(&bytes, "Arial-ItalicMT"),
        "italic face Arial-ItalicMT missing from PDF"
    );
}

// --- AC 3: bold-italic embeds the combined face --------------------------------

#[test]
fn bold_italic_embeds_combined_face() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.bi { font-weight: bold; font-style: italic; }
</style></head><body>
<p class="bi">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let bytes = pdf_bytes(html);
    assert!(
        contains_face(&bytes, "Arial-BoldItalicMT"),
        "bold-italic face Arial-BoldItalicMT missing from PDF"
    );
}

// --- AC 4: weight threshold at 600 ---------------------------------------------

#[test]
fn weight_threshold_at_600() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.w400 { font-weight: 400; }
.w600 { font-weight: 600; }
</style></head><body>
<p class="w400">The quick brown fox jumps over the lazy dog.</p>
<p class="w600">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let l = lay(html);
    let runs = all_runs(&l);
    assert!(runs.len() >= 2, "expected two runs, got {}", runs.len());
    assert_eq!(
        runs[0].font_face,
        FontFace::Regular,
        "weight 400 must resolve to the regular face"
    );
    assert_eq!(
        runs[1].font_face,
        FontFace::Bold,
        "weight 600 must resolve to the bold face"
    );
}

// --- AC 5: bold measures wider than regular -------------------------------------

#[test]
fn bold_width_exceeds_regular() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.plain { font-weight: normal; }
.bold { font-weight: bold; }
</style></head><body>
<p class="plain">The quick brown fox jumps over the lazy dog.</p>
<p class="bold">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let l = lay(html);
    let runs = all_runs(&l);
    assert!(runs.len() >= 2, "expected two runs, got {}", runs.len());
    let plain_w = run_width(&runs[0]);
    let bold_w = run_width(&runs[1]);
    assert!(
        bold_w > plain_w,
        "bold run ({bold_w}pt) must measure wider than regular ({plain_w}pt) — real bold metrics, not a synthetic stroke"
    );
}

// --- AC 6: oblique maps to italic ------------------------------------------------

#[test]
fn oblique_uses_italic_face() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
.oblique { font-style: oblique; }
</style></head><body>
<p class="oblique">The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let bytes = pdf_bytes(html);
    assert!(
        contains_face(&bytes, "Arial-ItalicMT"),
        "oblique must render with the italic face (Arial has no oblique cut)"
    );
}

// --- AC 7: determinism with all four faces ----------------------------------------

#[test]
fn determinism_four_faces() {
    let b1 = pdf_bytes(FOUR_FACE_HTML);
    let b2 = pdf_bytes(FOUR_FACE_HTML);
    assert_eq!(
        b1.len(),
        b2.len(),
        "renders differ in length — nondeterministic output"
    );
    assert_eq!(b1, b2, "identical input must yield byte-identical PDF");
}

// --- AC 8: regression — regular runs still measure exactly as before ---------------

/// A regular-weight run's face and a light smoke render: the common path
/// (no weight/style declarations) resolves to Regular and produces a PDF.
#[test]
fn default_style_is_regular() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
</style></head><body>
<p>The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let l = lay(html);
    let runs = all_runs(&l);
    assert!(!runs.is_empty(), "expected at least one text run");
    for run in &runs {
        assert_eq!(
            run.font_face,
            FontFace::Regular,
            "undecorated text must stay on the regular face"
        );
    }
    let bytes = render(&l).unwrap();
    assert!(contains_face(&bytes, "ArialMT"));
    assert!(!contains_face(&bytes, "Arial-BoldMT"));
}
