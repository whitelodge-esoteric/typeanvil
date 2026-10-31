// SPDX-License-Identifier: AGPL-3.0-only

//! Root-vertical page progression (CORE-553, writing-modes-layout.spec.md
//! Behaviors 4-5, 11-15).
//!
//! When the root element's writing mode is vertical, the document's block
//! axis is physical x and its inline axis is physical y. The block path
//! paginates along y, so a root-vertical body is laid out in a LOGICAL
//! (transposed) frame: logical x = inline axis, logical y = block axis. The
//! existing fragmentation, break-token and declared-extent continuation code
//! then runs unchanged along the block axis, so page progression, break
//! tokens and termination guards are the horizontal ones.
//!
//! Two pure functions do the work:
//!
//! - [`transpose_styles`] copies every computed style and rotates the
//!   physical fields of each vertical box into the logical frame (sizes swap,
//!   box edges permute geometrically, viewport lengths swap axis).
//! - [`map_root`] maps one laid-out logical subtree to physical points,
//!   parent-relative, once per page (spec "Frame mapping"). Text lines get a
//!   90° rotation matrix so glyphs run along the physical inline axis.
//!
//! Page chrome (margin boxes, footnotes, fixed boxes, the canvas fill) never
//! enters the logical frame, so it stays physical. Horizontal (orthogonal)
//! subtrees inside a vertical root are NOT re-laid in a physical frame here;
//! that is CORE-554 (orthogonal flows).

use crate::css::{ComputedStyle, PageWritingMode, ViewportLen};
use crate::dom::{Dom, NodeId};
use crate::frag::{Fragment, FragmentContent};
use crate::geom::{Point, Rect, Scalar};

/// For each LOGICAL side `k` (0 top, 1 right, 2 bottom, 3 left) the PHYSICAL
/// side (same indices) it lands on, for a root frame in `mode`. Purely
/// geometric: direction never enters (the logical layout handles `rtl` on its
/// own inline axis, and the mapping carries it to the physical one).
pub fn logical_to_physical_sides(mode: PageWritingMode) -> [usize; 4] {
    match mode {
        // x_p = H - y - h, y_p = x.
        PageWritingMode::VerticalRl | PageWritingMode::SidewaysRl => [1, 2, 3, 0],
        // x_p = y, y_p = x.
        PageWritingMode::VerticalLr => [3, 2, 1, 0],
        // x_p = y, y_p = W - x - w.
        PageWritingMode::SidewaysLr => [3, 0, 1, 2],
        PageWritingMode::HorizontalTb => [0, 1, 2, 3],
    }
}

fn permute_in<T: Copy>(phys: [T; 4], map: [usize; 4]) -> [T; 4] {
    [phys[map[0]], phys[map[1]], phys[map[2]], phys[map[3]]]
}

fn permute_out<T: Copy>(logical: [T; 4], map: [usize; 4]) -> [T; 4] {
    let mut out = logical;
    for k in 0..4 {
        out[map[k]] = logical[k];
    }
    out
}

fn flip_vp(v: Option<ViewportLen>) -> Option<ViewportLen> {
    v.map(|mut l| {
        l.vw = !l.vw;
        l
    })
}

/// Rotate one computed style into the logical frame of a `mode` root.
pub fn transpose_style(s: &ComputedStyle, mode: PageWritingMode) -> ComputedStyle {
    let mut t = s.clone();
    if !mode.is_vertical() {
        return t;
    }
    let map = logical_to_physical_sides(mode);
    std::mem::swap(&mut t.width, &mut t.height);
    std::mem::swap(&mut t.width_percent, &mut t.height_percent);
    std::mem::swap(&mut t.width_viewport, &mut t.height_viewport);
    // Viewport units are physical (Behavior 22): a `vw` length on the
    // logical height still measures the physical page width, which is the
    // logical frame's HEIGHT.
    t.width_viewport = flip_vp(t.width_viewport);
    t.height_viewport = flip_vp(t.height_viewport);
    std::mem::swap(&mut t.min_width, &mut t.min_height);
    std::mem::swap(&mut t.max_width, &mut t.max_height);

    let [a, b, c, d] = permute_in([s.margin_top, s.margin_right, s.margin_bottom, s.margin_left], map);
    (t.margin_top, t.margin_right, t.margin_bottom, t.margin_left) = (a, b, c, d);
    let [a, b, c, d] = permute_in(
        [s.margin_top_auto, s.margin_right_auto, s.margin_bottom_auto, s.margin_left_auto],
        map,
    );
    (t.margin_top_auto, t.margin_right_auto, t.margin_bottom_auto, t.margin_left_auto) = (a, b, c, d);
    let [a, b, c, d] = permute_in([s.padding_top, s.padding_right, s.padding_bottom, s.padding_left], map);
    (t.padding_top, t.padding_right, t.padding_bottom, t.padding_left) = (a, b, c, d);
    let [a, b, c, d] = permute_in([s.border_top, s.border_right, s.border_bottom, s.border_left], map);
    (t.border_top, t.border_right, t.border_bottom, t.border_left) = (a, b, c, d);
    let [a, b, c, d] = permute_in(
        [s.border_top_color, s.border_right_color, s.border_bottom_color, s.border_left_color],
        map,
    );
    (t.border_top_color, t.border_right_color, t.border_bottom_color, t.border_left_color) = (a, b, c, d);
    t.border_styles = permute_in(s.border_styles, map);
    let [a, b, c, d] = permute_in([s.inset_top, s.inset_right, s.inset_bottom, s.inset_left], map);
    (t.inset_top, t.inset_right, t.inset_bottom, t.inset_left) = (a, b, c, d);
    let [a, b, c, d] = permute_in(
        [
            s.inset_top_viewport,
            s.inset_right_viewport,
            s.inset_bottom_viewport,
            s.inset_left_viewport,
        ],
        map,
    );
    (
        t.inset_top_viewport,
        t.inset_right_viewport,
        t.inset_bottom_viewport,
        t.inset_left_viewport,
    ) = (flip_vp(a), flip_vp(b), flip_vp(c), flip_vp(d));
    t
}

/// The mode in effect at `id`: nearest ancestor-or-self declaration, else the
/// root element's mode.
pub fn mode_at(dom: &Dom, styles: &[ComputedStyle], id: NodeId) -> PageWritingMode {
    let mut cur = Some(id);
    while let Some(n) = cur {
        if styles[n].writing_mode_declared {
            return styles[n].writing_mode;
        }
        cur = dom.nodes[n].parent;
    }
    styles[dom.find_tag("html").unwrap_or(dom.root)].writing_mode
}

/// CORE-561: add the vertical glyph-orientation pseudo-feature to a
/// logical-frame style, so its lines shape as vertical text (`mixed` and
/// `upright` set CJK upright through `typography::ORIENTATION_TAG`). The
/// orientation is the nearest ancestor-or-self `text-orientation`
/// (default `mixed`); the sideways modes rotate every glyph (no feature).
pub fn add_orientation_feature(
    dom: &Dom,
    styles: &[ComputedStyle],
    id: NodeId,
    mode: PageWritingMode,
    t: &mut ComputedStyle,
) {
    use crate::css::TextOrientation as O;
    if !matches!(mode, PageWritingMode::VerticalRl | PageWritingMode::VerticalLr) {
        return;
    }
    let mut orient = O::Mixed;
    let mut cur = Some(id);
    while let Some(n) = cur {
        if n >= dom.nodes.len() {
            break;
        }
        if let Some(o) = styles[n].text_orientation {
            orient = o;
            break;
        }
        cur = dom.nodes[n].parent;
    }
    let v = match orient {
        O::Mixed => crate::typography::ORIENT_MIXED,
        O::Upright => crate::typography::ORIENT_UPRIGHT,
        O::Sideways => return,
    };
    t.ot_features.retain(|&(tag, _)| tag != crate::typography::ORIENTATION_TAG);
    t.ot_features.push((crate::typography::ORIENTATION_TAG, v));
}

/// The logical-frame copy of `styles` for a `root_mode` root. Every box whose
/// mode in effect is vertical is rotated with the ROOT frame's permutation;
/// orthogonal (horizontal) boxes are copied as-is (CORE-554 owns them).
pub fn transpose_styles(
    dom: &Dom,
    styles: &[ComputedStyle],
    root_mode: PageWritingMode,
) -> Vec<ComputedStyle> {
    (0..styles.len())
        .map(|id| {
            let s = &styles[id];
            // Anonymous table wrappers (ids past the DOM) inherit nothing
            // here: copy them.
            if id >= dom.nodes.len() || !mode_at(dom, styles, id).is_vertical() {
                s.clone()
            } else {
                let mut t = transpose_style(s, root_mode);
                add_orientation_feature(dom, styles, id, mode_at(dom, styles, id), &mut t);
                t
            }
        })
        .collect()
}

/// Map a logical fragment (offset relative to its parent, whose LOGICAL size
/// is `pw` x `ph`) to physical coordinates in place.
fn map_frag(f: &mut Fragment, mode: PageWritingMode, pw: Scalar, ph: Scalar) {
    let (x, y, w, h) = (f.offset.x, f.offset.y, f.size.0, f.size.1);
    let (px, py) = match mode {
        PageWritingMode::VerticalRl | PageWritingMode::SidewaysRl => (ph - y - h, x),
        PageWritingMode::VerticalLr => (y, x),
        PageWritingMode::SidewaysLr => (y, pw - x - w),
        PageWritingMode::HorizontalTb => (x, y),
    };
    // CORE-554: an orthogonal `horizontal-tb` box already laid its subtree
    // out in physical coordinates: place it, swap its size, stop.
    if f.physical_frame {
        f.offset = Point::new(px, py);
        f.size = (h, w);
        return;
    }
    if let (FragmentContent::Text(run), None, PageWritingMode::VerticalRl
        | PageWritingMode::VerticalLr
        | PageWritingMode::SidewaysRl
        | PageWritingMode::SidewaysLr) = (&mut f.content, f.transform, mode)
    {
        // CORE-561: a text line becomes a native vertical line
        // (`Fragment::orientation`). Its baseline point is the start of the
        // central baseline: the em-box centre across the column, the
        // line's inline start along it. The em-box centre sits
        // `(ascent + descent) / 2` above the alphabetic baseline; the
        // line's logical top faces physical right (left in `sideways-lr`),
        // the same sense the old 90° line rotation used.
        let (asc, desc) = crate::typography::font_metrics(run.font_face);
        let centre_from_top =
            (run.baseline.y - y).get() - (asc + desc) * 0.5 * run.font_size.get();
        let inline_start = (run.baseline.x - x - run.protrude_left).get();
        let up = mode == PageWritingMode::SidewaysLr;
        let (bx, by) = if up {
            (px.get() + centre_from_top, py.get() + w.get() - inline_start)
        } else {
            (px.get() + h.get() - centre_from_top, py.get() + inline_start)
        };
        run.baseline = Point::new(Scalar(bx), Scalar(by));
        run.protrude_left = Scalar::ZERO;
        run.protrude_right = Scalar::ZERO;
        f.orientation = if up {
            crate::frag::LineOrientation::VerticalUp
        } else {
            crate::frag::LineOrientation::VerticalDown
        };
    } else if let FragmentContent::Text(run) = &mut f.content {
        // A text line keeps its LOGICAL interior and paints under a 90°
        // rotation about its own top-left: clockwise for `-rl`/`vertical-lr`
        // (Latin `text-orientation: mixed`), counter-clockwise for
        // `sideways-lr`. The baseline is parent-relative: rebase it on the
        // line's new physical origin, keeping its line-local offset.
        let lx = run.baseline.x - x;
        let ly = run.baseline.y - y;
        run.baseline = Point::new(px + lx, py + ly);
        let rot: crate::transform::Affine = match mode {
            PageWritingMode::SidewaysLr => [0.0, -1.0, 1.0, 0.0, 0.0, w.get()],
            _ => [0.0, 1.0, -1.0, 0.0, h.get(), 0.0],
        };
        f.transform = Some(match f.transform {
            Some(m) => crate::transform::mul(&rot, &m),
            None => rot,
        });
        // Children stay logical: the rotation maps them.
    } else {
        if let FragmentContent::Border(b) = &mut f.content {
            let map = logical_to_physical_sides(mode);
            let [t, r, bo, l] = permute_out([b.top, b.right, b.bottom, b.left], map);
            (b.top, b.right, b.bottom, b.left) = (t, r, bo, l);
            let [t, r, bo, l] =
                permute_out([b.top_color, b.right_color, b.bottom_color, b.left_color], map);
            (b.top_color, b.right_color, b.bottom_color, b.left_color) = (t, r, bo, l);
            b.styles = permute_out(b.styles, map);
        }
        for c in &mut f.children {
            map_frag(c, mode, w, h);
        }
    }
    f.offset = Point::new(px, py);
    f.size = (h, w);
}

/// Map one page-level logical subtree (offset relative to the logical frame
/// origin) into the physical content rect `phys`. The logical frame is
/// `phys.height` wide (inline) and `phys.width` tall (block).
pub fn map_root(f: &mut Fragment, mode: PageWritingMode, phys: Rect) {
    map_frag(f, mode, phys.height, phys.width);
    f.offset = Point::new(f.offset.x + phys.x, f.offset.y + phys.y);
}

/// CORE-554: the LOGICAL rect (in a `mode` frame) of a rectangle that sits
/// at physical offset `(dx, dy)` and size `(dw, dh)` inside a
/// `physical_frame` box whose logical border box is `(lx, ly, lw, lh)`.
/// Mapping the result with the frame (`map_rect` / `map_frag`) lands on
/// the box's physical origin plus `(dx, dy)`.
pub fn unmap_inner_rect(
    mode: PageWritingMode,
    bx: (Scalar, Scalar, Scalar, Scalar),
    r: (Scalar, Scalar, Scalar, Scalar),
) -> (Scalar, Scalar, Scalar, Scalar) {
    let (lx, ly, lw, lh) = bx;
    let (dx, dy, dw, dh) = r;
    match mode {
        PageWritingMode::VerticalRl | PageWritingMode::SidewaysRl => (lx + dy, ly + lh - dx - dw, dh, dw),
        PageWritingMode::VerticalLr => (lx + dy, ly + dx, dh, dw),
        PageWritingMode::SidewaysLr => (lx + lw - dy - dh, ly + dx, dh, dw),
        PageWritingMode::HorizontalTb => (lx + dx, ly + dy, dw, dh),
    }
}

/// Map a logical page-level rectangle `(x, y, w, h)` (link annotations) into
/// the physical content rect `phys`.
pub fn map_rect(
    mode: PageWritingMode,
    phys: Rect,
    r: (Scalar, Scalar, Scalar, Scalar),
) -> (Scalar, Scalar, Scalar, Scalar) {
    let mut f = Fragment::block(Point::new(r.0, r.1), (r.2, r.3));
    map_root(&mut f, mode, phys);
    (f.offset.x, f.offset.y, f.size.0, f.size.1)
}
