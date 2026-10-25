// SPDX-License-Identifier: AGPL-3.0-only

//! `inspect_page` raster (CORE-575): rasterize one page of the exact PDF
//! bytes `render` returns, with the pure-Rust `hayro` rasterizer, to PNG.
//!
//! Decision (mcp-server spec S9): rasterization lives in the engine, so the
//! single binary needs no helper process and the PNG shows the real PDF.

use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

/// PNG bytes and pixel size of page `index` (0-based) at `dpi`.
pub fn page_png(pdf: &[u8], index: usize, dpi: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let doc = Pdf::new(Arc::new(pdf.to_vec())).map_err(|e| format!("cannot load PDF: {e:?}"))?;
    let pages = doc.pages();
    let page = pages.get(index).ok_or_else(|| format!("no page {}", index + 1))?;
    let scale = dpi as f32 / 72.0;
    let pixmap = render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    let (w, h) = (pixmap.width() as u32, pixmap.height() as u32);
    let png = pixmap.into_png().map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok((png, w, h))
}
