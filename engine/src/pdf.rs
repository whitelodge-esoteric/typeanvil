//! PDF export via krilla.
//!
//! One PDF page per fragmentainer. The emitter is a plain pre-order walk of the
//! fragment tree: each fragmentainer's descendant fragments are visited,
//! accumulating absolute positions (fragment offsets are parent-relative), and
//! drawn — `Block` backgrounds first, then `Line` text on top, matching the
//! layout paint order. There is no second pagination pass.
//!
//! krilla's surface uses a top-left origin (y grows downward), which matches
//! our layout coordinate system, so no axis flip is needed.
//!
//! Determinism: krilla derives the PDF `/ID` from a stable content hash (no
//! wall-clock), metadata is written only when the document declares it and is
//! input-derived only — never `creation_date`, `creator`, or `producer` (so no
//! `CreationDate` appears anywhere) — and we always embed the exact same font
//! bytes. Identical input therefore yields identical output bytes.

use anyhow::{anyhow, Context, Result};
use krilla::color::rgb;
use krilla::geom::{Point, Rect};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::text::{Font, GlyphId, KrillaGlyph, TextDirection};
use krilla::destination::XyzDestination;
use krilla::metadata::Metadata;
use krilla::outline::{Outline, OutlineNode};
use krilla::{Document, SerializeSettings};
use std::sync::LazyLock;

use crate::css::Color;
use crate::frag::{Fragment, FragmentContent};
use crate::layout::Layout;
use crate::typography::ShapedGlyph;

fn to_krilla_color(c: Color) -> rgb::Color {
    rgb::Color::new(c.r, c.g, c.b)
}

fn solid_fill(c: Color) -> Fill {
    Fill {
        paint: to_krilla_color(c).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

/// A fixed macOS system font bundle, embedded for deterministic output.
/// Verified to exist on this machine. (A later issue will ship a bundled font
/// / use fontique for portable discovery.)
static FACE_FONTS: [LazyLock<Option<Font>>; 4] = [
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FontFace::Regular);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FontFace::Bold);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FontFace::Italic);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FontFace::BoldItalic);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
];

fn font_for(face: crate::fonts::FontFace) -> Result<&'static Font> {
    let path = crate::fonts::face_path(face);
    FACE_FONTS[face as usize]
        .as_ref()
        .ok_or_else(|| anyhow!("krilla failed to parse font {path}"))
}

/// Document-level metadata derived from the HTML input (CORE-105).
///
/// All fields are input-derived only: the engine never sets `creation_date`,
/// `creator`, or `producer`, so identical input stays byte-identical.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>, // single author → vec![author]
    pub subject: Option<String>,
    pub keywords: Vec<String>,
}

/// Render a paginated layout to PDF bytes, without document metadata.
///
/// Existing callers keep this signature unchanged; behavior is identical to
/// `render_with_metadata` with `DocumentMetadata::default()`.
pub fn render(layout: &Layout) -> Result<Vec<u8>> {
    render_with_metadata(layout, &DocumentMetadata::default())
}

/// Render a paginated layout to PDF bytes, applying document metadata.
///
/// When `meta` carries any non-empty field, it is written to the PDF's Info
/// dict (Title/Author/Subject/Keywords) before finishing. `creation_date` is
/// never set, keeping output deterministic.
pub fn render_with_metadata(layout: &Layout, meta: &DocumentMetadata) -> Result<Vec<u8>> {

    // Disable tagging: the engine emits no semantic structure, and turning it
    // off keeps output smaller and free of an empty tag tree.
    let settings = SerializeSettings {
        enable_tagging: false,
        ..SerializeSettings::default()
    };
    let mut document = Document::new_with(settings);

    for page in &layout.pages {
        // Each fragmentainer carries its own resolved page size (an `@page`
        // rule may override the CLI default per page).
        let page_w = page.root.size.0.to_f32();
        let page_h = page.root.size.1.to_f32();
        let settings = PageSettings::from_wh(page_w, page_h)
            .ok_or_else(|| anyhow!("invalid page size {page_w}x{page_h}"))?;
        let mut pdf_page = document.start_page_with(settings);
        let mut surface = pdf_page.surface();

        // Page box background (CORE-66): a full-page fill from the resolved
        // `@page` background, painted under everything else.
        if let Some(bg) = page.background {
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, page_w, page_h) {
                let mut pb = krilla::geom::PathBuilder::new();
                pb.push_rect(rect);
                if let Some(path) = pb.finish() {
                    surface.set_fill(Some(solid_fill(bg)));
                    surface.draw_path(&path);
                }
            }
        }

        // `page-orientation` (CORE-66): rotate the laid-out content within the
        // page box. The layout itself is unrotated; the transform maps content
        // coordinates (top-left origin, y down) into their rotated positions.
        // rotate-right: (x, y) -> (H - y, x); rotate-left: (x, y) -> (y, W - x);
        // rotate-top/bottom: 180°, (x, y) -> (W - x, H - y).
        let rotated = if let Some(orient) = page.page_orientation {
            let (sx, ky, kx, sy, tx, ty) = match orient {
                crate::paged::PageOrientation::RotateRight => {
                    (0.0, 1.0, -1.0, 0.0, page_h, 0.0)
                }
                crate::paged::PageOrientation::RotateLeft => {
                    (0.0, -1.0, 1.0, 0.0, 0.0, page_w)
                }
                crate::paged::PageOrientation::RotateTop
                | crate::paged::PageOrientation::RotateBottom => {
                    (-1.0, 0.0, 0.0, -1.0, page_w, page_h)
                }
            };
            surface.push_transform(&krilla::geom::Transform::from_row(
                sx, ky, kx, sy, tx, ty,
            ));
            true
        } else {
            false
        };

        // Two passes over the fragment tree so backgrounds sit under text.
        let mut backgrounds: Vec<(f32, f32, f32, f32, Color)> = Vec::new();
        let mut borders: Vec<(f32, f32, f32, f32, f32, f32, f32, f32, Color)> = Vec::new();
        let mut texts: Vec<TextItem> = Vec::new();
        collect(
            &page.root,
            0.0,
            0.0,
            &mut backgrounds,
            &mut borders,
            &mut texts,
        );

        for (x, y, w, h, color) in &backgrounds {
            let rect = Rect::from_xywh(*x, *y, *w, *h)
                .ok_or_else(|| anyhow!("invalid background rect"))?;
            let mut pb = krilla::geom::PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                surface.set_fill(Some(solid_fill(*color)));
                surface.draw_path(&path);
            }
        }

        // Borders after backgrounds, before text: stroke each side whose
        // width > 0 as a thin filled rect (deterministic, no stroke state).
        for (x, y, w, h, t, r, b, l, color) in &borders {
            let fill = solid_fill(*color);
            let mut side = |px: f32, py: f32, pw: f32, ph: f32| {
                if pw <= 0.0 || ph <= 0.0 {
                    return;
                }
                if let Some(rect) = Rect::from_xywh(px, py, pw, ph) {
                    let mut pb = krilla::geom::PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        surface.set_fill(Some(fill.clone()));
                        surface.draw_path(&path);
                    }
                }
            };
            if *t > 0.0 {
                side(*x, *y, *w, *t);
            }
            if *b > 0.0 {
                side(*x, *y + h - b, *w, *b);
            }
            if *l > 0.0 {
                side(*x, *y, *l, *h);
            }
            if *r > 0.0 {
                side(*x + w - r, *y, *r, *h);
            }
        }

        for t in &texts {
            let font = font_for(t.font_face)?;
            surface.set_fill(Some(solid_fill(t.color)));
            if t.glyphs.is_empty() {
                // Fallback text path (empty run — nothing shaped). All normal
                // runs — body text, generated content, margin boxes — carry
                // shaped glyphs (CORE-85, CORE-83).
                surface.draw_text(
                    Point::from_xy(t.x, t.y),
                    font.clone(),
                    t.font_size,
                    &t.text,
                    false,
                    TextDirection::Auto,
                );
            } else {
                // Main text: shaped glyphs, with the typography layer's
                // protrusion (hang into the margin) and per-line expansion
                // (scale every advance by 1+expansion) applied at draw time.
                let glyphs: Vec<KrillaGlyph> =
                    to_krilla_glyphs(&t.glyphs, t.font_size, t.expansion, t.protrude_right);
                surface.draw_glyphs(
                    Point::from_xy(t.x - t.protrude_left, t.y),
                    &glyphs,
                    font.clone(),
                    &t.text,
                    t.font_size,
                    false,
                );
            }
        }

        // Pop any pushed graphics state (page-orientation rotation) before
        // finishing the page — krilla asserts a balanced push/pop.
        if rotated {
            surface.pop();
        }

        surface.finish();
        pdf_page.finish();
    }

    // PDF bookmarks: nest the DOM-order headings by level and emit them.
    if let Some(outline) = build_outline(layout) {
        document.set_outline(outline);
    }

    // Document metadata (CORE-105): written only when at least one field is
    // non-empty — krilla then emits the Info dict. No `creation_date`,
    // `creator`, or `producer` is ever set.
    if meta.title.is_some()
        || !meta.authors.is_empty()
        || meta.subject.is_some()
        || !meta.keywords.is_empty()
    {
        let mut m = Metadata::new();
        if let Some(title) = &meta.title {
            m = m.title(title.clone());
        }
        if !meta.authors.is_empty() {
            m = m.authors(meta.authors.clone());
        }
        if let Some(subject) = &meta.subject {
            m = m.description(subject.clone());
        }
        if !meta.keywords.is_empty() {
            m = m.keywords(meta.keywords.clone());
        }
        document.set_metadata(m);
    }

    document
        .finish()
        .map_err(|e| anyhow!("krilla export failed: {e:?}"))
}

/// Build a krilla [`Outline`] from the heading structure, nesting entries by
/// level (an `h2` after an `h1` becomes the `h1`'s child, etc.). Deeper-than-6
/// levels cannot occur (only `h1`–`h6` are collected). Returns `None` when the
/// document has no headings.
fn build_outline(layout: &Layout) -> Option<Outline> {
    if layout.headings.is_empty() {
        return None;
    }
    // A stack of (level, node) being built. We attach a finished node to its
    // parent (the nearest shallower entry) when the next heading is not deeper.
    let mut roots: Vec<OutlineNode> = Vec::new();
    // Stack holds indices into a side vec of nodes plus their level. We build
    // recursively via an explicit stack of (level, OutlineNode).
    let mut stack: Vec<(u8, OutlineNode)> = Vec::new();

    for h in &layout.headings {
        let dest = XyzDestination::new(h.page_index, Point::from_xy(0.0, 0.0));
        let node = OutlineNode::new(h.title.clone(), dest);
        // Pop deeper-or-equal entries off the stack, attaching each to the one
        // below it (or to roots).
        while let Some((lvl, _)) = stack.last() {
            if *lvl >= h.level {
                let (_, finished) = stack.pop().unwrap();
                match stack.last_mut() {
                    Some((_, parent)) => parent.push_child(finished),
                    None => roots.push(finished),
                }
            } else {
                break;
            }
        }
        stack.push((h.level, node));
    }
    // Drain the remaining stack bottom-up.
    while let Some((_, finished)) = stack.pop() {
        match stack.last_mut() {
            Some((_, parent)) => parent.push_child(finished),
            None => roots.push(finished),
        }
    }

    let mut outline = Outline::new();
    for r in roots {
        outline.push_child(r);
    }
    Some(outline)
}

/// Map shaped glyphs (advances in points at the run's font size) to krilla's
/// normalized-glyph representation. The per-line expansion factor scales every
/// advance by `1 + expansion` (font expansion, spec Behavior §8); krilla's
/// `draw_glyphs` multiplies by the font size itself, so we divide by it.
fn to_krilla_glyphs(
    glyphs: &[ShapedGlyph],
    font_size: f32,
    expansion: f32,
    protrude_right: f32,
) -> Vec<KrillaGlyph> {
    let scale = (1.0 + expansion) / font_size;
    let mut out: Vec<KrillaGlyph> = glyphs
        .iter()
        .map(|g| {
            KrillaGlyph::new(
                GlyphId::new(g.id),
                g.x_advance.to_f32() * scale,
                g.x_offset.to_f32() / font_size,
                0.0,
                0.0,
                // `ShapedGlyph.range` is the byte range of the glyph's cluster
                // in the run's text; krilla slices the text by it to build the
                // PDF ToUnicode map (CORE-85). Empty ranges would produce an
                // empty map and garbage text extraction.
                g.range.clone(),
                None,
            )
        })
        .collect();
    // Optical right hang: the last glyph's advance grows by the protrusion so
    // the punctuation's drawn ink crosses the content edge (draw-time only).
    if let Some(last) = out.last_mut() {
        let adv = last.x_advance;
        last.x_advance = adv + protrude_right / font_size;
    }
    out
}
/// One text draw call, resolved to absolute page coordinates.
///
/// All runs carry their shaped glyphs (body text, generated content, and
/// margin boxes — CORE-85, CORE-83); `draw_text` is only a fallback for
/// degenerate empty runs.
struct TextItem {
    x: f32,
    y: f32,
    font_size: f32,
    color: Color,
    font_face: crate::fonts::FontFace,
    text: String,
    glyphs: Vec<ShapedGlyph>,
    expansion: f32,
    protrude_left: f32,
    protrude_right: f32,
}

/// Pre-order walk accumulating absolute offsets from parent-relative fragment
/// geometry. Backgrounds (block fills) are collected before text so paint order
/// is backgrounds-under-text within each page.
fn collect(
    frag: &Fragment,
    parent_x: f32,
    parent_y: f32,
    backgrounds: &mut Vec<(f32, f32, f32, f32, Color)>,
    borders: &mut Vec<(f32, f32, f32, f32, f32, f32, f32, f32, Color)>,
    texts: &mut Vec<TextItem>,
) {
    let abs_x = parent_x + frag.offset.x.to_f32();
    let abs_y = parent_y + frag.offset.y.to_f32();

    match &frag.content {
        FragmentContent::Background(color) => {
            backgrounds.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                *color,
            ));
        }
        FragmentContent::Border(b) => {
            borders.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                b.top.to_f32(),
                b.right.to_f32(),
                b.bottom.to_f32(),
                b.left.to_f32(),
                b.color,
            ));
        }
        FragmentContent::Text(run) => {
            // The run's baseline is parent-relative; rebase off the same parent.
            texts.push(TextItem {
                x: parent_x + run.baseline.x.to_f32(),
                y: parent_y + run.baseline.y.to_f32(),
                font_size: run.font_size.to_f32(),
                color: run.color,
                font_face: run.font_face,
                text: run.text.clone(),
                glyphs: run.glyphs.clone(),
                expansion: run.expansion as f32,
                protrude_left: run.protrude_left.to_f32(),
                protrude_right: run.protrude_right.to_f32(),
            });
        }
        FragmentContent::None => {}
    }

    for child in &frag.children {
        collect(child, abs_x, abs_y, backgrounds, borders, texts);
    }
}
