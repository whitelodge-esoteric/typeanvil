//! CORE-523: Knuth-Plass line breaking must stay near-linear in paragraph
//! length. Pride and Prejudice has a few paragraphs of 10-14 KB; the DP used
//! to measure every (start, end) candidate pair from scratch (cubic), which
//! took ~68 s for the 350-page book.

use std::time::Instant;

use typeanvil::css::{cascade, Stylesheet};
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::typography::break_paragraph;

fn geometry() -> PageGeometry {
    let m = Scalar(36.0);
    PageGeometry {
        width: Scalar(432.0),
        height: Scalar(648.0),
        margin_top: m,
        margin_right: m,
        margin_bottom: m,
        margin_left: m,
    }
}

#[test]
fn long_justified_paragraph_breaks_in_bounded_time() {
    let dom = Dom::parse("<html><body><p>x</p></body></html>").unwrap();
    let ss = Stylesheet::parse("");
    let styles = cascade(&dom, &ss, &geometry());
    let style = styles[dom.find_tag("p").unwrap()].clone();
    let words = [
        "pride", "and", "prejudice", "universally", "acknowledged", "fortune", "wife",
        "neighbourhood", "of", "a",
    ];
    let text: Vec<&str> = (0..3000).map(|i| words[i % words.len()]).collect();
    let text = text.join(" ");
    let width = 300.0;
    let start = Instant::now();
    let lines = break_paragraph(&text, Scalar(width), &style, true, true);
    let elapsed = start.elapsed();
    assert!(lines.len() > 100, "expected many lines, got {}", lines.len());
    assert!(
        elapsed.as_secs_f64() < 10.0,
        "line breaking took {elapsed:?} for 3000 words (quadratic regression)"
    );
}
