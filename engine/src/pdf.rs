//! PDF export via krilla.
//!
//! One PDF page per laid-out page. Block backgrounds are filled rectangles;
//! text is drawn with krilla's high-level text API using a single embedded
//! font. krilla's surface uses a top-left origin (y grows downward), which
//! matches our layout coordinate system, so no axis flip is needed.
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

    // Disable tagging: the skeleton emits no semantic structure, and turning it
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

        // Backgrounds first, then text on top.
        for br in &page.rects {
            let rect = Rect::from_xywh(
                br.rect.x.to_f32(),
                br.rect.y.to_f32(),
                br.rect.width.to_f32(),
                br.rect.height.to_f32(),
            )
            .ok_or_else(|| anyhow!("invalid background rect"))?;
            let mut pb = krilla::geom::PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                surface.set_fill(Some(solid_fill(br.color)));
                surface.draw_path(&path);
            }
        }

        for line in &page.lines {
            surface.set_fill(Some(solid_fill(line.color)));
            surface.draw_text(
                Point::from_xy(line.origin.x.to_f32(), line.origin.y.to_f32()),
                font.clone(),
                line.font_size.to_f32(),
                &line.text,
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
