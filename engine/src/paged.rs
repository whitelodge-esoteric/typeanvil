// SPDX-License-Identifier: AGPL-3.0-only

//! Paged-media CSS: `@page` rules, margin boxes, running strings, counters,
//! and generated content — the wedge features layered on top of the CORE-51
//! fragment tree.
//!
//! stylo's servo build compiles neither `@page` nor the paged-media longhands,
//! so — exactly like `css::breaks` — this module runs a small, deterministic
//! author-CSS pass that parses only what it needs and never fails on the rest.
//! [`parse_page_rules`] extracts `@page` blocks (default + named, with
//! `:first`/`:left`/`:right` pseudos, `size`, margin longhands, and margin-box
//! sub-rules); [`resolve_page_spec`] picks the winning rule for one page by
//! name-in-effect then pseudo-by-index, falling back to the CLI geometry.
//!
//! Generated content ([`ContentPiece`]) is shared between margin boxes and the
//! element `content` property (see `css::ComputedStyle::content`, which the TOC
//! uses for `leader('.') target-counter(attr(href), page)`). Resolution to a
//! concrete string happens at fragmentainer-build time, once running strings,
//! the page counter, and the `target-counter` page map are known.
//!
//! Determinism: parsing is a single left-to-right scan; rule selection is a
//! total order over (name, pseudo-specificity, source order); running strings
//! and counters are threaded in document order. No hash-order dependence.

use crate::geom::{PageGeometry, Scalar};
use crate::css::Color;

/// A `@page` pseudo-class we support (css-page-3 subset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagePseudo {
    /// `@page :first` — the first page only.
    First,
    /// `@page :left` — verso pages (even 1-based index).
    Left,
    /// `@page :right` — recto pages (odd 1-based index; page 1 is right).
    Right,
    /// No pseudo — matches every page (of the given name).
    None,
}

/// The name of a page margin box (css-page-3 §7). The four side boxes
/// (`@left-*`/`@right-*`) are parsed and positioned but rendered as a single
/// horizontal line (no rotation — a documented non-goal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginBoxName {
    TopLeftCorner,
    TopLeft,
    TopCenter,
    TopRight,
    TopRightCorner,
    BottomLeftCorner,
    BottomLeft,
    BottomCenter,
    BottomRight,
    BottomRightCorner,
    LeftTop,
    LeftMiddle,
    LeftBottom,
    RightTop,
    RightMiddle,
    RightBottom,
}

impl MarginBoxName {
    /// Parse an at-keyword like `top-center` (without the leading `@`).
    fn parse(kw: &str) -> Option<MarginBoxName> {
        use MarginBoxName::*;
        Some(match kw.trim().to_ascii_lowercase().as_str() {
            "top-left-corner" => TopLeftCorner,
            "top-left" => TopLeft,
            "top-center" => TopCenter,
            "top-right" => TopRight,
            "top-right-corner" => TopRightCorner,
            "bottom-left-corner" => BottomLeftCorner,
            "bottom-left" => BottomLeft,
            "bottom-center" => BottomCenter,
            "bottom-right" => BottomRight,
            "bottom-right-corner" => BottomRightCorner,
            "left-top" => LeftTop,
            "left-middle" => LeftMiddle,
            "left-bottom" => LeftBottom,
            "right-top" => RightTop,
            "right-middle" => RightMiddle,
            "right-bottom" => RightBottom,
            _ => return None,
        })
    }

    /// Which margin row/column this box sits in.
    pub fn row(self) -> MarginRow {
        use MarginBoxName::*;
        match self {
            TopLeftCorner | TopLeft | TopCenter | TopRight | TopRightCorner => MarginRow::Top,
            BottomLeftCorner | BottomLeft | BottomCenter | BottomRight | BottomRightCorner => {
                MarginRow::Bottom
            }
            LeftTop | LeftMiddle | LeftBottom => MarginRow::Left,
            RightTop | RightMiddle | RightBottom => MarginRow::Right,
        }
    }

    /// The UA-default `text-align` for this box (css-page-3 §6.2, Table 2).
    ///
    /// Corners align their content toward the page area (`@top-left-corner` →
    /// `right`, `@top-right-corner` → `left`), side boxes center along the page
    /// edge, and edge boxes align toward the page area (`@top-left` → `left`,
    /// `@top-right` → `right`). Before this the engine derived an alignment
    /// from the box's *name* that was wrong on the corners and side boxes.
    pub fn default_text_align(self) -> crate::css::TextAlign {
        use crate::css::TextAlign;
        use MarginBoxName::*;
        match self {
            TopLeftCorner | TopRight | BottomLeftCorner | BottomRight => TextAlign::Right,
            TopRightCorner | TopLeft | BottomLeft | BottomRightCorner => TextAlign::Left,
            TopCenter | BottomCenter | LeftTop | LeftMiddle | LeftBottom | RightTop
            | RightMiddle | RightBottom => TextAlign::Center,
        }
    }

    /// The UA-default `vertical-align` for this box (css-page-3 §6.2, Table 2).
    pub fn default_vertical_align(self) -> VerticalAlign {
        use MarginBoxName::*;
        match self {
            LeftTop | RightTop => VerticalAlign::Top,
            LeftBottom | RightBottom => VerticalAlign::Bottom,
            _ => VerticalAlign::Middle,
        }
    }
}

/// The page-margin band a margin box lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginRow {
    Top,
    Bottom,
    Left,
    Right,
}

/// The computed `vertical-align` of a page-margin box (css-page-3 §6.2):
/// `top | middle | bottom`. Other CSS `vertical-align` keywords are invalid in
/// the margin context and are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// The css-gcpm-3 §7 keyword of a `string(name, keyword)` reference.
/// Semantics probed against Prince 16.2 (see
/// `docs/research/css-gcpm/prince-string-keywords-probe.md`): with A = this
/// page's assignments and C = the carried value entering the page,
/// `first`/default → first of A else C; `last` → last of A else C;
/// `start` → C always (a top-of-page element does NOT count);
/// `first-except` → empty when A is non-empty, else C.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StringKeyword {
    #[default]
    First,
    Start,
    Last,
    FirstExcept,
}

impl StringKeyword {
    fn parse(s: &str) -> StringKeyword {
        match s.trim().to_ascii_lowercase().as_str() {
            "start" => StringKeyword::Start,
            "last" => StringKeyword::Last,
            "first-except" => StringKeyword::FirstExcept,
            // "first" and anything malformed fall back to the default
            // (spec §Behavior 1).
            _ => StringKeyword::First,
        }
    }
}

/// One piece of generated content (the `content` property / margin-box
/// `content`). A `content` value is a sequence of these, concatenated.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentPiece {
    /// A quoted literal string.
    Literal(String),
    /// `string(name[, keyword])` — the running string's value for this page,
    /// with css-gcpm-3 §7 keyword semantics (default `first`; probed against
    /// Prince 16.2, see docs/research/css-gcpm/prince-string-keywords-probe.md).
    StringRef(String, StringKeyword),
    /// `counter(page)` — the decimal page counter.
    CounterPage,
    /// `counter(pages)` — the total page count, resolved by a bounded
    /// second layout pass (paged-media-css spec §8; see `layout.rs`).
    CounterPages,
    /// `counter(name)` — a named counter (decimal).
    CounterRef(String),
    /// `counters(name, separator)` — a named counter's value in scope
    /// (css-page-3 §8). Page/margin contexts *obscure* the document counter
    /// rather than nest, so a single innermost value is the correct rendering
    /// for every current fixture; the separator is retained for a future
    /// scope-chain join.
    CountersRef { name: String, separator: String },
    /// `target-counter(<target>, <counter-name>)` — the value of
    /// `<counter-name>` on the PAGE the target element lands on (css-gcpm-3
    /// §7 / CORE-129). The target is an `attr(name)` reference (the attribute
    /// of the element carrying the `content` — typically `href="#anchor"`);
    /// the counter is one of the built-ins (`page`, `pages`) or a named
    /// document counter (`chapter`, `section`, ...). The default
    /// `counter(page)` is recorded as `"page"`.
    TargetCounter { attr: String, counter: String },
    /// `target-text(<target>)` — the text content of the target element
    /// (css-gcpm-3 §7.1 / CORE-129). Same target grammar as `TargetCounter`;
    /// the text is the element's full text content (first-line model kept
    /// simple: the whole subtree text, whitespace-normalised by the DOM).
    TargetText { attr: String },
    /// `leader('.')` — fill to the content edge with a repeating character.
    Leader(char),
    /// `url(<path>)` — an image painted inline in the box's single line
    /// (css-page-3 margin boxes replace the element, so the image is atomic
    /// content of natural size). The path is resolved and interned in the
    /// image store at layout time, like an `<img src>`.
    Image(String),
}

/// The style declarations of one page-margin box (css-page-3 §5–§6, the
/// CSS2.1 subset that applies in a margin context). Every field is `None`
/// until declared, so a later rule merges property-by-property instead of
/// replacing the whole box.
#[derive(Clone, Debug, Default)]
pub struct MarginBoxStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// Margin-box `width` — the VARIABLE dimension of a top/bottom margin box
    /// (css-page-3 §5.3.2). `None` = not declared, which the sizing algorithm
    /// treats as `auto`.
    pub width: Option<PageLength>,
    /// Margin-box `height` — the FIXED dimension of a top/bottom margin box
    /// (css-page-3 §5.3.3). `None` = not declared (`auto`).
    pub height: Option<PageLength>,
    pub margin_top: Option<PageLength>,
    pub margin_right: Option<PageLength>,
    pub margin_bottom: Option<PageLength>,
    pub margin_left: Option<PageLength>,
    pub padding_top: Option<PageLength>,
    pub padding_right: Option<PageLength>,
    pub padding_bottom: Option<PageLength>,
    pub padding_left: Option<PageLength>,
    /// Per-side border: width in points + colour. `None` = no border on that
    /// side (declared `none`/`hidden` or never declared).
    pub border_top: Option<(Scalar, Color)>,
    pub border_right: Option<(Scalar, Color)>,
    pub border_bottom: Option<(Scalar, Color)>,
    pub border_left: Option<(Scalar, Color)>,
    pub text_align: Option<crate::css::TextAlign>,
    pub vertical_align: Option<VerticalAlign>,
    pub font_family: Option<Vec<crate::fonts::FamilySpec>>,
    pub font_size: Option<Scalar>,
    pub font_weight: Option<f32>,
    pub font_style: Option<crate::css::FontStyle>,
    /// Margin-context `counter-reset` (css-page-3 §8). `None` = not declared.
    pub counter_reset: Option<CounterValue>,
    /// Margin-context `counter-increment`.
    pub counter_increment: Option<CounterValue>,
}

impl MarginBoxStyle {
    /// Copy every declaration set in `other` over `self` (later rules win).
    fn merge(&mut self, other: &MarginBoxStyle) {
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.background.is_some() {
            self.background = other.background;
        }
        // The box model (§5.3). Each `Option` is copied only when the later
        // rule declared that property, so a rule that sets just `width` keeps
        // the borders a previous rule established.
        macro_rules! merge_opt {
            ($($f:ident),* $(,)?) => {
                $(if other.$f.is_some() {
                    self.$f = other.$f.clone();
                })*
            };
        }
        merge_opt!(
            width,
            height,
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
            padding_top,
            padding_right,
            padding_bottom,
            padding_left,
            border_top,
            border_right,
            border_bottom,
            border_left,
        );
        if other.text_align.is_some() {
            self.text_align = other.text_align;
        }
        if other.vertical_align.is_some() {
            self.vertical_align = other.vertical_align;
        }
        if other.font_family.is_some() {
            self.font_family = other.font_family.clone();
        }
        if other.font_size.is_some() {
            self.font_size = other.font_size;
        }
        if other.font_weight.is_some() {
            self.font_weight = other.font_weight;
        }
        if other.font_style.is_some() {
            self.font_style = other.font_style;
        }
        if other.counter_reset.is_some() {
            self.counter_reset = other.counter_reset.clone();
        }
        if other.counter_increment.is_some() {
            self.counter_increment = other.counter_increment.clone();
        }
    }
}

/// One parsed margin-box declaration inside an `@page` rule.
///
/// `content` is three-state: `None` = no `content` declaration at all (a
/// lower-precedence rule's content is kept), `Some(None)` = `content: none |
/// normal` (explicitly suppresses the box, overriding a default page),
/// `Some(Some(pieces))` = generated content (an empty piece list is still
/// generated — `content: ""` paints nothing but occupies its box).
#[derive(Clone, Debug)]
pub struct MarginBoxDecl {
    pub name: MarginBoxName,
    pub content: Option<Option<Vec<ContentPiece>>>,
    pub style: MarginBoxStyle,
}

/// Page margins in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageMargins {
    pub top: Scalar,
    pub right: Scalar,
    pub bottom: Scalar,
    pub left: Scalar,
}

impl PageMargins {
    pub const fn zero() -> Self {
        PageMargins {
            top: Scalar::ZERO,
            right: Scalar::ZERO,
            bottom: Scalar::ZERO,
            left: Scalar::ZERO,
        }
    }
}

/// A `size` declaration inside an `@page` rule (css-page-3 §4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SizeDecl {
    /// An absolute size (width, height) in points; a single value is square.
    Abs(Scalar, Scalar),
    /// `size: portrait` — the default page size, oriented tall (width < height).
    Portrait,
    /// `size: landscape` — the default page size, oriented wide.
    Landscape,
}

/// The `page-orientation` property (css-page-3): how the laid-out content is
/// rotated within the page box. Parsed and carried on the spec; the PDF
/// emitter applies the rotation (see `pdf.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageOrientation {
    RotateLeft,
    RotateRight,
    RotateTop,
    RotateBottom,
}

/// One margin/size value inside an `@page` rule: lengths, percentages
/// (relative to the page box), and the `auto`/`inherit` keywords.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageLength {
    /// An absolute length in points.
    Abs(Scalar),
    /// A percentage as a 0..=1 fraction of the page box size (width for
    /// left/right, height for top/bottom).
    Percent(f64),
    /// `auto` — participates in page-area centering (css-page-3 §4.1).
    Auto,
    /// `inherit` — resolves to the initial value (0) in the page context.
    Inherit,
}

/// The parsed value of `counter-reset` / `counter-increment` in a page or
/// margin context (css-page-3 §8). `none` and `inherit` are keywords; a list
/// is `name [integer]` groups (default integer: 0 for reset, 1 for increment).
#[derive(Clone, Debug, PartialEq, Default)]
pub enum CounterValue {
    /// `none` — no counters reset / incremented.
    #[default]
    None,
    /// `inherit` — take the enclosing context's value (resolved at spec time).
    Inherit,
    /// An explicit `(name, value)` list, in declaration order.
    List(Vec<(String, i32)>),
}

/// One parsed `@page` rule.
#[derive(Clone, Debug)]
pub struct PageRule {
    /// The page name, or `None` for the default page.
    pub name: Option<String>,
    /// The pseudo-class selector.
    pub pseudo: PagePseudo,
    /// Explicit `size` (width, height) or an orientation keyword.
    pub size: Option<SizeDecl>,
    /// Page-area `width` override (css-page-3 §4.1 overconstrained sizing).
    pub width: Option<PageLength>,
    /// Page-area `height` override.
    pub height: Option<PageLength>,
    /// Individually-set margins (any subset), as parsed values.
    pub margin_top: Option<PageLength>,
    pub margin_right: Option<PageLength>,
    pub margin_bottom: Option<PageLength>,
    pub margin_left: Option<PageLength>,
    /// Logical margins (css-page-3 §5.1 / css-logical-1), kept SYMBOLIC:
    /// they map to physical edges at resolution time against the page
    /// context's effective writing mode (page-box-008/009 — inline/block
    /// axes swap under vertical-rl). Physical decls stay in the fields above.
    pub margin_inline_start: Option<PageLength>,
    pub margin_inline_end: Option<PageLength>,
    pub margin_block_start: Option<PageLength>,
    pub margin_block_end: Option<PageLength>,
    /// Page-box padding (css-page-3 §5.1 subset: shorthand + longhands).
    /// Percentages resolve against the page-box size (vertical ones against
    /// the HEIGHT, unlike regular boxes) at spec resolution time.
    pub padding_top: Option<PageLength>,
    pub padding_right: Option<PageLength>,
    pub padding_bottom: Option<PageLength>,
    pub padding_left: Option<PageLength>,
    /// Logical padding longhands, kept symbolic like the logical margins.
    pub padding_inline_start: Option<PageLength>,
    pub padding_inline_end: Option<PageLength>,
    pub padding_block_start: Option<PageLength>,
    pub padding_block_end: Option<PageLength>,
    /// `writing-mode` declared in the @page rule (css-page-3 §3.2). `None` =
    /// not declared → the page context inherits the ROOT element's mode.
    pub writing_mode: Option<crate::css::PageWritingMode>,
    /// Page-box border (uniform width + color; style keywords ignored).
    /// Paints between the margin area and the padding area.
    pub border_width: Option<Scalar>,
    /// `visibility: hidden` in the page context (css-page-3): hides the page
    /// box's own decorations (border) while document content stays visible.
    pub visibility_hidden: bool,
    pub border_color: Option<Color>,
    /// Page-box outline (css-ui-3 §2.2 + css-page-3): a stroked rect painted
    /// OUTSIDE the page border box (the page area) by `outline_offset`.
    /// Width/color come from the `outline` shorthand (style keywords ignored,
    /// like border); `outline-offset` is a separate longhand.
    pub outline_width: Option<Scalar>,
    pub outline_color: Option<Color>,
    pub outline_offset: Option<Scalar>,
    /// Page box background color (paints the whole page, under content).
    pub background: Option<Color>,
    /// `page-orientation` — how the content rotates within the page box.
    pub page_orientation: Option<PageOrientation>,
    /// Page-context `counter-reset` (css-page-3 §8). `None` = not declared.
    pub counter_reset: Option<CounterValue>,
    /// Page-context `counter-increment`.
    pub counter_increment: Option<CounterValue>,
    /// Margin-box declarations, in source order.
    pub margin_boxes: Vec<MarginBoxDecl>,
    /// Page-context inherited style (css-page-3 §6): these inherit into the
    /// page-margin boxes when the margin box declares nothing itself.
    pub color: Option<Color>,
    pub font_family: Option<Vec<crate::fonts::FamilySpec>>,
    pub font_size: Option<Scalar>,
    pub font_weight: Option<f32>,
    pub font_style: Option<crate::css::FontStyle>,
    /// Source order, so later equal-specificity rules win.
    order: u32,
    /// Cascade-layer rank (css-cascade-5 §6): 0 = unlayered (beats every
    /// layer), else 1 + the layer's position in the layer order. `@layer a, b`
    /// (statement or block nesting) assigns a=1, b=2; later layers win over
    /// earlier ones regardless of source position.
    layer: u32,
}

/// One page-margin box in the winning page spec: generated content plus the
/// resolved style (page-context inheritance + UA defaults already applied).
#[derive(Clone, Debug)]
pub struct MarginBoxSpec {
    pub name: MarginBoxName,
    /// The generated `content` pieces (an empty list is still generated —
    /// `content: ""` paints nothing but occupies its box).
    pub content: Vec<ContentPiece>,
    pub color: Color,
    pub background: Option<Color>,
    pub font_size: Scalar,
    pub font_face: crate::fonts::FaceId,
    pub font_fallbacks: Vec<crate::fonts::FaceId>,
    pub line_height: Scalar,
    pub text_align: crate::css::TextAlign,
    pub vertical_align: VerticalAlign,
    /// The box model this margin box builds inside the page margin area
    /// (css-page-3 §5.3). `None` on a length = `auto` for the sizing
    /// algorithms; `None` on a border side = no border.
    pub width: Option<PageLength>,
    pub height: Option<PageLength>,
    pub margin_top: Option<PageLength>,
    pub margin_right: Option<PageLength>,
    pub margin_bottom: Option<PageLength>,
    pub margin_left: Option<PageLength>,
    pub padding_top: Option<PageLength>,
    pub padding_right: Option<PageLength>,
    pub padding_bottom: Option<PageLength>,
    pub padding_left: Option<PageLength>,
    pub border_top: Option<(Scalar, Color)>,
    pub border_right: Option<(Scalar, Color)>,
    pub border_bottom: Option<(Scalar, Color)>,
    pub border_left: Option<(Scalar, Color)>,
    /// Resolved margin-context `counter-reset` (`inherit` already resolved
    /// against the page context). `None` = the box does not obscure.
    pub counter_reset: CounterValue,
    /// Resolved margin-context `counter-increment`.
    pub counter_increment: CounterValue,
}

/// A fully-resolved page spec for one fragmentainer: geometry plus the margin
/// boxes to draw, with content still symbolic (resolved at build time).
#[derive(Clone, Debug)]
pub struct PageSpec {
    pub size: (Scalar, Scalar),
    pub margins: PageMargins,
    /// Page-box padding, resolved to points against the final page size
    /// (vertical percentages against the page HEIGHT, css-page-3).
    pub padding: PageMargins,
    /// Page-box border: uniform width (points) + color. `None` width = none.
    pub border: Option<(Scalar, Color)>,
    /// `@page { visibility: hidden }` — page box decorations don't paint
    /// (css-page-3); the border still insets content (visibility is
    /// paint-only).
    pub visibility_hidden: bool,
    /// Page-box outline: (width, color, offset) in points. `None` = no
    /// outline. Painted outside the page area by `offset` (css-ui-3 §2.2).
    pub outline: Option<(Scalar, Color, Scalar)>,
    pub background: Option<Color>,
    pub page_orientation: Option<PageOrientation>,
    pub margin_boxes: Vec<MarginBoxSpec>,
    /// Page-context `counter-reset` (resolved from the winning @page rules).
    pub counter_reset: CounterValue,
    /// Page-context `counter-increment`.
    pub counter_increment: CounterValue,
}

impl PageSpec {
    /// The page geometry this spec implies (for `content_rect`).
    pub fn geometry(&self) -> PageGeometry {
        PageGeometry {
            width: self.size.0,
            height: self.size.1,
            margin_top: self.margins.top,
            margin_right: self.margins.right,
            margin_bottom: self.margins.bottom,
            margin_left: self.margins.left,
        }
    }
}

/// Running-string state: named string -> its current value, threaded in
/// document order. Kept as a sorted `Vec` (not a `HashMap`) so any iteration is
/// deterministic.
#[derive(Clone, Debug, Default)]
pub struct RunningStrings {
    entries: Vec<(String, String)>,
}

impl RunningStrings {
    pub fn new() -> RunningStrings {
        RunningStrings {
            entries: Vec::new(),
        }
    }

    /// Assign `value` to the named string (the last assignment on a page wins).
    pub fn set(&mut self, name: &str, value: String) {
        match self.entries.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value,
            None => self.entries.push((name.to_string(), value)),
        }
    }

    /// The current value of the named string, or empty before any assignment.
    pub fn get(&self, name: &str) -> &str {
        self.entries
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

// --- @page parsing ---------------------------------------------------------

/// Parse every `@page` rule out of the author stylesheet. Unknown declarations
/// and unparsable blocks are skipped without failing.
pub fn parse_page_rules(css: &str) -> Vec<PageRule> {
    let css = strip_comments(css);
    let mut rules = Vec::new();
    let mut order = 0u32;

    // --- Cascade-layer pre-pass (css-cascade-5 §6, layers-002..004) -------
    // Pass 1 registers the layer order from `@layer a, b;` statements and
    // `@layer a {}` block preludes (first mention fixes the rank: a=1, b=2;
    // a later layer beats an earlier one regardless of source position;
    // unlayered = rank 0 beats every layer). The scan continues INSIDE layer
    // blocks so nested `@layer` declarations register too.
    let mut layer_rank: Vec<String> = Vec::new();
    {
        let mut i = 0;
        while i < css.len() {
            if css[i..].starts_with("@layer") {
                let after = i + "@layer".len();
                let next = css.as_bytes().get(after).copied().unwrap_or(b' ');
                // A name boundary must follow (@layerfoo is a different
                // at-keyword).
                if next.is_ascii_whitespace() || next == b';' || next == b'{' {
                    if let Some(rel) = css[after..].find(|c| c == ';' || c == '{') {
                        let stop = after + rel;
                        // Register every comma-separated prelude name.
                        for part in css[after..stop].split(',') {
                            let name = part.trim();
                            if !name.is_empty() && !layer_rank.iter().any(|l| l == name) {
                                layer_rank.push(name.to_string());
                            }
                        }
                        i = if css.as_bytes()[stop] == b';' {
                            stop + 1
                        } else {
                            // Block form: keep scanning the interior (nested
                            // @layer declarations register here too).
                            stop + 1
                        };
                        continue;
                    }
                }
            }
            let ch = css[i..].chars().next().expect("non-empty suffix");
            i += ch.len_utf8();
        }
    }

    // --- Layer-span map: (rule_start, rule_end, layer_rank) per @page ------
    // Pass 2 walks the original text with a brace-marker stack: every `{`
    // pushes a marker (`Some(previous_layer)` for a @layer block, `None` for
    // any other block); every `}` pops one and restores the layer when it
    // closed a @layer block. Skipped at-rules jump past their whole block, so
    // their braces never touch the stack.
    let mut spans: Vec<(usize, usize, u32)> = Vec::new();
    {
        let mut i = 0;
        let mut current_layer: u32 = 0;
        let mut markers: Vec<Option<u32>> = Vec::new();
        while i < css.len() {
            if css[i..].starts_with("@layer") {
                let after = i + "@layer".len();
                let next = css.as_bytes().get(after).copied().unwrap_or(b' ');
                if next.is_ascii_whitespace() || next == b';' || next == b'{' {
                    match css[after..].find(|c| c == ';' || c == '{') {
                        Some(rel) if css.as_bytes()[after + rel] == b';' => {
                            i = after + rel + 1;
                            continue;
                        }
                        Some(rel) => {
                            let brace = after + rel;
                            // Block form: `@layer a, b { body }` nests as
                            // a{ b{ body } } — the body belongs to the LAST
                            // prelude name.
                            let owner = css[after..brace]
                                .split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .last()
                                .unwrap_or("");
                            let rank = layer_rank
                                .iter()
                                .position(|l| l == owner)
                                .map(|p| (p + 1) as u32)
                                .unwrap_or(0);
                            markers.push(Some(current_layer));
                            current_layer = rank;
                            i = brace + 1;
                            continue;
                        }
                        None => break,
                    }
                }
            }
            match css.as_bytes()[i] {
                b'@' => {
                    // @page records its span at the current layer; any other
                    // at-rule is skipped wholesale (its braces never touch
                    // the stack).
                    if css[i..].starts_with("@page") {
                        if let Some(rest) = css[i..].find('{') {
                            let brace = i + rest;
                            if let Some(block_end) = matching_brace(&css, brace) {
                                spans.push((i, block_end + 1, current_layer));
                                i = block_end + 1;
                                continue;
                            }
                        }
                    } else if let Some(rest) = css[i..].find('{') {
                        let brace = i + rest;
                        if let Some(block_end) = matching_brace(&css, brace) {
                            i = block_end + 1;
                            continue;
                        }
                    }
                    i += 1;
                }
                b'{' => {
                    markers.push(None);
                    i += 1;
                }
                b'}' => {
                    if let Some(marker) = markers.pop() {
                        if let Some(prev) = marker {
                            current_layer = prev;
                        }
                    }
                    i += 1;
                }
                _ => {
                    // Advance by the WHOLE char: byte-wise `+= 1` would land
                    // mid-character inside multi-byte text (Trøndere's 'ø',
                    // content-002-ref) and the next `css[i..]` slice panics.
                    let ch = css[i..].chars().next().expect("non-empty suffix");
                    i += ch.len_utf8();
                }
            }
        }
    }

    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next `@page` at-keyword.
        let Some(at) = css[i..].find("@page") else {
            break;
        };
        let start = i + at;
        // The prelude runs from just after `@page` to the opening brace.
        let after_kw = start + "@page".len();
        let Some(brace_rel) = css[after_kw..].find('{') else {
            break;
        };
        let brace = after_kw + brace_rel;
        let prelude = css[after_kw..brace].trim();
        // Match the block braces (margin boxes nest one level of braces).
        let Some(block_end) = matching_brace(&css, brace) else {
            break;
        };
        let body = &css[brace + 1..block_end];

        // Layer rank of this rule, from the span map (identity by range).
        let layer = spans
            .iter()
            .find(|(s, e, _)| *s == start && *e == block_end + 1)
            .map(|(_, _, l)| *l)
            .unwrap_or(0);

        if let Some(mut rule) = parse_one_page_rule(prelude, body, order) {
            rule.layer = layer;
            rules.push(rule);
        }
        order += 1;
        i = block_end + 1;
    }
    rules
}

/// Parse the `@page` prelude (`landscape :first`, `:left`, etc.) into
/// (name, pseudo).
fn parse_prelude(prelude: &str) -> (Option<String>, PagePseudo) {
    let mut name = None;
    let mut pseudo = PagePseudo::None;
    for tok in prelude.split_whitespace() {
        // A named page with an attached pseudo (`a:first`, css-page-3 §3.1
        // page selector syntax — no whitespace required between name and
        // pseudo) must split; the whitespace loop alone turns `a:first`
        // into a page literally named "a:first" (pseudo-first-margin-002).
        // The colon never appears inside a page NAME, so the first colon
        // unambiguously starts the pseudo.
        let (name_part, pseudo_part) = match tok.split_once(':') {
            Some((n, p)) => (n, Some(p)),
            None => (tok, None),
        };
        if let Some(p) = pseudo_part {
            pseudo = match p.to_ascii_lowercase().as_str() {
                "first" => PagePseudo::First,
                "left" => PagePseudo::Left,
                "right" => PagePseudo::Right,
                _ => pseudo,
            };
        }
        if !name_part.is_empty() {
            name = Some(name_part.to_string());
        }
    }
    (name, pseudo)
}

fn parse_one_page_rule(prelude: &str, body: &str, order: u32) -> Option<PageRule> {
    let (name, pseudo) = parse_prelude(prelude);
    let mut rule = PageRule {
        name,
        pseudo,
        layer: 0,
        size: None,
        width: None,
        height: None,
        margin_top: None,
        margin_right: None,
        margin_bottom: None,
        margin_left: None,
        margin_inline_start: None,
        margin_inline_end: None,
        margin_block_start: None,
        margin_block_end: None,
        padding_top: None,
        padding_right: None,
        padding_bottom: None,
        padding_left: None,
        padding_inline_start: None,
        padding_inline_end: None,
        padding_block_start: None,
        padding_block_end: None,
        writing_mode: None,
        border_width: None,
        visibility_hidden: false,
        border_color: None,
        outline_width: None,
        outline_color: None,
        outline_offset: None,
        background: None,
        page_orientation: None,
        counter_reset: None,
        counter_increment: None,
        margin_boxes: Vec::new(),
        color: None,
        font_family: None,
        font_size: None,
        font_weight: None,
        font_style: None,
        order,
    };

    // Walk the body, extracting flat `prop: value;` declarations and nested
    // `@margin-box { ... }` sub-rules. Char-safe iteration (CORE-83): a
    // byte-wise loop would slice `body` mid-codepoint and mangle any
    // multi-byte UTF-8 in the declarations.
    let mut i = 0;
    let mut decl = String::new();
    let mut char_iter = body.char_indices();
    while let Some((ci, c)) = char_iter.next() {
        i = ci;
        if c == '@' {
            // A margin-box sub-rule. Read its keyword up to '{'.
            let Some(brace_rel) = body[i..].find('{') else {
                break;
            };
            let brace = i + brace_rel;
            let kw = body[i + 1..brace].trim();
            let Some(end) = matching_brace(body, brace) else {
                break;
            };
            let inner = &body[brace + 1..end];
            if let Some(name) = MarginBoxName::parse(kw) {
                let (content, style) = parse_margin_box_decls(inner);
                rule.margin_boxes.push(MarginBoxDecl {
                    name,
                    content,
                    style,
                });
            }
            decl.clear();
            // Skip past the sub-rule body (the char iterator is at `ci`, so
            // advance it to just past `end`).
            while let Some((nci, _)) = char_iter.next() {
                if nci >= end {
                    break;
                }
                let _ = nci;
            }
            continue;
        }
        if c == ';' {
            apply_page_decl(&mut rule, &decl);
            decl.clear();
        } else {
            decl.push(c);
        }
    }
    apply_page_decl(&mut rule, &decl);

    Some(rule)
}

/// Parse one margin-box body into `(content, style)`.
///
/// A margin box's declarations follow the CSS2.1 margin-context property list
/// (css-page-3 Appendix A): the inherited text properties (`color`/
/// `text-align`/`vertical-align`/`font-*`), `background`, and `content`. Any
/// other property is ignored (which is what `inapplicable-properties-print`
/// asserts). Later declarations in the same block win.
///
/// `content` is three-state (see [`MarginBoxDecl`]): `None` when there is no
/// `content` declaration, `Some(None)` for `content: none | normal`, and
/// `Some(Some(pieces))` otherwise.
fn parse_margin_box_decls(body: &str) -> (Option<Option<Vec<ContentPiece>>>, MarginBoxStyle) {
    let mut content = None;
    let mut style = MarginBoxStyle::default();
    for decl in body.split(';') {
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match prop.as_str() {
            "content" => {
                if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("normal") {
                    content = Some(None);
                } else {
                    content = Some(Some(parse_content(value)));
                }
            }
            "color" => {
                if let Some(c) = crate::css::parse_css_color(value) {
                    style.color = Some(c);
                }
            }
            "background" | "background-color" => {
                if let Some(c) = crate::css::parse_css_color(value) {
                    style.background = Some(c);
                }
            }
            // Margin-box box model (css-page-3 §5.3): the box that the margin
            // box's own `margin`/`border`/`padding`/`size` declarations build
            // inside the page margin area. `width` is the variable dimension of
            // a top/bottom box, `height` its fixed dimension (§5.3.2, §5.3.3).
            "width" => style.width = parse_page_length(value),
            "height" => style.height = parse_page_length(value),
            "margin" => {
                if let Some((t, r, b, l)) = parse_margin_shorthand(value) {
                    style.margin_top = Some(t);
                    style.margin_right = Some(r);
                    style.margin_bottom = Some(b);
                    style.margin_left = Some(l);
                }
            }
            "margin-top" => style.margin_top = parse_page_length(value),
            "margin-right" => style.margin_right = parse_page_length(value),
            "margin-bottom" => style.margin_bottom = parse_page_length(value),
            "margin-left" => style.margin_left = parse_page_length(value),
            "padding" | "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                apply_padding_decl(&mut style, &prop, value);
            }
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                if let Some(side) = parse_border_side(value) {
                    match prop.as_str() {
                        "border" => {
                            style.border_top = Some(side);
                            style.border_right = Some(side);
                            style.border_bottom = Some(side);
                            style.border_left = Some(side);
                        }
                        "border-top" => style.border_top = Some(side),
                        "border-right" => style.border_right = Some(side),
                        "border-bottom" => style.border_bottom = Some(side),
                        _ => style.border_left = Some(side),
                    }
                }
            }
            "text-align" => style.text_align = parse_text_align(value),
            "vertical-align" => style.vertical_align = parse_vertical_align(value),
            "font-family" => style.font_family = Some(parse_family_list(value)),
            "font-size" => style.font_size = parse_page_length(value).and_then(page_length_abs),
            "font-weight" => style.font_weight = parse_font_weight(value),
            "font-style" => style.font_style = parse_font_style(value),
            "font" => apply_font_shorthand(&mut style, value),
            // Margin-context counters (css-page-3 §8). `inherit` is resolved
            // against the page context at spec-resolution time.
            "counter-reset" => style.counter_reset = Some(parse_counter_value(value, 0)),
            "counter-increment" => style.counter_increment = Some(parse_counter_value(value, 1)),
            _ => {}
        }
    }
    (content, style)
}

/// The absolute point value of a resolved [`PageLength`], if it has one.
fn page_length_abs(l: PageLength) -> Option<Scalar> {
    match l {
        PageLength::Abs(v) => Some(v),
        _ => None,
    }
}

/// Parse a `text-align` value into the engine's computed enum. Unknown values
/// (including the margin-context-inapplicable `justify`) are ignored.
fn parse_text_align(value: &str) -> Option<crate::css::TextAlign> {
    use crate::css::TextAlign;
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => TextAlign::Left,
        "right" | "end" => TextAlign::Right,
        "center" => TextAlign::Center,
        _ => return None,
    })
}

/// Parse a margin-context `vertical-align`: only `top | middle | bottom` are
/// valid there (css-page-3 §6.2); everything else is invalid and ignored.
fn parse_vertical_align(value: &str) -> Option<VerticalAlign> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "top" => VerticalAlign::Top,
        "middle" | "center" => VerticalAlign::Middle,
        "bottom" => VerticalAlign::Bottom,
        _ => return None,
    })
}

/// Parse a `font-style` value (oblique folds into italic).
fn parse_font_style(value: &str) -> Option<crate::css::FontStyle> {
    match value.trim().to_ascii_lowercase().as_str() {
        "italic" | "oblique" => Some(crate::css::FontStyle::Italic),
        "normal" => Some(crate::css::FontStyle::Normal),
        _ => None,
    }
}

/// Parse a `font-weight` value: the numeric keywords plus `normal`/`bold`.
fn parse_font_weight(value: &str) -> Option<f32> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(400.0),
        "bold" => Some(700.0),
        "bolder" => Some(700.0),
        "lighter" => Some(300.0),
        other => other.parse::<f32>().ok(),
    }
}

/// Parse a comma-separated `font-family` list into registry [`FamilySpec`]s.
/// Generic keywords map to their generic specs; everything else is a family
/// name (quotes stripped).
fn parse_family_list(value: &str) -> Vec<crate::fonts::FamilySpec> {
    use crate::fonts::FamilySpec;
    value
        .split(',')
        .map(|part| {
            let name = part.trim().trim_matches(['"', '\'']).trim();
            match name.to_ascii_lowercase().as_str() {
                "serif" => FamilySpec::Serif,
                "sans-serif" => FamilySpec::SansSerif,
                "monospace" => FamilySpec::Monospace,
                "cursive" => FamilySpec::Cursive,
                "fantasy" => FamilySpec::Fantasy,
                _ => FamilySpec::Name(name.to_string()),
            }
        })
        .collect()
}

/// Apply the `font` shorthand (css-fonts-4 §4): the supported subset is the
/// `[style] [weight] size[/line-height] family` form. A value beginning with
/// a system-font keyword is ignored (unsupported, and must not clobber).
fn apply_font_shorthand(style: &mut MarginBoxStyle, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let mut rest = value;
    // Leading style/weight tokens, in any order, before the size.
    loop {
        let Some((tok, tail)) = rest.split_once(char::is_whitespace) else {
            return;
        };
        let lower = tok.to_ascii_lowercase();
        if let Some(fs) = parse_font_style(tok) {
            style.font_style = Some(fs);
            rest = tail.trim_start();
        } else if lower == "normal" {
            rest = tail.trim_start();
        } else if let Some(w) = parse_font_weight(tok) {
            style.font_weight = Some(w);
            rest = tail.trim_start();
        } else {
            break;
        }
    }
    // `size[/line-height] family...`
    let (size_tok, family) = match rest.split_once(char::is_whitespace) {
        Some((s, f)) => (s, f.trim()),
        None => return,
    };
    if let Some((sz, _lh)) = size_tok.split_once('/') {
        if let Some(abs) = parse_page_length(sz).and_then(page_length_abs) {
            style.font_size = Some(abs);
        }
    } else if let Some(abs) = parse_page_length(size_tok).and_then(page_length_abs) {
        style.font_size = Some(abs);
    }
    if !family.is_empty() {
        style.font_family = Some(parse_family_list(family));
    }
}

/// Apply one `prop: value` declaration to a page rule.
fn apply_page_decl(rule: &mut PageRule, decl: &str) {
    let Some((prop, value)) = decl.split_once(':') else {
        return;
    };
    let prop = prop.trim().to_ascii_lowercase();
    let value = value.trim();
    match prop.as_str() {
        "size" => rule.size = parse_size(value),
        // Page-context visibility (css-page-3): `hidden` hides the page box's
        // decorations; document content remains visible (page-visibility-
        // hidden-001).
        "visibility" => rule.visibility_hidden = value.trim().eq_ignore_ascii_case("hidden"),
        "width" => rule.width = parse_page_length(value),
        "height" => rule.height = parse_page_length(value),
        "margin" => {
            if let Some((t, r, b, l)) = parse_margin_shorthand(value) {
                rule.margin_top = Some(t);
                rule.margin_right = Some(r);
                rule.margin_bottom = Some(b);
                rule.margin_left = Some(l);
            }
        }
        "margin-top" => rule.margin_top = parse_page_length(value),
        "margin-right" => rule.margin_right = parse_page_length(value),
        "margin-bottom" => rule.margin_bottom = parse_page_length(value),
        "margin-left" => rule.margin_left = parse_page_length(value),
        // Logical margins in page context (css-page-3 §5.1, css-logical-1),
        // kept SYMBOLIC: they map to physical edges in `resolve_page_spec`
        // against the page context's effective writing mode (the page's own
        // `writing-mode` decl, else inherited from the root element —
        // page-box-008/009). The two-value `margin-inline`/`margin-block`
        // shorthands are NOT handled (single-value parser); the four
        // longhands cover the WPT fixtures.
        "margin-inline-start" => rule.margin_inline_start = parse_page_length(value),
        "margin-inline-end" => rule.margin_inline_end = parse_page_length(value),
        "margin-block-start" => rule.margin_block_start = parse_page_length(value),
        "margin-block-end" => rule.margin_block_end = parse_page_length(value),
        "writing-mode" => {
            rule.writing_mode = match value.trim().to_ascii_lowercase().as_str() {
                "horizontal-tb" => Some(crate::css::PageWritingMode::HorizontalTb),
                "vertical-rl" => Some(crate::css::PageWritingMode::VerticalRl),
                "vertical-lr" => Some(crate::css::PageWritingMode::VerticalLr),
                _ => None,
            };
        }
        // Page-box padding (CORE-144): shorthand + longhands. Percentages
        // stay symbolic; `resolve_page_spec` applies them against the page
        // box (vertical ones against the page HEIGHT, css-page-3 §5.1 — the
        // page context is NOT a regular box).
        "padding" => {
            if let Some((t, r, b, l)) = parse_margin_shorthand(value) {
                rule.padding_top = Some(t);
                rule.padding_right = Some(r);
                rule.padding_bottom = Some(b);
                rule.padding_left = Some(l);
            }
        }
        "padding-top" => rule.padding_top = parse_page_length(value),
        "padding-right" => rule.padding_right = parse_page_length(value),
        "padding-bottom" => rule.padding_bottom = parse_page_length(value),
        "padding-left" => rule.padding_left = parse_page_length(value),
        "padding-inline-start" => rule.padding_inline_start = parse_page_length(value),
        "padding-inline-end" => rule.padding_inline_end = parse_page_length(value),
        "padding-block-start" => rule.padding_block_start = parse_page_length(value),
        "padding-block-end" => rule.padding_block_end = parse_page_length(value),
        // Page-box border (CORE-144): uniform width + color; style keywords
        // and widths like `thin`/`medium` parse to a fallback width.
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            // css-backgrounds-3 §4.5: an omitted width means `medium`
            // (3px = 2.25pt) — `border: solid` paints a medium band
            // (page-margin-auto-and-non-zero). `none`/`hidden` zero it.
            let mut width: Option<Scalar> = None;
            let mut color: Option<crate::css::Color> = None;
            let mut saw_style = false;
            for tok in value.split_whitespace() {
                if let Some(c) = crate::css::parse_css_color(tok) {
                    color = Some(c);
                } else if let Ok(px) = tok.trim_end_matches("px").parse::<f64>() {
                    width = Some(crate::geom::Scalar(px * 0.75));
                } else {
                    let lower = tok.to_ascii_lowercase();
                    if lower == "none" || lower == "hidden" {
                        width = Some(Scalar::ZERO);
                    } else {
                        saw_style = true;
                    }
                }
            }
            let w = width.unwrap_or_else(|| {
                if saw_style || color.is_some() {
                    crate::geom::Scalar(2.25)
                } else {
                    Scalar::ZERO
                }
            });
            if w.get() > 0.0 {
                rule.border_width = Some(w);
                if let Some(c) = color {
                    rule.border_color = Some(c);
                }
            }
        }
        // Page-box outline (CORE-153): `outline` shorthand (width + color,
        // style keywords ignored like border) plus the separate
        // `outline-offset` longhand. `outline: none` suppresses the ring.
        "outline" => {
            if value.eq_ignore_ascii_case("none") {
                rule.outline_width = Some(Scalar::ZERO);
            } else {
                for tok in value.split_whitespace() {
                    if let Some(c) = crate::css::parse_css_color(tok) {
                        rule.outline_color = Some(c);
                    } else if let Some(l) = parse_length(tok) {
                        rule.outline_width = Some(l);
                    }
                }
            }
        }
        "outline-width" => rule.outline_width = parse_length(value),
        "outline-color" => {
            if let Some(c) = crate::css::parse_css_color(value) {
                rule.outline_color = Some(c);
            }
        }
        "outline-offset" => rule.outline_offset = parse_length(value),
        // An invalid color value is NOT assigned: css-syntax drops invalid
        // declarations at parse time, so a later bad `background` must not
        // clobber an earlier valid one (cascade fallback, CORE-153).
        "background" | "background-color" => {
            if let Some(c) = crate::css::parse_css_color(value) {
                rule.background = Some(c);
            }
        }
        "page-orientation" => rule.page_orientation = parse_page_orientation(value),
        // Page-context counters (css-page-3 §8). `inherit` inherits from the
        // root element, which the page parser cannot see; resolve to `None`
        // (no reset/increment) — same as the css-wide `none` initial.
        "counter-reset" => {
            rule.counter_reset = Some(parse_counter_value(value, 0));
        }
        "counter-increment" => {
            rule.counter_increment = Some(parse_counter_value(value, 1));
        }
        // Page-context style that INHERITS into the margin boxes
        // (css-page-3 §6: "properties that apply to the page-margin boxes can
        // also be set within the page context; if inheritable ... they
        // inherit"). `alignment-001` pins monospace 0.7em + blue this way.
        "color" => {
            if let Some(c) = crate::css::parse_css_color(value) {
                rule.color = Some(c);
            }
        }
        "font-family" => rule.font_family = Some(parse_family_list(value)),
        "font-size" => rule.font_size = parse_page_length(value).and_then(page_length_abs),
        "font-weight" => rule.font_weight = parse_font_weight(value),
        "font-style" => rule.font_style = parse_font_style(value),
        "font" => {
            let mut s = MarginBoxStyle {
                font_family: rule.font_family.clone(),
                font_size: rule.font_size,
                font_weight: rule.font_weight,
                font_style: rule.font_style,
                ..Default::default()
            };
            apply_font_shorthand(&mut s, value);
            rule.font_family = s.font_family;
            rule.font_size = s.font_size;
            rule.font_weight = s.font_weight;
            rule.font_style = s.font_style;
        }
        _ => {}
    }
}

/// Parse a `size` value: css-page size keywords, orientation keywords
/// (`landscape`/`portrait`), one length (square), or two lengths
/// (width height). A keyword + orientation pair orients the keyword's size.
fn parse_size(value: &str) -> Option<SizeDecl> {
    let v = value.trim();
    let parts: Vec<&str> = v.split_whitespace().collect();
    let mut lens: Vec<Scalar> = Vec::new();
    let mut orient: Option<SizeDecl> = None;
    for p in &parts {
        match p.to_ascii_lowercase().as_str() {
            "landscape" => orient = Some(SizeDecl::Landscape),
            "portrait" => orient = Some(SizeDecl::Portrait),
            // Predefined page sizes (css-page-3 §6.3). Values are computed
            // with the SAME mm→pt formula as `parse_length` (mm * 72 / 25.4)
            // so `size: a5` renders byte-identically to `size: 148mm 210mm`
            // (the WPT references spell the keywords out as mm).
            "a5" => lens = vec![Scalar(148.0 * 72.0 / 25.4), Scalar(210.0 * 72.0 / 25.4)],
            "a4" => lens = vec![Scalar(210.0 * 72.0 / 25.4), Scalar(297.0 * 72.0 / 25.4)],
            "a3" => lens = vec![Scalar(297.0 * 72.0 / 25.4), Scalar(420.0 * 72.0 / 25.4)],
            "b5" => lens = vec![Scalar(176.0 * 72.0 / 25.4), Scalar(250.0 * 72.0 / 25.4)],
            "b4" => lens = vec![Scalar(250.0 * 72.0 / 25.4), Scalar(353.0 * 72.0 / 25.4)],
            "jis-b5" => lens = vec![Scalar(182.0 * 72.0 / 25.4), Scalar(257.0 * 72.0 / 25.4)],
            "jis-b4" => lens = vec![Scalar(257.0 * 72.0 / 25.4), Scalar(364.0 * 72.0 / 25.4)],
            "letter" => lens = vec![Scalar(8.5 * 72.0), Scalar(11.0 * 72.0)],
            "legal" => lens = vec![Scalar(8.5 * 72.0), Scalar(14.0 * 72.0)],
            "ledger" => lens = vec![Scalar(11.0 * 72.0), Scalar(17.0 * 72.0)],
            _ => lens.push(parse_length(p)?),
        }
    }
    let abs = match lens.as_slice() {
        [] => None,
        [s] => Some((*s, *s)),
        [w, h] => Some((*w, *h)),
        _ => return None,
    };
    match (abs, orient) {
        (None, o) => o,
        (Some((w, h)), None) => Some(SizeDecl::Abs(w, h)),
        (Some((w, h)), Some(SizeDecl::Landscape)) => {
            Some(SizeDecl::Abs(Scalar(w.get().max(h.get())), Scalar(w.get().min(h.get()))))
        }
        (Some((w, h)), Some(SizeDecl::Portrait)) => {
            Some(SizeDecl::Abs(Scalar(w.get().min(h.get())), Scalar(w.get().max(h.get()))))
        }
        _ => None,
    }
}

/// Parse the `page-orientation` property.
fn parse_page_orientation(value: &str) -> Option<PageOrientation> {
    match value.trim().to_ascii_lowercase().as_str() {
        "rotate-left" => Some(PageOrientation::RotateLeft),
        "rotate-right" => Some(PageOrientation::RotateRight),
        "rotate-top" => Some(PageOrientation::RotateTop),
        "rotate-bottom" => Some(PageOrientation::RotateBottom),
        _ => None,
    }
}

/// Parse a CSS `margin` shorthand (1–4 values) into `(top, right, bottom, left)`
/// as [`PageLength`] values (lengths, percentages, `auto`, `inherit`).
fn parse_margin_shorthand(value: &str) -> Option<(PageLength, PageLength, PageLength, PageLength)> {
    let parts: Vec<PageLength> = value.split_whitespace().filter_map(parse_page_length).collect();
    match parts.as_slice() {
        [a] => Some((*a, *a, *a, *a)),
        [v, h] => Some((*v, *h, *v, *h)),
        [t, h, b] => Some((*t, *h, *b, *h)),
        [t, r, b, l] => Some((*t, *r, *b, *l)),
        _ => None,
    }
}

/// Apply a `padding` / `padding-<side>` declaration to a margin box.
///
/// `auto` is not a valid padding value; such a declaration is ignored rather
/// than treated as zero, so a bogus value cannot silently clear a real one.
fn apply_padding_decl(style: &mut MarginBoxStyle, prop: &str, value: &str) {
    fn one(value: &str) -> Option<PageLength> {
        match parse_page_length(value) {
            Some(PageLength::Abs(p)) => Some(PageLength::Abs(p)),
            Some(PageLength::Percent(f)) => Some(PageLength::Percent(f)),
            _ => None,
        }
    }
    let mut assign = |which: &str, v: Option<PageLength>| {
        let Some(v) = v else { return };
        match which {
            "padding-top" => style.padding_top = Some(v),
            "padding-right" => style.padding_right = Some(v),
            "padding-bottom" => style.padding_bottom = Some(v),
            _ => style.padding_left = Some(v),
        }
    };
    if prop != "padding" {
        assign(prop, one(value));
        return;
    }
    let Some((t, r, b, l)) = parse_margin_shorthand(value) else {
        return;
    };
    let vals: [(PageLength, &str); 4] = [
        (t, "padding-top"),
        (r, "padding-right"),
        (b, "padding-bottom"),
        (l, "padding-left"),
    ];
    for (v, which) in vals {
        let v = match v {
            PageLength::Abs(_) | PageLength::Percent(_) => Some(v),
            _ => None,
        };
        assign(which, v);
    }
}

/// The `medium` border width (css-backgrounds-3 §4.3) in points: 3px.
const MEDIUM_BORDER: Scalar = Scalar(2.25);

/// Parse one `border` / `border-<side>` value into `(width, colour)`.
///
/// Only the visible styles are honoured (this engine strokes every style as a
/// solid band, the CORE-165 model). `none` / `hidden` return a zero-width
/// border so an explicit declaration CLEARS an earlier one — `None` means
/// "nothing declared".
fn parse_border_side(value: &str) -> Option<(Scalar, Color)> {
    let mut width: Option<Scalar> = None;
    let mut color: Option<Color> = None;
    let mut visible = false;
    for part in value.split_whitespace() {
        match part.to_ascii_lowercase().as_str() {
            "none" | "hidden" => return Some((Scalar::ZERO, Color::BLACK)),
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset"
            | "outset" => visible = true,
            "thin" => width = Some(Scalar(1.0)),
            "medium" => width = Some(MEDIUM_BORDER),
            "thick" => width = Some(Scalar(5.0)),
            _ => {
                if let Some(w) = parse_length(part) {
                    width = Some(w);
                } else if let Some(c) = crate::css::parse_css_color(part) {
                    color = Some(c);
                }
            }
        }
    }
    if !visible {
        // `border: 2px` alone leaves the initial `border-style: none`.
        return None;
    }
    let w = width.unwrap_or(MEDIUM_BORDER);
    if w.get() <= 0.0 {
        return Some((Scalar::ZERO, Color::BLACK));
    }
    Some((w, color.unwrap_or(Color::BLACK)))
}

/// Parse one margin/size value: `auto`, `inherit`, a percentage, or an
/// absolute length.
fn parse_page_length(s: &str) -> Option<PageLength> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("auto") {
        return Some(PageLength::Auto);
    }
    if s.eq_ignore_ascii_case("inherit") {
        return Some(PageLength::Inherit);
    }
    if let Some(pct) = s.strip_suffix('%') {
        let n: f64 = pct.trim().parse().ok()?;
        return Some(PageLength::Percent(n / 100.0));
    }
    parse_length(s).map(PageLength::Abs)
}

/// Parse a CSS absolute length (`in`/`pt`/`px`/`cm`/`mm`/`em`) into points.
/// `em` resolves against the page context's default font-size (16px = 12pt,
/// css-page-3 §3).
pub fn parse_length(s: &str) -> Option<Scalar> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let end = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_str, unit) = s.split_at(end);
    let num: f64 = num_str.trim().parse().ok()?;
    let pt = match unit.trim().to_ascii_lowercase().as_str() {
        "in" => num * 72.0,
        "pt" | "" => num,
        "px" => num * 0.75,
        "cm" => num * 72.0 / 2.54,
        "mm" => num * 72.0 / 25.4,
        "em" => num * 12.0,
        _ => return None,
    };
    Some(Scalar(pt))
}

/// Parse a `counter-reset` / `counter-increment` value into a [`CounterValue`].
/// `none` → no counters; `inherit` → the enclosing context's value (resolved
/// at spec time); anything else is a `name [integer]` group list (default
/// integer: 0 for reset, 1 for increment).
fn parse_counter_value(value: &str, default: i32) -> CounterValue {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => CounterValue::None,
        "inherit" => CounterValue::Inherit,
        _ => CounterValue::List(parse_counters(value, default)),
    }
}

/// Parse `counter-reset` / `counter-increment` groups: `name [integer]`, the
/// integer defaulting to `default` when omitted. Multiple groups in one value
/// (`foo foo foo`) produce one entry each, so application sums the deltas.
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

/// Extract the attribute name from a `target-counter`/`target-text` argument
/// list (`attr(href)` → `href`). Absent/malformed → `None` (callers fall back
/// to `href`, the overwhelmingly common form).
fn parse_target_attr(args: &str) -> Option<String> {
    let pos = args.find("attr(")?;
    let rest = &args[pos + 5..];
    rest.find(')').map(|end| rest[..end].trim().to_string())
}

/// Parse a `content` property value into an ordered piece list: quoted
/// literals, `string(name)`, `counter(page)`/`counter(pages)`/`counter(name)`,
/// `target-counter(attr(href), page)`, and `leader('.')`. Unknown tokens are
/// skipped.
pub fn parse_content(value: &str) -> Vec<ContentPiece> {
    let mut pieces = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut lit = String::new();
            while i < chars.len() && chars[i] != quote {
                lit.push(chars[i]);
                i += 1;
            }
            i += 1; // closing quote
            pieces.push(ContentPiece::Literal(lit));
            continue;
        }
        // Read an identifier followed by an optional `( ... )`.
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
        {
            i += 1;
        }
        let ident: String = chars[start..i].iter().collect();
        if ident.is_empty() {
            i += 1;
            continue;
        }
        // Optional argument list.
        let mut args = String::new();
        if i < chars.len() && chars[i] == '(' {
            let mut depth = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '(' {
                    depth += 1;
                    if depth == 1 {
                        i += 1;
                        continue;
                    }
                } else if ch == ')' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                args.push(ch);
                i += 1;
            }
        }
        match ident.to_ascii_lowercase().as_str() {
            "string" => {
                // string(name[, keyword]) — split on the first comma; a
                // missing or malformed keyword falls back to First (spec
                // §Behavior 1).
                let (name, kw) = match args.split_once(',') {
                    Some((n, k)) => (n.trim(), StringKeyword::parse(k)),
                    None => (args.trim(), StringKeyword::First),
                };
                pieces.push(ContentPiece::StringRef(name.to_string(), kw));
            }
            "url" => {
                // `url(<path>)` — strip quotes/whitespace; an empty path is
                // dropped (no image).
                let path = args.trim().trim_matches(['"', '\'']).trim();
                if !path.is_empty() {
                    pieces.push(ContentPiece::Image(path.to_string()));
                }
            }
            "counter" => {
                let name = args.split(',').next().unwrap_or("").trim();
                if name.eq_ignore_ascii_case("page") {
                    pieces.push(ContentPiece::CounterPage);
                } else if name.eq_ignore_ascii_case("pages") {
                    pieces.push(ContentPiece::CounterPages);
                } else {
                    pieces.push(ContentPiece::CounterRef(name.to_string()));
                }
            }
            "counters" => {
                // counters(name, separator) — split on the first comma; the
                // separator is a quoted string (default "."). The `page` and
                // `pages` built-ins resolve to their single values (the page
                // context never nests them); other names keep the separator
                // for a future scope-chain join.
                let (name, sep) = match args.split_once(',') {
                    Some((n, s)) => (
                        n.trim(),
                        s.trim()
                            .trim_matches(|c| c == '"' || c == '\'')
                            .to_string(),
                    ),
                    None => (args.trim(), ".".to_string()),
                };
                if name.eq_ignore_ascii_case("page") {
                    pieces.push(ContentPiece::CounterPage);
                } else if name.eq_ignore_ascii_case("pages") {
                    pieces.push(ContentPiece::CounterPages);
                } else {
                    pieces.push(ContentPiece::CountersRef {
                        name: name.to_string(),
                        separator: sep,
                    });
                }
            }
            "target-counter" => {
                // target-counter(attr(href)[, counter-name]) — the first arg
                // names the attribute to read; the optional second arg names
                // the counter (default `page`). Split on the FIRST comma only
                // (the same argument-splitting rule as `string(name, kw)`):
                // counter names contain no commas, so a split_anywhere split
                // is safe, but split_once keeps the two functions symmetrical.
                let attr = parse_target_attr(&args).unwrap_or_else(|| "href".to_string());
                let counter = match args.split_once(',') {
                    Some((_, rest)) => rest.trim().to_string(),
                    None => "page".to_string(),
                };
                let counter = if counter.is_empty() {
                    "page".to_string()
                } else {
                    counter
                };
                pieces.push(ContentPiece::TargetCounter { attr, counter });
            }
            "target-text" => {
                // target-text(attr(href)) — the target element's text content.
                let attr = parse_target_attr(&args).unwrap_or_else(|| "href".to_string());
                pieces.push(ContentPiece::TargetText { attr });
            }
            "leader" => {
                let ch = args
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .chars()
                    .next()
                    .unwrap_or('.');
                pieces.push(ContentPiece::Leader(ch));
            }
            _ => {}
        }
    }
    pieces
}

// --- per-page resolution ---------------------------------------------------

/// Resolve the winning [`PageSpec`] for one fragmentainer.
///
/// - `page_name`: the named page in effect (from a `page: <name>` element), or
///   `None` for the default page.
/// - `global_index`: zero-based page index (page 1 == index 0).
/// - `cli`: the CLI geometry, used as the fallback default.
///
/// Selection: collect rules whose name matches (named rules first, then the
/// default page's rules apply as a base), and whose pseudo matches the page
/// parity; later, more-specific matches override earlier ones property by
/// property. Every field starts from the CLI geometry so unspecified props
/// inherit the default.
///
/// Resolution (css-page-3 §4.1): the page box size comes from `size` (with
/// `landscape`/`portrait` oriiented against the running size); the page AREA
/// (the content box) is either the explicit `width`/`height` overrides or the
/// page box minus margins. `auto` margins absorb the leftover space equally
/// (both auto) or entirely (single auto); with no auto margin, the leftover
/// goes to the end (right/bottom) margin, matching the overconstrained case.
pub fn resolve_page_spec(
    rules: &[PageRule],
    page_name: Option<&str>,
    global_index: usize,
    cli: &PageGeometry,
    inherit_margins: PageMargins,
    rtl_progression: bool,
    root_writing_mode: crate::css::PageWritingMode,
) -> PageSpec {
    let mut size = (cli.width, cli.height);
    let mut width: Option<PageLength> = None;
    let mut height: Option<PageLength> = None;
    let mut margins = (
        PageLength::Abs(cli.margin_top),
        PageLength::Abs(cli.margin_right),
        PageLength::Abs(cli.margin_bottom),
        PageLength::Abs(cli.margin_left),
    );
    let mut background = None;
    let mut page_orientation = None;
    // Page-context counters (css-page-3 §8), cascaded per property like the
    // rest: later matching rules win; `None` = no declaration in any rule.
    let mut counter_reset: Option<CounterValue> = None;
    let mut counter_increment: Option<CounterValue> = None;
    // Page-box padding + border accumulators (CORE-144).
    let mut visibility_hidden = false;
    let mut padding = (
        PageLength::Abs(Scalar::ZERO),
        PageLength::Abs(Scalar::ZERO),
        PageLength::Abs(Scalar::ZERO),
        PageLength::Abs(Scalar::ZERO),
    );
    let mut border: Option<(Scalar, Color)> = None;
    // Page-box outline accumulators (CORE-153).
    let mut outline_width: Option<Scalar> = None;
    let mut outline_color: Option<Color> = None;
    let mut outline_offset: Option<Scalar> = None;
    // Margin boxes accumulate by name; later matches merge property-by-property
    // (a later `@page` rule that only sets `color` must keep the earlier
    // `content`, per the cascade). `content` is three-state so an explicit
    // `content: none` overrides a default page's box (CORE-82 suppression).
    let mut boxes: Vec<(
        MarginBoxName,
        Option<Option<Vec<ContentPiece>>>,
        MarginBoxStyle,
    )> = Vec::new();
    // Page-context inherited style, merged the same way (css-page-3 §6).
    let mut page_color: Option<Color> = None;
    let mut page_font_family: Option<Vec<crate::fonts::FamilySpec>> = None;
    let mut page_font_size: Option<Scalar> = None;
    let mut page_font_weight: Option<f32> = None;
    let mut page_font_style: Option<crate::css::FontStyle> = None;

    // Build the ordered list of matching rules, weakest first, so later
    // applications win. Ordering key: (name_specificity, pseudo_specificity,
    // source order). Default-page rules (name None) are weaker than named
    // rules; :none pseudo weaker than :left/:right weaker than :first.
    let mut matching: Vec<&PageRule> = rules
        .iter()
        .filter(|r| rule_matches(r, page_name, global_index, rtl_progression))
        .collect();
    matching.sort_by_key(|r| {
        let name_spec = if r.name.is_some() { 1 } else { 0 };
        let pseudo_spec = match r.pseudo {
            PagePseudo::None => 0,
            PagePseudo::Left | PagePseudo::Right => 1,
            PagePseudo::First => 2,
        };
        // Layer rank dominates specificity (css-cascade-5 §6: unlayered
        // declarations win over ALL layered ones; among layers, later wins).
        // Map unlayered to u32::MAX so it sorts last (= strongest).
        let layer_key = if r.layer == 0 { u32::MAX } else { r.layer };
        (layer_key, name_spec, pseudo_spec, r.order)
    });

    // The page context's effective writing mode (css-page-3 §3.2): the
    // cascaded `@page { writing-mode }` wins; otherwise the page context
    // inherits the ROOT element's computed mode. Logical margin/padding
    // declarations map to physical edges against this (page-box-008/009 —
    // under vertical-rl, inline = top/bottom and block = right/left).
    let mut page_wm: Option<crate::css::PageWritingMode> = None;
    for r in &matching {
        if r.writing_mode.is_some() {
            page_wm = r.writing_mode;
        }
    }
    let eff_wm = page_wm.unwrap_or(root_writing_mode);
    // Logical → physical edge map per writing mode. Index = logical
    // property (0=inline-start, 1=inline-end, 2=block-start, 3=block-end);
    // value = physical slot (0=top, 1=right, 2=bottom, 3=left). The
    // percentage axis follows the physical slot: top/bottom % → HEIGHT,
    // left/right % → WIDTH (resolve_margin below).
    let logical_slots = |wm: crate::css::PageWritingMode| -> [usize; 4] {
        match wm {
            crate::css::PageWritingMode::HorizontalTb => [3, 1, 0, 2],
            crate::css::PageWritingMode::VerticalRl => [0, 2, 1, 3],
            crate::css::PageWritingMode::VerticalLr => [0, 2, 3, 1],
        }
    };
    let m_slots = logical_slots(eff_wm);
    let p_slots = m_slots;
    let set_margin = |margins: &mut (PageLength, PageLength, PageLength, PageLength),
                      slot: usize,
                      v: PageLength| {
        match slot {
            0 => margins.0 = v,
            1 => margins.1 = v,
            2 => margins.2 = v,
            _ => margins.3 = v,
        }
    };
    let set_padding = |padding: &mut (PageLength, PageLength, PageLength, PageLength),
                       slot: usize,
                       v: PageLength| {
        match slot {
            0 => padding.0 = v,
            1 => padding.1 = v,
            2 => padding.2 = v,
            _ => padding.3 = v,
        }
    };

    for r in matching {
        if let Some(s) = r.size {
            size = match s {
                SizeDecl::Abs(w, h) => (w, h),
                // Orientation keywords orient the *running* size (the default
                // page size) rather than a fresh pair.
                SizeDecl::Portrait => (
                    Scalar(size.0.get().min(size.1.get())),
                    Scalar(size.0.get().max(size.1.get())),
                ),
                SizeDecl::Landscape => (
                    Scalar(size.0.get().max(size.1.get())),
                    Scalar(size.0.get().min(size.1.get())),
                ),
            };
        }
        if let Some(v) = r.width {
            width = Some(v);
        }
        if let Some(v) = r.height {
            height = Some(v);
        }
        if let Some(v) = r.margin_top {
            margins.0 = v;
        }
        if let Some(v) = r.margin_right {
            margins.1 = v;
        }
        if let Some(v) = r.margin_bottom {
            margins.2 = v;
        }
        if let Some(v) = r.margin_left {
            margins.3 = v;
        }
        if let Some(v) = r.padding_top {
            padding.0 = v;
        }
        if let Some(v) = r.padding_right {
            padding.1 = v;
        }
        if let Some(v) = r.padding_bottom {
            padding.2 = v;
        }
        if let Some(v) = r.padding_left {
            padding.3 = v;
        }
        // Logical margins/padding map to physical edges per the page
        // context's effective writing mode. Edge case: within ONE rule,
        // a logical decl beats an earlier physical decl regardless of
        // source order (the fields are stored separately) — WPT fixtures
        // never mix the two forms in one rule.
        if let Some(v) = r.margin_inline_start {
            set_margin(&mut margins, m_slots[0], v);
        }
        if let Some(v) = r.margin_inline_end {
            set_margin(&mut margins, m_slots[1], v);
        }
        if let Some(v) = r.margin_block_start {
            set_margin(&mut margins, m_slots[2], v);
        }
        if let Some(v) = r.margin_block_end {
            set_margin(&mut margins, m_slots[3], v);
        }
        if let Some(v) = r.padding_inline_start {
            set_padding(&mut padding, p_slots[0], v);
        }
        if let Some(v) = r.padding_inline_end {
            set_padding(&mut padding, p_slots[1], v);
        }
        if let Some(v) = r.padding_block_start {
            set_padding(&mut padding, p_slots[2], v);
        }
        if let Some(v) = r.padding_block_end {
            set_padding(&mut padding, p_slots[3], v);
        }
        if let Some(w) = r.border_width {
            border = Some((w, r.border_color.unwrap_or(Color::BLACK)));
        }
        if let Some(w) = r.outline_width {
            outline_width = Some(w);
        }
        if let Some(c) = r.outline_color {
            outline_color = Some(c);
        }
        if let Some(o) = r.outline_offset {
            outline_offset = Some(o);
        }
        if let Some(c) = r.background {
            background = Some(c);
        }
        if let Some(o) = r.page_orientation {
            page_orientation = Some(o);
        }
        if r.visibility_hidden {
            visibility_hidden = true;
        }
        if r.counter_reset.is_some() {
            counter_reset = r.counter_reset.clone();
        }
        if r.counter_increment.is_some() {
            counter_increment = r.counter_increment.clone();
        }
        for mb in &r.margin_boxes {
            match boxes.iter_mut().find(|(n, _, _)| *n == mb.name) {
                Some((_, c, s)) => {
                    if mb.content.is_some() {
                        *c = mb.content.clone();
                    }
                    s.merge(&mb.style);
                }
                None => boxes.push((mb.name, mb.content.clone(), mb.style.clone())),
            }
        }
        if r.color.is_some() {
            page_color = r.color;
        }
        if r.font_family.is_some() {
            page_font_family = r.font_family.clone();
        }
        if r.font_size.is_some() {
            page_font_size = r.font_size;
        }
        if r.font_weight.is_some() {
            page_font_weight = r.font_weight;
        }
        if r.font_style.is_some() {
            page_font_style = r.font_style;
        }
    }

    // Resolve the page-context counter values. `inherit` in the page context
    // inherits from the root element, which the page parser cannot see — fold
    // it to `none` (no reset/increment), matching the css-wide initial.
    let page_counter_reset = match counter_reset.unwrap_or(CounterValue::None) {
        CounterValue::Inherit => CounterValue::None,
        v => v,
    };
    let page_counter_increment = match counter_increment.unwrap_or(CounterValue::None) {
        CounterValue::Inherit => CounterValue::None,
        v => v,
    };

    // Resolve margins to concrete values against the final page size. `Inherit`
    // resolves to the root element's computed margin (the page context inherits
    // from the root; css-page-3 §3, page-margin-006).
    let mt = resolve_margin(margins.0, size.1, inherit_margins.top);
    let mr = resolve_margin(margins.1, size.0, inherit_margins.right);
    let mb = resolve_margin(margins.2, size.1, inherit_margins.bottom);
    let ml = resolve_margin(margins.3, size.0, inherit_margins.left);
    let (mut margin_top, auto_top) = mt;
    let (mut margin_right, auto_right) = mr;
    let (mut margin_bottom, auto_bottom) = mb;
    let (mut margin_left, auto_left) = ml;

    // The page area: explicit width/height overrides, else size minus margins
    // (auto margins contribute 0 at this stage).
    let area_w = resolve_area(width, size.0).unwrap_or(size.0 - margin_left - margin_right);
    let area_h = resolve_area(height, size.1).unwrap_or(size.1 - margin_top - margin_bottom);

    // NOTE (CORE-141 follow-up): an earlier attempt grew the page box to a
    // declared page area plus its margins. It flipped no tests (the one flip in
    // that landing came from the §5.3 margin-box geometry, and the fixture it
    // was credited with declares no area at all), `width`/`height` are not
    // css-page-3 page descriptors, and Chromium keeps the page at the requested
    // size. Growing the box also contradicts the CLI contract ("honour the
    // page-size and margin flags exactly"). The area is still honoured inside
    // the page box, as CORE-144 specified.

    // Leftover space on each axis: distributed to auto margins (equal split
    // when both auto), or to the end margin when overconstrained (css-page-3
    // §4.1: "the used value of the right/bottom margin absorbs the extra").
    let leftover_w = Scalar((size.0 - area_w - margin_left - margin_right).get().max(0.0));
    match (auto_left, auto_right) {
        (true, true) => {
            margin_left += leftover_w * 0.5;
            margin_right += leftover_w * 0.5;
        }
        (true, false) => margin_left += leftover_w,
        (false, true) => margin_right += leftover_w,
        (false, false) => margin_right += leftover_w,
    }
    let leftover_h = Scalar((size.1 - area_h - margin_top - margin_bottom).get().max(0.0));
    match (auto_top, auto_bottom) {
        (true, true) => {
            margin_top += leftover_h * 0.5;
            margin_bottom += leftover_h * 0.5;
        }
        (true, false) => margin_top += leftover_h,
        (false, true) => margin_bottom += leftover_h,
        (false, false) => margin_bottom += leftover_h,
    }

    // Resolve page-box padding to points against the FINAL page size. The
    // page context is not a regular box: vertical percentage padding resolves
    // against the page box HEIGHT, horizontal against the WIDTH (css-page-3;
    // page-box-007's assertion). The border thickens into the padding band:
    // the padding box (where the @page background paints in Chromium's model)
    // sits INSIDE the border, so the border eats border-width off the padding
    // on each side. Padding never eats into the page AREA: the content box
    // stays `size - margins` (the margin boxes anchor there); the padding
    // band only narrows the painted chrome outward-in.
    let page_border = border.map(|(w, _)| w).unwrap_or(Scalar::ZERO);
    // currentColor (css-backgrounds-3 §3): an omitted border-color takes the
    // page's final cascaded `color` (resolved at used-value time, so
    // `@page :first { color: orange }` colors page 1's border — page-box-005).
    // A declared border-color wins; the BLACK default (set where the border
    // was accumulated) only survives when the page declared no color at all.
    let border_color = border.map(|(_, c)| {
        if c == Color::BLACK {
            page_color.unwrap_or(c)
        } else {
            c
        }
    });
    let resolve_pad = |len: PageLength, page_dim: Scalar| -> Scalar {
        match len {
            PageLength::Abs(v) => v,
            PageLength::Percent(f) => page_dim * f,
            _ => Scalar::ZERO,
        }
    };
    // CORE-165 correction: the border sits OUTSIDE the padding (border box ⊃
    // padding box ⊃ content box, css-box-3 §3), so the padding band keeps its
    // declared size — the earlier `pad - border_width` subtraction modeled
    // Chromium's geometry wrong (page-box-011 oracle: content inset =
    // border + padding, both bands full width).
    let mut pad_t = resolve_pad(padding.0, size.1);
    let mut pad_r = resolve_pad(padding.1, size.0);
    let mut pad_b = resolve_pad(padding.2, size.1);
    let mut pad_l = resolve_pad(padding.3, size.0);
    let zero = Scalar::ZERO;
    if pad_t.get() < 0.0 {
        pad_t = zero;
    }
    if pad_r.get() < 0.0 {
        pad_r = zero;
    }
    if pad_b.get() < 0.0 {
        pad_b = zero;
    }
    if pad_l.get() < 0.0 {
        pad_l = zero;
    }

    // Resolve each generated margin box: page-context inheritance first
    // (css-page-3 §6), then the box's own declarations, then the UA default
    // table (§6.2). A box whose `content` resolved to `None` — no declaration
    // at all, or an explicit `content: none | normal` — is not generated
    // (css-page-3 §5.2).
    let margin_boxes = boxes
        .into_iter()
        .filter_map(|(name, content, style)| {
            let content = content??;
            let font_family = style
                .font_family
                .clone()
                .or_else(|| page_font_family.clone())
                .unwrap_or_else(|| vec![crate::fonts::FamilySpec::SansSerif]);
            let font_size = style.font_size.or(page_font_size).unwrap_or(Scalar(12.0));
            let font_weight = style.font_weight.or(page_font_weight).unwrap_or(400.0);
            let font_style = style
                .font_style
                .or(page_font_style)
                .unwrap_or(crate::css::FontStyle::Normal);
            let resolved = crate::fonts::resolve_font(&font_family, font_weight, font_style);
            Some(MarginBoxSpec {
                name,
                content,
                color: style.color.or(page_color).unwrap_or(Color::BLACK),
                background: style.background,
                font_size,
                font_face: resolved.primary,
                font_fallbacks: resolved.fallbacks,
                line_height: font_size * crate::css::NORMAL_LINE_HEIGHT_FACTOR,
                text_align: style
                    .text_align
                    .unwrap_or_else(|| name.default_text_align()),
                vertical_align: style
                    .vertical_align
                    .unwrap_or_else(|| name.default_vertical_align()),
                // §5.3 box model: carried through unresolved (percentages need
                // the containing block, which is only known at layout time).
                width: style.width,
                height: style.height,
                margin_top: style.margin_top,
                margin_right: style.margin_right,
                margin_bottom: style.margin_bottom,
                margin_left: style.margin_left,
                padding_top: style.padding_top,
                padding_right: style.padding_right,
                padding_bottom: style.padding_bottom,
                padding_left: style.padding_left,
                border_top: style.border_top,
                border_right: style.border_right,
                border_bottom: style.border_bottom,
                border_left: style.border_left,
                counter_reset: resolve_box_counter(style.counter_reset.clone(), &page_counter_reset),
                counter_increment: resolve_box_counter(
                    style.counter_increment.clone(),
                    &page_counter_increment,
                ),
            })
        })
        .collect();

    PageSpec {
        size,
        margins: PageMargins {
            top: margin_top,
            right: margin_right,
            bottom: margin_bottom,
            left: margin_left,
        },
        padding: PageMargins {
            top: pad_t,
            right: pad_r,
            bottom: pad_b,
            left: pad_l,
        },
        border: border_color.map(|c| (page_border, c)),
        visibility_hidden,
        // Outline paints only when a width or color was declared (a bare
        // `outline-offset` alone does not paint: css-ui-3 outline-style
        // defaults to none). Width/color defaults: medium (3px) / black.
        outline: if outline_width.is_some() || outline_color.is_some() {
            Some((
                outline_width.unwrap_or_else(|| Scalar(2.25)),
                outline_color.unwrap_or(Color::BLACK),
                outline_offset.unwrap_or(Scalar::ZERO),
            ))
        } else {
            None
        },
        background,
        page_orientation,
        margin_boxes,
        counter_reset: page_counter_reset,
        counter_increment: page_counter_increment,
    }
}

/// Resolve a margin box's `counter-reset` / `counter-increment` against the
/// page context: `None` (no declaration) → the box does not obscure (`None`);
/// `inherit` → the page context's resolved value; anything else passes through.
fn resolve_box_counter(box_val: Option<CounterValue>, page_val: &CounterValue) -> CounterValue {
    match box_val {
        None => CounterValue::None,
        Some(CounterValue::Inherit) => page_val.clone(),
        Some(v) => v,
    }
}

/// Resolve one margin value against the page box dimension (width for
/// left/right, height for top/bottom). Returns the concrete value plus
/// whether it was `auto`. `inherit` resolves to the root element's computed
/// margin for this side (the page context inherits from the root).
fn resolve_margin(len: PageLength, page_dim: Scalar, inherit: Scalar) -> (Scalar, bool) {
    match len {
        PageLength::Abs(v) => (v, false),
        PageLength::Percent(f) => (page_dim * f, false),
        PageLength::Auto => (Scalar::ZERO, true),
        PageLength::Inherit => (inherit, false),
    }
}

/// Resolve a page-area `width`/`height` override against the page dimension.
/// `None` means no override (the area is size minus margins).
fn resolve_area(len: Option<PageLength>, page_dim: Scalar) -> Option<Scalar> {
    match len? {
        PageLength::Abs(v) => Some(v),
        PageLength::Percent(f) => Some(page_dim * f),
        PageLength::Auto | PageLength::Inherit => None,
    }
}

/// Whether a rule matches the given page name and index parity. Under root
/// rtl (`rtl_progression`, css-page-3 §4.1 — CORE-166) the left/right page
/// progression flips: page 1 is a `:left` page.
fn rule_matches(
    rule: &PageRule,
    page_name: Option<&str>,
    global_index: usize,
    rtl_progression: bool,
) -> bool {
    // Name: a named rule matches only when that name is in effect; the default
    // page (name None) always applies as the base.
    match (&rule.name, page_name) {
        (Some(rn), Some(pn)) if rn == pn => {}
        (Some(_), _) => return false,
        (None, _) => {}
    }
    pseudo_matches(rule.pseudo, global_index, rtl_progression)
}

/// Whether a page pseudo matches the given zero-based page index.
///
/// css-page-3: page 1 (index 0) is `:first` and `:right` (LTR progression);
/// odd 1-based indices are `:right`, even are `:left`. Under root rtl the
/// progression flips (page 1 = `:left` — CORE-166).
fn pseudo_matches(pseudo: PagePseudo, global_index: usize, rtl_progression: bool) -> bool {
    let one_based = global_index + 1;
    // The parity term decides `:right`/`:left`; root rtl inverts it.
    let right = if rtl_progression {
        one_based % 2 == 0
    } else {
        one_based % 2 == 1
    };
    match pseudo {
        PagePseudo::None => true,
        PagePseudo::First => global_index == 0,
        PagePseudo::Right => right,
        PagePseudo::Left => !right,
    }
}

// --- shared parsing helpers ------------------------------------------------

/// Strip `/* ... */` comments (mirrors `css::breaks::strip_comments`).
/// Char-safe: iterates code points, never raw bytes (CORE-83 — a byte-wise
/// loop turned every multi-byte UTF-8 char in the stylesheet into Latin-1
/// mojibake before margin-box content was parsed).
fn strip_comments(css: &str) -> String {
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

/// Given the byte index of a `{`, return the index of its matching `}`,
/// accounting for nested braces (margin boxes).
fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    debug_assert_eq!(bytes[open], b'{');
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_size_and_margins() {
        let rules = parse_page_rules("@page { size: 8.5in 11in; margin: 1in; }");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert!(r.name.is_none());
        assert_eq!(r.size, Some(SizeDecl::Abs(Scalar(612.0), Scalar(792.0))));
        assert_eq!(r.margin_top, Some(PageLength::Abs(Scalar(72.0))));
    }

    #[test]
    fn parses_orientation_and_margins() {
        let rules = parse_page_rules(
            "@page { size: portrait; margin: 10px 20px 30px 40px; } \
             @page wide { size: landscape; page-orientation: rotate-right; \
             width: 12em; height: 3em; margin: auto; background: yellow; }",
        );
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].size, Some(SizeDecl::Portrait));
        assert_eq!(
            rules[0].margin_left,
            Some(PageLength::Abs(Scalar(30.0))) // 40px = 30pt
        );
        assert_eq!(rules[1].size, Some(SizeDecl::Landscape));
        assert_eq!(rules[1].page_orientation, Some(PageOrientation::RotateRight));
        assert_eq!(rules[1].width, Some(PageLength::Abs(Scalar(144.0)))); // 12em = 144pt
        assert_eq!(rules[1].margin_top, Some(PageLength::Auto));
        assert!(rules[1].background.is_some());
    }

    #[test]
    fn resolves_auto_margins_and_area() {
        // size 20em x 7em (= 240 x 84pt), area 12em x 3em (= 144 x 36pt),
        // margin auto -> 48pt left/right, 24pt top/bottom (centered).
        let rules = parse_page_rules("@page { size: 20em 7em; width: 12em; height: 3em; margin: auto; }");
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let spec = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        assert_eq!(spec.size, (Scalar(240.0), Scalar(84.0)));
        assert_eq!(spec.margins.left, Scalar(48.0));
        assert_eq!(spec.margins.right, Scalar(48.0));
        assert_eq!(spec.margins.top, Scalar(24.0));
        assert_eq!(spec.margins.bottom, Scalar(24.0));
    }

    #[test]
    fn resolves_orientation_specificity() {
        // page-rule-specificity-001 semantics: :first portrait beats default
        // landscape on page 1; page 2 uses the default landscape.
        let rules = parse_page_rules("@page :first { size: portrait; } @page { size: landscape; }");
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let p1 = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        assert_eq!(p1.size, (Scalar(216.0), Scalar(360.0))); // portrait of 5x3in
        let p2 = resolve_page_spec(&rules, None, 1, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        assert_eq!(p2.size, (Scalar(360.0), Scalar(216.0))); // landscape (default)
    }

    #[test]
    fn parses_named_and_pseudo() {
        let rules =
            parse_page_rules("@page landscape { size: 11in 8.5in; } @page :first { margin-top: 2in; }");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name.as_deref(), Some("landscape"));
        assert_eq!(rules[1].pseudo, PagePseudo::First);
    }

    #[test]
    fn parses_margin_boxes() {
        let rules = parse_page_rules(
            "@page { @top-center { content: \"Report\"; } @bottom-right { content: counter(page); } }",
        );
        let r = &rules[0];
        assert_eq!(r.margin_boxes.len(), 2);
        assert_eq!(r.margin_boxes[0].name, MarginBoxName::TopCenter);
        assert_eq!(
            r.margin_boxes[0].content,
            Some(Some(vec![ContentPiece::Literal("Report".to_string())]))
        );
        assert_eq!(
            r.margin_boxes[1].content,
            Some(Some(vec![ContentPiece::CounterPage]))
        );
    }

    #[test]
    fn parses_toc_content() {
        let pieces = parse_content("leader('.') target-counter(attr(href), page)");
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], ContentPiece::Leader('.'));
        assert_eq!(
            pieces[1],
            ContentPiece::TargetCounter {
                attr: "href".to_string(),
                counter: "page".to_string(),
            }
        );
    }

    #[test]
    fn pseudo_parity() {
        assert!(pseudo_matches(PagePseudo::First, 0, false));
        assert!(!pseudo_matches(PagePseudo::First, 1, false));
        assert!(pseudo_matches(PagePseudo::Right, 0, false)); // page 1
        assert!(pseudo_matches(PagePseudo::Left, 1, false));
        // CORE-166: root rtl flips the left/right page progression — page 1
        // is a `:left` page (css-page-3 §4.1, Chromium-verified).
        assert!(pseudo_matches(PagePseudo::Left, 0, true));
        assert!(pseudo_matches(PagePseudo::Right, 1, true));
        assert!(!pseudo_matches(PagePseudo::Right, 0, true));
        // `:first` is direction-independent.
        assert!(pseudo_matches(PagePseudo::First, 0, true)); // page 2
    }

    #[test]
    fn parses_page_padding_and_border() {
        // CORE-144: page-box-007's percentages, page-box-005's borders. The
        // page context resolves vertical percentages against the page HEIGHT
        // (not the width as in regular boxes).
        let rules = parse_page_rules(
            "@page { padding: 5% 20% 15% 40%; border: 10px solid blue; }",
        );
        let r = &rules[0];
        // Percentages stay symbolic (resolved per-page against the final size).
        assert_eq!(r.padding_top, Some(PageLength::Percent(0.05)));
        assert_eq!(r.padding_right, Some(PageLength::Percent(0.20)));
        assert_eq!(r.padding_bottom, Some(PageLength::Percent(0.15)));
        assert_eq!(r.padding_left, Some(PageLength::Percent(0.40)));
        assert_eq!(r.border_width, Some(Scalar(7.5))); // 10px = 7.5pt
        assert!(r.border_color.is_some());
    }

    #[test]
    fn resolves_page_padding_percent_against_height() {
        // css-page-3: vertical percentage padding resolves against the page
        // box HEIGHT. 5% of 800px(=600pt) = 30pt top; 20% of 400px(=300pt) =
        // 60pt right; 15% of 600 = 90pt bottom; 40% of 300 = 120pt left.
        let rules = parse_page_rules(
            "@page { size: 400px 800px; padding: 5% 20% 15% 40%; background: red; }",
        );
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let spec = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        assert_eq!(spec.size, (Scalar(300.0), Scalar(600.0)));
        assert_eq!(spec.padding.top, Scalar(30.0));
        assert_eq!(spec.padding.right, Scalar(60.0));
        assert_eq!(spec.padding.bottom, Scalar(90.0));
        assert_eq!(spec.padding.left, Scalar(120.0));
    }

    #[test]
    fn logical_margins_map_per_writing_mode() {
        // page-box-008/009: a vertical-rl page context maps inline % → the
        // vertical axis and block % → the horizontal axis (css-writing-modes-1
        // logical properties; the refs simulate the margins with border widths
        // 16/32/48/80 top/right/bottom/left). 400px = 300pt, 800px = 600pt.
        let rules = parse_page_rules(
            "@page { writing-mode: vertical-rl; size: 400px 800px; \
             margin-inline-start: 2%; margin-block-start: 8%; \
             margin-inline-end: 6%; margin-block-end: 20%; \
             padding-inline-start: 2%; padding-block-start: 8%; \
             padding-inline-end: 6%; padding-block-end: 20%; }",
        );
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let spec = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        // @page declares vertical-rl itself → the page writing mode is
        // vertical-rl regardless of the root's mode.
        assert_eq!(spec.size, (Scalar(300.0), Scalar(600.0)));
        assert_eq!(spec.margins.top, Scalar(12.0)); // inline-start 2% of 600
        assert_eq!(spec.margins.right, Scalar(24.0)); // block-start 8% of 300
        assert_eq!(spec.margins.bottom, Scalar(36.0)); // inline-end 6% of 600
        assert_eq!(spec.margins.left, Scalar(60.0)); // block-end 20% of 300
        assert_eq!(spec.padding.top, Scalar(12.0));
        assert_eq!(spec.padding.right, Scalar(24.0));
        assert_eq!(spec.padding.bottom, Scalar(36.0));
        assert_eq!(spec.padding.left, Scalar(60.0));
    }

    #[test]
    fn logical_margins_inherit_root_writing_mode() {
        // page-box-008: NO @page writing-mode decl — the page context
        // inherits the ROOT element's vertical-rl (css-page-3 §3), so the
        // logical margins map like the sibling test above.
        let rules = parse_page_rules(
            "@page { size: 400px 800px; \
             margin-inline-start: 2%; margin-block-start: 8%; \
             margin-inline-end: 6%; margin-block-end: 20%; }",
        );
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let spec = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::VerticalRl);
        assert_eq!(spec.margins.top, Scalar(12.0));
        assert_eq!(spec.margins.right, Scalar(24.0));
        assert_eq!(spec.margins.bottom, Scalar(36.0));
        assert_eq!(spec.margins.left, Scalar(60.0));
        // Same rules with a horizontal-tb root keep the old (default) mapping:
        // inline-start → left, 2% of the WIDTH (300pt) = 6pt.
        let spec_tb = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        assert_eq!(spec_tb.margins.left, Scalar(6.0));
        assert_eq!(spec_tb.margins.right, Scalar(18.0)); // inline-end 6% of 300
        assert_eq!(spec_tb.margins.top, Scalar(48.0)); // block-start 8% of 600
        assert_eq!(spec_tb.margins.bottom, Scalar(120.0)); // block-end 20% of 600
    }

    #[test]
    fn parses_margin_box_style_declarations() {
        // css-page-3 Appendix A: the inherited text properties + background +
        // content parse; a box with no `content` declaration keeps `None`.
        let rules = parse_page_rules(
            "@page { @top-left { color: blue; background: yellow; text-align: center; \
             vertical-align: bottom; font-family: monospace; font-size: 10px; \
             font-weight: bold; font-style: italic; content: \"x\"; } \
             @top-right { color: red; } }",
        );
        let r = &rules[0];
        let b = &r.margin_boxes[0];
        assert_eq!(b.content, Some(Some(vec![ContentPiece::Literal("x".to_string())])));
        assert_eq!(b.style.color, Some(Color::rgb(0, 0, 255)));
        assert_eq!(b.style.background, Some(Color::rgb(255, 255, 0)));
        assert_eq!(b.style.text_align, Some(crate::css::TextAlign::Center));
        assert_eq!(b.style.vertical_align, Some(VerticalAlign::Bottom));
        assert_eq!(
            b.style.font_family,
            Some(vec![crate::fonts::FamilySpec::Monospace])
        );
        assert_eq!(b.style.font_size, Some(Scalar(7.5))); // 10px = 7.5pt
        assert_eq!(b.style.font_weight, Some(700.0));
        assert_eq!(b.style.font_style, Some(crate::css::FontStyle::Italic));
        // No `content` declaration: the box is not generated.
        assert_eq!(r.margin_boxes[1].content, None);
    }

    #[test]
    fn margin_box_content_suppression() {
        // css-page-3 §5.2: `content: none`/`normal` suppress generation; the
        // empty string still generates the box.
        let rules = parse_page_rules(
            "@page { @top-left { content: none; } @top-center { content: \"\"; } \
             @top-right { content: normal; } }",
        );
        let r = &rules[0];
        assert_eq!(r.margin_boxes[0].content, Some(None));
        assert_eq!(
            r.margin_boxes[1].content,
            Some(Some(vec![ContentPiece::Literal(String::new())]))
        );
        assert_eq!(r.margin_boxes[2].content, Some(None));
    }

    #[test]
    fn margin_box_default_alignment_table() {
        use crate::css::TextAlign;
        assert_eq!(MarginBoxName::TopLeftCorner.default_text_align(), TextAlign::Right);
        assert_eq!(MarginBoxName::TopLeft.default_text_align(), TextAlign::Left);
        assert_eq!(MarginBoxName::TopCenter.default_text_align(), TextAlign::Center);
        assert_eq!(MarginBoxName::TopRight.default_text_align(), TextAlign::Right);
        assert_eq!(MarginBoxName::TopRightCorner.default_text_align(), TextAlign::Left);
        assert_eq!(MarginBoxName::BottomLeftCorner.default_text_align(), TextAlign::Right);
        assert_eq!(MarginBoxName::BottomRightCorner.default_text_align(), TextAlign::Left);
        assert_eq!(MarginBoxName::LeftTop.default_text_align(), TextAlign::Center);
        assert_eq!(MarginBoxName::RightBottom.default_text_align(), TextAlign::Center);
        assert_eq!(MarginBoxName::LeftTop.default_vertical_align(), VerticalAlign::Top);
        assert_eq!(MarginBoxName::LeftMiddle.default_vertical_align(), VerticalAlign::Middle);
        assert_eq!(MarginBoxName::LeftBottom.default_vertical_align(), VerticalAlign::Bottom);
        assert_eq!(MarginBoxName::TopLeft.default_vertical_align(), VerticalAlign::Middle);
    }

    #[test]
    fn page_context_style_inherits_into_margin_boxes() {
        // css-page-3 §6: page-context font-*/color inherit into margin boxes,
        // and a box's own declaration wins.
        let rules = parse_page_rules(
            "@page { font-family: monospace; font-size: 0.7em; color: blue; \
             @top-left { content: \"a\"; } \
             @top-center { content: \"b\"; color: red; } }",
        );
        let cli = PageGeometry {
            width: Scalar(412.5),
            height: Scalar(300.0),
            margin_top: Scalar(37.5),
            margin_right: Scalar(37.5),
            margin_bottom: Scalar(37.5),
            margin_left: Scalar(37.5),
        };
        let spec = resolve_page_spec(&rules, None, 0, &cli, PageMargins::zero(), false, crate::css::PageWritingMode::HorizontalTb);
        assert_eq!(spec.margin_boxes.len(), 2);
        let a = &spec.margin_boxes[0];
        assert_eq!(a.color, Color::rgb(0, 0, 255));
        assert_eq!(a.font_size, Scalar(0.7 * 12.0));
        assert_eq!(
            a.line_height,
            Scalar(0.7 * 12.0) * crate::css::NORMAL_LINE_HEIGHT_FACTOR
        );
        // `text-align` comes from the §6.2 default table (top-left → left).
        assert_eq!(a.text_align, crate::css::TextAlign::Left);
        let b = &spec.margin_boxes[1];
        assert_eq!(b.color, Color::rgb(255, 0, 0), "own color wins");
        assert_eq!(b.text_align, crate::css::TextAlign::Center, "top-center default");
    }
}
