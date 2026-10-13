//! CORE-348: one overlong word must not disable wrapping for the whole
//! paragraph. css-text-3 §5: text breaks at soft wrap opportunities; a word
//! that cannot fit overflows ON ITS OWN LINE and the rest keeps wrapping.
//! Before the fix, Knuth-Plass found no feasible break set and fell back to a
//! single line spanning the whole paragraph (table-stress Description cells
//! overprinted the next column).

use typeanvil::css::{cascade, ComputedStyle, Stylesheet};
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::typography::break_paragraph;

fn style() -> ComputedStyle {
    let dom = Dom::parse("<html><body><p>x</p></body></html>").unwrap();
    let ss = Stylesheet::parse("");
    let geo = PageGeometry {
        width: Scalar(5.0 * 72.0),
        height: Scalar(3.0 * 72.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    };
    let styles = cascade(&dom, &ss, &geo);
    styles[dom.find_tag("p").unwrap()].clone()
}

/// Natural width of `text` set on one unbounded line.
fn width_of(text: &str, st: &ComputedStyle) -> f64 {
    let lines = break_paragraph(text, Scalar(1.0e6), st, false, false);
    assert_eq!(lines.len(), 1);
    lines[0].natural_width.get()
}

/// The invariant: every line fits `max_width`, except a line holding a
/// single word (no inter-word space) — the only content allowed to overflow.
fn assert_wraps(text: &str, max_width: f64, justify: bool, st: &ComputedStyle) -> Vec<String> {
    let lines = break_paragraph(text, Scalar(max_width), st, false, justify);
    let texts: Vec<String> = lines.iter().map(|l| l.text.trim().to_string()).collect();
    for (l, t) in lines.iter().zip(&texts) {
        let overflows = l.natural_width.get() > max_width + 0.01;
        assert!(
            !overflows || !t.contains(' '),
            "line {t:?} ({:.2}pt) overflows {max_width:.2}pt and is not a lone word; lines={texts:?}",
            l.natural_width.get()
        );
    }
    texts
}

#[test]
fn overlong_word_overflows_alone_and_rest_wraps() {
    let st = style();
    let long = "Supercalifragilistic";
    let w_long = width_of(long, &st);
    let w_rest = width_of("pogo stick", &st);
    // Room for "pogo stick" but not for the long word.
    let max_w = w_rest + 1.0;
    assert!(w_long > max_w, "fixture: long word must be wider than the line");
    let lines = assert_wraps("Supercalifragilistic pogo stick", max_w, false, &st);
    assert_eq!(lines, vec![long.to_string(), "pogo stick".to_string()]);
}

#[test]
fn overlong_word_mid_paragraph_keeps_neighbours_wrapping() {
    let st = style();
    let max_w = width_of("pogo stick", &st) + 1.0;
    let lines = assert_wraps("pogo stick Supercalifragilistic pogo stick", max_w, false, &st);
    assert_eq!(
        lines,
        vec!["pogo stick".to_string(), "Supercalifragilistic".to_string(), "pogo stick".to_string()]
    );
}

#[test]
fn overlong_word_justified_paragraph_still_wraps() {
    let st = style();
    let max_w = width_of("pogo stick", &st) + 1.0;
    let lines = assert_wraps("Supercalifragilistic pogo stick pogo stick", max_w, true, &st);
    assert!(lines.len() >= 3, "justified paragraph must wrap, got {lines:?}");
    assert_eq!(lines[0], "Supercalifragilistic");
}

/// An explicit hyphen is a soft wrap opportunity (browsers: "Rocket-/powered"
/// under the default `hyphens: manual`); the hyphen stays on the first line
/// and no extra hyphen glyph is added.
#[test]
fn breaks_after_explicit_hyphen_when_word_overflows() {
    let st = style();
    // Room for "powered" but not "Rocket-powered".
    let max_w = width_of("rollerblades", &st) + 1.0;
    assert!(width_of("Rocket-powered", &st) > max_w);
    let lines = assert_wraps("Rocket-powered rollerblades", max_w, false, &st);
    assert_eq!(lines, vec!["Rocket-".to_string(), "powered".to_string(), "rollerblades".to_string()]);
}

/// A hyphenated word that FITS is not split (the break costs a penalty).
#[test]
fn fitting_hyphenated_word_stays_whole() {
    let st = style();
    let max_w = width_of("well-known x", &st) + 1.0;
    let lines = assert_wraps("well-known x", max_w, false, &st);
    assert_eq!(lines, vec!["well-known x".to_string()]);
}

/// Chromium keeps slash-joined words and "-5" whole: no break there.
#[test]
fn no_break_after_slash_or_before_number() {
    let st = style();
    let max_w = width_of("clothes", &st) + 1.0;
    let lines = break_paragraph("Disappearing/reappearing clothes", Scalar(max_w), &st, false, false);
    let texts: Vec<String> = lines.iter().map(|l| l.text.trim().to_string()).collect();
    assert_eq!(texts, vec!["Disappearing/reappearing".to_string(), "clothes".to_string()]);
    let lines = break_paragraph("x-5", Scalar(1.0), &st, false, false);
    assert_eq!(lines.len(), 1, "no break inside x-5");
}
