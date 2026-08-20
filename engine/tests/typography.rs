//! Typography-layer acceptance tests — one per acceptance criterion in
//! `docs/specifications/typography-layer.spec.md`.
//!
//! The pure-breaker tests drive `typography::break_paragraph` / `shape_word`
//! / `line_break_opportunities` directly; the end-to-end tests drive the
//! library (`layout::layout`) and assert on the fragment tree's `TextRun`
//! glyph data, plus determinism through the CLI. Helpers mirror
//! `tests/paged_media.rs`.

use std::path::Path;
use std::process::Command;

use typeanvil::css::{cascade, ComputedStyle, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, Fragmentainer};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::typography::{break_paragraph, line_break_opportunities, shape_word};

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

/// A cascaded style for the first `<p>` element in `html` (default UA font
/// size 16px = 12pt, block display, no author overrides).
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

/// All text-run strings on one fragmentainer, in pre-order (document order).
fn page_texts(page: &Fragmentainer) -> Vec<String> {
    let mut out = Vec::new();
    collect_text(&page.root, &mut out);
    out
}

fn collect_text(frag: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &frag.content {
        out.push(run.text.clone());
    }
    for child in &frag.children {
        collect_text(child, out);
    }
}

/// All main-text text runs (non-empty `glyphs`) on a page, in pre-order.
fn page_glyph_runs(page: &Fragmentainer) -> Vec<typeanvil::frag::TextRun> {
    let mut out = Vec::new();
    collect_runs(&page.root, &mut out);
    out
}

fn collect_runs(frag: &Fragment, out: &mut Vec<typeanvil::frag::TextRun>) {
    if let FragmentContent::Text(run) = &frag.content {
        if !run.glyphs.is_empty() {
            out.push(run.clone());
        }
    }
    for child in &frag.children {
        collect_runs(child, out);
    }
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

// --- 1. Real shaping widths ------------------------------------------------

/// AC 1: real glyph metrics, not character count — "WWWW" and "iiii" at the
/// same font/size have different widths (W is wide, i is narrow).
#[test]
fn shaping_real_widths() {
    let wide = shape_word("WWWW", Scalar(12.0), typeanvil::fonts::FontFace::Regular);
    let narrow = shape_word("iiii", Scalar(12.0), typeanvil::fonts::FontFace::Regular);
    let w = wide.width.get();
    let n = narrow.width.get();
    assert!(
        (w - n).abs() > 1.0,
        "WWWW ({w:.2}pt) and iiii ({n:.2}pt) must differ by real glyph metrics"
    );
    // Same character count, so the difference cannot come from length.
    assert_eq!(wide.text.chars().count(), narrow.text.chars().count());
    assert!(!wide.glyphs.is_empty(), "shaped run must carry glyphs");
}

// --- 2. Glue model: justified lines fill the width -------------------------

/// AC 2: in a justified paragraph every non-final line's ink width equals the
/// content width within epsilon — glue stretch/shrink (and, within ±2%, font
/// expansion) distributes the excess/deficit.
#[test]
fn justified_lines_fill_width() {
    let style = p_style("<html><body><p>t</p></body></html>");
    let text = "The quick brown fox jumps over the lazy dog while the pack of \
                hounds barks at the distant moon and the farmer whistles \
                softly to his sheep.";
    let width = Scalar(200.0);
    let lines = break_paragraph(text, width, &style, false, true);
    assert!(lines.len() >= 3, "expected a multi-line paragraph, got {}", lines.len());
    // Epsilon 2.0pt (1% of the line): lines whose natural width plus full
    // stretch still leaves a gap hit the ±2% expansion clamp, which leaves a
    // small deterministic residual (spec edge case).
    for (i, l) in lines.iter().enumerate() {
        let fill = (l.drawn_width().get() - width.get()).abs();
        if i + 1 < lines.len() {
            // Spec Behavior §8 + Edge Cases: glue absorbs up to full
            // stretch/shrink; the ±2% expansion clamp absorbs the residual.
            // When even full stretch + max expansion cannot fill (a line too
            // short for its glue, i.e. TeX underfull), the residual is the
            // documented deterministic edge case — the line must either fill
            // within tolerance OR have hit the expansion clamp.
            assert!(
                fill < 2.0 || l.expansion.abs() >= 0.02 - 1e-9,
                "non-final line {:?} drawn {:.3} != width {:.3} (err {:.3}, expansion {:.3})",
                l.text, l.drawn_width().get(), width.get(), fill, l.expansion
            );
        }
        // The final line is not justified (spec Behavior §4): it must not be
        // expanded. Non-final lines MAY use expansion (spec Behavior §8) —
        // that is the feature under test above.
        if i + 1 == lines.len() {
            assert_eq!(l.expansion, 0.0, "final line must not expand");
        }
    }
}

// --- 3. K-P total fit beats greedy first-fit -------------------------------

/// AC 3: total-fit breaking is not first-fit in disguise. A paragraph with
/// several viable breakpoints is broken both ways; the K-P result's total
/// spacing deviation (per non-final line, distance from the content width)
/// is strictly lower than greedy first-fit's.
#[test]
fn total_fit_beats_greedy() {
    let style = p_style("<html><body><p>t</p></body></html>");
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa \
                lambda mu nu xi omicron pi rho sigma tau";
    let width = Scalar(150.0);

    let kp = break_paragraph(text, width, &style, false, true);
    let greedy = greedy_first_fit(text, width, &style);

    let kp_demerit = spacing_deviation(&kp, width.get());
    let greedy_demerit = spacing_deviation(&greedy, width.get());
    assert!(
        greedy_demerit > kp_demerit + 10.0,
        "total fit ({kp_demerit:.1}) must beat greedy ({greedy_demerit:.1}) by a clear margin"
    );
    // Both break the same text: no words lost or invented.
    let join = |ls: &[typeanvil::typography::LineResult]| {
        ls.iter().map(|l| l.text.clone()).collect::<Vec<_>>().join(" ")
    };
    assert_eq!(join(&kp), join(&greedy), "line reassembly must preserve the text");
}

/// First-fit reference: pack words (real shaped widths) until the next word
/// would overflow; never redistribute. Non-final lines are justified the same
/// way the K-P materializer does (glue then expansion).
fn greedy_first_fit(
    text: &str,
    max_width: Scalar,
    style: &ComputedStyle,
) -> Vec<typeanvil::typography::LineResult> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines: Vec<Vec<&str>> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut cur_w = 0.0f64;
    let space_w = shape_word(" ", style.font_size, typeanvil::fonts::FontFace::Regular).width.get();
    for w in words {
        let ww = shape_word(w, style.font_size, typeanvil::fonts::FontFace::Regular).width.get();
        let added = if cur.is_empty() { ww } else { cur_w + space_w + ww };
        if !cur.is_empty() && added > max_width.get() {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0.0;
        }
        if cur.is_empty() {
            cur.push(w);
            cur_w = ww;
        } else {
            cur.push(w);
            cur_w += space_w + ww;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    // Materialize each greedy line exactly like the K-P materializer.
    let mut out = Vec::new();
    for (i, lw) in lines.iter().enumerate() {
        let ltext = lw.join(" ");
        let natural = shape_word(&ltext, style.font_size, typeanvil::fonts::FontFace::Regular).width.get();
        let spaces = lw.len() as f64 - 1.0;
        let glue_w = space_w;
        let stretch = glue_w * 0.5 * spaces;
        let shrink = glue_w * (1.0 / 3.0) * spaces;
        let (mut stretch_used, mut shrink_used, mut expansion) = (0.0, 0.0, 0.0);
        let justify_line = i + 1 < lines.len();
        if justify_line {
            let diff = max_width.get() - natural;
            if diff > 0.0 && stretch > 0.0 {
                stretch_used = diff.min(stretch);
                expansion = ((max_width.get() - (natural + stretch_used)) / (natural + stretch_used))
                    .clamp(-0.02, 0.02);
            } else if diff < 0.0 && -diff <= shrink {
                shrink_used = -diff;
            }
        }
        out.push(typeanvil::typography::LineResult {
            text: ltext,
            glyphs: Vec::new(),
            natural_width: Scalar(natural),
            stretch_used: Scalar(stretch_used),
            shrink_used: Scalar(shrink_used),
            expansion,
            protrude_left: Scalar::ZERO,
            protrude_right: Scalar::ZERO,
            // This helper measures spacing only; no offset accounting.
            consumed: 0,
        });
    }
    out
}

/// Sum of per-line |drawn − content width| over non-final lines.
fn spacing_deviation(lines: &[typeanvil::typography::LineResult], width: f64) -> f64 {
    lines
        .iter()
        .take(lines.len().saturating_sub(1))
        .map(|l| (l.drawn_width().get() - width).abs())
        .sum()
}

// --- 4. Hyphenation --------------------------------------------------------

/// AC 4: with hyphenation on, a long word that cannot fit on a line breaks
/// with a hyphen glyph; without hyphenation the same word never splits.
#[test]
fn hyphenation_breaks() {
    let style = p_style("<html><body><p>t</p></body></html>");
    let word = "supercalifragilisticexpialidocious";
    // Narrow column: the word alone exceeds the line width, so it can only
    // appear via hyphenation (or overflow).
    let width = Scalar(90.0);

    let hyph = break_paragraph(&format!("start {word} end"), width, &style, true, false);
    let nohyph = break_paragraph(&format!("start {word} end"), width, &style, false, false);

    let joined_h: String = hyph.iter().map(|l| l.text.clone()).collect();
    let joined_n: String = nohyph.iter().map(|l| l.text.clone()).collect();
    assert!(
        joined_h.contains('-'),
        "hyphenated output must contain a hyphen, got: {joined_h:?}"
    );
    assert!(
        joined_h.len() >= word.len(),
        "hyphenation must not lose text: {joined_h:?}"
    );
    // Without hyphenation the word is monolithic: no hyphen, word intact.
    assert!(
        !joined_n.contains('-'),
        "no-hyphenation output must not invent hyphens: {joined_n:?}"
    );
    // The hyphenation line that ends with a hyphen carries a hyphen glyph.
    let has_hyphen_glyph = hyph.iter().any(|l| {
        l.text.ends_with('-') && l.glyphs.iter().any(|g| g.id != 0)
    });
    assert!(has_hyphen_glyph, "a hyphenated line must carry a trailing hyphen glyph");
}

// --- 5. Protrusion ---------------------------------------------------------

/// AC 5: a justified line ending with `.` or `,` hangs the punctuation into
/// the right margin — the run's right protrusion is the em-fraction hang, and
/// the drawn ink extends past the content box right edge by that amount
/// (optical only; line breaking is unchanged).
#[test]
fn protrusion_hangs() {
    let html = r#"<html><head><style>p { text-align: justify; }</style></head>
        <body><p>Alpha beta gamma. Delta epsilon zeta. Eta theta iota kappa.
        Lambda mu nu xi omicron pi.</p></body></html>"#;
    let geo = geometry(4.0, 2.0, 0.5); // content width 3in = 216pt
    let layout = lay(html, geo);
    let runs = page_glyph_runs(&layout.pages[0]);
    assert!(!runs.is_empty(), "expected shaped main-text runs");

    let content_right = geo.width - geo.margin_left - geo.margin_right;
    let mut hung = false;
    for r in &runs {
        let last = r.text.chars().next_back().unwrap_or(' ');
        let expected = match last {
            '.' | ',' => 0.7,
            ';' | ':' => 0.5,
            '!' | '?' => 0.2,
            _ => 0.0,
        };
        let protrude = r.protrude_right.get();
        if expected > 0.0 {
            // The hang is the em fraction at this font size, in points.
            let want = expected * r.font_size.get();
            assert!(
                (protrude - want).abs() < 0.01,
                "right protrusion for {last:?} = {protrude:.2}, want {want:.2}"
            );
            hung = true;
        } else {
            assert_eq!(protrude, 0.0, "no hang for {last:?}");
        }
        // A JUSTIFIED line's ink edge (baseline x + drawn width + hang)
        // crosses the content box right edge by the hang; the final line is
        // not justified and does not reach the edge.
        if expected > 0.0 && r.text.len() > 1 {
            // Drawn width = sum of glyph advances (incl. expansion), the same
            // quantity the PDF backend lays out.
            let adv: f64 = r.glyphs.iter().map(|g| g.x_advance.get()).sum();
            let drawn = adv * (1.0 + r.expansion);
            if (drawn - content_right.get()).abs() < 2.0 {
                let ink_edge = r.baseline.x.get() + drawn + r.protrude_right.get();
                assert!(
                    ink_edge > content_right.get(),
                    "punctuation must hang past the content edge ({ink_edge:.2} > {:.2})",
                    content_right.get()
                );
                assert!(
                    (ink_edge - content_right.get() - r.protrude_right.get()).abs() < 0.5,
                    "hang must equal the protrusion amount"
                );
            }
        }
    }
    assert!(hung, "at least one line must end in protruding punctuation");
}

// --- 6. Font expansion -----------------------------------------------------

/// AC 6: within a justified paragraph, at least one line uses a nonzero
/// expansion factor in [-2%, +2%], and that line's residual error is smaller
/// than it would be with glue alone.
#[test]
fn expansion_applied() {
    let style = p_style("<html><body><p>t</p></body></html>");
    // A width at which glue stretch alone cannot make every line fit exactly:
    // several lines' natural width plus full stretch undershoots the width,
    // so expansion must close the gap (within its ±2% bound).
    let text = "The quick brown fox jumps over the lazy dog while the pack of \
                hounds barks at the distant moon and the farmer whistles \
                softly to his sheep.";
    let width = Scalar(200.0);
    let lines = break_paragraph(text, width, &style, false, true);
    assert!(lines.len() >= 3, "expected multi-line paragraph, got {}", lines.len());

    let mut saw_expansion = false;
    for (i, l) in lines.iter().enumerate() {
        if i + 1 == lines.len() {
            continue; // final line: never expanded
        }
        let e = l.expansion;
        assert!(
            (-0.02..=0.02).contains(&e),
            "expansion {e} must stay within ±2%"
        );
        if e.abs() > 1e-6 {
            saw_expansion = true;
            // Residual with expansion is smaller than without (glue only).
            let without = l.natural_width.get() + l.stretch_used.get() - l.shrink_used.get();
            let with = without * (1.0 + e);
            assert!(
                (with - width.get()).abs() < (without - width.get()).abs(),
                "expansion must reduce residual error on {:?}: {with:.3} vs {without:.3}",
                l.text
            );
        }
    }
    assert!(saw_expansion, "at least one justified line must use expansion");
}

// --- 7. UAX #14 break opportunities ----------------------------------------

/// AC 7: `line_break_opportunities` yields breaks at Unicode line-break
/// boundaries (after spaces, after punctuation clusters) and never inside a
/// word — UAX #14 semantics.
#[test]
fn uax14_opportunities() {
    // "The quick brown fox." — breaks after each inter-word space; the
    // end-of-text opportunity is excluded. Byte offsets: 4, 10, 16.
    let opps = line_break_opportunities("The quick brown fox.");
    assert_eq!(opps, vec![4, 10, 16], "word-boundary breaks expected");

    // No opportunities inside a single word.
    assert!(
        line_break_opportunities("unbreakable").is_empty(),
        "no breaks inside a word"
    );

    // A punctuation cluster ("hello," + space) breaks after the space, not
    // between letters, and not after a lone leading quote.
    let opps2 = line_break_opportunities("hello, world!");
    assert_eq!(opps2, vec![7], "break after the comma+space cluster, got {opps2:?}");

    // Mandatory breaks (newline) are opportunities too.
    let opps3 = line_break_opportunities("one\ntwo");
    assert!(!opps3.is_empty(), "a mandatory break must be an opportunity");

    // Every opportunity must sit at a whitespace/punctuation boundary, never
    // splitting alphanumeric runs.
    for text in ["The quick brown fox.", "one two three", "a, b; c!"] {
        for off in line_break_opportunities(text) {
            let bytes = text.as_bytes();
            let left = bytes[off.saturating_sub(1)] as char;
            let right = bytes.get(off).copied().unwrap_or(b' ') as char;
            assert!(
                !(left.is_alphanumeric() && right.is_alphanumeric()),
                "break at {off} splits a word in {text:?}"
            );
        }
    }
}

// --- 8. Determinism --------------------------------------------------------

/// AC 8: the typography fixture rendered twice yields byte-identical PDFs —
/// shaping, K-P, protrusion, and expansion are all deterministic.
#[test]
fn determinism_typography() {
    let html = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/typography-demo.html");
    let tmp = std::env::temp_dir();
    let out1 = tmp.join("typeanvil-typography-det-1.pdf");
    let out2 = tmp.join("typeanvil-typography-det-2.pdf");
    render_cli(&html, &out1, "8.27in", "11.69in");
    render_cli(&html, &out2, "8.27in", "11.69in");
    let b1 = std::fs::read(&out1).expect("read first render");
    let b2 = std::fs::read(&out2).expect("read second render");
    assert_eq!(b1.len(), b2.len(), "renders differ in length");
    assert_eq!(b1, b2, "identical input must yield byte-identical PDF");
    let _ = std::fs::remove_file(&out1);
    let _ = std::fs::remove_file(&out2);
}

// --- 9. Demo fixture -------------------------------------------------------

/// AC 10: the typography fixture renders multi-page, deterministic, and its
/// main text carries shaped glyphs (the visible wedge: real shaping + K-P on
/// a typography-sensitive document).
#[test]
fn demo_fixture() {
    let html = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typography-demo.html"),
    )
    .expect("fixture must exist");
    // ~A4 feel with 1in margins.
    let geo = geometry(8.27, 11.69, 1.0);

    let a = lay(&html, geo);
    let b = lay(&html, geo);
    assert!(
        a.pages.len() >= 2,
        "typography fixture must be multi-page, got {}",
        a.pages.len()
    );
    // Determinism at the layout level: same page count and same text.
    assert_eq!(a.pages.len(), b.pages.len());
    let texts_a: Vec<String> = a
        .pages
        .iter()
        .flat_map(page_texts)
        .collect();
    let texts_b: Vec<String> = b
        .pages
        .iter()
        .flat_map(page_texts)
        .collect();
    assert_eq!(texts_a, texts_b, "layout must be deterministic");

    // Main text is shaped: at least one line carries glyphs with real widths.
    let runs: Vec<_> = a.pages.iter().flat_map(page_glyph_runs).collect();
    assert!(!runs.is_empty(), "fixture main text must be shaped");
    let shaped: usize = runs.iter().map(|r| r.glyphs.len()).sum();
    assert!(shaped > 100, "expected a text-heavy fixture, got {shaped} glyphs");

    // And the text survived: every paragraph's words appear across the pages.
    let all: String = texts_a.join("\n");
    assert!(all.contains("Knuth"), "fixture text must survive layout");
    assert!(all.contains("hyphen"), "fixture text must survive layout");
}
