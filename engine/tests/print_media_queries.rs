//! Print media query acceptance tests — one per acceptance criterion in
//! `docs/specifications/print-media-queries.spec.md`.
//!
//! The engine is a print engine: `@media print` must match, and dimensional
//! features (`min-width`, `max-height`, ...) must evaluate against the PAGE
//! BOX (mediaqueries-4 §4: the size of the page box from the system/user,
//! before author `@page` sizing applies). All tests exercise the production
//! public path (`cascade` / `layout`), never a geometry-free raw parser: the
//! `@media` seam must be shared by stylo's cascade, the manual author-CSS
//! passes, and the `@page` parser.

use typeanvil::css::{cascade, Color, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

/// Points from inches.
fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

/// The WPT harness default: 5x3in page with .5in margins on all sides.
fn wpt_geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

/// Collect `<style>` text into a stylesheet (mirrors the CLI).
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

/// Lay out an HTML string with the given geometry (the production path the
/// CLI uses; every `@page`/media interaction must survive it).
fn lay(html: &str, geo: PageGeometry) -> typeanvil::layout::Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geo)
}

/// Body background color as an (r,g,b) triple, via the production cascade.
fn body_bg(html: &str, geo: PageGeometry) -> (u8, u8, u8) {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    let styles = cascade(&dom, &ss, &geo);
    let body = dom.find_tag("body").expect("body exists");
    match styles[body].background_color {
        Some(Color { r, g, b, .. }) => (r, g, b),
        None => panic!("body background unresolved"),
    }
}

/// AC1 — dimensional print MQ matches the PAGE BOX. This is the
/// media-queries-001 window: at 5x3in the page box is inside the
/// 4–5in x 2–3in window; the fixed 1024x768 viewport is NOT
/// (1024px = 10.67in wide). The margins are irrelevant to the query (the
/// page AREA would be 4x2in — this window cannot distinguish box from area,
/// AC1b below does).
#[test]
fn mq_dimensional_matches_page_box() {
    let html = r#"<style>
        body { background: red; }
        @media print and (min-width: 4in) and (max-width: 5in) and (min-height: 2in) and (max-height: 3in) {
            body { background: green; }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "dimensional print MQ must match the page box (green)"
    );
}

/// AC1b — exact box/area boundary: width:5in height:3in matches
/// `(width: 5in) and (height: 3in)` ONLY under page-box semantics.
/// Under page-area semantics (4x2in at 0.5in margins) the query fails and
/// the body stays red. `max-width`/`max-height` alone cannot distinguish
/// box from area here (both 4x2 and 5x3 satisfy it); exact equality can.
/// The author `@page` size must not affect the result either.
#[test]
fn mq_exact_page_box_boundary() {
    let html = r#"<style>
        @page { size: 5in 3in; margin: 0.5in; }
        body { background: red; }
        @media (width: 5in) and (height: 3in) { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "width:5in/height:3in must satisfy (width:5in) and (height:3in) (page box), \
         and the author @page size must not change the query result"
    );
}

/// AC2 — `@media screen` never applies in a print engine.
#[test]
fn mq_screen_does_not_match() {
    let html = r#"<style>
        body { background: red; }
        @media screen and (min-width: 0in) { body { background: blue; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (255, 0, 0),
        "screen MQ must not apply"
    );
}

/// AC3 — a false `@media` must not leak paged-media, break, or border
/// declarations through the element-consumer cascade (stylo + manual
/// passes), verified through the production `cascade` entry point.
#[test]
fn mq_false_query_removed_everywhere() {
    let html = r#"<style>
        @media (min-width: 9000in) {
            div { page: weird; string-set: s content(); border: 5px solid red; break-before: page; }
        }
    </style><body><div>probe</div></body>"#;
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    let styles = cascade(&dom, &ss, &wpt_geometry());
    let div = dom.find_tag("div").expect("div exists");
    let st = &styles[div];
    assert_eq!(st.page, None, "false @media must hide page property");
    assert!(
        st.string_set.is_empty(),
        "false @media must hide string-set property"
    );
    assert_eq!(
        st.border_top,
        Scalar::ZERO,
        "false @media must hide border property"
    );
    assert_eq!(
        st.break_before,
        typeanvil::frag::BreakBetween::Auto,
        "false @media must hide break-before property"
    );
}

/// AC3b — a false `@media` must not leak a `@page` rule through the
/// production layout path (the @page parser consumes the evaluated text).
#[test]
fn mq_false_media_hides_page_rule_in_layout() {
    let html = r#"<style>
        @media (min-width: 9000in) { @page { size: 1in 1in; } }
    </style><body>probe</body>"#;
    let lay = lay(html, wpt_geometry());
    let w = lay.pages[0].root.size.0.get();
    let h = lay.pages[0].root.size.1.get();
    assert!(
        (w - 360.0).abs() < 0.5 && (h - 216.0).abs() < 0.5,
        "false @media @page must not apply: page stays 5x3in (got {}x{})",
        w,
        h
    );
}

/// AC4 — nested `@media` applies only when BOTH conditions hold; a true
/// outer with a false inner drops the inner body.
#[test]
fn mq_nested_media() {
    let both = r#"<style>
        body { background: red; }
        @media print {
            @media (min-width: 4in) { body { background: green; } }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(both, wpt_geometry()),
        (0, 128, 0),
        "nested @media must apply when both conditions hold"
    );

    let inner_false = r#"<style>
        body { background: red; }
        @media print {
            @media (min-width: 9000in) { body { background: blue; } }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(inner_false, wpt_geometry()),
        (255, 0, 0),
        "true outer with false inner must drop the inner body"
    );
}

/// AC5 — `@media`-wrapped `@page` applies: size 3in 5in wins over the CLI
/// geometry.
#[test]
fn mq_media_wrapped_page_rule() {
    let html = r#"<style>
        @media print { @page { size: 3in 5in; margin: 0.5in; } }
    </style><body>probe</body>"#;
    let lay = lay(html, wpt_geometry());
    let w = lay.pages[0].root.size.0.get();
    let h = lay.pages[0].root.size.1.get();
    assert!(
        (w - 216.0).abs() < 0.5 && (h - 360.0).abs() < 0.5,
        "media-wrapped @page size must apply (got {}x{})",
        w,
        h
    );
}

/// AC6 — source order preserved: with two matching rules on the same
/// selector, the later @media-wrapped rule wins.
#[test]
fn mq_source_order_preserved() {
    let html = r#"<style>
        body { background: red; }
        @media print and (min-width: 4in) { body { background: green; } }
        @media print and (max-width: 5in) { body { background: yellow; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (255, 255, 0),
        "later matching @media rule must win"
    );
}

/// AC7 — the scanner is tokenizer-aware: a `@media` literal inside a quoted
/// `content` string is string data, never a media block; the valid `@media`
/// rule after the string still applies.
#[test]
fn mq_literal_inside_string_not_evaluated() {
    let html = r#"<style>
        body { background: red; }
        div::after { content: "@media print { body { background: black; } }"; }
        @media print and (min-width: 4in) { body { background: green; } }
    </style><body><div>probe</div></body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "a @media literal inside a content string must not hide or become \
         a media rule; the real @media after it must still apply"
    );
}

/// AC8 — `@media` under `@supports` / `@layer`: the outer condition/layer
/// must survive media evaluation (non-media at-rules are copied verbatim,
/// never unwrapped or erased).
#[test]
fn mq_media_under_supports_and_layer_preserved() {
    let html = r#"<style>
        body { background: red; }
        @supports (display: block) {
            @media print and (min-width: 4in) { body { background: green; } }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "@media inside @supports must apply when both conditions hold"
    );

    // Cascade5 §6.4.3: unlayered NORMAL author declarations beat layered
    // NORMAL declarations, so the earlier unlayered `red` wins over the
    // layered green even though it comes first. The seam must PRESERVE the
    // layer wrapper (not unwrap/flatten it) for stylo to compute this.
    let layered = r#"<style>
        body { background: red; }
        @layer base {
            @media print and (min-width: 4in) { body { background: green; } }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(layered, wpt_geometry()),
        (255, 0, 0),
        "unlayered normal author declaration wins over layered normal \
         (css-cascade-5 §6.4.3); the @layer wrapper must survive media \
         evaluation"
    );

    // Same construct without the competing unlayered declaration: the
    // nested @media inside the layer DOES apply (the wrapper is retained
    // and its rule list recursively evaluated).
    let layered_applies = r#"<style>
        @layer base {
            @media print and (min-width: 4in) { body { background: green; } }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(layered_applies, wpt_geometry()),
        (0, 128, 0),
        "a true @media inside @layer must apply when no unlayered rule \
         competes"
    );

    // A false @media inside a layer is dropped like anywhere else.
    let layered_false = r#"<style>
        body { background: red; }
        @layer base {
            @media screen { body { background: green; } }
        }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(layered_false, wpt_geometry()),
        (255, 0, 0),
        "a false @media inside @layer must be dropped"
    );

    // Named-layer statement order (`@layer a, b;` — css-cascade-5 §2.3)
    // survives the seam: the later-named layer b wins over a.
    let named_layers = r#"<style>
        @layer a, b;
        @layer a { body { background: green; } }
        @layer b { body { background: blue; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(named_layers, wpt_geometry()),
        (0, 0, 255),
        "statement @layer a, b; must survive and order the layers (b wins)"
    );
}

/// AC9 — a malformed unknown at-rule consumes to its end (block or EOF) but
/// must not swallow a following valid rule when the block is properly
/// closed. (An UNCLOSED unknown at-rule consumes to EOF per css-syntax-3
/// §5.4.2 — content after it is NOT a sibling and may legitimately vanish;
/// this test only pins the closed-block recovery case.)
#[test]
fn mq_malformed_at_rule_does_not_swallow_next_rule() {
    let html = r#"<style>
        body { background: red; }
        @frobnicate { totally broken ;;; { nested } }
        @media print and (min-width: 4in) { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "a malformed at-rule must not swallow the valid @media after it"
    );
}

/// AC10 — an unclosed unknown at-rule consumes to EOF (css-syntax-3 §5.4.2):
/// rules appearing inside its unclosed block are not real siblings.
/// The unmatched `{` swallows everything after it, so the false-query
/// `@page` inside never applies. This pins the EOF behavior explicitly.
#[test]
fn mq_unclosed_at_rule_consumes_to_eof() {
    let html = r#"<style>
        body { background: red; }
        @frobnicate { unclosed { @media (min-width: 0in) { @page { size: 1in 1in; } } }
    </style><body>probe</body>"#;
    let lay = lay(html, wpt_geometry());
    let w = lay.pages[0].root.size.0.get();
    let h = lay.pages[0].root.size.1.get();
    assert!(
        (w - 360.0).abs() < 0.5 && (h - 216.0).abs() < 0.5,
        "content inside an unclosed at-rule block must be swallowed: page \
         stays 5x3in (got {}x{})",
        w,
        h
    );
}

/// AC11 — an escaped, uppercase at-keyword (`@\6d edia` == `@media`,
/// css-syntax-3 §4.3.11 ident escape) must still be recognized as media.
#[test]
fn mq_escaped_uppercase_keyword_recognized() {
    let html = r#"<style>
        body { background: red; }
        @MEDIA print and (min-width: 4in) { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "case-insensitive @MEDIA must be recognized (css-syntax-3 at-keyword \
         matching is ASCII case-insensitive)"
    );
}

/// AC12 — an author `@page { size: ... }` must not change the MQ device:
/// the query evaluates against the ORIGINAL supplied geometry, not the
/// author-resized one.
#[test]
fn mq_device_ignores_author_page_size() {
    let html = r#"<style>
        @page { size: 1in 1in; }
        body { background: red; }
        @media (min-width: 4in) and (min-height: 2in) { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "author @page size must not shrink the media-query device"
    );
}

/// AC13 — a false `@supports` must not leak its body through the manual
/// parsing passes either (the manual passes see the same evaluated text).
#[test]
fn mq_false_supports_hides_page_rule() {
    let html = r#"<style>
        @supports (display: doesnotexist) { @page { size: 1in 1in; } }
    </style><body>probe</body>"#;
    let lay = lay(html, wpt_geometry());
    let w = lay.pages[0].root.size.0.get();
    let h = lay.pages[0].root.size.1.get();
    assert!(
        (w - 360.0).abs() < 0.5 && (h - 216.0).abs() < 0.5,
        "false @supports must not leak @page through the manual passes (got {}x{})",
        w,
        h
    );
}

/// AC14 — multibyte unicode inside a block must not break rule recovery
/// (no byte-wise slicing mid-codepoint).
#[test]
fn mq_unicode_content_does_not_break_scanning() {
    let html = r#"<style>
        body { background: red; }
        div::after { content: "Trøndere — 日本語"; }
        @media print and (min-width: 4in) { body { background: green; } }
    </style><body><div>probe</div></body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "unicode content must not derail the @media that follows"
    );
}

/// AC15 — a truly escaped at-keyword (`@\6d edia` decodes to `@media`,
/// css-syntax-3 §4.3.11) is recognized as media by the tokenizer-driven
/// seam.
#[test]
fn mq_escape_decoded_at_keyword_is_media() {
    let html = r#"<style>
        body { background: red; }
        @\6d edia print and (min-width: 4in) { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "@\\6d edia must decode to @media (css-syntax-3 ident escapes)"
    );
}

/// AC16 — media query list semantics (mediaqueries-4 §2.3/§3): an EMPTY
/// media list matches; an invalid query alone never matches; a comma list
/// matches when ANY query matches.
#[test]
fn mq_media_list_semantics() {
    let empty = r#"<style>
        @media { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(empty, wpt_geometry()),
        (0, 128, 0),
        "an empty media query list evaluates to true (mediaqueries-4 §2.3)"
    );

    let invalid = r#"<style>
        body { background: red; }
        @media (invalid-feature-here) { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(invalid, wpt_geometry()),
        (255, 0, 0),
        "an invalid media query is 'not all' and never matches"
    );

    let comma = r#"<style>
        body { background: red; }
        @media (invalid-feature-here), print { body { background: green; } }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(comma, wpt_geometry()),
        (0, 128, 0),
        "a comma list matches when ANY query matches"
    );
}

/// AC17 — a closed unknown at-rule is dropped from the evaluated text
/// entirely (CSS ignores unknown rules): the manual page parser must not
/// see a `@page` inside it, while a `@page` AFTER it still applies.
#[test]
fn mq_unknown_block_suppresses_inner_page_rule() {
    let inner = r#"<style>
        @frobnicate { @page { size: 1in 1in; } }
    </style><body>probe</body>"#;
    let result = lay(inner, wpt_geometry());
    let w = result.pages[0].root.size.0.get();
    let h = result.pages[0].root.size.1.get();
    assert!(
        (w - 360.0).abs() < 0.5 && (h - 216.0).abs() < 0.5,
        "unknown closed at-rule must be dropped, not unwrapped (got {}x{})",
        w,
        h
    );

    let after = r#"<style>
        @frobnicate { junk }
        @page { size: 2in 3in; }
    </style><body>probe</body>"#;
    let result = lay(after, wpt_geometry());
    let w = result.pages[0].root.size.0.get();
    let h = result.pages[0].root.size.1.get();
    assert!(
        (w - 144.0).abs() < 0.5 && (h - 216.0).abs() < 0.5,
        "@page after a dropped unknown rule must still apply (got {}x{})",
        w,
        h
    );
}

/// AC18 — a true `@media`-wrapped `@page` with margin-box subrules keeps
/// BOTH the page declarations and the margin boxes (declaration bodies are
/// preserved whole, never rule-list-recursed).
#[test]
fn mq_media_wrapped_page_keeps_margin_boxes() {
    let html = r#"<style>
        @media print {
            @page {
                size: 3in 5in;
                @bottom-center { content: "folio"; }
            }
        }
    </style><body>probe</body>"#;
    let lay = lay(html, wpt_geometry());
    let page = &lay.pages[0];
    let w = page.root.size.0.get();
    let h = page.root.size.1.get();
    assert!(
        (w - 216.0).abs() < 0.5 && (h - 360.0).abs() < 0.5,
        "media-wrapped @page size must apply (got {}x{})",
        w,
        h
    );
    // The margin box renders as a line fragment whose text is "folio";
    // it lives in the page margin (y >= content bottom).
    let folio = page.root.children.iter().any(|f| {
        f.offset.y.get() > h / 2.0
            && matches!(&f.content, typeanvil::frag::FragmentContent::Text(run)
                if run.text.contains("folio"))
    });
    assert!(folio, "media-wrapped @page margin box must survive the seam");
}

/// AC19 — a top-level `@property` definition registers a custom property
/// with the cascade: `var(--bg)` resolves to its initial value.
#[test]
fn property_initial_value_applies() {
    let html = r#"<style>
        @property --bg-property-initial {
            syntax: "<color>";
            inherits: false;
            initial-value: green;
        }
        body { background: var(--bg-property-initial); }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "@property initial-value must apply via the cascade"
    );
}

/// AC20 — `@property` registration rejects an invalid specified value for
/// its syntax and the declaration falls back to the registered
/// initial-value (css-properties-values-api-1 §2.4.2).
#[test]
fn property_invalid_value_falls_back_to_initial() {
    let html = r#"<style>
        @property --bg-property-fallback {
            syntax: "<color>";
            inherits: false;
            initial-value: green;
        }
        body { background: var(--bg-property-fallback); --bg-property-fallback: 12px; }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "an invalid value for a registered property must fall back to the initial-value"
    );
}

/// AC21 — `@property` works inside both a true and a false `@media`: the
/// true one registers, the false one does not (unique property names so the
/// global registration table in one test cannot leak into another).
#[test]
fn property_inside_media() {
    let html = r#"<style>
        @media print {
            @property --bg-media-true {
                syntax: "<color>";
                inherits: false;
                initial-value: green;
            }
        }
        body { background: var(--bg-media-true); }
    </style><body>probe</body>"#;
    assert_eq!(
        body_bg(html, wpt_geometry()),
        (0, 128, 0),
        "@property inside a true @media must register"
    );

    let html = r#"<style>
        @media (min-width: 9000in) {
            @property --bg-media-false {
                syntax: "<color>";
                inherits: false;
                initial-value: green;
            }
        }
        body { background: var(--bg-media-false); }
    </style><body>probe</body>"#;
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    let styles = cascade(&dom, &ss, &wpt_geometry());
    let body = dom.find_tag("body").expect("body exists");
    assert!(
        styles[body].background_color.is_none(),
        "@property inside a false @media must not register"
    );
}
