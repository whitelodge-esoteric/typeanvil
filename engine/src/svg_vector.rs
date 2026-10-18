// SPDX-License-Identifier: AGPL-3.0-only

//! Vector SVG embedding (CORE-522).
//!
//! An interned SVG keeps its source text next to the PNG raster fallback
//! (`StoredImage::svg_source`). At emit time [`draw_svg`] parses the source
//! with usvg and replays the tree as krilla paths, so the PDF carries vector
//! operators instead of an image XObject.
//!
//! Supported: groups (transform, opacity), paths with solid fill/stroke
//! (fill rule, caps, joins, miter limit, dashes, paint order), and text
//! (usvg's flattened outline group). Anything else (gradients, patterns,
//! clip paths, masks, filters, nested images, non-normal blend modes)
//! returns `false` from [`is_vector_safe`]; the caller then embeds the
//! raster. A partial vector draw never happens: the check runs first.
//!
//! Determinism: a pure function of the source bytes. The fontdb and font
//! family match the rasterizer in `images.rs`.

use krilla::geom::{PathBuilder, Size, Transform};
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke, StrokeDash};
use krilla::surface::Surface;
use resvg::usvg;

/// Parse the SVG source the same way the rasterizer does.
pub fn parse(source: &str) -> Option<usvg::Tree> {
    let mut opts = usvg::Options::default();
    opts.fontdb = crate::images::svg_fontdb();
    opts.font_family = "Arial".into();
    usvg::Tree::from_str(source, &opts).ok()
}

/// True when every node in the tree maps to a krilla vector operation.
pub fn is_vector_safe(tree: &usvg::Tree) -> bool {
    group_safe(tree.root())
}

fn group_safe(g: &usvg::Group) -> bool {
    if g.clip_path().is_some()
        || g.mask().is_some()
        || !g.filters().is_empty()
        || g.blend_mode() != usvg::BlendMode::Normal
    {
        return false;
    }
    g.children().iter().all(|n| match n {
        usvg::Node::Group(c) => group_safe(c),
        usvg::Node::Path(p) => path_safe(p),
        usvg::Node::Text(t) => group_safe(t.flattened()),
        usvg::Node::Image(_) => false,
    })
}

fn path_safe(p: &usvg::Path) -> bool {
    let solid = |paint: &usvg::Paint| matches!(paint, usvg::Paint::Color(_));
    p.fill().map_or(true, |f| solid(f.paint())) && p.stroke().map_or(true, |s| solid(s.paint()))
}

/// Draw the tree scaled to `size` (CSS-pt box) at the surface's current
/// origin. Caller must have checked [`is_vector_safe`].
pub fn draw_svg(surface: &mut Surface<'_>, tree: &usvg::Tree, size: Size) {
    let ts = tree.size();
    if ts.width() <= 0.0 || ts.height() <= 0.0 {
        return;
    }
    // Clip to the viewport so content outside the viewBox does not leak.
    if let Some(rect) = krilla::geom::Rect::from_xywh(0.0, 0.0, size.width(), size.height()) {
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        if let Some(clip) = pb.finish() {
            surface.push_clip_path(&clip, &FillRule::NonZero);
            surface.push_transform(&Transform::from_scale(
                size.width() / ts.width(),
                size.height() / ts.height(),
            ));
            draw_group(surface, tree.root());
            surface.pop();
            surface.pop();
        }
    }
}

fn draw_group(surface: &mut Surface<'_>, g: &usvg::Group) {
    for node in g.children() {
        match node {
            usvg::Node::Group(c) => {
                let t = c.transform();
                surface.push_transform(&to_transform(t));
                let op = c.opacity().get();
                let layered = op < 1.0;
                if layered {
                    surface.push_opacity(NormalizedF32::new(op).unwrap_or(NormalizedF32::ONE));
                }
                draw_group(surface, c);
                if layered {
                    surface.pop();
                }
                surface.pop();
            }
            usvg::Node::Path(p) => draw_path(surface, p),
            usvg::Node::Text(t) => draw_group(surface, t.flattened()),
            usvg::Node::Image(_) => {}
        }
    }
}

fn to_transform(t: usvg::Transform) -> Transform {
    Transform::from_row(t.sx, t.ky, t.kx, t.sy, t.tx, t.ty)
}

fn rgb(c: usvg::Color) -> krilla::color::rgb::Color {
    krilla::color::rgb::Color::new(c.red, c.green, c.blue)
}

fn draw_path(surface: &mut Surface<'_>, p: &usvg::Path) {
    if !p.is_visible() {
        return;
    }
    let mut pb = PathBuilder::new();
    for seg in p.data().segments() {
        use usvg::tiny_skia_path::PathSegment as S;
        match seg {
            S::MoveTo(a) => pb.move_to(a.x, a.y),
            S::LineTo(a) => pb.line_to(a.x, a.y),
            S::QuadTo(a, b) => pb.quad_to(a.x, a.y, b.x, b.y),
            S::CubicTo(a, b, c) => pb.cubic_to(a.x, a.y, b.x, b.y, c.x, c.y),
            S::Close => pb.close(),
        }
    }
    let Some(path) = pb.finish() else {
        return;
    };

    let fill = p.fill().and_then(|f| match f.paint() {
        usvg::Paint::Color(c) => Some(Fill {
            paint: rgb(*c).into(),
            opacity: NormalizedF32::new(f.opacity().get()).unwrap_or(NormalizedF32::ONE),
            rule: match f.rule() {
                usvg::FillRule::NonZero => FillRule::NonZero,
                usvg::FillRule::EvenOdd => FillRule::EvenOdd,
            },
        }),
        _ => None,
    });
    let stroke = p.stroke().and_then(|s| match s.paint() {
        usvg::Paint::Color(c) => Some(Stroke {
            paint: rgb(*c).into(),
            width: s.width().get(),
            miter_limit: s.miterlimit().get(),
            line_cap: match s.linecap() {
                usvg::LineCap::Butt => LineCap::Butt,
                usvg::LineCap::Round => LineCap::Round,
                usvg::LineCap::Square => LineCap::Square,
            },
            line_join: match s.linejoin() {
                usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
                usvg::LineJoin::Round => LineJoin::Round,
                usvg::LineJoin::Bevel => LineJoin::Bevel,
            },
            opacity: NormalizedF32::new(s.opacity().get()).unwrap_or(NormalizedF32::ONE),
            dash: s.dasharray().map(|a| StrokeDash {
                array: a.to_vec(),
                offset: s.dashoffset(),
            }),
        }),
        _ => None,
    });

    let stroke_first = p.paint_order() == usvg::PaintOrder::StrokeAndFill;
    let mut paint_fill = |surface: &mut Surface<'_>| {
        if let Some(f) = fill.clone() {
            surface.set_fill(Some(f));
            surface.set_stroke(None);
            surface.draw_path(&path);
        }
    };
    let paint_stroke = |surface: &mut Surface<'_>| {
        if let Some(s) = stroke.clone() {
            surface.set_fill(None);
            surface.set_stroke(Some(s));
            surface.draw_path(&path);
        }
    };
    if stroke_first {
        paint_stroke(surface);
        paint_fill(surface);
    } else {
        paint_fill(surface);
        paint_stroke(surface);
    }
    surface.set_fill(None);
    surface.set_stroke(None);
}
