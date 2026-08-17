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

    let page_w = layout.geometry.width.to_f32();
    let page_h = layout.geometry.height.to_f32();

    for page in &layout.pages {
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

    document
        .finish()
        .map_err(|e| anyhow!("krilla export failed: {e:?}"))
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
