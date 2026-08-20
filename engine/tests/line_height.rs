//! Line-height acceptance tests — one per criterion in
//! `docs/specifications/line-height.spec.md`.

use std::path::Path;
use std::process::Command;

use typeanvil::css::{cascade, ComputedStyle, Stylesheet, NORMAL_LINE_HEIGHT_FACTOR};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, FragmentKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::typography::baseline_offset;

const EPS: f64 = 1e-6;

// --- helpers ---------------------------------------------------------------

/// Points from inches.
fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

/// A page geometry with uniform margins (inches).
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

/// Collect all `<style>` text into a stylesheet (mirrors the CLI).
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

/// Lay out an HTML string with the given geometry.
fn lay(html: &str, geo: PageGeometry) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geo)
}

/// A cascaded style for the first `<p>` element in `html`.
fn p_style(html: &str) -> ComputedStyle {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    let styles = cascade(&dom, &ss, &geometry(5.0, 3.0, 0.5));
    let p_id = dom
        .nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::Element(el) if el.tag == "p"))
        .expect("doc must contain a <p>");
    styles[p_id].clone()
}

/// First line height (points) from the fragment tree.
fn first_line_height(layout: &Layout) -> Scalar {
    let page = layout.pages.first().expect("layout must contain a page");
    find_first_line_height(&page.root).expect("layout must contain a line fragment")
}

/// Baseline y of the first main-text line on page 1, in page coordinates
/// (sums ancestor fragment offsets; a text run's baseline is box-local).
fn first_baseline(layout: &Layout) -> Scalar {
    let page = layout.pages.first().expect("layout must contain a page");
    let mut acc = Scalar::ZERO;
    find_first_baseline(&page.root, &mut acc).expect("layout must contain a text run")
}

fn find_first_baseline(frag: &Fragment, acc: &mut Scalar) -> Option<Scalar> {
    let here = *acc + frag.offset.y;
    if let FragmentContent::Text(run) = &frag.content {
        if !run.glyphs.is_empty() {
            return Some(here + run.baseline.y);
        }
    }
    for child in &frag.children {
        if let Some(b) = find_first_baseline(child, &mut (*acc + frag.offset.y)) {
            return Some(b);
        }
    }
    None
}

fn find_first_line_height(frag: &Fragment) -> Option<Scalar> {
    if matches!(frag.kind, FragmentKind::Line) {
        if matches!(frag.content, FragmentContent::Text(_)) {
            return Some(frag.size.1);
        }
    }
    for child in &frag.children {
        if let Some(h) = find_first_line_height(child) {
            return Some(h);
        }
    }
    None
}

fn assert_close(got: Scalar, want: Scalar, label: &str) {
    let diff = (got.get() - want.get()).abs();
    assert!(
        diff < EPS,
        "{label}: got {:.6}pt want {:.6}pt (diff {:.6})",
        got.get(),
        want.get(),
        diff
    );
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typeanvil")
}

fn render_cli(html: &Path, out: &Path, w: &str, h: &str) {
    let status = Command::new(bin())
        .args([
            "render",
            html.to_str().unwrap(),
            "--page-width",
            w,
            "--page-height",
            h,
            "--margin-top",
            "0.5in",
            "--margin-right",
            "0.5in",
            "--margin-bottom",
            "0.5in",
            "--margin-left",
            "0.5in",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("failed to spawn typeanvil");
    assert!(status.success(), "engine exited non-zero: {status:?}");
}

// --- 1. Unitless number resolves ------------------------------------------

#[test]
fn unitless_number_resolves() {
    let html_16 = r#"<html><head><style>
        p { line-height: 1.6; }
    </style></head><body><p>Hello world.</p></body></html>"#;
    let html_12 = r#"<html><head><style>
        p { line-height: 1.2; }
    </style></head><body><p>Hello world.</p></body></html>"#;

    let style = p_style(html_16);
    assert_close(
        style.line_height,
        style.font_size * 1.6,
        "computed line-height for 1.6",
    );

    let geo = geometry(5.0, 3.0, 0.5);
    let h_16 = first_line_height(&lay(html_16, geo));
    let h_12 = first_line_height(&lay(html_12, geo));
    let ratio = h_16.get() / h_12.get();
    let want = 1.6 / 1.2;
    assert!(
        (ratio - want).abs() < EPS,
        "line-height ratio got {:.6} want {:.6}",
        ratio,
        want
    );
}

// --- 2. Default unchanged --------------------------------------------------

#[test]
fn default_line_height_is_normal() {
    let style = p_style("<html><body><p>Default.</p></body></html>");
    assert_close(
        style.line_height,
        style.font_size * NORMAL_LINE_HEIGHT_FACTOR,
        "default line-height",
    );
}

// --- 3. normal keyword -----------------------------------------------------

#[test]
fn normal_keyword_resolves() {
    let html = r#"<html><head><style>
        p { line-height: normal; }
    </style></head><body><p>Normal.</p></body></html>"#;
    let style = p_style(html);
    assert_close(
        style.line_height,
        style.font_size * NORMAL_LINE_HEIGHT_FACTOR,
        "normal line-height",
    );
}

// --- 4. Absolute length ----------------------------------------------------

#[test]
fn absolute_length_line_height() {
    let html = r#"<html><head><style>
        p { font-size: 10pt; line-height: 24pt; }
    </style></head><body><p>Fixed.</p></body></html>"#;
    let style = p_style(html);
    assert_close(style.line_height, Scalar(24.0), "computed 24pt line-height");

    let geo = geometry(5.0, 3.0, 0.5);
    let line_h = first_line_height(&lay(html, geo));
    assert_close(line_h, Scalar(24.0), "layout line-height 24pt");
}

// --- 5. Percentage / em ----------------------------------------------------

#[test]
fn percentage_and_em_resolve() {
    let html_pct = r#"<html><head><style>
        p { font-size: 20pt; line-height: 150%; }
    </style></head><body><p>Percent.</p></body></html>"#;
    let style_pct = p_style(html_pct);
    assert_close(
        style_pct.line_height,
        style_pct.font_size * 1.5,
        "150% line-height",
    );

    let html_em = r#"<html><head><style>
        p { font-size: 20pt; line-height: 1.5em; }
    </style></head><body><p>Em.</p></body></html>"#;
    let style_em = p_style(html_em);
    assert_close(
        style_em.line_height,
        style_em.font_size * 1.5,
        "1.5em line-height",
    );
}

// --- 6. Inheritance --------------------------------------------------------

#[test]
fn unitless_number_inherits_as_number() {
    let html = r#"<html><head><style>
        div { line-height: 1.6; }
        p { font-size: 12pt; }
    </style></head><body><div><p>Nested.</p></div></body></html>"#;
    let style = p_style(html);
    assert_close(
        style.line_height,
        style.font_size * 1.6,
        "inherited unitless line-height",
    );
}

// --- 7. Determinism --------------------------------------------------------

const FIXTURE: &str = r#"<html><head><style>
    p { font-size: 14px; line-height: 1.6; color: #333; }
</style></head><body><p>Deterministic output with line-height.</p></body></html>"#;

#[test]
fn output_is_deterministic_with_line_height() {
    let dir = tempfile::tempdir().unwrap();
    let html = dir.path().join("line-height.html");
    std::fs::write(&html, FIXTURE).unwrap();

    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render_cli(&html, &a, "5in", "3in");
    render_cli(&html, &b, "5in", "3in");

    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert_eq!(ba, bb, "PDF output is not byte-identical across runs");
}

// --- 8. Baseline placement follows CSS2.1 §10.8.1 half-leading (CORE-90) ---
//
// Prince places the first baseline at content_top + ascent + (line_height −
// ascent − descent)/2 (half the leading split around the font's natural
// metrics). TypeAnvil historically used `content_top + font_size` (slope 0
// vs factor), which is what made prose page counts cross between line-height
// 1.2 (TA taller) and 1.6 (Prince taller). Verified against Prince 16.2 with
// Arial hhea (asc 1854/2048, desc 434/2048): every factor reproduced to
// 0.001pt. This test pins the formula on the fragment tree.

const ARIA_ASC_EM: f64 = 1854.0 / 2048.0;
const ARIA_DESC_EM: f64 = 434.0 / 2048.0;

fn expected_offset(font_size: f64, line_height: f64) -> f64 {
    let asc = font_size * ARIA_ASC_EM;
    let desc = font_size * ARIA_DESC_EM;
    asc + (line_height - asc - desc) * 0.5
}

#[test]
fn first_baseline_uses_half_leading() {
    let geo = geometry(5.0, 3.0, 0.5);
    let cases = [
        (1.0, 10.0f64),
        (1.2, 10.0),
        (1.6, 10.0),
        (2.0, 10.0),
        (1.2, 15.0),
        (1.6, 15.0),
    ];
    for (factor, fs) in cases {
        let html = format!(
            r#"<html><head><style>
                body {{ margin: 0; }}
                h1 {{ font-size: {fs}pt; line-height: {factor}; margin: 0; }}
            </style></head><body><h1>Heading</h1></body></html>"#
        );
        let lay = lay(&html, geo);
        // Content top = page 36 (0.5in margin). The baseline must be
        // content_top + expected_offset, NOT content_top + font_size.
        let baseline = first_baseline(&lay).get();
        let want = 36.0 + expected_offset(fs, fs * factor);
        assert!(
            (baseline - want).abs() < 0.01,
            "factor {factor} fs {fs}: baseline {baseline:.3} want {want:.3} \
             (old behavior: {})",
            36.0 + fs
        );
        // And it must differ from the old `content_top + font_size` rule
        // whenever half-leading is nonzero.
        if (fs * factor - (fs * ARIA_ASC_EM + fs * ARIA_DESC_EM)).abs() > 0.01 {
            assert!(
                (baseline - (36.0 + fs)).abs() > 0.05,
                "factor {factor} fs {fs}: baseline must move off content_top + font_size"
            );
        }
    }
}

#[test]
fn baseline_offset_matches_prince_formula() {
    // The pure function itself, against the Prince-verified numbers
    // (measured via pypdfium2 charbox bottoms on Prince 16.2 output).
    let face = typeanvil::fonts::FontFace::Regular;
    let measured: &[(f64, f64, f64)] = &[
        (10.0, 10.0, 8.467),
        (10.0, 12.0, 9.467),
        (10.0, 15.0, 10.967),
        (10.0, 16.0, 11.467),
        (10.0, 20.0, 13.467),
        (15.0, 15.0, 12.701),
        (15.0, 18.0, 14.200),
        (15.0, 24.0, 17.200),
        (15.0, 30.0, 20.200),
    ];
    for &(fs, lh, want) in measured {
        let got = baseline_offset(Scalar(fs), Scalar(lh), face).get();
        assert!(
            (got - want).abs() < 0.01,
            "baseline_offset({fs}pt, {lh}pt) = {got:.4} want {want:.4}"
        );
    }
}
