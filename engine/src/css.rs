// SPDX-License-Identifier: AGPL-3.0-only

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
use style_traits::{CSSPixel, DevicePixel};
use style::properties::generated::longhands::column_span::computed_value::T as StyloColumnSpan;
use style::properties::generated::longhands::flex_direction::computed_value::T as StyloFlexDirection;
use style::properties::generated::longhands::flex_wrap::computed_value::T as StyloFlexWrap;
use style::values::computed::align::{
    ContentDistribution as StyloContentDistribution, ItemPlacement as StyloItemPlacement,
    SelfAlignment as StyloSelfAlignment,
};
use style::values::computed::box_::Float as StyloFloat;
pub use style::properties::longhands::box_sizing::computed_value::T as StyloBoxSizing;
use style::values::computed::column::ColumnCount as StyloColumnCount;
use style::values::computed::flex::FlexBasis as StyloFlexBasis;
use style::values::computed::font::{FontFamily, LineHeight, SingleFontFamily};
use style::values::computed::font::FontStyle as StyloFontStyle;
use style::values::computed::length::{
    NonNegativeLengthOrAuto as StyloColumnWidth, NonNegativeLengthPercentageOrNormal as StyloColumnGap,
};
use style::values::computed::position::{Inset as StyloInset, ZIndex as StyloZIndex};
use style::values::computed::Color as ComputedColor;
use style::values::computed::{Length, PositionProperty, Size as StyloSize};
use style::values::specified::align::AlignFlags;
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style::values::specified::font::FONT_MEDIUM_PX;
use style::values::specified::text::TextAlignKeyword;
use url::Url;

use crate::dom::{Dom, NodeId, NodeKind};
use crate::geom::{px_to_pt, Scalar};
use crate::stylo_dom::{TyBackend, TyElement};
use style::dom::TElement as _;

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
    /// Block-level flex container (css-flexbox-1 §2).
    Flex,
    /// Inline-level flex container; treated as a block-level flex container
    /// in paged flow (spec Goal 1 — true inline-fragment behavior deferred).
    InlineFlex,
    /// Inline-level block container (`display: inline-block`,
    /// css-display-3 §2.3): atomic on the outside, flow inside.
    InlineBlock,
}
/// The computed `column-span` value (css-multicol-1 §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColumnSpan {
    #[default]
    None,
    All,
}

/// The computed `float` value (css-box-3 §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
}

/// The computed `position` value (css-position-3 §3). `Sticky` maps to
/// `Relative` until sticky semantics land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

/// The computed `flex-direction` value (css-flexbox-1 §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// The computed `flex-wrap` value (css-flexbox-1 §3). Layout honors `Nowrap`
/// and `Wrap`; `WrapReverse` folds to `Wrap` (reverse-wrap lines deferred).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default]
    Nowrap,
    Wrap,
    WrapReverse,
}

/// The computed `flex-basis` value (css-flexbox-1 §7.2.3). Stylo computes
/// `auto` to `Size(Auto)` and `0%` to `Size(0%)`; we keep the resolved shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlexBasis {
    /// `auto` — the item's `width` (row) / content main size (column).
    Auto,
    /// `content` — the item's max-content main size.
    Content,
    /// `<width>`: a definite length and/or a percentage of the container's
    /// inner main size (either may be present; a percentage without a
    /// length resolves against the container at layout time).
    Size {
        length: Option<Scalar>,
        percent: Option<f64>,
    },
}

/// The computed `align-items` value (css-align-3). Default `stretch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
}

/// The computed `align-self` value (css-align-3). `Auto` resolves against
/// the parent container's `align-items` at layout time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlignSelf {
    #[default]
    Auto,
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
}

/// The computed `justify-content` value (css-align-3). Only the four basic
/// distribution values are modeled (spec Non-Goals: `space-around`,
/// `space-evenly`, baseline and overflow-safe variants deferred).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
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

/// The computed `font-style` value (css-fonts-4 §3). Oblique folds into italic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// The computed `bookmark-level` value (css-gcpm-3 §Bookmarks, CORE-128).
/// `None` suppresses the outline entry; `Level(1..=6)` sets the entry's
/// nesting depth (UA default maps `h1`–`h6` to levels 1–6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookmarkLevel {
    /// No outline entry for this box.
    None,
    /// Outline level 1–6.
    Level(u8),
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
    /// The computed numeric `font-weight` (400 = normal, 700 = bold).
    pub font_weight: f32,
    /// The computed `font-style` (oblique folded into italic).
    pub font_style: FontStyle,
    /// The resolved primary face for this element's `font-family` stack
    /// (CORE-103). Bundled fallback when nothing resolves.
    pub font_face: crate::fonts::FaceId,
    /// Later stack members for per-character glyph fallback (spec
    /// Behavior 8). Empty when the stack had one resolvable family.
    pub font_fallbacks: Vec<crate::fonts::FaceId>,
    /// The first computed family NAME (kept for diagnostics/tests; layout
    /// and shaping read `font_face`).
    pub font_family: String,
    pub display: Display,
    /// The computed `float` value (css-box-3 §2). `Left`/`Right` take the
    /// element out of flow; layout places it at the content edge and wraps
    /// in-flow text around it.
    pub float: Float,
    /// The computed `width` property, `None` = `auto` (shrink-to-fit for
    /// floats and abspos). Points.
    pub width: Option<Scalar>,
    /// The raw `width` percentage as a 0..=1 fraction, kept when the declared
    /// width is a percentage so table layout can resolve it against the
    /// containing block (CORE-81). `None` = no percentage declared.
    pub width_percent: Option<f64>,
    /// The computed `height` property (points), `None` = `auto`. The block
    /// path deliberately ignores height (auto-height self-consistency,
    /// CORE-66), but flex item sizing (cross axis for rows, main axis for
    /// columns) resolves it — the flex tests pin explicit item heights.
    pub height: Option<Scalar>,
    /// The raw `height` percentage as a 0..=1 fraction, kept for flex item
    /// sizing against the flex line's cross size / container height.
    pub height_percent: Option<f64>,
    /// The computed `box-sizing` (css-ui-3). `border-box` makes `width`/
    /// `height` include padding + border; the inline-block placement uses it
    /// to convert the declared width into a content width (CORE-120).
    pub box_sizing: StyloBoxSizing,
    /// The computed `position` value. `Absolute`/`Fixed` take the element out
    /// of flow; the fragment attaches to the fragmentainer.
    pub position: Position,
    /// Non-`auto` insets (css-position-3 §9), resolved to pt. Points.
    pub inset_top: Option<Scalar>,
    pub inset_right: Option<Scalar>,
    pub inset_bottom: Option<Scalar>,
    pub inset_left: Option<Scalar>,
    /// `z-index`, `None` = `auto` (paint in tree order).
    pub z_index: Option<i32>,
    /// `column-count`, `None` = `auto` (css-multicol-1).
    pub column_count: Option<u32>,
    /// `column-width`, `None` = `auto`. Points.
    pub column_width: Option<Scalar>,
    /// `column-span` (spanners interrupt the column flow).
    pub column_span: ColumnSpan,
    /// `column-gap`; `normal`/`auto` resolve to 1em. Points.
    pub column_gap: Scalar,
    /// Flex container properties (css-flexbox-1). Computed by stylo (all
    /// flex longhands compile in the servo build — verified 2026-08-18);
    /// layout reads these only for `display: flex | inline-flex` boxes.
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f64,
    pub flex_shrink: f64,
    pub flex_basis: FlexBasis,
    pub align_items: AlignItems,
    pub align_self: AlignSelf,
    pub justify_content: JustifyContent,
    pub order: i32,
    /// `row-gap` (block-axis gap between flex lines / column items).
    /// `normal` resolves to 0 (css-align-3 §8). Points.
    pub row_gap: Scalar,
    /// `column-gap` for FLEX row containers (inline-axis gap between items).
    /// Distinct from [`ComputedStyle::column_gap`], which carries multicol
    /// semantics (`normal` → 1em). `normal` → 0 for flex (css-align-3).
    /// Points.
    pub flex_column_gap: Scalar,
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
    /// Default 1 (not CSS's initial 2) for Prince parity — CORE-97 probed
    /// Prince 16.2 allowing a 1-line widow (float-showcase splits 8+1,
    /// widows probe [10,10,10,10,1]); the 2 default pulled page breaks back
    /// and cascaded +2 pages on float-showcase. Explicit author CSS still
    /// overrides.
    pub widows: u32,
    /// The `page` property: the named page this box switches to (paged-media).
    pub page: Option<String>,
    /// True when an explicit `writing-mode` declaration applied to this
    /// element (CORE-127: orthogonal-flow suppression for page-change
    /// breaks — the engine renders every flow horizontally in v1).
    pub writing_mode_declared: bool,
    /// True when `float: footnote` matched this element (CORE-107). The
    /// element produces no in-flow box; it renders in its call page's
    /// footnote area with a superscript call marker in the body text.
    pub float_footnote: bool,
    /// `string-set` declarations: `(string name, value)` pairs.
    pub string_set: Vec<(String, StringSetValue)>,
    /// `counter-reset` declarations: `(counter name, value)` pairs.
    pub counter_reset: Vec<(String, i32)>,
    /// `counter-increment` declarations: `(counter name, delta)` pairs.
    pub counter_increment: Vec<(String, i32)>,
    /// `bookmark-level` (css-gcpm-3, CORE-128). `None` suppresses the
    /// outline entry; `Level(1..=6)` sets the nesting depth.
    pub bookmark_level: BookmarkLevel,
    /// `bookmark-label` as an ordered content-piece list (parsed by the
    /// same parser as `content`). Empty = `contents()` (element text).
    pub bookmark_label: Vec<crate::paged::ContentPiece>,
    /// Whether a `bookmark-state: closed` declaration won the cascade
    /// (default `false` = open, the css-gcpm initial).
    pub bookmark_closed: bool,
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
    /// Explicit OpenType feature settings (CORE-113): packed big-endian tag +
    /// value pairs copied verbatim from stylo's computed
    /// `font-feature-settings`. Empty for the default (`normal`).
    pub feature_settings: Vec<(u32, i32)>,
    /// Resolved OpenType features to apply at shaping time (CORE-113):
    /// `(packed_tag, u32_value)` in application order. Combines the
    /// `font-variant-*` longhand mappings with `feature_settings`
    /// (which come last and win duplicates). Resolved once per style so
    /// every shaping call sees an identical list; empty = engine defaults.
    pub ot_features: Vec<(u32, u32)>,
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
            font_weight: 400.0,
            font_style: FontStyle::Normal,
            font_face: crate::fonts::FACE_REGULAR,
            font_fallbacks: Vec::new(),
            font_family: "sans-serif".to_string(),
            display: Display::Inline,
            float: Float::None,
            width: None,
            width_percent: None,
            height: None,
            height_percent: None,
            box_sizing: StyloBoxSizing::ContentBox,
            position: Position::Static,
            inset_top: None,
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
            z_index: None,
            column_count: None,
            column_width: None,
            column_span: ColumnSpan::None,
            column_gap: px_to_pt(16.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Nowrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: FlexBasis::Auto,
            align_items: AlignItems::Stretch,
            align_self: AlignSelf::Auto,
            justify_content: JustifyContent::FlexStart,
            order: 0,
            row_gap: Scalar::ZERO,
            flex_column_gap: Scalar::ZERO,
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
            widows: 1,
            text_align: TextAlign::Start,
            hyphens: Hyphens::Manual,
            feature_settings: Vec::new(),
            ot_features: Vec::new(),
            page: None,
            writing_mode_declared: false,
            string_set: Vec::new(),
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            // Initial = "not bookmarked" (suppressed). The UA sheet's heading
            // rules set the real defaults through `apply_paged_properties`;
            // non-heading elements without an author declaration stay None.
            bookmark_level: BookmarkLevel::None,
            bookmark_label: Vec::new(),
            bookmark_closed: false,
            content: Vec::new(),
            float_footnote: false,
        }
    }
}

// --- OpenType feature resolution (CORE-113) --------------------------------

/// Pack four ASCII tag bytes big-endian (matches stylo's `FontTag` and
/// HarfRust's `Tag([u8; 4])` layout).
const fn tag4(b: &[u8; 4]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | b[3] as u32
}

/// Resolve the computed font-variant longhands + explicit
/// `font-feature-settings` into one ordered shaping feature list
/// `(packed_tag, value)` (spec Behavior 2/5):
/// ligatures → caps → numeric → east-asian, then explicit settings last
/// (css-fonts-4 §10.3: low-level settings win). Duplicates collapse
/// last-wins by tag; output order is deterministic.
///
/// Bit values mirror stylo's `FontVariantLigatures(u16)` /
/// `FontVariantNumeric(u8)` / `FontVariantEastAsian(u16)` bitflags
/// (verified in stylo 0.20 `values/specified/font.rs`, 2026-08-24).
#[allow(clippy::too_many_arguments)]
fn resolve_ot_features(
    ligatures: u16,
    caps_small: bool,
    numeric: u8,
    east_asian: u16,
    feature_settings: &[(u32, i32)],
) -> Vec<(u32, u32)> {
    use style::values::specified::font::{
        FontVariantEastAsian, FontVariantLigatures, FontVariantNumeric,
    };
    let _ = (
        core::mem::size_of::<FontVariantLigatures>(),
        core::mem::size_of::<FontVariantNumeric>(),
        core::mem::size_of::<FontVariantEastAsian>(),
    );

    let mut out: Vec<(u32, u32)> = Vec::new();
    // font-variant-ligatures.
    const L_NONE: u16 = 1;
    const L_COMMON: u16 = 1 << 1;
    const L_NO_COMMON: u16 = 1 << 2;
    const L_DISCRETIONARY: u16 = 1 << 3;
    const L_NO_DISCRETIONARY: u16 = 1 << 4;
    const L_HISTORICAL: u16 = 1 << 5;
    const L_NO_HISTORICAL: u16 = 1 << 6;
    const L_CONTEXTUAL: u16 = 1 << 7;
    const L_NO_CONTEXTUAL: u16 = 1 << 8;
    if ligatures & L_NONE != 0 {
        // none disables ALL ligature/alternate features.
        for t in [b"liga", b"clig", b"dlig", b"hlig", b"calt"] {
            out.push((tag4(t), 0));
        }
    } else {
        if ligatures & L_NO_COMMON != 0 && ligatures & L_COMMON == 0 {
            out.push((tag4(b"liga"), 0));
            out.push((tag4(b"clig"), 0));
        }
        if ligatures & L_DISCRETIONARY != 0 {
            out.push((tag4(b"dlig"), 1));
        }
        if ligatures & L_NO_DISCRETIONARY != 0 {
            out.push((tag4(b"dlig"), 0));
        }
        if ligatures & L_HISTORICAL != 0 {
            out.push((tag4(b"hlig"), 1));
        }
        if ligatures & L_NO_HISTORICAL != 0 {
            out.push((tag4(b"hlig"), 0));
        }
        if ligatures & L_CONTEXTUAL != 0 {
            out.push((tag4(b"calt"), 1));
        }
        if ligatures & L_NO_CONTEXTUAL != 0 {
            out.push((tag4(b"calt"), 0));
        }
    }
    // font-variant-caps (servo build: normal | small-caps only).
    if caps_small {
        out.push((tag4(b"smcp"), 1));
    }
    // font-variant-numeric.
    const N_LINING: u8 = 1 << 0;
    const N_OLDSTYLE: u8 = 1 << 1;
    const N_PROPORTIONAL: u8 = 1 << 2;
    const N_TABULAR: u8 = 1 << 3;
    const N_DIAGONAL_FRACTIONS: u8 = 1 << 4;
    const N_STACKED_FRACTIONS: u8 = 1 << 5;
    const N_SLASHED_ZERO: u8 = 1 << 6;
    const N_ORDINAL: u8 = 1 << 7;
    if numeric & N_LINING != 0 {
        out.push((tag4(b"lnum"), 1));
    }
    if numeric & N_OLDSTYLE != 0 {
        out.push((tag4(b"onum"), 1));
    }
    if numeric & N_PROPORTIONAL != 0 {
        out.push((tag4(b"pnum"), 1));
    }
    if numeric & N_TABULAR != 0 {
        out.push((tag4(b"tnum"), 1));
    }
    if numeric & N_DIAGONAL_FRACTIONS != 0 {
        out.push((tag4(b"frac"), 1));
    }
    if numeric & N_STACKED_FRACTIONS != 0 {
        out.push((tag4(b"afrc"), 1));
    }
    if numeric & N_SLASHED_ZERO != 0 {
        out.push((tag4(b"zero"), 1));
    }
    if numeric & N_ORDINAL != 0 {
        out.push((tag4(b"ordn"), 1));
    }
    // font-variant-east-asian.
    const E_JIS78: u16 = 1 << 0;
    const E_JIS83: u16 = 1 << 1;
    const E_JIS90: u16 = 1 << 2;
    const E_JIS04: u16 = 1 << 3;
    const E_SIMPLIFIED: u16 = 1 << 4;
    const E_TRADITIONAL: u16 = 1 << 5;
    const E_FULL_WIDTH: u16 = 1 << 6;
    const E_PROPORTIONAL_WIDTH: u16 = 1 << 7;
    const E_RUBY: u16 = 1 << 8;
    if east_asian & E_JIS78 != 0 {
        out.push((tag4(b"jp78"), 1));
    }
    if east_asian & E_JIS83 != 0 {
        out.push((tag4(b"jp83"), 1));
    }
    if east_asian & E_JIS90 != 0 {
        out.push((tag4(b"jp90"), 1));
    }
    if east_asian & E_JIS04 != 0 {
        out.push((tag4(b"jp04"), 1));
    }
    if east_asian & E_SIMPLIFIED != 0 {
        out.push((tag4(b"smpl"), 1));
    }
    if east_asian & E_TRADITIONAL != 0 {
        out.push((tag4(b"trad"), 1));
    }
    if east_asian & E_FULL_WIDTH != 0 {
        out.push((tag4(b"fwid"), 1));
    }
    if east_asian & E_PROPORTIONAL_WIDTH != 0 {
        out.push((tag4(b"pwid"), 1));
    }
    if east_asian & E_RUBY != 0 {
        out.push((tag4(b"ruby"), 1));
    }
    // Explicit font-feature-settings last (wins duplicates).
    for &(tag, value) in feature_settings {
        out.push((tag, value.max(0) as u32));
    }
    // Dedup by tag, LAST entry wins (HarfBuzz semantics). Stable order:
    // first occurrence position of each surviving tag.
    let mut deduped: Vec<(u32, u32)> = Vec::with_capacity(out.len());
    let mut seen: Vec<u32> = Vec::new();
    for &(tag, value) in out.iter().rev() {
        if !seen.contains(&tag) {
            seen.push(tag);
            deduped.push((tag, value));
        }
    }
    deduped.reverse();
    deduped
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
        // column-count/width/span are pref-gated in the servo build; enable
        // the gate before any declaration parsing (CORE-63).
        static_prefs::set_pref!("layout.columns.enabled", true);
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
pub(crate) struct CascadeSession {
    stylist: Stylist,
    _lock: SharedRwLock,
}

impl CascadeSession {
    /// Minimal UA stylesheet: the HTML default block layout the engine needs.
    /// (Stylo ships no defaults; without this, h1/p/etc. compute as inline.)
    pub(crate) const UA_CSS: &'static str = r#"
        html, body, div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, dl, dt, dd,
        blockquote, pre, section, article,
        header, footer, nav, main, aside, figure, figcaption { display: block; }
        table { display: table; border-collapse: collapse; }
        thead { display: table-header-group; }
        tbody { display: table-row-group; }
        tfoot { display: table-footer-group; }
        tr { display: table-row; }
        td, th { display: table-cell; }
        /* Prince 16.2 html.css bolds th (line 482); browsers' UA sheets do
           too (th { font-weight: bolder }). Without it, header cells measure
           ~10% narrower than Prince's, the header column freezes narrower,
           and rows wrap to an extra line (CORE-96). */
        th { font-weight: bold; }
        h1, h2, h3, h4, h5, h6 { font-weight: bold; }
        /* CORE-95: Prince's print UA sheet (lib/prince/style/html.css, 16.2)
           uses FIXED point heading sizes and margins, and 1.12em block
           margins. The HTML4/WHATWG em-scaled screen defaults scale headings
           with body font-size (an unstyled h1 at body 10pt computed to 20pt
           text + 13.4pt margins vs Prince's fixed 24pt + 16pt) and deferred
           floats past page boundaries by ~3.6pt. */
        h1 { font-size: 24pt; margin: 16pt 0; }
        h2 { font-size: 18pt; margin: 15pt 0; }
        h3 { font-size: 14pt; margin: 14pt 0; }
        h4 { font-size: 12pt; margin: 16pt 0; }
        h5 { font-size: 10pt; margin: 16.5pt 0; }
        h6 { font-size: 8pt; margin: 21pt 0; }
        /* CORE-128: Prince html.css maps headings to bookmark levels 1-6
           with label contents() and open state. The label default is
           contents() (empty piece list), so only level + state are declared.
           NOTE: comment text must not contain a `/*` marker pair, because the
           author-CSS strip_comments skips from the FIRST comment open to the
           FIRST close (CORE-95: a stray `//` line comment poisons stylo's
           rule stream; the same scanner consumes the paged pass). */
        h1 { bookmark-level: 1; bookmark-state: open; }
        h2 { bookmark-level: 2; bookmark-state: open; }
        h3 { bookmark-level: 3; bookmark-state: open; }
        h4 { bookmark-level: 4; bookmark-state: open; }
        h5 { bookmark-level: 5; bookmark-state: open; }
        h6 { bookmark-level: 6; bookmark-state: open; }
        p { margin: 1.12em 0; }
        ul, ol { padding-left: 40pt; margin: 1.12em 0; }
        // CORE-92: Prince applies no default body margin in print (probe
        // 2026-08-20: first baseline = content top + half-leading exactly).
        // The HTML4/WHATWG `body { margin: 8px }` UA default is a screen
        // convention; honoring it here pushed every page-1 block 6pt down
        // vs Prince and flipped prose page counts at line-height 1.2.
        body { margin: 0; }
        blockquote { margin: 1.12em 22.5pt; }
        pre { margin: 1.12em 0; font-family: monospace; }
        table { border-collapse: collapse; }
        td, th { display: table-cell; }
    "#;

    fn new(lock: &SharedRwLock, css: &str, geometry: &crate::geom::PageGeometry) -> Self {
        let guard = lock.read();
        let guards = StylesheetGuards::same(&guard);

        let default_values = ComputedValues::initial_values_with_font_override(Font::initial_values());
        // The stylo viewport stays at the engine's historical 1024x768 (the
        // paged-media content-box viewport was tried for CORE-66 — it fixes
        // page-size-009's 100vw/100vh div but regresses the monolithic-overflow
        // and fixedpos suites, whose shared references are authored against
        // the fixed viewport). vw/vh correctness is tracked separately.
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
        // Inline `style=""` participates in the cascade (CORE-126): the
        // element's parsed declaration block rides the normal rule-collection
        // path, landing ABOVE every author rule (css-cascade-4 §6.4) — the
        // stylist handles ordering; no manual override pass is needed.
        let style_attr_block = element.style_attribute();
        self.stylist.push_applicable_declarations(
            element,
            None,
            style_attr_block,
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
        // Computed border widths (CORE-126): stylo resolves the `border`
        // shorthand (thin/medium/thick + lengths) exactly, so the dedicated
        // author-CSS border pass can keep its selector rules but inline and
        // stylesheet-declared borders both flow through here. Width 0 ==
        // `border-style: none` never happens (medium=3px is the initial
        // width); a side only paints when its STYLE is non-none, so mirror
        // that: read style keywords and zero the width for `none`/`hidden`.
        let border = values.get_border();
        let border_side_pt = |w: style::values::computed::BorderSideWidth| -> Scalar {
            crate::geom::px_to_pt(w.0.to_f64_px())
        };
        let style_none = |s: style::values::computed::BorderStyle| -> bool {
            matches!(s, style::values::computed::BorderStyle::None | style::values::computed::BorderStyle::Hidden)
        };
        let border_style = border.clone_border_top_style();
        let border_top = if style_none(border_style) { Scalar::ZERO } else { border_side_pt(border.clone_border_top_width()) };
        let border_style = border.clone_border_right_style();
        let border_right = if style_none(border_style) { Scalar::ZERO } else { border_side_pt(border.clone_border_right_width()) };
        let border_style = border.clone_border_bottom_style();
        let border_bottom = if style_none(border_style) { Scalar::ZERO } else { border_side_pt(border.clone_border_bottom_width()) };
        let border_style = border.clone_border_left_style();
        let border_left = if style_none(border_style) { Scalar::ZERO } else { border_side_pt(border.clone_border_left_width()) };
        let border_color_stylo = border.clone_border_top_color();

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
                DisplayInside::Flex => {
                    if matches!(d.outside(), DisplayOutside::Block) {
                        Display::Flex
                    } else {
                        // `inline-flex` — treated as a block-level flex
                        // container in paged flow (spec Goal 1).
                        Display::InlineFlex
                    }
                }
                DisplayInside::TableColumn | DisplayInside::TableColumnGroup => {
                    // Column boxes are unsupported; fall back to block.
                    Display::Block
                }
                DisplayInside::FlowRoot => {
                    if matches!(d.outside(), DisplayOutside::Inline) {
                        // `inline-block` — atomic inline-level box.
                        Display::InlineBlock
                    } else {
                        Display::Block
                    }
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
        let position = values.get_position();
        let box_sizing = position.clone_box_sizing();
        let float = match box_.clone_float() {
            StyloFloat::None => Float::None,
            StyloFloat::Left => Float::Left,
            StyloFloat::Right => Float::Right,
            _ => Float::None,
        };
        let (width, width_percent) = match position.clone_width() {
            StyloSize::Auto => (None, None),
            StyloSize::LengthPercentage(lp) => {
                // NonNegative<LengthPercentage>: `.0` unwraps the non-negative
                // marker. Lengths resolve to points; percentages have no
                // absolute length (`to_length` is None) and are carried as a
                // 0..=1 fraction so table layout can resolve them against the
                // containing block (CORE-81).
                let len = lp.0.to_length().map(|l| px_to_pt(l.px() as f64));
                // Computed Percentage is a plain fraction (pub CSSFloat).
                let pct = lp.0.to_percentage().map(|p| p.0 as f64);
                (len, pct)
            }
            _ => (None, None),
        };
        // The computed `height` (points or percentage), carried for flex
        // item sizing only — the block path ignores height (auto-height
        // self-consistency, CORE-66).
        let (height, height_percent) = match position.clone_height() {
            StyloSize::Auto => (None, None),
            StyloSize::LengthPercentage(lp) => {
                let len = lp.0.to_length().map(|l| px_to_pt(l.px() as f64));
                let pct = lp.0.to_percentage().map(|p| p.0 as f64);
                (len, pct)
            }
            _ => (None, None),
        };
        // The computed `height` is carried above for FLEX item sizing only
        // (CORE-65). The BLOCK path deliberately ignores it: the skeleton
        // has no containing-block/block-size resolution (CORE-66: adding it
        // regressed the monolithic-overflow and body-background suites,
        // whose references rely on auto-height self-consistency).
        // `position` and the insets/z-index live on stylo's *position* struct
        // (the same one that carries `width`) — the CORE-62 `clone_width`
        // lesson; only the `position` longhand itself is a box property.
        let position_prop = match box_.clone_position() {
            PositionProperty::Static => Position::Static,
            PositionProperty::Relative | PositionProperty::Sticky => Position::Relative,
            PositionProperty::Absolute => Position::Absolute,
            PositionProperty::Fixed => Position::Fixed,
        };
        let inset = |v: StyloInset| match v {
            StyloInset::Auto => None,
            StyloInset::LengthPercentage(lp) => {
                lp.to_length().map(|len| px_to_pt(len.px() as f64))
            }
            _ => None,
        };
        let inset_top = inset(position.clone_top());
        let inset_right = inset(position.clone_right());
        let inset_bottom = inset(position.clone_bottom());
        let inset_left = inset(position.clone_left());
        let z_index = match position.clone_z_index() {
            StyloZIndex::Auto => None,
            StyloZIndex::Integer(n) => Some(n),
        };
        // css-multicol longhands. column-gap lives on the *position* style
        // struct; the rest on the *column* struct.
        let column = values.get_column();
        let column_count = match column.clone_column_count() {
            StyloColumnCount::Integer(n) => Some(n.0 as u32),
            StyloColumnCount::Auto => None,
        };
        let column_width = match column.clone_column_width() {
            StyloColumnWidth::Auto => None,
            StyloColumnWidth::LengthPercentage(lp) => Some(px_to_pt(lp.0.px() as f64)),
        };
        let column_span = match column.clone_column_span() {
            StyloColumnSpan::None => ColumnSpan::None,
            StyloColumnSpan::All => ColumnSpan::All,
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
        let font_weight = font.clone_font_weight().value();
        let font_style = match font.clone_font_style() {
            s if s == StyloFontStyle::NORMAL => FontStyle::Normal,
            _ => FontStyle::Italic,
        };
        // OpenType features (CORE-113): copy stylo's computed
        // `font-feature-settings` (packed big-endian tag + i32 value), then
        // resolve the full shaping list including the font-variant-* maps.
        let feature_settings: Vec<(u32, i32)> = font
            .clone_font_feature_settings()
            .0
            .iter()
            .map(|f| (f.tag.0, f.value as i32))
            .collect();
        // Bitflag longhands convert via `.bits()`; caps is a keyword enum
        // (`normal | small-caps` in the servo build).
        let variant_ligatures = font.clone_font_variant_ligatures().bits();
        let variant_caps_small = font.clone_font_variant_caps()
            == style::properties::generated::longhands::font_variant_caps::computed_value::T::SmallCaps;
        let variant_numeric = font.clone_font_variant_numeric().bits();
        let variant_east_asian = font.clone_font_variant_east_asian().bits();
        let ot_features = resolve_ot_features(
            variant_ligatures,
            variant_caps_small,
            variant_numeric,
            variant_east_asian,
            &feature_settings,
        );
        // column-gap resolves `normal` to 1em against the font size.
        let column_gap = match position.clone_column_gap() {
            StyloColumnGap::Normal => font_size,
            StyloColumnGap::LengthPercentage(lp) => lp
                .0
                .to_length()
                .map(|len| px_to_pt(len.px() as f64))
                .unwrap_or(font_size),
        };

        // Flex container properties (css-flexbox-1). All the flex longhands
        // live on stylo's *position* style struct (verified in the generated
        // `properties.rs`, 2026-08-20 — same struct that carries `width`).
        let flex_direction = match position.clone_flex_direction() {
            StyloFlexDirection::Row => FlexDirection::Row,
            StyloFlexDirection::RowReverse => FlexDirection::RowReverse,
            StyloFlexDirection::Column => FlexDirection::Column,
            StyloFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
        };
        let flex_wrap = match position.clone_flex_wrap() {
            StyloFlexWrap::Nowrap => FlexWrap::Nowrap,
            StyloFlexWrap::Wrap => FlexWrap::Wrap,
            StyloFlexWrap::WrapReverse => FlexWrap::WrapReverse,
        };
        let flex_grow = position.clone_flex_grow().0 as f64;
        let flex_shrink = position.clone_flex_shrink().0 as f64;
        // `flex-basis: auto` computes to `Size(Auto)` in stylo (the
        // `FlexBasis::auto()` constructor); intrinsic keywords fold to
        // `Auto` (treated as content-based at layout time).
        let flex_basis = match position.clone_flex_basis() {
            StyloFlexBasis::Content => FlexBasis::Content,
            StyloFlexBasis::Size(size) => match size {
                StyloSize::LengthPercentage(lp) => {
                    let length = lp.0.to_length().map(|l| px_to_pt(l.px() as f64));
                    let percent = lp.0.to_percentage().map(|p| p.0 as f64);
                    FlexBasis::Size { length, percent }
                }
                _ => FlexBasis::Auto,
            },
        };
        // css-align. `normal`/`stretch` behave per the flex model; the
        // legacy `start`/`end` keywords fold to their flex equivalents.
        let items_flag = position.clone_align_items().0;
        let align_items = if items_flag == AlignFlags::STRETCH {
            AlignItems::Stretch
        } else if items_flag == AlignFlags::FLEX_START || items_flag == AlignFlags::START {
            AlignItems::FlexStart
        } else if items_flag == AlignFlags::FLEX_END || items_flag == AlignFlags::END {
            AlignItems::FlexEnd
        } else if items_flag == AlignFlags::CENTER {
            AlignItems::Center
        } else {
            AlignItems::Stretch
        };
        let self_flag = position.clone_align_self().0;
        let align_self = if self_flag == AlignFlags::AUTO || self_flag == AlignFlags::NORMAL {
            AlignSelf::Auto
        } else if self_flag == AlignFlags::STRETCH {
            AlignSelf::Stretch
        } else if self_flag == AlignFlags::FLEX_START || self_flag == AlignFlags::START {
            AlignSelf::FlexStart
        } else if self_flag == AlignFlags::FLEX_END || self_flag == AlignFlags::END {
            AlignSelf::FlexEnd
        } else if self_flag == AlignFlags::CENTER {
            AlignSelf::Center
        } else {
            AlignSelf::Auto
        };
        let justify_flag = position.clone_justify_content().primary();
        let justify_content = if justify_flag == AlignFlags::FLEX_END
            || justify_flag == AlignFlags::END
        {
            JustifyContent::FlexEnd
        } else if justify_flag == AlignFlags::CENTER {
            JustifyContent::Center
        } else if justify_flag == AlignFlags::SPACE_BETWEEN {
            JustifyContent::SpaceBetween
        } else {
            // `normal`, `flex-start`, `start` and anything else → the default.
            JustifyContent::FlexStart
        };
        let order = position.clone_order();
        // Flex gaps: `normal` resolves to 0 for flex containers (css-align-3
        // §8), unlike multicol's 1em default carried in `column_gap`.
        let flex_gap = |v: &StyloColumnGap| match v {
            StyloColumnGap::Normal => Scalar::ZERO,
            StyloColumnGap::LengthPercentage(lp) => lp
                .0
                .to_length()
                .map(|len| px_to_pt(len.px() as f64))
                .unwrap_or(Scalar::ZERO),
        };
        let row_gap = flex_gap(&position.clone_row_gap());
        let flex_column_gap = flex_gap(&position.clone_column_gap());
        let font_family = first_family_name(font.clone_font_family())
            .unwrap_or_else(|| "sans-serif".to_string());
        let (font_face, font_fallbacks) = {
            let specs = family_specs(font.clone_font_family());
            let resolution = crate::fonts::resolve_font(&specs, font_weight, font_style);
            (resolution.primary, resolution.fallbacks)
        };
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

        // Border color: stylo's computed top-side color (the engine's border
        // model is one shared color — css.rs notes CORE-66's precedent). The
        // `border-color` fallback pass below overrides when IT matched.
        let stylo_border_color = match border_color_stylo {
            ComputedColor::Absolute(c) if !c.is_transparent() => {
                let [r, g, b, _a] = c.to_nscolor().to_le_bytes();
                Some(Color { r, g, b })
            }
            _ => None,
        };

        ComputedStyle {
            color,
            background_color,
            box_sizing,
            // Borders come from stylo's computed values (CORE-126): both the
            // stylesheet `border` shorthand and inline declarations flow
            // through the cascade, styled sides only (`none`/`hidden` → 0).
            // The legacy border pass below only fills the shared COLOR slot
            // when stylo's cascade is missing it.
            border_top,
            border_right,
            border_bottom,
            border_left,
            border_color: stylo_border_color,
            font_size,
            line_height,
            font_family,
            font_face,
            font_fallbacks,
            feature_settings,
            ot_features,
            display,
            font_weight,
            font_style,
            float,
            width,
            width_percent,
            height,
            height_percent,
            position: position_prop,
            inset_top,
            inset_right,
            inset_bottom,
            inset_left,
            z_index,
            column_count,
            column_width,
            column_span,
            column_gap,
            flex_direction,
            flex_wrap,
            flex_grow,
            flex_shrink,
            flex_basis,
            align_items,
            align_self,
            justify_content,
            order,
            row_gap,
            flex_column_gap,
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
            widows: 1,
            text_align,
            hyphens: Hyphens::Manual,
            // Paged-media element props are likewise absent from the servo
            // stylo build; filled by `apply_paged_properties` (see `cascade`).
            page: None,
            writing_mode_declared: false,
            string_set: Vec::new(),
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            // Bookmark defaults come from the UA sheet via the paged pass
            // (h1-h6 levels 1-6, open). Initial here = suppressed: a
            // `Level(1)` initial would bookmark every element.
            bookmark_level: BookmarkLevel::None,
            bookmark_label: Vec::new(),
            bookmark_closed: false,
            content: Vec::new(),
            float_footnote: false,
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

/// Convert a computed `font-family` list into registry [`FamilySpec`]s,
/// preserving stack order (named families AND generics — CORE-103).
fn family_specs(family: FontFamily) -> Vec<crate::fonts::FamilySpec> {
    use style::values::computed::font::GenericFontFamily;
    family
        .families
        .list
        .iter()
        .map(|f| match f {
            SingleFontFamily::FamilyName(name) => {
                crate::fonts::FamilySpec::Name(name.name.to_string())
            }
            SingleFontFamily::Generic(g) => match g {
                GenericFontFamily::Serif => crate::fonts::FamilySpec::Serif,
                GenericFontFamily::SansSerif => crate::fonts::FamilySpec::SansSerif,
                GenericFontFamily::Monospace => crate::fonts::FamilySpec::Monospace,
                GenericFontFamily::Cursive => crate::fonts::FamilySpec::Cursive,
                GenericFontFamily::Fantasy => crate::fonts::FamilySpec::Fantasy,
                _ => crate::fonts::FamilySpec::SansSerif,
            },
        })
        .collect()
}

// --- @font-face (CORE-103) --------------------------------------------------

/// One parsed `@font-face` rule.
#[derive(Clone, Debug)]
pub struct FontFaceRule {
    /// The `font-family` descriptor: the family name this rule defines.
    pub family: String,
    /// Resolved `src` file paths, in declaration order (local()/url() both
    /// resolve to files; network sources are dropped).
    pub sources: Vec<String>,
    /// `font-weight` descriptor (single value; default 400).
    pub weight: f32,
    /// `font-style: italic` descriptor (default false).
    pub italic: bool,
}

/// Parse every `@font-face` rule out of a stylesheet. Balanced-brace scan
/// over comment-stripped source (same technique as the break/page passes);
/// char-safe iteration throughout (the CORE-83 lesson).
pub fn parse_font_face_rules(css: &str) -> Vec<FontFaceRule> {
    let css = breaks::strip_comments(css);
    let mut rules = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while let Some(at) = css[i..].find("@font-face") {
        let start = i + at;
        // The rule's opening brace must come before any '}' or ';' — an
        // earlier brace belongs to a previous block we already consumed.
        let Some(brace_rel) = css[start..].find('{') else {
            break;
        };
        let brace = start + brace_rel;
        // Find the balanced close.
        let mut depth = 1usize;
        let mut end = brace + 1;
        while end < bytes.len() && depth > 0 {
            match bytes[end] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            end += 1;
        }
        if depth != 0 {
            break; // unbalanced stylesheet tail; ignore the partial rule
        }
        if let Some(rule) = parse_one_font_face(&css[brace + 1..end - 1]) {
            rules.push(rule);
        }
        i = end;
    }
    rules
}

/// Parse one `@font-face` body into a [`FontFaceRule`]. `None` when there
/// is no usable `font-family` descriptor or no resolvable src.
fn parse_one_font_face(body: &str) -> Option<FontFaceRule> {
    let mut family: Option<String> = None;
    let mut sources: Vec<String> = Vec::new();
    let mut weight = 400.0f32;
    let mut italic = false;
    for decl in split_top_level_decls(body) {
        let decl = decl.trim();
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        match prop.as_str() {
            "font-family" => {
                // Strip quotes; the descriptor takes ONE family name.
                let name = value.trim().trim_matches(|c| c == '\'' || c == '"');
                if !name.is_empty() {
                    family = Some(name.to_string());
                }
            }
            "src" => {
                for part in split_src_list(value) {
                    let part = part.trim();
                    if let Some(p) = part.strip_prefix("url(") {
                        if let Some(path) = p.strip_suffix(')') {
                            sources.push(path.trim().trim_matches(|c| c == '\'' || c == '"').to_string());
                        }
                    } else if let Some(p) = part.strip_prefix("local(") {
                        if let Some(name) = p.strip_suffix(')') {
                            sources.push(format!("local:{}", name.trim()));
                        }
                    }
                    // format() hints and other trailing tokens are ignored.
                }
            }
            "font-weight" => {
                weight = parse_font_weight_descriptor(value).unwrap_or(400.0);
            }
            "font-style" => {
                italic = value.eq_ignore_ascii_case("italic")
                    || value.eq_ignore_ascii_case("oblique");
            }
            _ => {}
        }
    }
    let family = family?;
    if sources.is_empty() {
        return None;
    }
    Some(FontFaceRule { family, sources, weight, italic })
}

/// Split a declaration body on top-level `;` (ignoring parens, e.g. inside
/// url(...)). Char-safe (byte-index based, ASCII delimiters only).
fn split_top_level_decls(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b';' if depth == 0 => {
                out.push(&body[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start < body.len() {
        out.push(&body[start..]);
    }
    out
}

/// Split a `src` value's comma list, ignoring commas inside url(...).
fn split_src_list(value: &str) -> Vec<&str> {
    split_top_level_decls(value)
}

/// The `font-weight` descriptor: single number, named weights, or a range
/// (`400 700` — take the lower bound).
fn parse_font_weight_descriptor(value: &str) -> Option<f32> {
    let first = value.split_whitespace().next()?;
    match first.to_ascii_lowercase().as_str() {
        "normal" => Some(400.0),
        "bold" => Some(700.0),
        n => n.parse::<f32>().ok().filter(|w| (1.0..=1000.0).contains(w)),
    }
}

/// Register every `@font-face` rule's sources into the face registry, in
/// stylesheet order. Sources resolve like `<img src>` (CORE-106): relative
/// paths against `--base-url`, absolute paths pass through; `local(...)`
/// entries match an installed family name via fontdb. Unreadable or
/// unparseable sources are skipped deterministically.
fn apply_font_face_rules(css_source: &str) {
    for rule in parse_font_face_rules(css_source) {
        for src in &rule.sources {
            if let Some(local_name) = src.strip_prefix("local:") {
                // local(): register every installed face of that family so
                // weight/style matching can pick among them.
                for face in crate::fonts::system_family_faces(local_name) {
                    crate::fonts::register_system_face_pub(&face);
                }
            } else if let Some(bytes) = read_font_file(src) {
                // url(): file bytes. Registration is idempotent per
                // (family, weight, style, content).
                crate::fonts::register_face_bytes(&rule.family, rule.weight, rule.italic, bytes);
            }
        }
    }
}

/// Read a font file source. Relative paths are NOT resolved here — the
/// engine has no base URL at cascade time, so relative paths resolve
/// against the process CWD (matching how tests run from the repo root);
/// document-relative resolution threads through `--base-url` at the CLI
/// layer by rewriting `url()` before parse (see main.rs).
fn read_font_file(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
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

/// Parse a CSS color (`#rgb`/`#rrggbb` or named), reused by the `@page` pass
/// for the page box background. `None` for `transparent`/unrecognized.
pub fn parse_css_color(s: &str) -> Option<Color> {
    borders::parse_color(s)
}

/// The cascade entry point. Produces a `ComputedStyle` per DOM node id
/// (indexed by `NodeId`). Text nodes inherit their parent's style.
///
/// This is the swap boundary between the engine and stylo: layout and PDF
/// read only the returned `Vec<ComputedStyle>`.
pub fn cascade(dom: &Dom, stylesheet: &Stylesheet, geometry: &crate::geom::PageGeometry) -> Vec<ComputedStyle> {
    // @font-face registration (CORE-103) happens BEFORE any resolution so
    // custom families shadow system fonts. Idempotent (content-hash dedup
    // in the registry), so repeated cascades are safe.
    apply_font_face_rules(stylesheet.source());
    let lock = SharedRwLock::new();
    let mut backend = TyBackend::new(dom, &lock);
    let mut session = CascadeSession::new(&lock, stylesheet.source(), geometry);
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
    // content / bookmark-*). The UA sheet's element rules (CORE-128 heading
    // bookmark defaults) join at UA origin — any author rule wins.
    paged_props::apply_paged_properties(
        dom,
        stylesheet.source(),
        CascadeSession::UA_CSS,
        &mut styles,
    );
    // Fourth pass: fill border widths/colors (CORE-61 tables; the engine's
    // ComputedStyle carries borders for border-collapse rendering).
    borders::apply_border_properties(dom, stylesheet.source(), &mut styles);
    // Fifth pass: collapse adjacent table-cell borders (CORE-119 #5) — the
    // shared edge between two neighboring cells must stroke once.
    crate::table::collapse_cell_borders(dom, &mut styles);
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

    /// A structural pseudo-class the author-CSS passes can match (css-selectors-4
    /// subset, added for CORE-66: WPT fixtures use `:first-of-type` /
    /// `:nth-of-type(N)` for page-break and named-page rules, and `:root` for
    /// `print-color-adjust`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum StructuralPseudo {
        FirstOfType,
        NthOfType(u32),
        Root,
    }

    /// A parsed simple selector: an optional tag plus required classes/id.
    pub(super) struct SimpleSelector {
        pub(super) tag: Option<String>,
        pub(super) id: Option<String>,
        pub(super) classes: Vec<String>,
        pub(super) pseudo: Option<StructuralPseudo>,
        /// Higher wins ties; approximates specificity (id=100, class=10,
        /// tag=1, pseudo-class=10) then declaration order.
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
            if !self.classes.iter().all(|c| el.classes.iter().any(|x| x == c)) {
                return false;
            }
            match self.pseudo {
                None => true,
                Some(StructuralPseudo::Root) => id == dom.root,
                Some(p) => structural_pseudo_matches(dom, id, p),
            }
        }
    }

    /// Evaluate a structural pseudo against an element's sibling position.
    /// `:first-of-type` / `:nth-of-type(N)` are relative to the element's
    /// parent: the N-th element child (1-based) with the same tag.
    fn structural_pseudo_matches(dom: &Dom, id: NodeId, p: StructuralPseudo) -> bool {
        let Some(parent) = dom.nodes[id].parent else {
            return false;
        };
        let NodeKind::Element(el) = &dom.nodes[id].kind else {
            return false;
        };
        let mut nth = 0u32;
        for &sib in &dom.nodes[parent].children {
            let NodeKind::Element(se) = &dom.nodes[sib].kind else {
                continue;
            };
            if !se.tag.eq_ignore_ascii_case(&el.tag) {
                continue;
            }
            nth += 1;
            if sib == id {
                return match p {
                    StructuralPseudo::FirstOfType => nth == 1,
                    StructuralPseudo::NthOfType(n) => nth == n,
                    StructuralPseudo::Root => false,
                };
            }
        }
        false
    }

    /// Parse one compound simple selector like `section.note#x:first-of-type`.
    /// Returns `None` for anything with a combinator or a pseudo we do not
    /// support.
    pub(super) fn parse_simple(sel: &str) -> Option<SimpleSelector> {
        let sel = sel.trim();
        if sel.is_empty() || sel.contains([' ', '>', '+', '~', '[', '*']) {
            return None;
        }
        // Split a trailing `:pseudo` (the first `:` starts the pseudo token).
        let (core, pseudo) = match sel.find(':') {
            Some(p) => {
                let (head, tail) = sel.split_at(p);
                (head, parse_pseudo(tail)?)
            }
            None => (sel, None),
        };
        if core.is_empty() && pseudo.is_none() {
            return None;
        }
        let mut tag = None;
        let mut id = None;
        let mut classes = Vec::new();
        let mut spec = 0u32;
        let mut chars = core.chars().peekable();
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
        if pseudo.is_some() {
            spec += 10;
        }
        Some(SimpleSelector {
            tag,
            id,
            classes,
            pseudo,
            specificity: spec,
        })
    }

    /// Parse a trailing pseudo-class token (`:first-of-type`, `:nth-of-type(N)`,
    /// `:root`). `None` for unknown pseudos (the rule is dropped).
    fn parse_pseudo(tok: &str) -> Option<Option<StructuralPseudo>> {
        let t = tok.trim().to_ascii_lowercase();
        match t.as_str() {
            ":first-of-type" => Some(Some(StructuralPseudo::FirstOfType)),
            ":root" => Some(Some(StructuralPseudo::Root)),
            _ => {
                if let Some(inner) = t.strip_prefix(":nth-of-type(") {
                    let n: u32 = inner.trim_end_matches(')').trim().parse().ok()?;
                    if n >= 1 {
                        return Some(Some(StructuralPseudo::NthOfType(n)));
                    }
                }
                None
            }
        }
    }

    /// Split a `style=""` attribute value into (property, value) pairs, in
    /// source order. Used by every author-CSS pass so inline declarations win
    /// the cascade exactly like a stylesheet rule with maximal specificity.
    pub(super) fn parse_inline_decls(attr: &str) -> Vec<(String, String)> {
        attr.split(';')
            .filter_map(|d| d.split_once(':'))
            .map(|(p, v)| (p.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect()
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
    /// Char-safe: iterates code points, never raw bytes (CORE-83 — a
    /// byte-wise loop turned every multi-byte UTF-8 char in the stylesheet
    /// into Latin-1 mojibake before declarations were parsed).
    pub(super) fn strip_comments(css: &str) -> String {
        let mut out = String::with_capacity(css.len());
        let mut rest = css;
        while !rest.is_empty() {
            if rest.starts_with("/*") {
                match rest.find("*/") {
                    Some(end) => rest = &rest[end + 2..],
                    // Unterminated comment: drop the remainder (matches the
                    // byte-wise loop, which ran off the end of the buffer).
                    None => break,
                }
            } else {
                let ch = rest.chars().next().expect("non-empty rest");
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
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

        // Inline `style=""` declarations win over every stylesheet rule
        // (same model as the border/paged passes; WPT fixtures and tests
        // force breaks inline, e.g. `style="break-before: page"`).
        let mut inline_order = 0u32;
        for id in 0..dom.nodes.len() {
            let NodeKind::Element(el) = &dom.nodes[id].kind else {
                continue;
            };
            let Some(attr) = el.attr("style") else {
                continue;
            };
            for (prop, value) in parse_inline_decls(attr) {
                let Some(decl) = parse_decl(&prop, &value) else {
                    continue;
                };
                let prio = (u32::MAX, inline_order);
                inline_order += 1;
                match decl {
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
                    _ => {}
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
            // css-color-3 basic keywords + the WPT fixture palette (CORE-66:
            // page-box tests paint the page box with yellow/cyan/hotpink).
            "yellow" => (255, 255, 0),
            "cyan" | "aqua" => (0, 255, 255),
            "magenta" | "fuchsia" => (255, 0, 255),
            "hotpink" => (255, 105, 180),
            "pink" => (255, 192, 203),
            "lightblue" => (173, 216, 230),
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
            // Per-side color longhands (CORE-66, page-orientation mismatch
            // tests use `border-bottom-color`). The engine's border model is a
            // single shared color, so the per-side override sets that slot —
            // enough to make test/notref differ where the tests require.
            "border-top-color" | "border-right-color" | "border-bottom-color"
            | "border-left-color" => {
                if let Some(c) = c {
                    Some(BorderDecl::Color(c))
                } else {
                    None
                }
            }
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

        // Inline `style=""` declarations win over every stylesheet rule
        // (CORE-66: WPT fixtures set border colors/widths inline).
        let mut inline_order = 0u32;
        for id in 0..dom.nodes.len() {
            let crate::dom::NodeKind::Element(el) = &dom.nodes[id].kind else {
                continue;
            };
            let Some(attr) = el.attr("style") else {
                continue;
            };
            for (prop, value) in super::breaks::parse_inline_decls(attr) {
                let Some(decl) = parse_decl(&prop, &value) else {
                    continue;
                };
                let prio = (u32::MAX, inline_order);
                inline_order += 1;
                match &decl {
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
        /// `writing-mode: <value>` — recorded as a boolean flag only (CORE-127
        /// orthogonal-flow suppression; the value itself is unused in v1).
        WritingMode,
        /// `display: <value>` from an inline style (CORE-127): the stylo
        /// inline-style seam ignores display, so the paged pass carries it —
        /// fixtures rely on inline `display: flex / inline-block / none`.
        Display(super::Display),
        /// `position: <value>` from an inline style (CORE-127, same seam gap).
        Position(super::Position),
        /// `float: left | right` from an inline style (CORE-127, same seam
        /// gap; `float: footnote` is handled by `FloatFootnote`).
        FloatSide(super::Float),
        FloatFootnote(bool),
        StringSet(Vec<(String, StringSetValue)>),
        CounterReset(Vec<(String, i32)>),
        CounterIncrement(Vec<(String, i32)>),
        Content(Vec<ContentPiece>),
        /// `bookmark-level: none | <integer>` (CORE-128).
        BookmarkLevel(super::BookmarkLevel),
        /// `bookmark-label: <content-list>` — parsed by the same parser as
        /// `content`.
        BookmarkLabel(Vec<ContentPiece>),
        /// `bookmark-state: open | closed` — `true` when `closed`.
        BookmarkState(bool),
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
            // Flag only (CORE-127): the value (vertical-rl etc.) is unused in
            // v1; presence alone suppresses page-change breaks in the subtree.
            "writing-mode" => Some(PagedDecl::WritingMode),
            // Inline-only (CORE-127): the stylo seam ignores inline display,
            // so the paged pass carries it for the fixtures that need it.
            "display" if !value.trim().is_empty() => {
                let v = value.trim().to_ascii_lowercase();
                match v.as_str() {
                    "block" => Some(PagedDecl::Display(super::Display::Block)),
                    "inline" => Some(PagedDecl::Display(super::Display::Inline)),
                    "none" => Some(PagedDecl::Display(super::Display::None)),
                    "flex" => Some(PagedDecl::Display(super::Display::Flex)),
                    "inline-flex" => Some(PagedDecl::Display(super::Display::InlineFlex)),
                    "inline-block" => Some(PagedDecl::Display(super::Display::InlineBlock)),
                    _ => None,
                }
            }
            // Same seam gap (CORE-127): inline position/float. The float arm
            // comes first so `footnote` still reaches FloatFootnote below.
            "position" => {
                let v = value.trim().to_ascii_lowercase();
                match v.as_str() {
                    "static" => Some(PagedDecl::Position(super::Position::Static)),
                    "relative" => Some(PagedDecl::Position(super::Position::Relative)),
                    "absolute" => Some(PagedDecl::Position(super::Position::Absolute)),
                    "fixed" => Some(PagedDecl::Position(super::Position::Fixed)),
                    _ => None,
                }
            }
            "float" if value.trim().eq_ignore_ascii_case("footnote") => {
                Some(PagedDecl::FloatFootnote(true))
            }
            "float" => {
                let v = value.trim().to_ascii_lowercase();
                match v.as_str() {
                    "left" => Some(PagedDecl::FloatSide(super::Float::Left)),
                    "right" => Some(PagedDecl::FloatSide(super::Float::Right)),
                    _ => None,
                }
            }
            "string-set" => Some(PagedDecl::StringSet(parse_string_set(value))),
            "counter-reset" => Some(PagedDecl::CounterReset(parse_counters(value, 0))),
            "counter-increment" => Some(PagedDecl::CounterIncrement(parse_counters(value, 1))),
            "content" => Some(PagedDecl::Content(parse_content(value))),
            "bookmark-level" => {
                let v = value.trim();
                if v.eq_ignore_ascii_case("none") {
                    Some(PagedDecl::BookmarkLevel(super::BookmarkLevel::None))
                } else if let Ok(n) = v.parse::<u8>() {
                    // css-gcpm-3: 1..=6; out-of-range integers are invalid and
                    // the whole declaration is ignored (UA default survives).
                    if (1..=6).contains(&n) {
                        Some(PagedDecl::BookmarkLevel(super::BookmarkLevel::Level(n)))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "bookmark-label" => {
                let pieces = parse_content(value);
                // Malformed/empty labels fall back to `contents()` (empty
                // piece list = element text, spec Behavior 3).
                Some(PagedDecl::BookmarkLabel(pieces))
            }
            "bookmark-state" => {
                let v = value.trim().to_ascii_lowercase();
                match v.as_str() {
                    "open" => Some(PagedDecl::BookmarkState(false)),
                    "closed" => Some(PagedDecl::BookmarkState(true)),
                    _ => None,
                }
            }
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
    /// `ua_css` carries the UA sheet's own element rules (CORE-128: the
    /// heading bookmark defaults live there, at UA origin — an author rule
    /// wins the cascade slot by construction since UA rules are applied
    /// first and any later match with prio >= an UA match overwrites).
    pub fn apply_paged_properties(dom: &Dom, css: &str, ua_css: &str, styles: &mut [ComputedStyle]) {
        let mut rules = parse_rules(css);
        let mut ua_rules = parse_rules(ua_css);
        // UA origin loses to any author rule. Renumber UA rules to 0..n so
        // their priorities sit BELOW every author rule, then shift author
        // orders up by the UA count: an author rule's (spec, order) always
        // compares >= a UA rule's whenever both matched (same-shape guard
        // as the `>= w` write below), so the later (author) write wins.
        // NOTE: renumber rather than reuse the scanned order values —
        // `parse_rules` bumps `order` for every block it scans (including
        // font-size etc. rules the paged parser drops), so UA order values
        // run ~20+ while `ua_rules.len()` is only 6.
        let ua_count = ua_rules.len() as u32;
        for (i, r) in ua_rules.iter_mut().enumerate() {
            r.order = i as u32;
        }
        for r in rules.iter_mut() {
            r.order = r.order.saturating_add(ua_count);
        }
        let mut all: Vec<Rule> = ua_rules;
        all.extend(rules);
        let rules = all;

        #[derive(Clone, Copy, Default)]
        struct Won {
            page: Option<(u32, u32)>,
            string_set: Option<(u32, u32)>,
            counter_reset: Option<(u32, u32)>,
            counter_increment: Option<(u32, u32)>,
            content: Option<(u32, u32)>,
            bookmark_level: Option<(u32, u32)>,
            bookmark_label: Option<(u32, u32)>,
            bookmark_state: Option<(u32, u32)>,
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
                            PagedDecl::WritingMode => {
                                styles[id].writing_mode_declared = true;
                            }
                            PagedDecl::Display(v) => {
                                styles[id].display = *v;
                            }
                            PagedDecl::Position(v) => {
                                styles[id].position = *v;
                            }
                            PagedDecl::FloatSide(v) => {
                                styles[id].float = *v;
                            }
                            PagedDecl::FloatFootnote(v) => {
                                if won[id].content.is_none_or(|w| prio >= w) {
                                    // Footnote floats share the `content` slot
                                    // in `Won` (both are rare; a rule setting
                                    // both on the same element is pathological).
                                    // The bool itself always applies when the
                                    // declaration matches.
                                    styles[id].float_footnote = *v;
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
                            PagedDecl::BookmarkLevel(v) => {
                                if won[id].bookmark_level.is_none_or(|w| prio >= w) {
                                    styles[id].bookmark_level = *v;
                                    won[id].bookmark_level = Some(prio);
                                }
                            }
                            PagedDecl::BookmarkLabel(v) => {
                                if won[id].bookmark_label.is_none_or(|w| prio >= w) {
                                    styles[id].bookmark_label = v.clone();
                                    won[id].bookmark_label = Some(prio);
                                }
                            }
                            PagedDecl::BookmarkState(v) => {
                                if won[id].bookmark_state.is_none_or(|w| prio >= w) {
                                    styles[id].bookmark_closed = *v;
                                    won[id].bookmark_state = Some(prio);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Inline `style=""` declarations win over every stylesheet rule
        // (CORE-66: WPT fixtures set `page:` inline, e.g. `style="page:a"`).
        let mut inline_order = 0u32;
        for id in 0..dom.nodes.len() {
            let NodeKind::Element(el) = &dom.nodes[id].kind else {
                continue;
            };
            let Some(attr) = el.attr("style") else {
                continue;
            };
            for (prop, value) in super::breaks::parse_inline_decls(attr) {
                let Some(decl) = parse_decl(&prop, &value) else {
                    continue;
                };
                let prio = (u32::MAX, inline_order);
                inline_order += 1;
                match decl {
                    PagedDecl::FloatFootnote(v) => {
                        styles[id].float_footnote = v;
                    }
                    PagedDecl::Display(v) => {
                        styles[id].display = v;
                    }
                    PagedDecl::Position(v) => {
                        styles[id].position = v;
                    }
                    PagedDecl::FloatSide(v) => {
                        styles[id].float = v;
                    }
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
                    PagedDecl::BookmarkLevel(v) => {
                        if won[id].bookmark_level.is_none_or(|w| prio >= w) {
                            styles[id].bookmark_level = v;
                            won[id].bookmark_level = Some(prio);
                        }
                    }
                    PagedDecl::BookmarkLabel(v) => {
                        if won[id].bookmark_label.is_none_or(|w| prio >= w) {
                            styles[id].bookmark_label = v.clone();
                            won[id].bookmark_label = Some(prio);
                        }
                    }
                    PagedDecl::BookmarkState(v) => {
                        if won[id].bookmark_state.is_none_or(|w| prio >= w) {
                            styles[id].bookmark_closed = v;
                            won[id].bookmark_state = Some(prio);
                        }
                    }
                    PagedDecl::WritingMode => {
                        styles[id].writing_mode_declared = true;
                    }
                    PagedDecl::Display(v) => {
                        styles[id].display = v;
                    }
                }
            }
        }
    }
}
