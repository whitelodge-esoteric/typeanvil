//! CSS cascade — driven by Servo's `stylo` engine.
//!
//! [`ComputedStyle`] is the output contract: layout and PDF read only this.
//! [`cascade`] is the single entry point; the hand-rolled cssparser cascade
//! that previously lived here was replaced wholesale by stylo's matching and
//! cascade (CORE-56).
//!
//! Pipeline per call to [`cascade`]:
//!
//! 1. Parse the author stylesheet with `style::stylesheets::Stylesheet::from_str`.
//! 2. Build a print [`Device`] and a [`Stylist`] seeded with the stylesheet.
//! 3. Build a [`SharedStyleContext`] and a [`ThreadLocalStyleContext`], then
//!    resolve each element in pre-order with `StyleResolverForElement`
//!    (parents first, so `borrow_data`-based inheritance works).
//! 4. Convert the resulting `ComputedValues` into [`ComputedStyle`].
//!
//! Supported today: inline `<style>` rules; element / `.class` / `#id` /
//! descendant selectors; specificity ordering; inheritance of `color`,
//! `font-size`, `font-family`; non-inherited `display`, `background-color`,
//! `margin`, `padding`.
//!
//! Determinism note: stylo's rule tree and style sharing cache are pure
//! functions of (stylesheet, element data), and every element is resolved
//! from an empty `ElementData` on a fresh backend, so the output is
//! byte-identical across runs.

use euclid::{Scale, Size2D};
use style_traits::{CSSPixel, DevicePixel};
use servo_arc::Arc;
use style::device::Device;
use style::device::servo::FontMetricsProvider;
use style::media_queries::MediaType;
use style::properties::style_structs::Font;
use style::properties::ComputedValues;
use style::queries::values::PrefersColorScheme;
use style::servo::media_features::PointerCapabilities;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::stylist::{RuleInclusion, Stylist};
use style::stylesheets::{AllowImportRules, Origin, Stylesheet as StylesheetFromStylo, UrlExtraData};
use style::values::computed::font::{FontFamily, LineHeight, SingleFontFamily};
use style::values::computed::Color as ComputedColor;
use style::values::computed::Length;
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style::values::specified::font::FONT_MEDIUM_PX;
use style::values::specified::text::TextAlignKeyword;
use url::Url;

use crate::dom::{Dom, NodeId, NodeKind};
use crate::geom::{px_to_pt, Scalar};
use crate::stylo_dom::{TyBackend, TyElement};

/// An sRGB color, 8 bits per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
}
/// The CSS `normal` line-height factor (font-size multiple).
pub const NORMAL_LINE_HEIGHT_FACTOR: f64 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Display {
    Block,
    Inline,
    None,
    Table,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
}

/// The `text-align` computed value (css-text-3 §8). Read from stylo's
/// inherited-text struct in [`cascade`] — unlike the css-break longhands,
/// `text-align` IS compiled in the servo build, so it gets the full cascade
/// (specificity, `!important`, inheritance) for free.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// `start` — flush to the inline-start edge (left in LTR). The default.
    #[default]
    Start,
    /// `left`.
    Left,
    /// `right`.
    Right,
    /// `center`.
    Center,
    /// `justify` — non-final lines fill the content width via the K-P glue
    /// model (reaches layout as the `justify` flag of `break_paragraph`).
    Justify,
    /// `end` — flush to the inline-end edge (right in LTR).
    End,
}

/// The `hyphens` computed value (css-text-3 §6). `hyphens` is
/// `engine = "gecko"` in stylo's servo build, so — like `orphans`/`widows` —
/// it is absent from `ComputedValues` and filled by the author-CSS pass
/// (`breaks::apply_break_properties`), then inherited down the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Hyphens {
    /// Hyphenation disabled: breaks only at UAX #14 opportunities.
    None,
    /// Manual hyphenation only (soft hyphens). Our breaker does not yet
    /// honor `&shy;`, so this behaves like `none`. The CSS default.
    #[default]
    Manual,
    /// Automatic Liang hyphenation (`hyphenate` in the K-P breaker).
    Auto,
}

/// The value of a single `string-set` assignment.
///
/// Only `content()` (the element's own text content) is modeled; `attr()` and
/// literal forms parse but resolve as content for now (spec §Interfaces).
#[derive(Clone, Debug, PartialEq)]
pub enum StringSetValue {
    /// `content()` / `content(text)` — the element's text content.
    Content,
}

/// Break behavior at a box's leading/trailing boundary: the computed value of
/// `break-before` / `break-after`. Re-exported from [`crate::frag`] so the
/// cascade output contract carries it directly.
pub use crate::frag::{BreakBetween, BreakInside};

/// Fully computed style for one element. This is the cascade's output contract;
/// layout and PDF read only this.
#[derive(Clone, Debug)]
pub struct ComputedStyle {
    pub color: Color,
    pub background_color: Option<Color>,
    /// Border widths (points) on each side; 0 = no border. Colors come from
    /// [`ComputedStyle::border_color`].
    pub border_top: Scalar,
    pub border_right: Scalar,
    pub border_bottom: Scalar,
    pub border_left: Scalar,
    /// Border color shared by all four sides (CORE-61: collapse model, single
    /// color). `None` means the border is transparent / not painted.
    pub border_color: Option<Color>,
    pub font_size: Scalar,
    /// The resolved line box height in points (`line-height` property).
    pub line_height: Scalar,
    pub font_family: String,
    pub display: Display,
    pub margin_top: Scalar,
    pub margin_right: Scalar,
    pub margin_bottom: Scalar,
    pub margin_left: Scalar,
    pub padding_top: Scalar,
    pub padding_right: Scalar,
    pub padding_bottom: Scalar,
    pub padding_left: Scalar,
    /// `break-before` computed value (+ legacy `page-break-before`).
    pub break_before: BreakBetween,
    /// `break-after` computed value (+ legacy `page-break-after`).
    pub break_after: BreakBetween,
    /// `break-inside` computed value (+ legacy `page-break-inside`).
    pub break_inside: BreakInside,
    /// `orphans`: minimum lines left at the bottom of a fragmentainer.
    pub orphans: u32,
    /// `widows`: minimum lines carried to the top of the next fragmentainer.
    pub widows: u32,
    /// The `page` property: the named page this box switches to (paged-media).
    pub page: Option<String>,
    /// `string-set` declarations: `(string name, value)` pairs.
    pub string_set: Vec<(String, StringSetValue)>,
    /// `counter-reset` declarations: `(counter name, value)` pairs.
    pub counter_reset: Vec<(String, i32)>,
    /// `counter-increment` declarations: `(counter name, delta)` pairs.
    pub counter_increment: Vec<(String, i32)>,
    /// The `content` property, as an ordered generated-content piece list
    /// (empty when unset). Used by the TOC:
    /// `content: leader('.') target-counter(attr(href), page)`.
    ///
    /// Deviation from the spec's Interfaces sketch: the spec lists only
    /// `page`/`string_set`/`counter_*` on `ComputedStyle`, but Acceptance
    /// Criterion #8 (TOC via `target-counter` + `leader`) requires generated
    /// content on ordinary elements, so `content` is carried here too.
    pub content: Vec<crate::paged::ContentPiece>,
    /// `text-align` computed value (typography layer: justify must reach
    /// layout). Read from stylo in [`convert`]; inherited.
    pub text_align: TextAlign,
    /// `hyphens` computed value (typography layer). Filled by the author-CSS
    /// pass like the break longhands; inherited.
    pub hyphens: Hyphens,
}

impl ComputedStyle {
    /// The initial / root style (the base of inheritance).
    fn initial() -> ComputedStyle {
        ComputedStyle {
            color: Color::BLACK,
            background_color: None,
            border_top: Scalar::ZERO,
            border_right: Scalar::ZERO,
            border_bottom: Scalar::ZERO,
            border_left: Scalar::ZERO,
            border_color: None,
            font_size: px_to_pt(16.0),
            line_height: px_to_pt(16.0) * NORMAL_LINE_HEIGHT_FACTOR,
            font_family: "sans-serif".to_string(),
            display: Display::Inline,
            margin_top: Scalar::ZERO,
            margin_right: Scalar::ZERO,
            margin_bottom: Scalar::ZERO,
            margin_left: Scalar::ZERO,
            padding_top: Scalar::ZERO,
            padding_right: Scalar::ZERO,
            padding_bottom: Scalar::ZERO,
            padding_left: Scalar::ZERO,
            break_before: BreakBetween::Auto,
            break_after: BreakBetween::Auto,
            break_inside: BreakInside::Auto,
            orphans: 2,
            widows: 2,
            text_align: TextAlign::Start,
            hyphens: Hyphens::Manual,
            page: None,
            string_set: Vec::new(),
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            content: Vec::new(),
        }
    }

}

/// A parsed stylesheet.
///
/// The stylo engine parses CSS lazily inside [`cascade`]; this type exists so
/// the seam (`Stylesheet::parse`) survives and layout can thread the CSS text
/// through. It intentionally carries no structure of its own — stylo owns the
/// parsing and matching.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    css: String,
}

impl Stylesheet {
    /// Collect CSS source text. Parsing is deferred to stylo inside [`cascade`].
    pub fn parse(css: &str) -> Stylesheet {
        Stylesheet {
            css: css.to_string(),
        }
    }

    /// The raw CSS source.
    pub fn source(&self) -> &str {
        &self.css
    }
}

// --- Stylo plumbing --------------------------------------------------------

/// The sole [`FontMetricsProvider`] this engine needs: stylo queries font
/// metrics for `ex`/`ch`/etc. units, none of which the skeleton uses. We
/// answer with the CSS-defined default assumptions.
#[derive(Debug)]
struct SkeletonFontMetrics;

impl FontMetricsProvider for SkeletonFontMetrics {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &style::properties::style_structs::Font,
        _base_size: style::values::computed::CSSPixelLength,
        _flags: style::values::specified::font::QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        style::font_metrics::FontMetrics::default()
    }

    fn base_size_for_generic(&self, _generic: style::values::computed::font::GenericFontFamily) -> Length {
        Length::new(FONT_MEDIUM_PX)
    }
}

/// The stylo bookkeeping for one [`cascade`] call: the device, the stylist
/// seeded with the author stylesheet, and the lock that guards the parsed
/// rules. The [`TyBackend`] (element-data arena) lives alongside at the call
/// site so its `'a`-tied references don't fight the session's own borrows.
struct CascadeSession {
    stylist: Stylist,
    _lock: SharedRwLock,
}

impl CascadeSession {
    /// Minimal UA stylesheet: the HTML default block layout the engine needs.
    /// (Stylo ships no defaults; without this, h1/p/etc. compute as inline.)
    const UA_CSS: &'static str = r#"
        html, body, div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, dl, dt, dd,
        blockquote, pre, section, article,
        header, footer, nav, main, aside, figure, figcaption { display: block; }
        table { display: table; border-collapse: collapse; }
        thead { display: table-header-group; }
        tbody { display: table-row-group; }
        tfoot { display: table-footer-group; }
        tr { display: table-row; }
        td, th { display: table-cell; }
        h1, h2, h3, h4, h5, h6 { font-weight: bold; }
        h1 { font-size: 2em; margin: 0.67em 0; }
        h2 { font-size: 1.5em; margin: 0.83em 0; }
        h3 { font-size: 1.17em; margin: 1em 0; }
        h4 { font-size: 1em; margin: 1.33em 0; }
        h5 { font-size: 0.83em; margin: 1.67em 0; }
        h6 { font-size: 0.67em; margin: 2.33em 0; }
        p { margin: 1em 0; }
        ul, ol { padding-left: 2.5em; margin: 1em 0; }
        body { margin: 8px; }
        blockquote { margin: 1em 2.5em; }
        pre { margin: 1em 0; font-family: monospace; }
        table { border-collapse: collapse; }
        td, th { display: table-cell; }
    "#;

    fn new(lock: &SharedRwLock, css: &str) -> Self {
        let guard = lock.read();
        let guards = StylesheetGuards::same(&guard);

        let default_values = ComputedValues::initial_values_with_font_override(Font::initial_values());
        let viewport = Size2D::<f32, CSSPixel>::new(1024.0, 768.0);
        let device_size = Size2D::<f32, DevicePixel>::new(1024.0, 768.0);
        let url_data = UrlExtraData(Arc::new(
            Url::parse("http://localhost/").expect("static URL is valid"),
        ));
        let parse_sheet = |css: &str, origin: Origin| -> StylesheetFromStylo {
            StylesheetFromStylo::from_str(
                css,
                url_data.clone(),
                origin,
                Arc::new(lock.wrap(style::media_queries::MediaList::empty())),
                lock.clone(),
                None,
                None,
                style::context::QuirksMode::NoQuirks,
                AllowImportRules::No,
            )
        };
        let ua_sheet = parse_sheet(Self::UA_CSS, Origin::UserAgent);
        let author_sheet = parse_sheet(css, Origin::Author);

        let mut stylist = Stylist::new(
            Device::new(
                MediaType::print(),
                style::context::QuirksMode::NoQuirks,
                viewport,
                device_size,
                Scale::new(1.0),
                Box::new(SkeletonFontMetrics),
                default_values.clone(),
                PrefersColorScheme::Light,
                PointerCapabilities::empty(),
                PointerCapabilities::empty(),
            ),
            style::context::QuirksMode::NoQuirks,
        );
        stylist.append_stylesheet(
            style::stylesheets::DocumentStyleSheet(Arc::new(ua_sheet)),
            &lock.read(),
        );
        stylist.append_stylesheet(
            style::stylesheets::DocumentStyleSheet(Arc::new(author_sheet)),
            &lock.read(),
        );
        stylist.flush(&guards);

        CascadeSession {
            stylist,
            _lock: lock.clone(),
        }
    }

    /// Resolve the style for one element. `parent` is the already-resolved
    /// parent `ComputedValues` (or `None` for the root).
    ///
    /// This drives matching + cascade directly on the stylist, bypassing
    /// `StyleResolverForElement`: stylo's style-sharing cache transmutes the
    /// element type to a `usize` in thread-local storage and asserts the
    /// element is pointer-sized. Our `TyElement` is 16 bytes (NodeId +
    /// backend ref), so that path panics; the manual route is functionally
    /// identical (same rule collection, same cascade), just without the
    /// sharing-cache optimization.
    fn resolve<'a>(
        &mut self,
        element: TyElement<'a>,
        parent: Option<&Arc<ComputedValues>>,
    ) -> (Arc<ComputedValues>, ComputedStyle) {
        let guard = self._lock.read();
        let guards = StylesheetGuards::same(&guard);

        // 1. Collect the applicable declarations (matching).
        let mut selector_caches = selectors::context::SelectorCaches::default();
        let mut matching_context = selectors::context::MatchingContext::new_for_visited(
            selectors::matching::MatchingMode::Normal,
            None, // no bloom filter: selector matching is still exact, just uncached
            &mut selector_caches,
            selectors::matching::VisitedHandlingMode::AllLinksUnvisited,
            selectors::matching::QuirksMode::NoQuirks,
            selectors::matching::NeedsSelectorFlags::Yes,
            selectors::matching::MatchingForInvalidation::No,
        );
        let mut applicable = style::applicable_declarations::ApplicableDeclarationList::new();
        self.stylist.push_applicable_declarations(
            element,
            None,
            None,
            None,
            style::properties::AnimationDeclarations::default(),
            RuleInclusion::All,
            &mut applicable,
            &mut matching_context,
        );

        // 2. Build the rule node from the matched rules.
        let rule_node = self.stylist.rule_tree().compute_rule_node(&mut applicable, &guards);
        let inputs = style::context::CascadeInputs {
            rules: Some(rule_node),
            visited_rules: None,
            flags: matching_context.extra_data.cascade_input_flags,
            included_cascade_flags: style::rule_tree::RuleCascadeFlags::empty(),
        };

        // 3. Cascade against the parent style.
        let parent = parent.map(|p| &**p);
        let tactic = style::values::specified::position::PositionTryFallbacksTryTactic(
            Default::default(),
        );
        let mut rule_cache_conditions = style::rule_cache::RuleCacheConditions::default();
        let mut tree_counting = style::context::TreeCountingCaches::default();
        let primary = self.stylist.cascade_style_and_visited(
            Some(element),
            None,
            &inputs,
            &guards,
            parent,
            parent,
            style::properties::FirstLineReparenting::No,
            &tactic,
            None, // no rule cache (safe: `try_to_use_cached_reset_properties` returns false)
            &mut rule_cache_conditions,
            &mut tree_counting,
        );

        let computed = Self::convert(&primary);
        (primary, computed)
    }

    /// Convert stylo's `ComputedValues` into the engine's [`ComputedStyle`].
    fn convert(values: &ComputedValues) -> ComputedStyle {
        let color = values.clone_color();
        let font = values.get_font();
        let box_ = values.get_box();
        let background = values.get_background();
        let margin = values.get_margin();
        let padding = values.get_padding();
        let text = values.get_inherited_text();

        // `text-align` compiles in the servo build (unlike the break
        // longhands), so stylo's cascade computed it — including inheritance
        // and the `start`/`end` logical keywords.
        let text_align = match text.clone_text_align() {
            TextAlignKeyword::Start => TextAlign::Start,
            TextAlignKeyword::Left | TextAlignKeyword::MozLeft => TextAlign::Left,
            TextAlignKeyword::Right | TextAlignKeyword::MozRight => TextAlign::Right,
            TextAlignKeyword::Center | TextAlignKeyword::MozCenter => TextAlign::Center,
            TextAlignKeyword::Justify => TextAlign::Justify,
            TextAlignKeyword::End => TextAlign::End,
        };

        let display = match box_.clone_display() {
            d if d.is_none() => Display::None,
            d => match d.inside() {
                DisplayInside::Table => {
                    if matches!(d.outside(), DisplayOutside::Block) {
                        Display::Table
                    } else {
                        // inline-table is unsupported → block fallback.
                        Display::Block
                    }
                }
                DisplayInside::TableRowGroup => Display::TableRowGroup,
                DisplayInside::TableHeaderGroup => Display::TableHeaderGroup,
                DisplayInside::TableFooterGroup => Display::TableFooterGroup,
                DisplayInside::TableRow => Display::TableRow,
                DisplayInside::TableCell => Display::TableCell,
                DisplayInside::TableColumn | DisplayInside::TableColumnGroup => {
                    // Column boxes are unsupported; fall back to block.
                    Display::Block
                }
                _ => {
                    if matches!(
                        d.outside(),
                        DisplayOutside::Block | DisplayOutside::TableCaption | DisplayOutside::InternalTable
                    ) {
                        Display::Block
                    } else {
                        Display::Inline
                    }
                }
            },
        };

        // `color` is the computed `color` property, always an absolute color
        // once resolved (currentcolor etc. are resolved by the cascade).
        let [r, g, b, _a] = color.to_nscolor().to_le_bytes();
        let color = Color { r, g, b };

        let background_color = match background.clone_background_color() {
            ComputedColor::Absolute(c) if !c.is_transparent() => {
                let [r, g, b, _a] = c.to_nscolor().to_le_bytes();
                Some(Color { r, g, b })
            }
            _ => None,
        };

        let font_size = px_to_pt(font.clone_font_size().computed_size().px() as f64);
        let font_family = first_family_name(font.clone_font_family())
            .unwrap_or_else(|| "sans-serif".to_string());
        let line_height = match font.clone_line_height() {
            LineHeight::Normal => font_size * NORMAL_LINE_HEIGHT_FACTOR,
            LineHeight::Number(n) => font_size * n.0 as f64,
            LineHeight::Length(l) => px_to_pt(l.px() as f64),
        };

        let margin_top = lp_or_auto_to_pt(&margin.clone_margin_top());
        let margin_right = lp_or_auto_to_pt(&margin.clone_margin_right());
        let margin_bottom = lp_or_auto_to_pt(&margin.clone_margin_bottom());
        let margin_left = lp_or_auto_to_pt(&margin.clone_margin_left());
        let padding_top = nn_lp_to_pt(&padding.clone_padding_top());
        let padding_right = nn_lp_to_pt(&padding.clone_padding_right());
        let padding_bottom = nn_lp_to_pt(&padding.clone_padding_bottom());
        let padding_left = nn_lp_to_pt(&padding.clone_padding_left());

        ComputedStyle {
            color,
            background_color,
            // Borders default to none here; filled by
            // `apply_border_properties` from a targeted author-CSS parse
            // (see `cascade`).
            border_top: Scalar::ZERO,
            border_right: Scalar::ZERO,
            border_bottom: Scalar::ZERO,
            border_left: Scalar::ZERO,
            border_color: None,
            font_size,
            line_height,
            font_family,
            display,
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
            padding_top,
            padding_right,
            padding_bottom,
            padding_left,
            // stylo's servo build does not compile the css-break longhands
            // (`break-*`, `orphans`, `widows`, `hyphens` are
            // `engine = "gecko"`), so they are absent from `ComputedValues`.
            // They default here and are filled by `apply_break_properties`
            // from a targeted author-CSS parse (see `cascade`). This is the
            // documented deviation. `text-align` comes from stylo above.
            break_before: BreakBetween::Auto,
            break_after: BreakBetween::Auto,
            break_inside: BreakInside::Auto,
            orphans: 2,
            widows: 2,
            text_align,
            hyphens: Hyphens::Manual,
            // Paged-media element props are likewise absent from the servo
            // stylo build; filled by `apply_paged_properties` (see `cascade`).
            page: None,
            string_set: Vec::new(),
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            content: Vec::new(),
        }
    }
}


/// Convert a stylo `LengthPercentageOrAuto` (the margin/padding computed
/// values) to points. `auto` and percentages resolve to zero (the skeleton
/// has no containing-block resolution).
fn lp_or_auto_to_pt(value: &style::values::computed::Margin) -> Scalar {
    use style::values::generics::length::GenericMargin::*;
    match value {
        LengthPercentage(lp) => lp
            .to_length()
            .map(|len| px_to_pt(len.px() as f64))
            .unwrap_or(Scalar::ZERO),
        Auto | AnchorSizeFunction(_) | AnchorContainingCalcFunction(_) => Scalar::ZERO,
    }
}

/// Convert a stylo `NonNegativeLengthPercentage` (the padding computed
/// values) to points. Percentages resolve to zero (the skeleton has no
/// containing-block resolution).
fn nn_lp_to_pt(value: &style::values::computed::NonNegativeLengthPercentage) -> Scalar {
    value
        .0
        .to_length()
        .map(|len| px_to_pt(len.px() as f64))
        .unwrap_or(Scalar::ZERO)
}
/// The first family name from a computed `font-family`, or `None`.
fn first_family_name(family: FontFamily) -> Option<String> {
    let mut it = family.families.list.iter();
    match it.next() {
        Some(SingleFontFamily::FamilyName(name)) => Some(name.name.to_string()),
        _ => None,
    }
}

/// Recurse the DOM in pre-order, resolving each element with stylo.
fn walk<'a>(
    backend: &'a TyBackend<'a>,
    session: &mut CascadeSession,
    id: NodeId,
    parent_values: Option<&Arc<ComputedValues>>,
    parent_style: Option<&ComputedStyle>,
    out: &mut [ComputedStyle],
) {
    let children = backend.dom.nodes[id].children.clone();
    let is_element = matches!(backend.dom.nodes[id].kind, NodeKind::Element(_));
    let mut own_values: Option<Arc<ComputedValues>> = None;
    let mut own_style: Option<ComputedStyle> = None;

    if is_element {
        let element = TyElement::new(id, backend);
        let (values, style) = session.resolve(element, parent_values);
        out[id] = style;
        own_values = Some(values);
        own_style = Some(out[id].clone());
    } else {
        // Root and text nodes inherit the parent style verbatim.
        let style = parent_style.cloned().unwrap_or_else(ComputedStyle::initial);
        out[id] = style;
    }

    let child_parent_values = own_values.as_ref().or(parent_values);
    let child_parent_style = own_style.as_ref().or(parent_style);
    for child in children {
        walk(
            backend,
            session,
            child,
            child_parent_values,
            child_parent_style,
            out,
        );
    }
}

/// The cascade entry point. Produces a `ComputedStyle` per DOM node id
/// (indexed by `NodeId`). Text nodes inherit their parent's style.
///
/// This is the swap boundary between the engine and stylo: layout and PDF
/// read only the returned `Vec<ComputedStyle>`.
pub fn cascade(dom: &Dom, stylesheet: &Stylesheet) -> Vec<ComputedStyle> {
    let lock = SharedRwLock::new();
    let backend = TyBackend::new(dom, &lock);
    let mut session = CascadeSession::new(&lock, stylesheet.source());
    let mut styles = vec![ComputedStyle::initial(); dom.nodes.len()];
    // Pre-order walk: parents are resolved before children, and the parent's
    // `ComputedValues` is threaded down explicitly (the element-data map on
    // the backend is not needed for inheritance in this path).
    walk(
        &backend,
        &mut session,
        dom.root,
        None,
        None,
        &mut styles,
    );
    // Second pass: fill the css-break longhands stylo's servo build omits.
    breaks::apply_break_properties(dom, stylesheet.source(), &mut styles);
    // Third pass: fill paged-media properties (page / string-set / counters /
    // content).
    paged_props::apply_paged_properties(dom, stylesheet.source(), &mut styles);
    // Fourth pass: fill border widths/colors (CORE-61 tables; the engine's
    // ComputedStyle carries borders for border-collapse rendering).
    borders::apply_border_properties(dom, stylesheet.source(), &mut styles);
    styles
}

/// Author-CSS parse for the css-break longhands stylo's servo build omits.
///
/// stylo declares `break-before`/`break-after`/`break-inside`/`orphans`/
/// `widows` with `engine = "gecko"`; the servo build compiled here drops them
/// entirely, so `ComputedValues` has no accessor for them. Rather than fork
/// stylo, this module does a small, deterministic pass over the same author
/// stylesheet text: it parses only these five properties, matches simple
/// selectors (tag / `.class` / `#id`, comma lists) against the DOM in source
/// order, and writes onto the already-cascaded [`ComputedStyle`] vec. Later /
/// more-specific matches win, mirroring the cascade; the legacy `page-break-*`
/// aliases map onto the same fields.
mod breaks {
    use super::{BreakBetween, BreakInside, ComputedStyle, Hyphens};
    use crate::dom::{Dom, NodeId, NodeKind};

    /// A parsed simple selector: an optional tag plus required classes/id.
    pub(super) struct SimpleSelector {
        pub(super) tag: Option<String>,
        pub(super) id: Option<String>,
        pub(super) classes: Vec<String>,
        /// Higher wins ties; approximates specificity (id=100, class=10,
        /// tag=1) then declaration order.
        pub(super) specificity: u32,
    }

    impl SimpleSelector {
        pub(super) fn matches(&self, dom: &Dom, id: NodeId) -> bool {
            let NodeKind::Element(el) = &dom.nodes[id].kind else {
                return false;
            };
            if let Some(tag) = &self.tag {
                if !tag.eq_ignore_ascii_case(&el.tag) {
                    return false;
                }
            }
            if let Some(want) = &self.id {
                if el.id.as_deref() != Some(want.as_str()) {
                    return false;
                }
            }
            self.classes.iter().all(|c| el.classes.iter().any(|x| x == c))
        }
    }

    /// Parse one compound simple selector like `section.note#x`. Returns `None`
    /// for anything with a combinator or pseudo we do not support.
    pub(super) fn parse_simple(sel: &str) -> Option<SimpleSelector> {
        let sel = sel.trim();
        if sel.is_empty() || sel.contains([' ', '>', '+', '~', ':', '[', '*']) {
            return None;
        }
        let mut tag = None;
        let mut id = None;
        let mut classes = Vec::new();
        let mut spec = 0u32;
        let mut chars = sel.chars().peekable();
        // Optional leading type selector.
        let mut lead = String::new();
        while let Some(&c) = chars.peek() {
            if c == '.' || c == '#' {
                break;
            }
            lead.push(c);
            chars.next();
        }
        if !lead.is_empty() {
            tag = Some(lead);
            spec += 1;
        }
        while let Some(c) = chars.next() {
            let mut ident = String::new();
            while let Some(&nc) = chars.peek() {
                if nc == '.' || nc == '#' {
                    break;
                }
                ident.push(nc);
                chars.next();
            }
            if ident.is_empty() {
                return None;
            }
            match c {
                '.' => {
                    classes.push(ident);
                    spec += 10;
                }
                '#' => {
                    id = Some(ident);
                    spec += 100;
                }
                _ => return None,
            }
        }
        Some(SimpleSelector {
            tag,
            id,
            classes,
            specificity: spec,
        })
    }

    /// One break declaration, keyed to a field.
    #[derive(Clone, Copy)]
    enum BreakDecl {
        Before(BreakBetween),
        After(BreakBetween),
        Inside(BreakInside),
        Orphans(u32),
        Widows(u32),
        Hyphens(Hyphens),
    }

    fn parse_between(v: &str) -> Option<BreakBetween> {
        match v.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(BreakBetween::Auto),
            // `always` is the legacy page-break value = page break.
            "page" | "always" => Some(BreakBetween::Page),
            "left" => Some(BreakBetween::Left),
            "right" => Some(BreakBetween::Right),
            // `avoid` on a between-property is not a page break; treat as auto
            // (this engine models between-avoidance only via appeal, not here).
            "avoid" => Some(BreakBetween::Auto),
            _ => None,
        }
    }

    fn parse_inside(v: &str) -> Option<BreakInside> {
        match v.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(BreakInside::Auto),
            "avoid" | "avoid-page" => Some(BreakInside::Avoid),
            _ => None,
        }
    }

    /// Parse a declaration `prop: value` into zero or more [`BreakDecl`]s.
    fn parse_decl(prop: &str, value: &str) -> Option<BreakDecl> {
        let prop = prop.trim().to_ascii_lowercase();
        match prop.as_str() {
            "break-before" | "page-break-before" => parse_between(value).map(BreakDecl::Before),
            "break-after" | "page-break-after" => parse_between(value).map(BreakDecl::After),
            "break-inside" | "page-break-inside" => parse_inside(value).map(BreakDecl::Inside),
            "orphans" => value.trim().parse::<u32>().ok().map(BreakDecl::Orphans),
            "widows" => value.trim().parse::<u32>().ok().map(BreakDecl::Widows),
            "hyphens" => parse_hyphens(value).map(BreakDecl::Hyphens),
            _ => None,
        }
    }

    fn parse_hyphens(v: &str) -> Option<Hyphens> {
        match v.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Hyphens::None),
            "manual" => Some(Hyphens::Manual),
            "auto" => Some(Hyphens::Auto),
            _ => None,
        }
    }

    /// A matched (selector, declarations) rule with source order preserved.
    struct Rule {
        selectors: Vec<SimpleSelector>,
        decls: Vec<BreakDecl>,
        order: u32,
    }

    /// Strip `/* ... */` comments so they never leak into selectors/values.
    pub(super) fn strip_comments(css: &str) -> String {
        let mut out = String::with_capacity(css.len());
        let bytes = css.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }

    /// Parse only the break rules out of a stylesheet. Blocks whose selector or
    /// declarations we do not understand are skipped without failing.
    ///
    /// Balanced-brace matching (not naive `find('}')`): a stylesheet that
    /// contains `@page { ... @top-center { content: ... } }` has nested braces,
    /// and a naive first-`}` scan would consume only the inner one, leaving a
    /// stray ` }` that mangles the following rule's selector. This mirrors the
    /// `paged_props` scanner.
    fn parse_rules(css: &str) -> Vec<Rule> {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut order = 0u32;
        let bytes = css.as_bytes();
        let mut i = 0;
        while let Some(brace_rel) = css[i..].find('{') {
            let brace = i + brace_rel;
            let prelude = css[i..brace].trim();
            let Some(end) = matching_brace(bytes, brace) else {
                break;
            };

            // Skip at-rules (e.g. @page, @media) whole — balanced braces
            // include margin-box blocks.
            if prelude.starts_with('@') {
                order += 1;
                i = end + 1;
                continue;
            }

            let body = &css[brace + 1..end];
            let mut decls = Vec::new();
            for decl in body.split(';') {
                let Some((prop, value)) = decl.split_once(':') else {
                    continue;
                };
                if let Some(d) = parse_decl(prop, value) {
                    decls.push(d);
                }
            }
            if !decls.is_empty() {
                let selectors: Vec<SimpleSelector> =
                    prelude.split(',').filter_map(parse_simple).collect();
                if !selectors.is_empty() {
                    rules.push(Rule {
                        selectors,
                        decls,
                        order,
                    });
                }
            }
            order += 1;
            i = end + 1;
        }
        rules
    }

    /// Find the index of the brace matching `open` (which must be `{`),
    /// counting nesting depth.
    fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
        let mut depth = 0;
        let mut i = open;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Apply parsed break rules onto the cascaded styles, then inherit the
    /// inherited break properties (`orphans`, `widows`) down the tree.
    pub fn apply_break_properties(dom: &Dom, css: &str, styles: &mut [ComputedStyle]) {
        let rules = parse_rules(css);

        // Track the winning (specificity, order) per (node, field) so a
        // higher-priority declaration is not overwritten by a weaker one.
        #[derive(Clone, Copy, Default)]
        struct Won {
            before: Option<(u32, u32)>,
            after: Option<(u32, u32)>,
            inside: Option<(u32, u32)>,
            orphans: Option<(u32, u32)>,
            widows: Option<(u32, u32)>,
            hyphens: Option<(u32, u32)>,
        }
        let mut won = vec![Won::default(); styles.len()];
        // Which nodes explicitly set orphans/widows/hyphens (so inheritance
        // skips them).
        let mut set_orphans = vec![false; styles.len()];
        let mut set_widows = vec![false; styles.len()];
        let mut set_hyphens = vec![false; styles.len()];

        for rule in &rules {
            for sel in &rule.selectors {
                let prio = (sel.specificity, rule.order);
                for id in 0..dom.nodes.len() {
                    if !sel.matches(dom, id) {
                        continue;
                    }
                    for decl in &rule.decls {
                        match *decl {
                            BreakDecl::Before(v) => {
                                if won[id].before.is_none_or(|w| prio >= w) {
                                    styles[id].break_before = v;
                                    won[id].before = Some(prio);
                                }
                            }
                            BreakDecl::After(v) => {
                                if won[id].after.is_none_or(|w| prio >= w) {
                                    styles[id].break_after = v;
                                    won[id].after = Some(prio);
                                }
                            }
                            BreakDecl::Inside(v) => {
                                if won[id].inside.is_none_or(|w| prio >= w) {
                                    styles[id].break_inside = v;
                                    won[id].inside = Some(prio);
                                }
                            }
                            BreakDecl::Orphans(v) => {
                                if won[id].orphans.is_none_or(|w| prio >= w) {
                                    styles[id].orphans = v;
                                    won[id].orphans = Some(prio);
                                    set_orphans[id] = true;
                                }
                            }
                            BreakDecl::Widows(v) => {
                                if won[id].widows.is_none_or(|w| prio >= w) {
                                    styles[id].widows = v;
                                    won[id].widows = Some(prio);
                                    set_widows[id] = true;
                                }
                            }
                            BreakDecl::Hyphens(v) => {
                                if won[id].hyphens.is_none_or(|w| prio >= w) {
                                    styles[id].hyphens = v;
                                    won[id].hyphens = Some(prio);
                                    set_hyphens[id] = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // orphans/widows/hyphens are inherited: propagate from parent in
        // pre-order for any node that did not set them explicitly.
        inherit(dom, dom.root, styles, &set_orphans, &set_widows, &set_hyphens);
    }

    fn inherit(
        dom: &Dom,
        id: NodeId,
        styles: &mut [ComputedStyle],
        set_orphans: &[bool],
        set_widows: &[bool],
        set_hyphens: &[bool],
    ) {
        if let Some(parent) = dom.nodes[id].parent {
            if !set_orphans[id] {
                styles[id].orphans = styles[parent].orphans;
            }
            if !set_widows[id] {
                styles[id].widows = styles[parent].widows;
            }
            if !set_hyphens[id] {
                styles[id].hyphens = styles[parent].hyphens;
            }
        }
        for &child in &dom.nodes[id].children {
            inherit(
                dom,
                child,
                styles,
                set_orphans,
                set_widows,
                set_hyphens,
            );
        }
    }
}

/// Author-CSS parse for the `border` shorthand/longhands (CORE-61 tables).
///
/// stylo's servo build DOES compile border widths/colors, but the engine's
/// `ComputedStyle` only carries margins/padding — borders were added for
/// tables (border-collapse). This module reuses the `breaks` selector
/// machinery and does a small, deterministic pass over the author stylesheet
/// text: it parses `border` (1–4 widths, style token, color), the
/// `border-top/right/bottom/left` longhands, and `border-color`, matching
/// simple selectors in source order (later/more-specific wins, mirroring the
/// cascade). Style keywords (`solid`, `dashed`, ...) are accepted and ignored
/// (width+color are what render); `none`/`0` clears.
mod borders {
    use super::breaks::{parse_simple, strip_comments, SimpleSelector};
    use super::{Color, ComputedStyle};
    use crate::dom::Dom;
    use crate::geom::Scalar;
    use crate::paged::parse_length;

    /// A parsed border declaration: optional per-side widths and a color.
    #[derive(Clone, Debug, PartialEq)]
    enum BorderDecl {
        /// `border: <width> <style> <color>` — all four sides.
        Shorthand {
            width: Option<Scalar>,
            color: Option<Color>,
        },
        /// `border-top` (etc.) longhand.
        Side {
            side: Side,
            width: Option<Scalar>,
            color: Option<Color>,
        },
        /// `border-color: <color>` — all four sides.
        Color(Color),
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Side {
        Top,
        Right,
        Bottom,
        Left,
    }

    struct Rule {
        selectors: Vec<SimpleSelector>,
        decls: Vec<BorderDecl>,
        order: u32,
    }

    /// Parse a CSS color: `#rgb`/`#rrggbb` or a small named set. `None` for
    /// `transparent` and anything unrecognized (caller keeps prior value).
    pub(super) fn parse_color(s: &str) -> Option<Color> {
        let s = s.trim().to_ascii_lowercase();
        if s == "transparent" {
            return None;
        }
        if let Some(hex) = s.strip_prefix('#') {
            let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            let (r, g, b) = match hex.len() {
                3 => {
                    let cv = |c: char| u8::from_str_radix(&c.to_string().repeat(2), 16).ok();
                    (cv(hex.chars().nth(0)?)?, cv(hex.chars().nth(1)?)?, cv(hex.chars().nth(2)?)?)
                }
                6 => {
                    let cv = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                    (cv(0)?, cv(2)?, cv(4)?)
                }
                _ => return None,
            };
            return Some(Color { r, g, b });
        }
        let named = match s.as_str() {
            "black" => (0, 0, 0),
            "white" => (255, 255, 255),
            "red" => (255, 0, 0),
            "green" => (0, 128, 0),
            "blue" => (0, 0, 255),
            "gray" | "grey" => (128, 128, 128),
            "silver" => (192, 192, 192),
            "maroon" => (128, 0, 0),
            "olive" => (128, 128, 0),
            "lime" => (0, 255, 0),
            "teal" => (0, 128, 128),
            "navy" => (0, 0, 128),
            "purple" => (128, 0, 128),
            "orange" => (255, 165, 0),
            _ => return None,
        };
        Some(Color { r: named.0, g: named.1, b: named.2 })
    }

    /// Parse one declaration's value into a width (if present) + color.
    fn parse_border_value(value: &str) -> (Option<Scalar>, Option<Color>) {
        let mut width = None;
        let mut color = None;
        for tok in value.split_whitespace() {
            if tok.eq_ignore_ascii_case("none") || tok == "0" {
                width = Some(Scalar::ZERO);
                continue;
            }
            if matches!(
                tok.to_ascii_lowercase().as_str(),
                "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset"
                    | "hidden"
            ) {
                continue;
            }
            if let Some(s) = parse_length(tok) {
                width = Some(s);
                continue;
            }
            if let Some(c) = parse_color(tok) {
                color = Some(c);
            }
        }
        (width, color)
    }

    fn parse_decl(prop: &str, value: &str) -> Option<BorderDecl> {
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        let (w, c) = parse_border_value(value);
        match prop.as_str() {
            "border" | "border-width" => Some(BorderDecl::Shorthand { width: w, color: c }),
            "border-color" => {
                if let Some(c) = c {
                    Some(BorderDecl::Color(c))
                } else {
                    None
                }
            }
            "border-top" | "border-top-width" => Some(BorderDecl::Side {
                side: Side::Top,
                width: w,
                color: c,
            }),
            "border-right" | "border-right-width" => Some(BorderDecl::Side {
                side: Side::Right,
                width: w,
                color: c,
            }),
            "border-bottom" | "border-bottom-width" => Some(BorderDecl::Side {
                side: Side::Bottom,
                width: w,
                color: c,
            }),
            "border-left" | "border-left-width" => Some(BorderDecl::Side {
                side: Side::Left,
                width: w,
                color: c,
            }),
            _ => None,
        }
    }

    fn parse_rules(css: &str) -> Vec<Rule> {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut order = 0u32;
        let bytes = css.as_bytes();
        let mut i = 0;
        while let Some(rel) = css[i..].find('{') {
            let brace = i + rel;
            let prelude = css[i..brace].trim();
            let Some(end) = matching_brace(bytes, brace) else {
                break;
            };
            if prelude.starts_with('@') {
                order += 1;
                i = end + 1;
                continue;
            }
            let body = &css[brace + 1..end];
            let mut decls = Vec::new();
            for decl in body.split(';') {
                if let Some((prop, value)) = decl.split_once(':') {
                    if let Some(d) = parse_decl(prop, value) {
                        decls.push(d);
                    }
                }
            }
            if !decls.is_empty() {
                let selectors: Vec<SimpleSelector> = prelude
                    .split(',')
                    .filter_map(|s| parse_simple(s.trim()))
                    .collect();
                if !selectors.is_empty() {
                    rules.push(Rule { selectors, decls, order });
                }
            }
            order += 1;
            i = end + 1;
        }
        rules
    }

    /// Balanced-brace scan (mirrors `breaks`/`paged_props`).
    fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (off, &b) in bytes.iter().enumerate().skip(open) {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(off);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn apply_border_properties(dom: &Dom, css: &str, styles: &mut [ComputedStyle]) {
        let rules = parse_rules(css);
        #[derive(Clone, Copy, Default)]
        struct Won {
            top: Option<(u32, u32)>,
            right: Option<(u32, u32)>,
            bottom: Option<(u32, u32)>,
            left: Option<(u32, u32)>,
            color: Option<(u32, u32)>,
        }
        let mut won = vec![Won::default(); styles.len()];

        for rule in &rules {
            for sel in &rule.selectors {
                let prio = (sel.specificity, rule.order);
                for id in 0..dom.nodes.len() {
                    if !sel.matches(dom, id) {
                        continue;
                    }
                    for decl in &rule.decls {
                        match decl {
                            BorderDecl::Shorthand { width, color } => {
                                let st = &mut styles[id];
                                if let Some(w) = width {
                                    if won[id].top.is_none_or(|p| prio >= p) {
                                        st.border_top = *w;
                                        won[id].top = Some(prio);
                                    }
                                    if won[id].right.is_none_or(|p| prio >= p) {
                                        st.border_right = *w;
                                        won[id].right = Some(prio);
                                    }
                                    if won[id].bottom.is_none_or(|p| prio >= p) {
                                        st.border_bottom = *w;
                                        won[id].bottom = Some(prio);
                                    }
                                    if won[id].left.is_none_or(|p| prio >= p) {
                                        st.border_left = *w;
                                        won[id].left = Some(prio);
                                    }
                                }
                                if let Some(c) = color {
                                    if won[id].color.is_none_or(|p| prio >= p) {
                                        st.border_color = Some(*c);
                                        won[id].color = Some(prio);
                                    }
                                }
                            }
                            BorderDecl::Side { side, width, color } => {
                                let st = &mut styles[id];
                                let (slot, flag) = match side {
                                    Side::Top => (&mut st.border_top, &mut won[id].top),
                                    Side::Right => (&mut st.border_right, &mut won[id].right),
                                    Side::Bottom => (&mut st.border_bottom, &mut won[id].bottom),
                                    Side::Left => (&mut st.border_left, &mut won[id].left),
                                };
                                if let Some(w) = width {
                                    if flag.is_none_or(|p| prio >= p) {
                                        *slot = *w;
                                        *flag = Some(prio);
                                    }
                                }
                                if let Some(c) = color {
                                    if won[id].color.is_none_or(|p| prio >= p) {
                                        st.border_color = Some(*c);
                                        won[id].color = Some(prio);
                                    }
                                }
                            }
                            BorderDecl::Color(c) => {
                                if won[id].color.is_none_or(|p| prio >= p) {
                                    styles[id].border_color = Some(*c);
                                    won[id].color = Some(prio);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Author-CSS parse for the paged-media *element* properties stylo's servo
/// build omits: `page`, `string-set`, `counter-reset`, `counter-increment`,
/// and `content`. Mirrors [`breaks`]: a small deterministic pass over the same
/// stylesheet text, reusing that module's selector matcher. `@page` at-rules
/// are parsed separately (see [`crate::paged`]); this pass skips them.
mod paged_props {
    use super::breaks::{parse_simple, strip_comments, SimpleSelector};
    use super::{ComputedStyle, StringSetValue};
    use crate::dom::{Dom, NodeKind};
    use crate::paged::{parse_content, ContentPiece};

    /// One paged-media declaration keyed to a field.
    enum PagedDecl {
        Page(Option<String>),
        StringSet(Vec<(String, StringSetValue)>),
        CounterReset(Vec<(String, i32)>),
        CounterIncrement(Vec<(String, i32)>),
        Content(Vec<ContentPiece>),
    }

    /// Parse `string-set: name content();` (comma-separated pairs).
    fn parse_string_set(value: &str) -> Vec<(String, StringSetValue)> {
        let mut out = Vec::new();
        for pair in value.split(',') {
            let mut it = pair.split_whitespace();
            let Some(name) = it.next() else { continue };
            // The remainder is the value expression; only content() is modeled.
            let rest: String = it.collect::<Vec<_>>().join(" ");
            let lower = rest.to_ascii_lowercase();
            if lower.starts_with("content") || lower.starts_with("attr") {
                out.push((name.to_string(), StringSetValue::Content));
            }
        }
        out
    }

    /// Parse `counter-reset` / `counter-increment`: `name [int]` groups, with a
    /// default of `0` (reset) or `1` (increment) when the int is omitted.
    fn parse_counters(value: &str, default: i32) -> Vec<(String, i32)> {
        let mut out = Vec::new();
        let toks: Vec<&str> = value.split_whitespace().collect();
        let mut i = 0;
        while i < toks.len() {
            let name = toks[i].to_string();
            i += 1;
            let n = if i < toks.len() {
                match toks[i].parse::<i32>() {
                    Ok(v) => {
                        i += 1;
                        v
                    }
                    Err(_) => default,
                }
            } else {
                default
            };
            out.push((name, n));
        }
        out
    }

    fn parse_decl(prop: &str, value: &str) -> Option<PagedDecl> {
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        match prop.as_str() {
            "page" => {
                let v = value.trim();
                if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                    Some(PagedDecl::Page(None))
                } else {
                    Some(PagedDecl::Page(Some(v.to_string())))
                }
            }
            "string-set" => Some(PagedDecl::StringSet(parse_string_set(value))),
            "counter-reset" => Some(PagedDecl::CounterReset(parse_counters(value, 0))),
            "counter-increment" => Some(PagedDecl::CounterIncrement(parse_counters(value, 1))),
            "content" => Some(PagedDecl::Content(parse_content(value))),
            _ => None,
        }
    }

    struct Rule {
        selectors: Vec<SimpleSelector>,
        decls: Vec<PagedDecl>,
        order: u32,
    }

    /// Parse only the paged-media element rules; `@page` and other at-rules are
    /// skipped (their braces are balanced-matched so nested margin boxes do not
    /// confuse the scanner).
    fn parse_rules(css: &str) -> Vec<Rule> {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut order = 0u32;
        let bytes = css.as_bytes();
        let mut i = 0;
        while let Some(brace_rel) = css[i..].find('{') {
            let brace = i + brace_rel;
            let prelude = css[i..brace].trim();
            let Some(end) = matching_brace(bytes, brace) else {
                break;
            };

            if prelude.starts_with('@') {
                // Skip at-rules whole (balanced braces include margin boxes).
                order += 1;
                i = end + 1;
                continue;
            }

            let body = &css[brace + 1..end];
            let mut decls = Vec::new();
            for decl in body.split(';') {
                if let Some((prop, value)) = decl.split_once(':') {
                    if let Some(d) = parse_decl(prop, value) {
                        decls.push(d);
                    }
                }
            }
            if !decls.is_empty() {
                let selectors: Vec<SimpleSelector> =
                    prelude.split(',').filter_map(parse_simple).collect();
                if !selectors.is_empty() {
                    rules.push(Rule {
                        selectors,
                        decls,
                        order,
                    });
                }
            }
            order += 1;
            i = end + 1;
        }
        rules
    }

    fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
        let mut depth = 0;
        let mut i = open;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Apply paged-media element rules onto the cascaded styles. Later /
    /// more-specific matches win (same priority model as [`breaks`]).
    pub fn apply_paged_properties(dom: &Dom, css: &str, styles: &mut [ComputedStyle]) {
        let rules = parse_rules(css);

        #[derive(Clone, Copy, Default)]
        struct Won {
            page: Option<(u32, u32)>,
            string_set: Option<(u32, u32)>,
            counter_reset: Option<(u32, u32)>,
            counter_increment: Option<(u32, u32)>,
            content: Option<(u32, u32)>,
        }
        let mut won = vec![Won::default(); styles.len()];

        for rule in &rules {
            for sel in &rule.selectors {
                let prio = (sel.specificity, rule.order);
                for id in 0..dom.nodes.len() {
                    if !matches!(dom.nodes[id].kind, NodeKind::Element(_)) {
                        continue;
                    }
                    if !sel.matches(dom, id) {
                        continue;
                    }
                    for decl in &rule.decls {
                        match decl {
                            PagedDecl::Page(v) => {
                                if won[id].page.is_none_or(|w| prio >= w) {
                                    styles[id].page = v.clone();
                                    won[id].page = Some(prio);
                                }
                            }
                            PagedDecl::StringSet(v) => {
                                if won[id].string_set.is_none_or(|w| prio >= w) {
                                    styles[id].string_set = v.clone();
                                    won[id].string_set = Some(prio);
                                }
                            }
                            PagedDecl::CounterReset(v) => {
                                if won[id].counter_reset.is_none_or(|w| prio >= w) {
                                    styles[id].counter_reset = v.clone();
                                    won[id].counter_reset = Some(prio);
                                }
                            }
                            PagedDecl::CounterIncrement(v) => {
                                if won[id].counter_increment.is_none_or(|w| prio >= w) {
                                    styles[id].counter_increment = v.clone();
                                    won[id].counter_increment = Some(prio);
                                }
                            }
                            PagedDecl::Content(v) => {
                                if won[id].content.is_none_or(|w| prio >= w) {
                                    styles[id].content = v.clone();
                                    won[id].content = Some(prio);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
