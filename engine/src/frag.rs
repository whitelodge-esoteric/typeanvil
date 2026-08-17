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
use crate::geom::{Point, Scalar};

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
    /// A run of text drawn at the fragment's baseline.
    Text(TextRun),
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
    /// Resolved font family (carried for the shaping stage; the PDF backend
    /// currently embeds a single font).
    #[allow(dead_code)]
    pub font_family: String,
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
}

impl Fragment {
    /// A block fragment with no children yet.
    pub fn block(offset: Point, size: (Scalar, Scalar)) -> Fragment {
        Fragment {
            kind: FragmentKind::Block,
            offset,
            size,
            children: Vec::new(),
            content: FragmentContent::None,
            break_token: None,
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
}

impl BreakToken {
    /// A start-fresh token for a not-yet-started box (`IsBreakBefore`).
    pub fn break_before() -> BreakToken {
        BreakToken {
            break_before: true,
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
/// `index` is the position of the child among the parent box's *block
/// children*, so resume can skip finished siblings and match the right one.
#[derive(Clone, Debug)]
pub struct ChildToken {
    /// Position among the parent box's block children.
    pub index: usize,
    /// The child's own continuation.
    pub token: BreakToken,
}

/// A first-class fragment with no source box: a page.
#[derive(Clone, Debug)]
pub struct Fragmentainer {
    /// Zero-based page number.
    pub index: usize,
    /// The root fragment for this page (kind [`FragmentKind::Fragmentainer`]).
    pub root: Fragment,
}

impl Fragmentainer {
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
            },
        }
    }
}
