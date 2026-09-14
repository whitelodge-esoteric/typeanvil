//! Page-margin box sizing — the css-page-3 §5.3 geometry math.
//!
//! Margin boxes live in the page margin area. Each box is a normal CSS box
//! (margin / border / padding / content), but its two axes are solved by two
//! different rule sets:
//!
//! * the **variable dimension** — the inline axis of the three boxes sharing
//!   one edge (start, middle, end: `@top-left`/`@top-center`/`@top-right`) is
//!   distributed across the boxes by §5.3.2.2, using their min-content and
//!   max-content widths;
//! * the **fixed dimension** — the axis across that edge (the height of the
//!   top/bottom boxes) is solved by §5.3.3, where the box's own size and
//!   margins must sum to the page margin.
//!
//! Both solvers are pure: the caller supplies measured content sizes and gets
//! back used sizes, so the whole module is unit-testable without fonts.
//!
//! Corner boxes are ALWAYS fixed in both dimensions, so they use
//! [`resolve_fixed`] twice (once per axis). The side boxes (`@left-top` …
//! `@right-bottom`) run the same two solvers with the axes swapped: their
//! fixed dimension is the width, their variable dimension the height.

use crate::geom::Scalar;

/// One margin box reduced to what the §5.3 solvers need: the box's
/// declarations along ONE axis, plus the measured content extent along it.
///
/// Every `Option` field is `None` for "not declared", which is `auto` in the
/// algorithms (`auto` margins, `auto` size).
#[derive(Clone, Copy, Debug, Default)]
pub struct AxisBox {
    /// Declared content size (`width` along the inline axis, `height` along
    /// the block axis). `None` = `auto`.
    pub size: Option<Scalar>,
    /// Start-edge margin (`margin-left` / `margin-top`). `None` = `auto`.
    pub margin_start: Option<Scalar>,
    /// End-edge margin (`margin-right` / `margin-bottom`). `None` = `auto`.
    pub margin_end: Option<Scalar>,
    /// Start-edge border width.
    pub border_start: Scalar,
    /// End-edge border width.
    pub border_end: Scalar,
    /// Start-edge padding.
    pub padding_start: Scalar,
    /// End-edge padding.
    pub padding_end: Scalar,
    /// min-content size of the generated content along this axis.
    pub min_content: Scalar,
    /// max-content size of the generated content along this axis.
    pub max_content: Scalar,
}

impl AxisBox {
    /// Everything outside a box's content box along this axis, excluding the
    /// margins (§5.3.1 "outer" sizes add the margins).
    pub fn noncontent(&self) -> Scalar {
        self.border_start + self.padding_start + self.padding_end + self.border_end
    }

    /// §5.3.1 outer width with a DECLARED size: margins + border + padding +
    /// the declared size. `auto` is not handled here.
    fn outer_with(&self, content: Scalar) -> Scalar {
        let ms = self.margin_start.unwrap_or(Scalar::ZERO);
        let me = self.margin_end.unwrap_or(Scalar::ZERO);
        ms + me + self.noncontent() + content
    }

    /// §5.3.1 "outer max width": the declared size, else the max-content size.
    pub fn outer_max(&self) -> Scalar {
        self.outer_with(self.size.unwrap_or(self.max_content))
    }

    /// §5.3.1 "outer min width": the declared size, else the min-content size.
    pub fn outer_min(&self) -> Scalar {
        self.outer_with(self.size.unwrap_or(self.min_content))
    }
}

/// §5.3.2.2 — resolve the used OUTER sizes of the start / middle / end boxes
/// of one edge against the available size.
///
/// `boxes[i]` is `None` when that box is not generated (no `@`-rule for it);
/// such a box is "assumed to have a width and an outer width of zero".
///
/// Returns the used outer size of each box, in the same order. A box whose
/// declaration is not `auto` keeps its declared outer size (it is not
/// redistributed — §5.3.2.2 only resolves `auto` widths).
pub fn resolve_variable(boxes: [Option<&AxisBox>; 3], available: Scalar) -> [Scalar; 3] {
    let mut used = [Scalar::ZERO; 3];
    let declared: [bool; 3] = [
        boxes[0].is_some_and(|b| b.size.is_some()),
        boxes[1].is_some_and(|b| b.size.is_some()),
        boxes[2].is_some_and(|b| b.size.is_some()),
    ];
    for i in 0..3 {
        if let Some(b) = boxes[i] {
            // A declared size is used as-is; only `auto` participates below.
            if declared[i] {
                used[i] = b.outer_with(b.size.unwrap_or(Scalar::ZERO));
            }
        }
    }
    let middle_generated = boxes[1].is_some();

    if !middle_generated {
        // §5.3.2.2: distribute the available width to the side boxes.
        let auto_side = [0usize, 2];
        let auto: Vec<usize> = auto_side
            .into_iter()
            .filter(|&i| boxes[i].is_some() && !declared[i])
            .collect();
        match auto.as_slice() {
            [] => {}
            [only] => {
                // Exactly one auto box takes whatever is left over.
                let other = if *only == 0 { 2 } else { 0 };
                used[*only] = Scalar((available - used[other]).get().max(0.0));
            }
            [a, c] => {
                let (ba, bc) = (boxes[*a].unwrap(), boxes[*c].unwrap());
                let pair = distribute_pair(
                    [ba.outer_min(), ba.outer_max()],
                    [bc.outer_min(), bc.outer_max()],
                    available,
                );
                used[*a] = pair[0];
                used[*c] = pair[1];
            }
            _ => {}
        }
        return used;
    }

    // §5.3.2.2 (middle box generated): resolve the middle box first against an
    // imaginary pair box "AC" whose each dimension is double the maximum of A
    // and C — this preserves B's centering — then give the side boxes the
    // remaining space.
    let mid = boxes[1].unwrap();
    if !declared[1] {
        let (a_min, a_max) = side_extremes(boxes[0]);
        let (c_min, c_max) = side_extremes(boxes[2]);
        let ac_min = Scalar(2.0 * a_min.get().max(c_min.get()));
        let ac_max = Scalar(2.0 * a_max.get().max(c_max.get()));
        if ac_max.get() == 0.0 {
            // No side boxes (or empty ones): the imaginary AC is zero, so
            // the middle box ALONE takes the whole edge. The "flex factors
            // assumed 1" fallback would split the space 50/50 with a box
            // that does not exist; Chrome gives the lone box the full edge
            // (background-001's @top-center renders a full-width band, and
            // the non-empty case already does this below via distribute_pair
            // because AC's factor is then nonzero).
            used[1] = available;
        } else {
            let pair = distribute_pair(
                [mid.outer_min(), mid.outer_max()],
                [ac_min, ac_max],
                available,
            );
            used[1] = pair[0];
        }
    }
    for i in [0usize, 2] {
        if boxes[i].is_some() && !declared[i] {
            used[i] = Scalar(((available - used[1]) * 0.5).get().max(0.0));
        }
    }
    used
}

/// The (min, max) outer extents of a side box; a missing box is zero.
fn side_extremes(b: Option<&AxisBox>) -> (Scalar, Scalar) {
    match b {
        Some(b) => (b.outer_min(), b.outer_max()),
        None => (Scalar::ZERO, Scalar::ZERO),
    }
}

/// The §5.3.2.2 three-step flex distribution between two `auto` boxes.
///
/// Each argument is the box's `[outer min, outer max]`. The steps are:
///
/// 1. max-content total fits → start at max, share the leftover in
///    proportion to the max-content sizes;
/// 2. min-content total fits → start at min, share the leftover in proportion
///    to `max − min` (the "flex" of each box);
/// 3. otherwise → start at min, share the (negative) leftover in proportion
///    to the min-content sizes.
fn distribute_pair(a: [Scalar; 2], c: [Scalar; 2], available: Scalar) -> [Scalar; 2] {
    let sum_max = a[1] + c[1];
    let sum_min = a[0] + c[0];
    if sum_max <= available {
        flex_from(a[1], c[1], available - sum_max, a[1], c[1])
    } else if sum_min <= available {
        flex_from(a[0], c[0], available - sum_min, a[1] - a[0], c[1] - c[0])
    } else {
        flex_from(a[0], c[0], available - sum_min, a[0], c[0])
    }
}

/// `base[i] + flex_space × factor[i] ÷ Σfactors` — §5.3.2.2's shared shape.
/// Both factors are taken as 1 when their sum is zero.
fn flex_from(
    base_a: Scalar,
    base_c: Scalar,
    flex_space: Scalar,
    fa: Scalar,
    fc: Scalar,
) -> [Scalar; 2] {
    let sum_f = fa + fc;
    if sum_f.get() == 0.0 {
        return [base_a + flex_space * 0.5, base_c + flex_space * 0.5];
    }
    [
        base_a + Scalar(flex_space.get() * fa.get() / sum_f.get()),
        base_c + Scalar(flex_space.get() * fc.get() / sum_f.get()),
    ]
}

/// §5.3.3 — the fixed-dimension solution: `(start margin, content size, end
/// margin)` such that the three plus `b.noncontent()` sum to `band`.
///
/// `ignore_end_when_overconstrained` picks the edge the over-constrained rule
/// drops: `false` for a top/left box (its start margin is treated as `auto`),
/// `true` for a bottom/right box (its end margin is).
///
/// The safe-printable inset of §5.3.3 rules 2/5/6 is zero for this engine, so
/// the corresponding adjustments are no-ops.
pub fn resolve_fixed(
    b: &AxisBox,
    band: Scalar,
    ignore_end_when_overconstrained: bool,
) -> (Scalar, Scalar, Scalar) {
    let noncontent = b.noncontent();
    let mut m_start = b.margin_start;
    let mut m_end = b.margin_end;
    let size = b.size;

    // Rule 3: over-constrained — every one of size, start margin and end
    // margin is non-auto. Exactly one margin becomes auto (the edge the spec
    // ignores for this side).
    if size.is_some() && m_start.is_some() && m_end.is_some() {
        if ignore_end_when_overconstrained {
            m_end = None;
        } else {
            m_start = None;
        }
    }

    // Rule 5: an auto size takes the space the margins leave; auto margins
    // become zero (the safe-printable inset of this engine).
    if size.is_none() {
        let ms = m_start.unwrap_or(Scalar::ZERO);
        let me = m_end.unwrap_or(Scalar::ZERO);
        let h = Scalar((band - noncontent - ms - me).get().max(0.0));
        return (ms, h, me);
    }

    // Rules 4 and 6: solve for the one auto margin, or split the leftover
    // equally when both are auto (which centers the box in the band).
    let content = size.unwrap_or(Scalar::ZERO);
    let leftover = band - noncontent - content;
    match (m_start, m_end) {
        (None, None) => {
            let half = Scalar(leftover.get() * 0.5);
            (half, content, Scalar((leftover - half).get().max(0.0)))
        }
        (None, Some(me)) => (Scalar((leftover - me).get().max(0.0)), content, me),
        (Some(ms), None) => (ms, content, Scalar((leftover - ms).get().max(0.0))),
        (Some(ms), Some(me)) => (ms, content, me),
    }
}

// ---------------------------------------------------------------------------
// Placement: from §5.3 sizes to page-absolute boxes
// ---------------------------------------------------------------------------

use crate::paged::{MarginBoxName, MarginBoxSpec, PageLength};

/// The measured content of one generated margin box.
///
/// Margin boxes hold a single line (css-page-3 §5.3), so the block extent is
/// that line's height and the inline extents are the text's min-content
/// (widest word) and max-content (whole line) sizes.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoxMetrics {
    /// min-content inline size.
    pub min_inline: Scalar,
    /// max-content inline size.
    pub max_inline: Scalar,
    /// Block size of the content.
    pub block: Scalar,
}

/// A margin box placed on the page.
///
/// `x`/`y`/`w`/`h` are the BORDER box in page-absolute points; `border` and
/// `padding` are the insets that separate it from the content box.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlacedBox {
    pub x: Scalar,
    pub y: Scalar,
    pub w: Scalar,
    pub h: Scalar,
    /// Border widths in page order (top, right, bottom, left).
    pub border: [Scalar; 4],
    /// Padding widths in page order (top, right, bottom, left).
    pub padding: [Scalar; 4],
}

impl PlacedBox {
    /// The content box as `(x, y, width, height)`.
    pub fn content_rect(&self) -> (Scalar, Scalar, Scalar, Scalar) {
        let (bl, br) = (self.border[3], self.border[1]);
        let (bt, bb) = (self.border[0], self.border[2]);
        let (pl, pr) = (self.padding[3], self.padding[1]);
        let (pt, pb) = (self.padding[0], self.padding[2]);
        (
            self.x + bl + pl,
            self.y + bt + pt,
            Scalar((self.w - bl - br - pl - pr).get().max(0.0)),
            Scalar((self.h - bt - bb - pt - pb).get().max(0.0)),
        )
    }
}

/// The five boxes of one page edge, in css-page-3 painting order.
#[derive(Clone, Copy, Debug)]
pub struct EdgeSlots {
    /// Fixed-size corner box at the start of the edge.
    pub start_corner: MarginBoxName,
    /// Variable-size box at the start of the edge.
    pub start: MarginBoxName,
    /// The edge's middle box.
    pub middle: MarginBoxName,
    /// Variable-size box at the end of the edge.
    pub end: MarginBoxName,
    /// Fixed-size corner box at the end of the edge.
    pub end_corner: MarginBoxName,
}

impl EdgeSlots {
    /// The top row: corners, `@top-left`, `@top-center`, `@top-right`.
    pub fn top() -> EdgeSlots {
        use MarginBoxName::*;
        EdgeSlots {
            start_corner: TopLeftCorner,
            start: TopLeft,
            middle: TopCenter,
            end: TopRight,
            end_corner: TopRightCorner,
        }
    }

    /// The bottom row.
    pub fn bottom() -> EdgeSlots {
        use MarginBoxName::*;
        EdgeSlots {
            start_corner: BottomLeftCorner,
            start: BottomLeft,
            middle: BottomCenter,
            end: BottomRight,
            end_corner: BottomRightCorner,
        }
    }

    /// The left column; its "variable" axis is vertical (`@left-top` is the
    /// start of the edge, `@left-bottom` the end).
    pub fn left() -> EdgeSlots {
        use MarginBoxName::*;
        EdgeSlots {
            start_corner: TopLeftCorner,
            start: LeftTop,
            middle: LeftMiddle,
            end: LeftBottom,
            end_corner: BottomLeftCorner,
        }
    }

    /// The right column.
    pub fn right() -> EdgeSlots {
        use MarginBoxName::*;
        EdgeSlots {
            start_corner: TopRightCorner,
            start: RightTop,
            middle: RightMiddle,
            end: RightBottom,
            end_corner: BottomRightCorner,
        }
    }
}

/// Resolve a declared length against the containing block extent along its
/// axis. `None` (not declared), `auto` and `inherit` all resolve to `auto` —
/// a margin box has no parent to inherit from in this engine.
fn resolve_len(l: Option<PageLength>, available: Scalar) -> Option<Scalar> {
    match l? {
        PageLength::Abs(v) => Some(v),
        PageLength::Percent(f) => Some(Scalar(available.get() * f)),
        PageLength::Auto | PageLength::Inherit => None,
    }
}

/// Padding never takes `auto`; an `auto`/unresolvable value is zero.
fn resolve_pad(l: Option<PageLength>, available: Scalar) -> Scalar {
    resolve_len(l, available).unwrap_or(Scalar::ZERO)
}

/// The width of a declared border side (zero when undeclared).
fn border_width(b: Option<(Scalar, crate::css::Color)>) -> Scalar {
    b.map(|(w, _)| w).unwrap_or(Scalar::ZERO)
}

/// The box's declarations along the INLINE axis (§5.3.2's variable dimension).
fn horizontal_axis(mb: &MarginBoxSpec, m: BoxMetrics, available: Scalar) -> AxisBox {
    AxisBox {
        size: resolve_len(mb.width, available),
        margin_start: resolve_len(mb.margin_left, available),
        margin_end: resolve_len(mb.margin_right, available),
        border_start: border_width(mb.border_left),
        border_end: border_width(mb.border_right),
        padding_start: resolve_pad(mb.padding_left, available),
        padding_end: resolve_pad(mb.padding_right, available),
        min_content: m.min_inline,
        max_content: m.max_inline,
    }
}

/// The box's declarations along the BLOCK axis (§5.3.3's fixed dimension).
/// `inline` is the containing block's inline size, which padding percentages
/// resolve against even on this axis.
fn vertical_axis(mb: &MarginBoxSpec, m: BoxMetrics, available: Scalar, inline: Scalar) -> AxisBox {
    AxisBox {
        size: resolve_len(mb.height, available),
        margin_start: resolve_len(mb.margin_top, available),
        margin_end: resolve_len(mb.margin_bottom, available),
        border_start: border_width(mb.border_top),
        border_end: border_width(mb.border_bottom),
        padding_start: resolve_pad(mb.padding_top, inline),
        padding_end: resolve_pad(mb.padding_bottom, inline),
        min_content: m.block,
        max_content: m.block,
    }
}

/// Auto margins on a variable axis are zero (§5.3.2.1).
fn auto_zero(m: Option<Scalar>) -> Scalar {
    m.unwrap_or(Scalar::ZERO)
}

/// Assemble a placed box from its solved axes.
///
/// `hb`/`vb` supply the insets, `(h_ms, h_me)`/`(v_ms, v_me)` the used
/// margins. For a variable axis the caller passes the outer size separately
/// because §5.3.2 sizes the OUTER box; the border box is what remains.
#[allow(clippy::too_many_arguments)]
fn assemble(
    outer_x: Scalar,
    outer_y: Scalar,
    outer_w: Scalar,
    outer_h: Scalar,
    hb: &AxisBox,
    vb: &AxisBox,
    h_ms: Scalar,
    h_me: Scalar,
    v_ms: Scalar,
    v_me: Scalar,
) -> PlacedBox {
    PlacedBox {
        x: outer_x + h_ms,
        y: outer_y + v_ms,
        w: Scalar((outer_w - h_ms - h_me).get().max(0.0)),
        h: Scalar((outer_h - v_ms - v_me).get().max(0.0)),
        border: [
            vb.border_start,
            hb.border_end,
            vb.border_end,
            hb.border_start,
        ],
        padding: [
            vb.padding_start,
            hb.padding_end,
            vb.padding_end,
            hb.padding_start,
        ],
    }
}

/// Resolve every generated margin box's used geometry for one page.
///
/// `margins` is `[top, right, bottom, left]` and `content` is the page area as
/// `(x, y, width, height)`. The result is indexed like `boxes`.
///
/// The available size for a row's edge boxes is the page AREA's extent along
/// that row: the corner boxes take the side margins, and the three edge boxes
/// share what is left (css-page-3 §5.3.1's containing block for a non-corner
/// margin box). This matches the WPT `css-page/margin-boxes` references, which
/// paint corner boxes over the side margins and size edge boxes to the page
/// area.
pub fn place_boxes(
    boxes: &[MarginBoxSpec],
    metrics: &[BoxMetrics],
    page_w: Scalar,
    page_h: Scalar,
    content: (Scalar, Scalar, Scalar, Scalar),
    margins: [Scalar; 4],
) -> Vec<Option<PlacedBox>> {
    let (cx, cy, cw, ch) = content;
    let [mt, mr, mb, ml] = margins;
    let mut out: Vec<Option<PlacedBox>> = vec![None; boxes.len()];
    let at = |name: MarginBoxName| boxes.iter().position(|b| b.name == name);

    // ---- rows: top and bottom -------------------------------------------------
    let rows = [
        // slots, band, band origin y, ignore END margin when over-constrained
        (EdgeSlots::top(), mt, Scalar::ZERO, false),
        (
            EdgeSlots::bottom(),
            mb,
            Scalar(page_h.get() - mb.get()),
            true,
        ),
    ];
    for (slots, band, band_y, ignore_end) in rows {
        let names = [slots.start, slots.middle, slots.end];
        let idx: [Option<usize>; 3] = [at(names[0]), at(names[1]), at(names[2])];
        let hb: [Option<AxisBox>; 3] =
            std::array::from_fn(|i| idx[i].map(|i| horizontal_axis(&boxes[i], metrics[i], cw)));
        let used = resolve_variable([hb[0].as_ref(), hb[1].as_ref(), hb[2].as_ref()], cw);
        let outer_x = [
            cx,
            Scalar(cx.get() + (cw.get() - used[1].get()) * 0.5),
            Scalar(cx.get() + cw.get() - used[2].get()),
        ];
        for i in 0..3 {
            let (Some(bi), Some(hb)) = (idx[i], hb[i].as_ref()) else {
                continue;
            };
            let vb = vertical_axis(&boxes[bi], metrics[bi], band, cw);
            let (v_ms, _, v_me) = resolve_fixed(&vb, band, ignore_end);
            out[bi] = Some(assemble(
                outer_x[i],
                band_y,
                used[i],
                band,
                hb,
                &vb,
                auto_zero(hb.margin_start),
                auto_zero(hb.margin_end),
                v_ms,
                v_me,
            ));
        }
        // Corners: BOTH axes are the fixed dimension, sized by the page
        // margins that meet there.
        for (corner, corner_x, corner_w, ignore_end_h) in [
            (slots.start_corner, Scalar::ZERO, ml, false),
            (slots.end_corner, Scalar(page_w.get() - mr.get()), mr, true),
        ] {
            let Some(bi) = at(corner) else { continue };
            let hb = horizontal_axis(&boxes[bi], metrics[bi], corner_w);
            let vb = vertical_axis(&boxes[bi], metrics[bi], band, corner_w);
            let (h_ms, _, h_me) = resolve_fixed(&hb, corner_w, ignore_end_h);
            let (v_ms, _, v_me) = resolve_fixed(&vb, band, ignore_end);
            out[bi] = Some(assemble(
                corner_x, band_y, corner_w, band, &hb, &vb, h_ms, h_me, v_ms, v_me,
            ));
        }
    }

    // ---- columns: left and right ---------------------------------------------
    let cols = [
        (EdgeSlots::left(), ml, Scalar::ZERO, false),
        (
            EdgeSlots::right(),
            mr,
            Scalar(page_w.get() - mr.get()),
            true,
        ),
    ];
    for (slots, band, band_x, ignore_end) in cols {
        let names = [slots.start, slots.middle, slots.end];
        let idx: [Option<usize>; 3] = [at(names[0]), at(names[1]), at(names[2])];
        let vb: [Option<AxisBox>; 3] =
            std::array::from_fn(|i| idx[i].map(|i| vertical_axis(&boxes[i], metrics[i], ch, band)));
        let used = resolve_variable([vb[0].as_ref(), vb[1].as_ref(), vb[2].as_ref()], ch);
        let outer_y = [
            cy,
            Scalar(cy.get() + (ch.get() - used[1].get()) * 0.5),
            Scalar(cy.get() + ch.get() - used[2].get()),
        ];
        for i in 0..3 {
            let (Some(bi), Some(vb)) = (idx[i], vb[i].as_ref()) else {
                continue;
            };
            let hb = horizontal_axis(&boxes[bi], metrics[bi], band);
            let (h_ms, _, h_me) = resolve_fixed(&hb, band, ignore_end);
            out[bi] = Some(assemble(
                band_x,
                outer_y[i],
                band,
                used[i],
                &hb,
                vb,
                h_ms,
                h_me,
                auto_zero(vb.margin_start),
                auto_zero(vb.margin_end),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sb(size: Option<f64>, min: f64, max: f64) -> AxisBox {
        AxisBox {
            size: size.map(Scalar),
            min_content: Scalar(min),
            max_content: Scalar(max),
            ..AxisBox::default()
        }
    }

    /// dimensions-006 §5.3.2.2 case 2: both side boxes auto, the max-content
    /// total overflows the available size while the min-content total does
    /// not, so the leftover is shared in proportion to `max − min`.
    #[test]
    fn distributes_between_mins_when_max_overflows() {
        // Left min 4em max 17em, right min 2em max 5em, available 20em
        // (1em = 12pt) -> left 15.375em, right 4.625em.
        let left = sb(None, 48.0, 204.0);
        let right = sb(None, 24.0, 60.0);
        let used = resolve_variable([Some(&left), None, Some(&right)], Scalar(240.0));
        assert!(
            (used[0].get() - 184.5).abs() < 0.01,
            "left {}",
            used[0].get()
        );
        assert!(
            (used[2].get() - 55.5).abs() < 0.01,
            "right {}",
            used[2].get()
        );
        assert!((used[0] + used[2]).get() == 240.0);
    }

    /// dimensions-006 §5.3.2.2 case 1: the max-content total fits, so the
    /// boxes start at max-content and share the leftover proportionally.
    #[test]
    fn distributes_above_max_content_proportionally() {
        let left = sb(None, 0.0, 120.0);
        let right = sb(None, 0.0, 40.0);
        let used = resolve_variable([Some(&left), None, Some(&right)], Scalar(200.0));
        // flex space 40 split 120:40 -> +30 / +10.
        assert!(
            (used[0].get() - 150.0).abs() < 0.01,
            "left {}",
            used[0].get()
        );
        assert!(
            (used[2].get() - 50.0).abs() < 0.01,
            "right {}",
            used[2].get()
        );
    }

    /// A single auto box in the row takes the whole edge.
    #[test]
    fn single_generated_box_spans_the_edge() {
        let center = sb(None, 10.0, 30.0);
        let used = resolve_variable([None, Some(&center), None], Scalar(352.0));
        assert_eq!(used[1], Scalar(352.0));
    }

    /// With the middle box generated the side boxes split what it leaves, so
    /// the middle box stays centered.
    #[test]
    fn middle_box_centers_and_sides_split_the_rest() {
        let center = sb(None, 20.0, 40.0);
        let used = resolve_variable([None, Some(&center), None], Scalar(200.0));
        // B resolves first (alone against an empty AC), then each side gets
        // half of nothing because B took the whole edge.
        assert_eq!(used[1], Scalar(200.0));
        assert_eq!(used[0], Scalar::ZERO);
        assert_eq!(used[2], Scalar::ZERO);

        let left = sb(None, 0.0, 0.0);
        let right = sb(None, 0.0, 0.0);
        let used = resolve_variable([Some(&left), Some(&center), Some(&right)], Scalar(300.0));
        assert_eq!(used[1], Scalar(300.0));
        assert_eq!(used[0], Scalar::ZERO);
        assert_eq!(used[2], Scalar::ZERO);
    }

    /// An EMPTY middle box with no side boxes still takes the whole edge
    /// (background-001: `@top-center { content: ""; background: url(...) }`
    /// paints a full-width band; Chrome renders it full-width, and the 50/50
    /// "assumed 1" fallback would hand half the edge to a box that does not
    /// exist).
    #[test]
    fn empty_middle_box_alone_takes_the_whole_edge() {
        let center = sb(None, 0.0, 0.0);
        let used = resolve_variable([None, Some(&center), None], Scalar(264.0));
        assert_eq!(used[1], Scalar(264.0), "empty lone middle box spans the edge");
        assert_eq!(used[0], Scalar::ZERO);
        assert_eq!(used[2], Scalar::ZERO);
    }

    /// A declared size is never redistributed.
    #[test]
    fn declared_sizes_are_kept() {
        let left = sb(Some(100.0), 0.0, 0.0);
        let right = sb(None, 0.0, 0.0);
        let used = resolve_variable([Some(&left), None, Some(&right)], Scalar(300.0));
        assert_eq!(used[0], Scalar(100.0));
        assert_eq!(used[2], Scalar(200.0), "the auto side takes the remainder");
    }

    /// §5.3.3: a declared height with both margins auto centers the box in the
    /// band; the three parts plus the box's borders/padding fill it exactly.
    #[test]
    fn fixed_dimension_centers_with_auto_margins() {
        let b = AxisBox {
            size: Some(Scalar(50.0)),
            ..AxisBox::default()
        };
        let (ms, h, me) = resolve_fixed(&b, Scalar(100.0), false);
        assert_eq!((ms, h, me), (Scalar(25.0), Scalar(50.0), Scalar(25.0)));
    }

    /// §5.3.3 rule 5: an auto height takes the band minus the margins, and
    /// auto margins resolve to zero.
    #[test]
    fn fixed_dimension_auto_height_fills_the_band() {
        let b = AxisBox::default();
        let (ms, h, me) = resolve_fixed(&b, Scalar(64.0), false);
        assert_eq!((ms, h, me), (Scalar::ZERO, Scalar(64.0), Scalar::ZERO));
    }

    /// §5.3.3 rule 3: with size and both margins declared the top box drops
    /// its START margin, the bottom box its END margin.
    #[test]
    fn fixed_dimension_overconstrained_drops_the_ignored_edge() {
        let b = AxisBox {
            size: Some(Scalar(40.0)),
            margin_start: Some(Scalar(10.0)),
            margin_end: Some(Scalar(10.0)),
            ..AxisBox::default()
        };
        let (ms, h, me) = resolve_fixed(&b, Scalar(100.0), false);
        assert_eq!((ms, h, me), (Scalar(50.0), Scalar(40.0), Scalar(10.0)));
        let (ms, h, me) = resolve_fixed(&b, Scalar(100.0), true);
        assert_eq!((ms, h, me), (Scalar(10.0), Scalar(40.0), Scalar(50.0)));
    }

    /// Borders and padding count toward the band, so the content box shrinks.
    #[test]
    fn fixed_dimension_accounts_for_border_and_padding() {
        let b = AxisBox {
            size: Some(Scalar(30.0)),
            border_start: Scalar(2.25),
            border_end: Scalar(2.25),
            padding_start: Scalar(5.0),
            padding_end: Scalar(5.0),
            ..AxisBox::default()
        };
        let (ms, h, me) = resolve_fixed(&b, Scalar(100.0), false);
        assert_eq!(h, Scalar(30.0));
        assert!(((ms + me).get() - 55.5).abs() < 0.01, "{}", (ms + me).get());
    }
}
