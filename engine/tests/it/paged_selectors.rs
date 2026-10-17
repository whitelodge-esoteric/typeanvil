//! Paged-pass selector matching (CORE-368).
//!
//! The paged-media pass (`page`, `string-set`, counters, `content`,
//! `bookmark-*`, `float: footnote`, page floats, `writing-mode`) matches its
//! rules with the same `selectors`-crate matcher stylo uses, so combinators
//! (`#wrap p`, `#wrap > p`, `+`, `~`) and structural pseudos all apply
//! exactly as they do in the cascade. Box properties stylo already computes (`display`, `position`) are
//! never rewritten from stylesheet rules — stylo's cascade is authoritative —
//! so a UA rule re-read by the pass cannot clobber a more specific author rule.
//!
//! Spec: `docs/specifications/paged-media-css.spec.md` § Paged-pass selector
//! matching. Every test goes through the production `cascade` entry point.

use typeanvil::css::{
    cascade, BookmarkLevel, ComputedStyle, Display, Float, PageWritingMode, Position, Stylesheet,
};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

/// Cascade `html` and return the style of the element with `id="t"`.
fn style_of_t(html: &str) -> ComputedStyle {
    style_of(html, "t")
}

fn style_of(html: &str, want: &str) -> ComputedStyle {
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    let styles = cascade(&dom, &Stylesheet::parse(&css), &geometry());
    let id = dom
        .nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::Element(el) if el.id.as_deref() == Some(want)))
        .unwrap_or_else(|| panic!("no element #{want}"));
    styles[id].clone()
}

fn doc(css: &str) -> String {
    format!(
        "<html><head><style>{css}</style></head><body>\
         <section id=\"wrap\"><p id=\"t\">x</p><div><p id=\"deep\">y</p></div></section>\
         <p id=\"out\">z</p></body></html>"
    )
}

// --- box properties: stylo stays authoritative ------------------------------

/// The ticket's case: the UA `p { display: block }` the pass re-read used to
/// overwrite stylo's `inline` from the more specific `#wrap p`.
#[test]
fn display_descendant_combinator_survives_ua_rule() {
    let st = style_of_t(&doc("#wrap p { display: inline }"));
    assert_eq!(st.display, Display::Inline);
}

#[test]
fn display_child_combinator_survives_ua_rule() {
    let st = style_of_t(&doc("#wrap > p { display: inline }"));
    assert_eq!(st.display, Display::Inline);
}

/// A display value the pass never mapped (`table`) must not be replaced by
/// the UA `div { display: block }` rule either.
#[test]
fn display_value_unknown_to_pass_survives_ua_rule() {
    let html = "<html><head><style>div.x { display: table }</style></head>\
                <body><div class=\"x\" id=\"t\">x</div></body></html>";
    assert_eq!(style_of_t(html).display, Display::Table);
}

/// Specificity, not source order: `#wrap p` beats a later `p`.
#[test]
fn position_more_specific_combinator_wins_over_later_simple_rule() {
    let st = style_of_t(&doc("#wrap p { position: absolute } p { position: relative }"));
    assert_eq!(st.position, Position::Absolute);
}

/// Guard: a plain simple-selector display rule keeps working.
#[test]
fn display_simple_selector_unchanged() {
    let st = style_of_t(&doc("p { display: inline }"));
    assert_eq!(st.display, Display::Inline);
}

// --- paged-only properties: combinators now match ----------------------------

#[test]
fn page_descendant_combinator() {
    assert_eq!(style_of_t(&doc("#wrap p { page: a }")).page.as_deref(), Some("a"));
}

#[test]
fn page_child_combinator_skips_grandchild() {
    let html = doc("#wrap > p { page: a }");
    assert_eq!(style_of(&html, "t").page.as_deref(), Some("a"));
    assert_eq!(style_of(&html, "deep").page, None, "grandchild must not match `>`");
    assert_eq!(style_of(&html, "out").page, None, "outside #wrap must not match");
}

#[test]
fn string_set_descendant_combinator() {
    assert!(!style_of_t(&doc("#wrap p { string-set: t content() }")).string_set.is_empty());
}

#[test]
fn counter_reset_descendant_combinator() {
    let st = style_of_t(&doc("#wrap p { counter-reset: c 3 }"));
    assert_eq!(st.counter_reset, vec![("c".to_string(), 3)]);
}

#[test]
fn counter_increment_descendant_combinator() {
    let st = style_of_t(&doc("#wrap p { counter-increment: c 2 }"));
    assert_eq!(st.counter_increment, vec![("c".to_string(), 2)]);
}

#[test]
fn content_descendant_combinator() {
    assert!(!style_of_t(&doc("#wrap p { content: \"x\" }")).content.is_empty());
}

#[test]
fn bookmark_level_descendant_combinator() {
    let st = style_of_t(&doc("#wrap p { bookmark-level: 2 }"));
    assert_eq!(st.bookmark_level, BookmarkLevel::Level(2));
}

#[test]
fn bookmark_label_descendant_combinator() {
    assert!(!style_of_t(&doc("#wrap p { bookmark-label: \"L\" }")).bookmark_label.is_empty());
}

#[test]
fn bookmark_state_descendant_combinator() {
    assert!(style_of_t(&doc("#wrap p { bookmark-state: closed }")).bookmark_closed);
}

#[test]
fn float_footnote_descendant_combinator() {
    assert!(style_of_t(&doc("#wrap p { float: footnote }")).float_footnote);
}

#[test]
fn page_float_descendant_combinator() {
    assert_eq!(style_of_t(&doc("#wrap p { float: top }")).float, Float::Top);
}

/// The float write is specificity-guarded like its neighbours.
#[test]
fn float_more_specific_combinator_wins_over_later_simple_rule() {
    let st = style_of_t(&doc("#wrap p { float: left } p { float: top }"));
    assert_eq!(st.float, Float::Left);
}

#[test]
fn writing_mode_descendant_combinator() {
    let st = style_of_t(&doc("#wrap p { writing-mode: vertical-rl }"));
    assert_eq!(st.writing_mode, PageWritingMode::VerticalRl);
    assert!(st.writing_mode_declared);
}

/// The writing-mode write is specificity-guarded like its neighbours.
#[test]
fn writing_mode_more_specific_wins_over_later_simple_rule() {
    let st = style_of_t(&doc("#wrap p { writing-mode: vertical-rl } p { writing-mode: horizontal-tb }"));
    assert_eq!(st.writing_mode, PageWritingMode::VerticalRl);
}

/// Sibling combinators match too (the stylo adapter's element-only sibling
/// walk, selector-matching spec).
#[test]
fn sibling_combinators_match() {
    let html = "<html><head><style>h1 + p { page: a } h1 ~ p { string-set: s content() }</style></head>\
                <body><h1>h</h1>\n<p id=\"t\">x</p><p id=\"second\">y</p></body></html>";
    assert_eq!(style_of(html, "t").page.as_deref(), Some("a"));
    assert_eq!(style_of(html, "second").page, None, "`+` is the next sibling only");
    assert!(!style_of(html, "second").string_set.is_empty(), "`~` reaches later siblings");
}

// --- guards: behaviour that must not change ----------------------------------

#[test]
fn simple_selectors_unchanged() {
    let st = style_of_t(&doc("p { page: a } .none { page: b } #t { string-set: s content() }"));
    assert_eq!(st.page.as_deref(), Some("a"));
    assert!(!st.string_set.is_empty());
}

#[test]
fn structural_pseudo_unchanged() {
    let html = "<html><head><style>p:first-of-type { page: a } :root { writing-mode: vertical-rl }</style></head>\
                <body><p id=\"t\">x</p><p id=\"second\">y</p></body></html>";
    assert_eq!(style_of(html, "t").page.as_deref(), Some("a"));
    assert_eq!(style_of(html, "second").page, None);
}

/// A rule whose selector targets a pseudo-element styles that pseudo-element,
/// never its originating element.
#[test]
fn pseudo_element_rule_does_not_style_the_element() {
    let st = style_of_t(&doc("p::before { content: \"x\"; string-set: s content() }"));
    assert!(st.string_set.is_empty());
}

/// Inline `style=""` still beats every stylesheet rule.
#[test]
fn inline_style_still_wins() {
    let html = "<html><head><style>#wrap p { page: a }</style></head>\
                <body><section id=\"wrap\"><p id=\"t\" style=\"page: b\">x</p></section></body></html>";
    assert_eq!(style_of_t(html).page.as_deref(), Some("b"));
}

// --- attribute selectors (CORE-383) -----------------------------------------

fn attr_doc(css: &str, attrs: &str) -> String {
    format!(
        "<html><head><style>{css}</style></head><body>\
         <p id=\"t\" {attrs}>x</p><p id=\"other\">y</p></body></html>"
    )
}

/// Paged pass: `[data-k] { page: a }`.
#[test]
fn attribute_selector_matches() {
    let html = attr_doc("[data-k] { page: a }", "data-k=\"1\"");
    assert_eq!(style_of(&html, "t").page.as_deref(), Some("a"));
    assert_eq!(style_of(&html, "other").page, None);
}

/// Cascade (stylo): `[data-k] { color }` applies.
#[test]
fn attribute_selector_applies_in_cascade() {
    let ctl = attr_doc("p { color: rgb(255, 0, 0) }", "data-k=\"1\"");
    let c = style_of(&ctl, "t").color;
    assert_eq!((c.r, c.g, c.b), (255, 0, 0), "control: type selector color");
    let alt = attr_doc("p[data-k] { color: rgb(255, 0, 0) }", "data-k=\"1\"");
    let a = style_of(&alt, "t").color;
    assert_eq!((a.r, a.g, a.b), (255, 0, 0), "p[data-k]");
    let html = attr_doc("[data-k] { color: rgb(255, 0, 0) }", "data-k=\"1\"");
    let t = style_of(&html, "t").color;
    let o = style_of(&html, "other").color;
    assert_eq!((t.r, t.g, t.b), (255, 0, 0));
    assert_ne!((o.r, o.g, o.b), (255, 0, 0));
}

/// Every operator, including case flag and non-matching controls.
#[test]
fn attribute_operators() {
    let cases: &[(&str, &str, bool)] = &[
        ("[data-k=\"abc\"]", "data-k=\"abc\"", true),
        ("[data-k=\"ab\"]", "data-k=\"abc\"", false),
        ("[data-k~=\"b\"]", "data-k=\"a b c\"", true),
        ("[data-k~=\"bc\"]", "data-k=\"a b c\"", false),
        ("[data-k|=\"en\"]", "data-k=\"en-US\"", true),
        ("[data-k|=\"e\"]", "data-k=\"en-US\"", false),
        ("[data-k^=\"#\"]", "data-k=\"#top\"", true),
        ("[data-k^=\"x\"]", "data-k=\"#top\"", false),
        ("[data-k$=\".pdf\"]", "data-k=\"a.pdf\"", true),
        ("[data-k$=\".png\"]", "data-k=\"a.pdf\"", false),
        ("[data-k*=\"mid\"]", "data-k=\"a-mid-b\"", true),
        ("[data-k*=\"zzz\"]", "data-k=\"a-mid-b\"", false),
        ("[data-k=\"ABC\" i]", "data-k=\"abc\"", true),
        ("[data-k=\"ABC\"]", "data-k=\"abc\"", false),
        ("[data-k]", "data-other=\"1\"", false),
    ];
    for (sel, attrs, want) in cases {
        let html = attr_doc(&format!("{sel} {{ page: a }}"), attrs);
        let got = style_of(&html, "t").page.as_deref() == Some("a");
        assert_eq!(got, *want, "{sel} vs {attrs}");
    }
}
