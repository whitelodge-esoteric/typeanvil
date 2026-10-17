//! CSS color alpha preservation (CORE-153, css-color-3/4).
//!
//! The stylo computed color carries an alpha channel; layout and PDF emit
//! must not drop it. These tests assert the seam invariants:
//! * `parse_css_color` accepts `#rgba`/`#rrggbbaa`, `rgb()`/`rgba()` (comma
//!   and space syntax, percentage and number components) and the
//!   `transparent` keyword; forms without alpha are opaque. Invalid
//!   declarations (malformed hex tokens, non-finite numbers, wrong channel
//!   arity, mixed channel kinds, bad alpha) are rejected so the cascade
//!   falls back (css-syntax-3: invalid declarations are ignored).
//! * A semitransparent body background survives the cascade into the
//!   fragmentainer's canvas background (css-backgrounds-3 §2.2 propagation)
//!   with its alpha intact; the `@page` background stays opaque.

use typeanvil::css::{parse_css_color, Color, Stylesheet};
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

/// Extract all `<style>` text into a stylesheet (mirrors the CLI).
fn stylesheet_of(html: &str) -> String {
    use typeanvil::dom::NodeKind;
    let dom = Dom::parse(html).unwrap();
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    css
}

fn layout_html(html: &str) -> Layout {
    let dom = Dom::parse(html).expect("parse html");
    let ss = Stylesheet::parse(&stylesheet_of(html));
    layout(&dom, &ss, geometry())
}


// --- parser -----------------------------------------------------------------

#[test]
fn invalid_declarations_are_rejected() {
    // Malformed hex: the whole token must be a valid hash (css-color-4 §4.2
    // hash-token grammar); filtering non-hexdigit chars must not rescue it.
    assert_eq!(parse_css_color("# f 0 0"), None);
    assert_eq!(parse_css_color("#f00zz"), None);
    assert_eq!(parse_css_color("#f0 0"), None);

    // Non-finite numbers are not valid components (css-color-4 §5.1).
    assert_eq!(parse_css_color("rgba(255, 0, 0, NaN)"), None);
    assert_eq!(parse_css_color("rgba(255, 0, 0, inf)"), None);
    assert_eq!(parse_css_color("rgb(NaN, 0, 0)"), None);
    assert_eq!(parse_css_color("rgba(255, 0, 0, garbage)"), None);

    // Legacy comma syntax takes EXACTLY 3 or 4 channels; extra channels are
    // a parse failure, not a silent truncation.
    assert_eq!(parse_css_color("rgb(255, 0, 0, 0.5, 123)"), None);

    // Space syntax takes 3 channels, optionally `/ alpha` — no bare 4th.
    assert_eq!(parse_css_color("rgb(255 0 0 123)"), None);
    assert_eq!(parse_css_color("rgb(255 0 0)"), Some(Color::rgb(255, 0, 0)));

    // Legacy comma syntax requires one uniform channel kind ("either all...
    // or none", css-color-4 §5.1); the modern space syntax may mix them.
    // The alpha value is independent of channel kind either way.
    assert_eq!(parse_css_color("rgb(100%, 0, 0)"), None);
    assert_eq!(
        parse_css_color("rgb(50% 0% 0% / 0.5)"),
        Some(Color::rgba(128, 0, 0, 128))
    );
}

#[test]
fn invalid_alpha_preserves_cascade_fallback() {
    // An invalid background declaration is ignored entirely: the earlier
    // valid `background: blue` in the same @page rule must survive
    // (css-cascade: declarations with syntax errors are dropped at parse
    // time, leaving the previous declaration in effect).
    let html = r#"<html><head><style>
        @page { margin: 0; background: blue; background: rgba(255, 0, 0, garbage); }
    </style></head><body></body></html>"#;
    let layout = layout_html(html);
    assert_eq!(
        layout.pages[0].background,
        Some(Color::rgb(0, 0, 255)),
        "invalid rgba alpha must be ignored, keeping the earlier blue fallback"
    );
}

#[test]
fn hex_alpha_forms() {
    assert_eq!(parse_css_color("#f008"), Some(Color::rgba(255, 0, 0, 0x88)));
    assert_eq!(
        parse_css_color("#ff000088"),
        Some(Color::rgba(255, 0, 0, 0x88))
    );
    // Forms without alpha are opaque.
    assert_eq!(parse_css_color("#f00"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(parse_css_color("#ff0000"), Some(Color::rgb(255, 0, 0)));
}


#[test]
fn rgb_function_forms() {
    assert_eq!(
        parse_css_color("rgba(255, 0, 0, 0.5)"),
        Some(Color::rgba(255, 0, 0, 128))
    );
    assert_eq!(
        parse_css_color("rgba(100% 0% 0% / 50%)"),
        Some(Color::rgba(255, 0, 0, 128))
    );
    assert_eq!(
        parse_css_color("rgb(0 0 255)"),
        Some(Color::rgb(0, 0, 255))
    );
    // Alpha 0 parses (does not collapse to a parse failure).
    assert_eq!(
        parse_css_color("rgba(0, 0, 0, 0)"),
        Some(Color::rgba(0, 0, 0, 0))
    );
    // Components clamp.
    assert_eq!(
        parse_css_color("rgba(300, -20, 0, 2)"),
        Some(Color::rgba(255, 0, 0, 255))
    );
    // css-color-4 §7.1: rgba() is an alias of rgb(); both comma forms take
    // THREE channels and an optional fourth alpha, independent of spelling.
    assert_eq!(parse_css_color("rgba(1, 2, 3)"), Some(Color::rgb(1, 2, 3)));
    assert_eq!(
        parse_css_color("rgb(1, 2, 3, 0.5)"),
        Some(Color::rgba(1, 2, 3, 128))
    );
    // Function names are ASCII case-insensitive.
    assert_eq!(parse_css_color("RGB(1 2 3)"), Some(Color::rgb(1, 2, 3)));
    assert_eq!(
        parse_css_color("RGBa(255, 0, 0, 0.5)"),
        Some(Color::rgba(255, 0, 0, 128))
    );
    // Modern space syntax allows MIXED number/percentage channels; only
    // legacy comma syntax requires uniform kinds.
    assert_eq!(
        parse_css_color("rgb(100% 0 0 / 50%)"),
        Some(Color::rgba(255, 0, 0, 128))
    );
    // Legacy percentages with spaces around commas parse.
    assert_eq!(
        parse_css_color("rgb( 100% , 0% , 0% )"),
        Some(Color::rgb(255, 0, 0))
    );
}

#[test]
fn legacy_mixed_kinds_still_rejected() {
    // Only the MODERN branch lifts the uniform-kind rule; legacy comma
    // syntax still requires all-numbers or all-percentages.
    assert_eq!(parse_css_color("rgb(100%, 0, 0)"), None);
    assert_eq!(parse_css_color("rgba(100%, 0, 0, 0.5)"), None);
    // Modern space syntax mixed kinds are valid (positive control).
    assert_eq!(
        parse_css_color("rgb(50% 0% 0)"),
        Some(Color::rgb(128, 0, 0))
    );
}

#[test]
fn transparent_keyword_is_fully_transparent_black() {
    // css-color-3: `transparent` is rgba(0,0,0,0) — a real color, not a
    // parse failure.
    assert_eq!(parse_css_color("transparent"), Some(Color::TRANSPARENT));
}

// --- seam: cascade → canvas propagation -------------------------------------

#[test]
fn semitransparent_body_background_keeps_alpha_in_canvas_propagation() {
    // page-box-002 shape: opaque blue @page, semitransparent red body. The
    // canvas background (the propagated body fill) must carry alpha 0x88;
    // the page box fill must stay opaque blue.
    let html = r#"<html><head><style>
        @page { margin: 0; background: #00f; }
        body { background: #f008; }
    </style></head><body></body></html>"#;
    let layout = layout_html(html);

    assert_eq!(layout.pages.len(), 1);
    let page = &layout.pages[0];
    assert_eq!(page.background, Some(Color::rgb(0, 0, 255)));
    assert_eq!(
        page.canvas_background,
        Some(Color::rgba(255, 0, 0, 0x88)),
        "canvas propagation dropped the body background's alpha"
    );
}

#[test]
fn opaque_backgrounds_default_to_opaque_alpha() {
    let html = r#"<html><head><style>
        @page { margin: 0; }
        body { background: blue; }
    </style></head><body></body></html>"#;
    let layout = layout_html(html);
    let page = &layout.pages[0];
    let bg = page.canvas_background.expect("body background propagates");
    assert_eq!(bg.a, 255, "plain named colors must be opaque");
}
