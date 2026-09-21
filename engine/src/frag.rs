// SPDX-License-Identifier: AGPL-3.0-only

//! The fragment tree: the sole, immutable output of layout.
//!
//! LayoutNG's model, greenfield. Layout is a pure function
//! `(node, constraints, break_token) → (fragment, outgoing_token)`; this module
//! defines the value types on both sides of that arrow.
//!
//! - [`Fragment`] is one immutable layout-output node — one per box *per
//!   fragmentainer it lands in*. A box split across three pages produces three
//!   `Block` fragments, each with resolved physical geometry.
//! - [`Fragmentainer`] is a first-class fragment with *no* source box: a page.
//!   It carries a page [`index`](Fragmentainer::index) and a root [`Fragment`].
//! - [`BreakToken`] is a resumable-layout continuation (a serializable
//!   "coroutine state"): `consumed_block_size` + `seen_all_children`, with
//!   child tokens nested to mirror the path of unfinished nodes.
//! - [`BreakAppeal`] scores candidate breakpoints (perfect → last-resort).
//!
//! ## Deviation from the spec's Interfaces sketch
//!
//! The spec lists a `parent` back-link on `Fragment`. We instead express
//! physical containment structurally: children are *owned* by their parent's
//! `children` vec, and each child's `offset` is physical, relative to that
//! parent. An owned back-pointer would defeat `Clone`/immutability and pull in
//! `Rc`/lifetimes for no gain — paint/PDF only ever walks top-down. Offsets
//! being parent-relative is exactly the LayoutNG property that lets a subtree
//! be repositioned without relayout.

use crate::css::Color;
use crate::dom::NodeId;
use crate::geom::{Point, Scalar};
use crate::typography::ShapedGlyph;

/// How a box may break relative to a sibling boundary: the computed value of
/// `break-before` / `break-after` (and the legacy `page-break-*` aliases).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BreakBetween {
    /// No forced break; the breakpoint is chosen by appeal scoring.
    #[default]
    Auto,
    /// Force a page break (`page` / `always`).
    Page,
    /// Force a break to the next left page (treated as a page break here).
    Left,
    /// Force a break to the next right page (treated as a page break here).
    Right,
}

impl BreakBetween {
    /// Whether this value forces a page break at the adjoining boundary.
    #[inline]
    pub fn is_forced(self) -> bool {
        !matches!(self, BreakBetween::Auto)
    }
}

/// How a box may break *within* itself: the computed value of `break-inside`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BreakInside {
    /// Breaking inside is allowed.
    #[default]
    Auto,
    /// Breaking inside should be avoided (raises the appeal requirement).
    Avoid,
}

/// Break-quality score for a candidate breakpoint.
///
/// LayoutNG's golden rule: break at the highest-appeal point that fits the most
/// content. Higher is better; ordering is by the `u8` discriminant. `Perfect`
/// is a class boundary (e.g. a forced break or a clean between-blocks break);
/// `LastResort` places monolithic content that cannot otherwise fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum BreakAppeal {
    /// Overflow-forcing: only used when nothing else fits (monolithic content).
    LastResort = 0,
    /// Violates a `break-inside: avoid` / orphans / widows constraint.
    AvoidViolating = 1,
    /// An acceptable break that respects orphans/widows but is not ideal.
    Tolerable = 2,
    /// A clean break at a block boundary.
    Good = 3,
    /// A forced break, or the natural end of content.
    Perfect = 4,
}

/// The kind of a [`Fragment`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FragmentKind {
    /// A page. First-class fragment with no source box.
    Fragmentainer,
    /// A block-level box fragment (one per fragmentainer the box lands in).
    Block,
    /// A single line of inline content (monolithic: never sliced).
    Line,
}

/// Renderable payload attached to a [`Fragment`], by kind.
///
/// Fragmentainers carry nothing; blocks may carry a background fill; lines
/// carry their shaped-ish text run.
#[derive(Clone, Debug, Default)]
pub enum FragmentContent {
    /// No paint of its own (fragmentainers, anonymous blocks).
    #[default]
    None,
    /// A block background fill (drawn before descendant text).
    Background(Color),
    /// A rectangular border box (CORE-61 tables): the fragment's size is the
    /// cell/row box; the emitter strokes each side whose style width > 0.
    Border(BorderBox),
    /// A run of text drawn at the fragment's baseline.
    Text(TextRun),
    /// An embedded raster image painted over the fragment's rect
    /// (CORE-106). The fragment is monolithic — it never splits across
    /// fragmentainers.
    Image(ImageRun),
    /// A background image tiled over the fragment's rect (CORE-141 margin
    /// boxes): each tile is drawn at the image's natural size from the box's
    /// top-left, clipped to the border box (css-backgrounds-3 §2.1 — painted
    /// under the border, above the background colour).
    BackgroundImage(BackgroundImageRun),
}

/// Rendering data for an `<img>` fragment (CORE-106).
#[derive(Clone, Debug)]
pub struct ImageRun {
    /// Cache key (SHA-256 of the source bytes) indexing the engine's
    /// image store.
    pub key: [u8; 32],
    /// Alt text, if any (drawn only for broken-image placeholders).
    pub alt: Option<String>,
    /// True when the source failed to load or decode (placeholder mode).
    pub broken: bool,
}

/// Rendering data for a tiled background image (CORE-141 margin boxes).
#[derive(Clone, Debug)]
pub struct BackgroundImageRun {
    /// Cache key (SHA-256 of the source bytes) indexing the engine's
    /// image store.
    pub key: [u8; 32],
    /// Natural tile width in points (96 DPI px -> pt, 0.75 factor), kept
    /// from layout so the paint pass never re-reads the image store size.
    pub tile_w: Scalar,
    /// Natural tile height in points.
    pub tile_h: Scalar,
    /// True when the source failed to load or decode (paints nothing).
    pub broken: bool,
}

/// Border rendering data for a table cell/row (border-collapse: collapse)
/// and any block border (CORE-201: each side carries its OWN color).
#[derive(Clone, Debug)]
pub struct BorderBox {
    /// Widths on each side, points (0 = no border on that side).
    pub top: Scalar,
    pub right: Scalar,
    pub bottom: Scalar,
    pub left: Scalar,
    /// Border color per side (CORE-201). `currentColor` is resolved to a
    /// concrete Color at fragment build time (the CORE-165 model), so these
    /// are never `None`.
    pub top_color: Color,
    pub right_color: Color,
    pub bottom_color: Color,
    pub left_color: Color,
}

/// A laid-out run of text for a [`FragmentKind::Line`] fragment.
#[derive(Clone, Debug)]
pub struct TextRun {
    /// The text of this line.
    pub text: String,
    /// Baseline origin (points, top-left coordinate system), relative to the
    /// containing fragmentainer (resolved to absolute at emit time).
    pub baseline: Point,
    /// Font size in points.
    pub font_size: Scalar,
    /// Fill color.
    pub color: Color,
    /// Resolved font face for this run (used by PDF emit).
    pub font_face: crate::fonts::FaceId,
    /// The shaped glyphs of the line (typography layer). All runs — body
    /// text, generated content, margin boxes — carry shaped glyphs with
    /// source-text ranges (CORE-85, CORE-83); empty only for degenerate runs.
    pub glyphs: Vec<ShapedGlyph>,
    /// Per-line glyph-advance scale in [-0.02, 0.02] (font expansion),
    /// applied to every glyph advance at draw time.
    pub expansion: f64,
    /// Optical left hang of the first glyph (punctuation), in points.
    /// Draw-time only; never affects line breaking or measured width.
    pub protrude_left: Scalar,
    /// Optical right hang of the last glyph (punctuation), in points.
    /// Draw-time only; never affects line breaking or measured width.
    pub protrude_right: Scalar,
}

/// An immutable layout-output node.
///
/// Geometry is physical and resolved: `offset` is top-left relative to the
/// parent fragment, `size` is width × height, both in points.
#[derive(Clone, Debug)]
pub struct Fragment {
    /// What this fragment is.
    pub kind: FragmentKind,
    /// Top-left offset relative to the parent fragment (points).
    pub offset: Point,
    /// Width × height (points).
    pub size: (Scalar, Scalar),
    /// Child fragments, in paint (pre-order) order.
    pub children: Vec<Fragment>,
    /// Renderable payload for this fragment.
    pub content: FragmentContent,
    /// The outgoing continuation. `Some` iff this fragment breaks inside — i.e.
    /// more of the corresponding box remains for a later fragmentainer.
    pub break_token: Option<BreakToken>,
    /// The DOM node that produced this fragment, if any. `None` for
    /// fragmentainers, anonymous boxes, and margin boxes. Paged-media features
    /// (`target-counter`, PDF bookmarks) use this to map an element to the page
    /// its box landed on.
    pub source: Option<NodeId>,
    /// In-flow stacking-context key (CORE-202): set by flex/grid item
    /// placement when the item's computed `z-index` is not `auto`. The
    /// emitter groups paint by it: each keyed fragment's whole subtree
    /// paints as one unit, and sibling units at the same level sort by
    /// (z_index, tree order) per CSS2.1 Appendix E / css-flexbox-1 §6.6 /
    /// css-grid-1. `None` = ordinary in-flow content (no own context).
    pub z_index: Option<i32>,
}

impl Fragment {
    /// A block fragment with no children yet and no source box.
    pub fn block(offset: Point, size: (Scalar, Scalar)) -> Fragment {
        Fragment {
            kind: FragmentKind::Block,
            offset,
            size,
            children: Vec::new(),
            content: FragmentContent::None,
            break_token: None,
            source: None,
            z_index: None,
        }
    }

    /// A line fragment carrying a text run.
    pub fn line(offset: Point, size: (Scalar, Scalar), run: TextRun) -> Fragment {
        Fragment {
            kind: FragmentKind::Line,
            offset,
            size,
            content: FragmentContent::Text(run),
            children: Vec::new(),
            break_token: None,
            source: None,
            z_index: None,
        }
    }
}

/// A resumable-layout continuation.
///
/// Break tokens *are* continuations: laying out fragmentainer N+1 replays the
/// same algorithms with the token tree in hand — skipping finished siblings,
/// resuming unfinished ones. The tree nests to mirror exactly the path of
/// unfinished nodes; its depth is bounded by DOM depth, never by page count.
#[derive(Clone, Debug, Default)]
pub struct BreakToken {
    /// Block size (height) already consumed by earlier fragments of this box.
    /// Lets specified heights resolve correctly across breaks.
    pub consumed_block_size: Scalar,
    /// Source bytes consumed by the previous fragment of a text run (Rust
    /// `&str` slicing is byte-indexed).
    /// Used to resume at a source offset when available widths vary.
    pub consumed_chars: Option<usize>,
    /// `HasSeenAllChildren`: true once every child has been fully laid out.
    /// Disambiguates "no child tokens because we're done" from "not started",
    /// which is what prevents infinite page generation.
    pub seen_all_children: bool,
    /// Continuations for children that broke inside (or have a break-before).
    /// Indexed positionally against the box's block children.
    pub child_tokens: Vec<ChildToken>,
    /// `IsBreakBefore`: true when the box itself has not started yet (no
    /// fragment produced), distinguishing resume-inside from start-fresh.
    pub break_before: bool,
    /// Flex-container continuation state (CORE-65). `None` for non-flex
    /// boxes and for flex containers that have not fragmented yet.
    pub flex: Option<FlexToken>,
    /// CORE-109 deadlock guard: the row that produced this token already
    /// deferred once via break-before. A second defer would loop forever
    /// when no fragmentainer can ever fit the row (e.g. a repeating header
    /// eats into every page), so the row force-places instead.
    pub deferred_once: bool,
    /// Flex cross-axis stretch (css-flexbox-1 §9.4 step 4): the used cross
    /// size a flex container resolved for this item. Honored by the block
    /// path like a declared height (larger-of content vs target) so a
    /// stretched item's paint box grows; pagination stays content-based.
    pub cross_override: Option<Scalar>,
}

impl BreakToken {
    /// A start-fresh token for a not-yet-started box (`IsBreakBefore`).
    pub fn break_before() -> BreakToken {
        BreakToken {
            break_before: true,
            ..BreakToken::default()
        }
    }

    /// A start-fresh token that also marks the row as having deferred once
    /// (CORE-109): the next fragmentainer must place it, never defer again.
    pub fn break_before_deferred() -> BreakToken {
        BreakToken {
            break_before: true,
            deferred_once: true,
            ..BreakToken::default()
        }
    }

    /// Whether this token means "start this box fresh" rather than "resume".
    #[inline]
    pub fn is_break_before(&self) -> bool {
        self.break_before
    }
}

/// A child continuation nested inside a parent [`BreakToken`].
///
/// `index` is the position of the child among the parent box's block
/// children, so resume can skip finished siblings and match the right one.
#[derive(Clone, Debug)]
pub struct ChildToken {
    /// Position among the parent box's block children.
    pub index: usize,
    /// The child's own continuation.
    pub token: BreakToken,
}

/// Flex-container continuation state (CORE-65).
///
/// A row flex container lays out *lines* down the page; each line holds
/// items placed along the main axis. When the container fragments, the next
/// page must know which line and which item-within-line to resume, because a
/// mid-line item break (a row item taller than the page) resumes the *line*
/// at the top of the next page, not just a single block. The flex item list
/// is the container's block children in document order, so `next_item`
/// doubles as the positional child index for `child_tokens`.
#[derive(Clone, Debug, Default)]
pub struct FlexToken {
    /// Index of the next flex item to lay out (into the container's flex
    /// item list, which is document order).
    pub next_item: usize,
    /// Index of the line that item starts (or resumes) on.
    pub line: usize,
    /// True when resuming an item that broke mid-line: the previous fragment
    /// of this item ended on the previous page, and the line's remaining
    /// items follow it at the top of this page.
    pub mid_line: bool,
}

/// A page-margin box waiting to be painted (css-page-3 §3.1).
///
/// Margin boxes are their own stacking contexts, so the emitter paints each
/// one as a UNIT — its background, then its border, then its content — rather
/// than letting the page-wide passes interleave one box's text under the next
/// box's background. They also never interleave with document content: the
/// document canvas, page borders and all document content behave as one
/// `z-index: 0` stacking context, so a negative-`z-index` margin box paints
/// behind it and a zero/positive one in front.
#[derive(Clone, Debug)]
pub struct MarginBoxFragment {
    /// Resolved `z-index` (`auto` = 0, the initial value).
    pub z_index: i32,
    /// The spec's default paint order among margin boxes — the index of the
    /// box in css-page-3 §3.1's list (`@top-left-corner` = 0, clockwise).
    /// `z-index` overrides it; within one `z-index` this order breaks ties.
    pub order: u8,
    /// The box's fragment subtree (background/border/content).
    pub fragment: Fragment,
}

/// A first-class fragment with no source box: a page.
#[derive(Clone, Debug)]
pub struct Fragmentainer {
    /// Zero-based page number.
    pub index: usize,
    /// The root fragment for this page (kind [`FragmentKind::Fragmentainer`]).
    pub root: Fragment,
    /// The page's margin boxes, kept OUT of `root` so the emitter can order
    /// and paint them per css-page-3 §3.1 (`MarginBoxFragment`).
    pub margin_boxes: Vec<MarginBoxFragment>,
    /// Page box background color, from the resolved `@page` rule (CORE-66).
    pub background: Option<crate::css::Color>,
    /// `page-orientation` from the resolved `@page` rule (CORE-66). Carried
    /// so the PDF emitter can rotate content within the page box.
    pub page_orientation: Option<crate::paged::PageOrientation>,
    /// Page-absolute origin of the page's content box (points). Recorded so
    /// the fixed-position attachment pass can shift each fixed fragment from
    /// its anchor page's geometry to any page whose `@page` rule resolves a
    /// different size/margin (named pages; CORE-127 slice b).
    pub content_origin: Point,
    /// Page-absolute size of the page's content box (points). Together with
    /// `content_origin` this is the page content rect — the canvas-background
    /// propagation area (CORE-144; origin alone cannot size the fill).
    pub content_size: (Scalar, Scalar),
    /// The propagated document canvas background (CORE-144): the html (else
    /// body) background, painted over the page CONTENT area beneath all
    /// content — the donor box paints none of its own. `None` = no
    /// propagation (default white). The `@page` box background is separate
    /// (`Fragmentainer::background`) and paints under this fill.
    pub canvas_background: Option<crate::css::Color>,
    /// Page-box outline (CORE-153): (width, color, offset) resolved to points.
    /// Painted outside the page border box (the page area) by `offset`.
    pub outline: Option<(crate::geom::Scalar, crate::css::Color, crate::geom::Scalar)>,
    /// @page border ring (CORE-144/165): uniform width + color, painted as a
    /// band at the OUTER edge of the page area rect on every page. Width is
    /// always Some when a border was declared — even when hidden — because
    /// the border still insets content (visibility affects paint, never
    /// layout); `hidden` flags the suppressed paint.
    pub page_border: Option<(crate::geom::Scalar, crate::css::Color)>,
    /// `@page { visibility: hidden }` (css-page-3): the page box's own
    /// decorations do not paint; document content stays visible.
    pub page_chrome_hidden: bool,
    /// html root box border (CORE-165): (width, color, padding-t/r/b/l). The
    /// root element's own border paints at the page area edge on every page
    /// (the root box repeats per fragment); its background propagates to the
    /// canvas instead (CORE-144).
    pub root_border: Option<(
        crate::geom::Scalar,
        crate::css::Color,
        (crate::geom::Scalar, crate::geom::Scalar, crate::geom::Scalar, crate::geom::Scalar),
    )>,
}

impl Fragmentainer {
    /// The page's margin boxes in css-page-3 §3.1 painting order: ascending
    /// `z-index`, then the spec's default tree order within a `z-index`.
    /// (The emitter paints each box as a unit in this order; it splits the
    /// list at `z_index < 0` to put the negative half behind the canvas.)
    pub fn margin_boxes_in_paint_order(&self) -> Vec<&MarginBoxFragment> {
        let mut boxes: Vec<&MarginBoxFragment> = self.margin_boxes.iter().collect();
        boxes.sort_by_key(|m| (m.z_index, m.order));
        boxes
    }

    /// Every fragment subtree this page paints, in §3.1 layer order: the
    /// negative-`z-index` margin boxes (they paint behind the document
    /// canvas), then the document content root, then the remaining margin
    /// boxes. A caller that needs "everything this page paints" — tests,
    /// probes — must walk this rather than the content root, because margin
    /// boxes deliberately live OUTSIDE the content tree.
    pub fn paint_roots(&self) -> Vec<&Fragment> {
        let ordered = self.margin_boxes_in_paint_order();
        let mut roots: Vec<&Fragment> = ordered
            .iter()
            .filter(|m| m.z_index < 0)
            .map(|m| &m.fragment)
            .collect();
        roots.push(&self.root);
        roots.extend(
            ordered
                .iter()
                .filter(|m| m.z_index >= 0)
                .map(|m| &m.fragment),
        );
        roots
    }

    /// Build a fragmentainer of the given page size at the given page index.
    pub fn new(index: usize, size: (Scalar, Scalar)) -> Fragmentainer {
        Fragmentainer {
            index,
            root: Fragment {
                kind: FragmentKind::Fragmentainer,
                offset: Point::new(Scalar::ZERO, Scalar::ZERO),
                size,
                children: Vec::new(),
                content: FragmentContent::None,
                break_token: None,
                source: None,
                z_index: None,
            },
            background: None,
            page_orientation: None,
            margin_boxes: Vec::new(),
            // Default: origin at (0, 0); `paginate` overwrites it with
            // the real content-box origin for every page it lays out.
            content_origin: Point::default(),
            content_size: size,
            canvas_background: None,
            page_border: None,
            page_chrome_hidden: false,
            root_border: None,
            outline: None,
        }
    }
}
