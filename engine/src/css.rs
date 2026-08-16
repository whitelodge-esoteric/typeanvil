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
use style::values::computed::font::{FontFamily, SingleFontFamily};
use style::values::computed::Color as ComputedColor;
use style::values::computed::Length;
use style::values::specified::box_::DisplayOutside;
use style::values::specified::font::FONT_MEDIUM_PX;
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Display {
    Block,
    Inline,
    None,
}

/// Fully computed style for one element. This is the cascade's output contract;
/// layout and PDF read only this.
#[derive(Clone, Debug)]
pub struct ComputedStyle {
    pub color: Color,
    pub background_color: Option<Color>,
    pub font_size: Scalar,
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
}

impl ComputedStyle {
    /// The initial / root style (the base of inheritance).
    fn initial() -> ComputedStyle {
        ComputedStyle {
            color: Color::BLACK,
            background_color: None,
            font_size: px_to_pt(16.0),
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
        blockquote, pre, table, thead, tbody, tfoot, tr, section, article,
        header, footer, nav, main, aside, figure, figcaption { display: block; }
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

        let display = match box_.clone_display() {
            d if d.is_none() => Display::None,
            d if d.outside() == DisplayOutside::Block => Display::Block,
            _ => Display::Inline,
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
            font_size,
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
    styles
}
