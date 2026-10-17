//! Font weight/style acceptance tests — one per acceptance criterion in
//! `docs/specifications/font-weight-style.spec.md` (CORE-80).
//!
//! Face-presence assertions check the rendered PDF bytes for the PostScript
//! base name krilla embeds (subset tags may prefix the name). The expected
//! names come from the face file `typeanvil::fonts::face_path` resolved on
//! this host (macOS Arial, or Liberation Sans on Linux hosts without Arial,
//! such as CI), so the tests hold on both font sets. Width assertions
//! use the fragment tree's `TextRun` glyphs (sum of `x_advance`), which is
//! the same metric the typography layer measures.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::fonts::{FaceId, FACE_BOLD, FACE_BOLD_ITALIC, FACE_ITALIC, FACE_REGULAR};
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

/// The PostScript name (nameID 6) of the face file the engine resolved for
/// `face` on this host.
fn expected_ps_name(face: FaceId) -> String {
    use read_fonts::tables::name::NameId;
    use read_fonts::TableProvider;
    let path = typeanvil::fonts::face_path(face);
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let font = read_fonts::FontRef::new(&data).unwrap_or_else(|e| panic!("parse {path}: {e}"));
    let name = font.name().unwrap_or_else(|e| panic!("name table of {path}: {e}"));
    let strings = name.string_data();
    name.name_record()
        .iter()
        .filter(|r| r.name_id() == NameId::POSTSCRIPT_NAME)
        .find_map(|r| r.string(strings).ok().map(|s| s.to_string()))
        .unwrap_or_else(|| panic!("no PostScript name in {path}"))
}

/// True when `bytes` contains `base_name` as a whole name: the byte after
/// the match is not a name character. `LiberationSans` is a prefix of
/// `LiberationSans-Bold`, so a plain substring test cannot tell them apart.
fn contains_face(bytes: &[u8], base_name: &str) -> bool {
    let needle = base_name.as_bytes();
    bytes.windows(needle.len() + 1).any(|w| {
        &w[..needle.len()] == needle
            && !(w[needle.len()].is_ascii_alphanumeric() || w[needle.len()] == b'-')
    })
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
    let (regular, bold) = (expected_ps_name(FACE_REGULAR), expected_ps_name(FACE_BOLD));
    assert!(
        contains_face(&bytes, &regular),
        "regular face {regular} missing from PDF"
    );
    assert!(
        contains_face(&bytes, &bold),
        "bold face {bold} missing from PDF"
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
    assert!(
        contains_face(&bytes, &expected_ps_name(FACE_REGULAR)),
        "regular face missing"
    );
    assert!(
        !contains_face(&bytes, &expected_ps_name(FACE_BOLD)),
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
    let italic = expected_ps_name(FACE_ITALIC);
    assert!(
        contains_face(&bytes, &italic),
        "italic face {italic} missing from PDF"
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
    let bold_italic = expected_ps_name(FACE_BOLD_ITALIC);
    assert!(
        contains_face(&bytes, &bold_italic),
        "bold-italic face {bold_italic} missing from PDF"
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
        FACE_REGULAR,
        "weight 400 must resolve to the regular face"
    );
    assert_eq!(
        runs[1].font_face,
        FACE_BOLD,
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
        contains_face(&bytes, &expected_ps_name(FACE_ITALIC)),
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
            FACE_REGULAR,
            "undecorated text must stay on the regular face"
        );
    }
    let bytes = render(&l).unwrap();
    assert!(contains_face(&bytes, &expected_ps_name(FACE_REGULAR)));
    assert!(!contains_face(&bytes, &expected_ps_name(FACE_BOLD)));
}

// --- CORE-361: an unstyled margin box uses the same default face as body ----

/// A margin box with no `font-family` (own or page context) falls back to the
/// initial value `serif`, the same family unstyled body text gets — so the
/// header/footer and the body match (WPT margin-boxes/content-00x refs draw
/// the margin text as body text). Before CORE-361 the margin-box fallback was
/// `sans-serif`, which only agreed with the body while every generic mapped
/// to the bundled Arial set.
#[test]
fn unstyled_margin_box_matches_unstyled_body_face() {
    let html = r#"<!DOCTYPE html><html><head><style>
@page { margin: 1in; @top-center { content: "Header text"; } }
body { margin: 0; }
</style></head><body><p>Body text.</p></body></html>"#;
    let l = lay(html);
    let page = &l.pages[0];
    let mut body = Vec::new();
    collect_runs(&page.root, &mut body);
    let mut header = Vec::new();
    for mb in &page.margin_boxes {
        collect_runs(&mb.fragment, &mut header);
    }
    assert!(!body.is_empty() && !header.is_empty(), "body and header text present");
    assert_eq!(
        header[0].font_face, body[0].font_face,
        "unstyled margin box must use the unstyled body face"
    );
}
