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
//! wall-clock), we set no `Metadata` (so no `CreationDate`), and we always
//! embed the exact same font bytes. Identical input therefore yields identical
//! output bytes.

use anyhow::{anyhow, Context, Result};
use krilla::color::rgb;
use krilla::geom::{Point, Rect};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::text::{Font, TextDirection};
use krilla::destination::XyzDestination;
use krilla::outline::{Outline, OutlineNode};
use krilla::{Document, SerializeSettings};

use crate::css::Color;
use crate::frag::{Fragment, FragmentContent};
use crate::layout::Layout;

/// A fixed macOS system font, embedded for deterministic output. Verified to
/// exist on this machine. (A later issue will ship a bundled font / use
/// fontique for portable discovery.)
const FONT_PATH: &str = "/System/Library/Fonts/Supplemental/Arial.ttf";

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

/// Load the embedded font once.
fn load_font() -> Result<Font> {
    let data =
        std::fs::read(FONT_PATH).with_context(|| format!("reading embedded font {FONT_PATH}"))?;
    Font::new(data.into(), 0).ok_or_else(|| anyhow!("krilla failed to parse font {FONT_PATH}"))
}

/// Render a paginated layout to PDF bytes.
pub fn render(layout: &Layout) -> Result<Vec<u8>> {
    let font = load_font()?;

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

        // Two passes over the fragment tree so backgrounds sit under text.
        let mut backgrounds: Vec<(f32, f32, f32, f32, Color)> = Vec::new();
        let mut texts: Vec<TextItem> = Vec::new();
        collect(&page.root, 0.0, 0.0, &mut backgrounds, &mut texts);

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

        for t in &texts {
            surface.set_fill(Some(solid_fill(t.color)));
            surface.draw_text(
                Point::from_xy(t.x, t.y),
                font.clone(),
                t.font_size,
                &t.text,
                false,
                TextDirection::Auto,
            );
        }

        surface.finish();
        pdf_page.finish();
    }

    // PDF bookmarks: nest the DOM-order headings by level and emit them.
    if let Some(outline) = build_outline(layout) {
        document.set_outline(outline);
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

/// One text draw call, resolved to absolute page coordinates.
struct TextItem {
    x: f32,
    y: f32,
    font_size: f32,
    color: Color,
    text: String,
}

/// Pre-order walk accumulating absolute offsets from parent-relative fragment
/// geometry. Backgrounds (block fills) are collected before text so paint order
/// is backgrounds-under-text within each page.
fn collect(
    frag: &Fragment,
    parent_x: f32,
    parent_y: f32,
    backgrounds: &mut Vec<(f32, f32, f32, f32, Color)>,
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
        FragmentContent::Text(run) => {
            // The run's baseline is parent-relative; rebase off the same parent.
            texts.push(TextItem {
                x: parent_x + run.baseline.x.to_f32(),
                y: parent_y + run.baseline.y.to_f32(),
                font_size: run.font_size.to_f32(),
                color: run.color,
                text: run.text.clone(),
            });
        }
        FragmentContent::None => {}
    }

    for child in &frag.children {
        collect(child, abs_x, abs_y, backgrounds, texts);
    }
}
