// SPDX-License-Identifier: AGPL-3.0-only

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
use krilla::destination::XyzDestination;
use krilla::geom::{Point, Rect};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::tagging::{Artifact, ArtifactType, ContentTag};
use krilla::text::{Font, GlyphId, KrillaGlyph, TextDirection};
use krilla::{Document, SerializeSettings};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::css::Color;
use crate::dom::NodeId;
use crate::frag::{Fragment, FragmentContent};
use crate::layout::Layout;
use crate::typography::ShapedGlyph;

/// The marked-content tag for a draw owned by `owner`: real content when the
/// owner resolves to a DOM node, a Layout artifact otherwise. Artifacts get
/// dummy identifiers — they are excluded from the structure tree by design.
fn tag_for_owner(owner: Option<NodeId>) -> ContentTag<'static> {
    match owner {
        Some(_) => ContentTag::Other,
        None => ContentTag::Artifact(Artifact::with_kind(ArtifactType::Layout)),
    }
}

/// Record one tagged draw owned by a DOM node (CORE-111). Unsourced draws
/// carry no tree entry.
fn record_draw(
    draws: &mut Vec<crate::tags::DrawRef>,
    page: usize,
    ident: krilla::tagging::Identifier,
    owner: Option<NodeId>,
) {
    if let Some(source) = owner {
        draws.push(crate::tags::DrawRef {
            page,
            ident,
            source: Some(source),
        });
    }
}

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
        let path = crate::fonts::face_path(crate::fonts::FACE_REGULAR);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FACE_BOLD);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FACE_ITALIC);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
    LazyLock::new(|| {
        let path = crate::fonts::face_path(crate::fonts::FACE_BOLD_ITALIC);
        std::fs::read(path)
            .with_context(|| format!("reading embedded font {path}"))
            .ok()
            .and_then(|data| Font::new(data.into(), 0))
    }),
];

fn font_for(face: crate::fonts::FaceId) -> Result<&'static Font> {
    // Bundled faces (ids 0..4) keep the fixed table; registry faces
    // (@font-face / system, CORE-103) get a leaked krilla Font keyed by id.
    if (face.0 as usize) < 4 {
        let path = crate::fonts::face_path(face);
        FACE_FONTS[face.0 as usize]
            .as_ref()
            .ok_or_else(|| anyhow!("krilla failed to parse font {path}"))
    } else {
        // Registry faces (@font-face / system, CORE-103): parse once per id
        // and leak the Font (deterministic; one allocation per face).
        static PARSED: LazyLock<Mutex<HashMap<u32, usize>>> =
            LazyLock::new(|| Mutex::new(HashMap::new()));
        let mut parsed = PARSED.lock().unwrap();
        let ptr = *parsed.entry(face.0).or_insert_with(|| {
            Font::new(
                crate::fonts::face_bytes(face).into(),
                crate::fonts::face_index(face),
            )
            .map(|font| Box::leak(Box::new(font)) as *const Font as usize)
            .unwrap_or(0)
        });
        if ptr == 0 {
            Err(anyhow!("krilla failed to parse registered font {face:?}"))
        } else {
            Ok(unsafe { &*(ptr as *const Font) })
        }
    }
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
    render_with_options(layout, None, meta, false, false)
}

/// Render with full control (CORE-111): tagging and the PDF/UA validator are
/// opt-in. `dom` must be `Some` when `tagged` is true — the structure tree is
/// built from the DOM, with draws attributed through fragment sources.
///
/// Determinism: identical inputs + identical flags yield byte-identical
/// output. All tree construction is index-ordered; krilla's `/ID` derives
/// from content; no clock is read.
pub fn render_with_options(
    layout: &Layout,
    dom: Option<&crate::dom::Dom>,
    meta: &DocumentMetadata,
    tagged: bool,
    ua: bool,
) -> Result<Vec<u8>> {
    // Tagging requires the DOM (the tree is semantic, not geometric).
    let tagged = tagged && dom.is_some();
    let settings = if ua {
        SerializeSettings {
            enable_tagging: true,
            configuration: krilla::configure::ConfigurationBuilder::new()
                .with_accessibility_validator(krilla::configure::Accessibility::UA1)
                .finish()
                .map_err(|e| anyhow!("invalid PDF configuration: {e:?}"))?,
            ..SerializeSettings::default()
        }
    } else {
        SerializeSettings {
            enable_tagging: tagged,
            ..SerializeSettings::default()
        }
    };
    let mut document = Document::new_with(settings);

    // Recorded tagged draws across ALL pages: (page, Identifier, owner node).
    // Collected only when tagging so the plain path allocates nothing.
    let mut draws: Vec<crate::tags::DrawRef> = Vec::new();

    for (page_idx, page) in layout.pages.iter().enumerate() {
        // Each fragmentainer carries its own resolved page size (an `@page`
        // rule may override the CLI default per page).
        let page_w = page.root.size.0.to_f32();
        let page_h = page.root.size.1.to_f32();
        let settings = PageSettings::from_wh(page_w, page_h)
            .ok_or_else(|| anyhow!("invalid page size {page_w}x{page_h}"))?;
        let mut pdf_page = document.start_page_with(settings);
        let mut surface = pdf_page.surface();

        // Page box background (CORE-66): a full-page fill from the resolved
        // `@page` background, painted under everything else. Always an
        // Artifact (page chrome, not document content).
        if let Some(bg) = page.background {
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, page_w, page_h) {
                let mut pb = krilla::geom::PathBuilder::new();
                pb.push_rect(rect);
                if let Some(path) = pb.finish() {
                    surface.set_fill(Some(solid_fill(bg)));
                    // Page background: an Artifact (Layout), never in the tree.
                    if tagged {
                        surface.start_tagged(ContentTag::Artifact(Artifact::with_kind(
                            ArtifactType::Background,
                        )));
                        surface.draw_path(&path);
                        surface.end_tagged();
                    } else {
                        surface.draw_path(&path);
                    }
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
                crate::paged::PageOrientation::RotateRight => (0.0, 1.0, -1.0, 0.0, page_h, 0.0),
                crate::paged::PageOrientation::RotateLeft => (0.0, -1.0, 1.0, 0.0, 0.0, page_w),
                crate::paged::PageOrientation::RotateTop
                | crate::paged::PageOrientation::RotateBottom => {
                    (-1.0, 0.0, 0.0, -1.0, page_w, page_h)
                }
            };
            surface.push_transform(&krilla::geom::Transform::from_row(sx, ky, kx, sy, tx, ty));
            true
        } else {
            false
        };

        // Two passes over the fragment tree so backgrounds sit under text.
        // Each collected item carries its OWNER: the nearest ancestor
        // fragment with a source DOM node (CORE-111). Unsourced items are
        // artifacts (margin boxes) or page chrome.
        let mut backgrounds: Vec<(f32, f32, f32, f32, Color, Option<NodeId>)> = Vec::new();
        let mut borders: Vec<(
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            f32,
            Color,
            Option<NodeId>,
        )> = Vec::new();
        let mut texts: Vec<TextItem> = Vec::new();
        // (x, y, w, h, store key, owner) — the rect is the fragment's own size.
        let mut images: Vec<(f32, f32, f32, f32, [u8; 32], Option<NodeId>)> = Vec::new();
        collect(
            &page.root,
            0.0,
            0.0,
            None,
            &mut backgrounds,
            &mut borders,
            &mut texts,
            &mut images,
        );

        for (x, y, w, h, color, owner) in &backgrounds {
            let rect = Rect::from_xywh(*x, *y, *w, *h)
                .ok_or_else(|| anyhow!("invalid background rect"))?;
            let mut pb = krilla::geom::PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                surface.set_fill(Some(solid_fill(*color)));
                if tagged {
                    let ident = surface.start_tagged(tag_for_owner(*owner));
                    surface.draw_path(&path);
                    surface.end_tagged();
                    record_draw(&mut draws, page_idx, ident, *owner);
                } else {
                    surface.draw_path(&path);
                }
            }
        }

        // Borders after backgrounds, before text: stroke each side whose
        // width > 0 as a thin filled rect (deterministic, no stroke state).
        for (x, y, w, h, t, r, b, l, color, owner) in &borders {
            let fill = solid_fill(*color);
            let mut sides: Vec<(f32, f32, f32, f32)> = Vec::new();
            if *t > 0.0 {
                sides.push((*x, *y, *w, *t));
            }
            if *b > 0.0 {
                sides.push((*x, *y + h - b, *w, *b));
            }
            if *l > 0.0 {
                sides.push((*x, *y, *l, *h));
            }
            if *r > 0.0 {
                sides.push((*x + w - r, *y, *r, *h));
            }
            if !tagged {
                for (px, py, pw, ph) in &sides {
                    if pw <= &0.0 || ph <= &0.0 {
                        continue;
                    }
                    if let Some(rect) = Rect::from_xywh(*px, *py, *pw, *ph) {
                        let mut pb = krilla::geom::PathBuilder::new();
                        pb.push_rect(rect);
                        if let Some(path) = pb.finish() {
                            surface.set_fill(Some(fill.clone()));
                            surface.draw_path(&path);
                        }
                    }
                }
            } else if sides.is_empty() {
                continue;
            } else {
                // One marked-content sequence wraps ALL border sides of the
                // box — they are one logical graphic.
                let ident = surface.start_tagged(tag_for_owner(*owner));
                for (px, py, pw, ph) in &sides {
                    if *pw <= 0.0 || *ph <= 0.0 {
                        continue;
                    }
                    if let Some(rect) = Rect::from_xywh(*px, *py, *pw, *ph) {
                        let mut pb = krilla::geom::PathBuilder::new();
                        pb.push_rect(rect);
                        if let Some(path) = pb.finish() {
                            surface.set_fill(Some(fill.clone()));
                            surface.draw_path(&path);
                        }
                    }
                }
                surface.end_tagged();
                record_draw(&mut draws, page_idx, ident, *owner);
            }
        }

        // Images after backgrounds/borders, before text (CORE-106). The
        // fragment rect IS the used box: CSS sizing was applied at layout
        // time, so draw at exactly that size. Broken images carry no Image
        // payload bytes and are skipped here; their alt text rides the
        // normal text pass as a child line of the placeholder fragment.
        // Each krilla `Image` is built once per unique content key per page;
        // krilla dedupes embedded objects by its own content hash.
        for (x, y, w, h, key, owner) in &images {
            let Some(stored) = layout.images.get(key) else {
                continue;
            };
            let crate::images::ImageEntry::Loaded(img) = stored else {
                continue;
            };
            if *w <= 0.0 || *h <= 0.0 {
                continue;
            }
            let Some(size) = krilla::geom::Size::from_wh(*w, *h) else {
                continue;
            };
            let raw = krilla::Data::from(img.original.clone());
            let kimg = match img.kind {
                crate::images::ImageKind::Png => krilla::image::Image::from_png(raw, false),
                crate::images::ImageKind::Jpeg => krilla::image::Image::from_jpeg(raw, false),
            };
            let Ok(kimg) = kimg else {
                continue;
            };
            surface.push_transform(&krilla::geom::Transform::from_row(
                1.0, 0.0, 0.0, 1.0, *x, *y,
            ));
            if tagged {
                let ident = surface.start_tagged(tag_for_owner(*owner));
                surface.draw_image(kimg, size);
                surface.end_tagged();
                record_draw(&mut draws, page_idx, ident, *owner);
            } else {
                surface.draw_image(kimg, size);
            }
            surface.pop();
        }

        for t in &texts {
            let font = font_for(t.font_face)?;
            surface.set_fill(Some(solid_fill(t.color)));
            if tagged && t.owner.is_some() {
                let ident = surface.start_tagged(tag_for_owner(t.owner));
                if t.glyphs.is_empty() {
                    // Fallback text path (empty run — nothing shaped). All
                    // normal runs — body text, generated content, margin
                    // boxes — carry shaped glyphs (CORE-85, CORE-83).
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
                    // (scale every advance by 1+expansion) applied at draw
                    // time.
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
                surface.end_tagged();
                record_draw(&mut draws, page_idx, ident, t.owner);
            } else {
                if t.glyphs.is_empty() {
                    surface.draw_text(
                        Point::from_xy(t.x, t.y),
                        font.clone(),
                        t.font_size,
                        &t.text,
                        false,
                        TextDirection::Auto,
                    );
                } else {
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
        }

        // Pop any pushed graphics state (page-orientation rotation) before
        // finishing the page — krilla asserts a balanced push/pop.
        if rotated {
            surface.pop();
        }

        // Tagged link annotations must be registered BEFORE `surface.finish`
        // so krilla can bind them into the page's parent tree. The untagged
        // path keeps its original after-finish position.
        let pending_links: Vec<&crate::layout::PageLink> = layout
            .links
            .iter()
            .filter(|pl| pl.page_index == page_idx)
            .collect();
        let mut tagged_link_idents: Vec<(NodeId, krilla::tagging::Identifier)> = Vec::new();
        if tagged && !pending_links.is_empty() {
            // Drop the finished surface's borrow; annotations go through
            // `pdf_page` directly in tagged mode.
            drop(surface);
            for pl in &pending_links {
                let ident = add_link_annotation(
                    &mut pdf_page,
                    page_w,
                    page_h,
                    rotated,
                    &page.page_orientation,
                    pl,
                    true,
                );
                if let Some(id) = ident {
                    tagged_link_idents.push((pl.node, id));
                }
            }
            for (node, ident) in tagged_link_idents {
                draws.push(crate::tags::DrawRef {
                    page: page_idx,
                    ident,
                    source: Some(node),
                });
            }
            pdf_page.finish();
            continue;
        }
        drop(surface);

        // Link annotations (CORE-104): added after the surface is finished —
        // `surface()` holds a mutable borrow of the page for its lifetime.
        for pl in pending_links {
            add_link_annotation(
                &mut pdf_page,
                page_w,
                page_h,
                rotated,
                &page.page_orientation,
                pl,
                false,
            );
        }
        pdf_page.finish();
    }

    // Structure tree (CORE-111): built from the DOM + recorded draws when
    // tagging. Document order over the arena Vec — deterministic.
    if let Some(dom) = dom.filter(|_| tagged) {
        let tree = crate::tags::build_tag_tree(dom, &draws, None);
        document.set_tag_tree(tree);
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
/// degenerate empty runs. `owner` is the nearest sourced ancestor DOM node
/// (CORE-111): tagged draws wrap in marked content; unsourced runs (margin
/// boxes) draw untagged — they are page chrome.
struct TextItem {
    x: f32,
    y: f32,
    font_size: f32,
    color: Color,
    font_face: crate::fonts::FaceId,
    text: String,
    glyphs: Vec<ShapedGlyph>,
    expansion: f32,
    protrude_left: f32,
    protrude_right: f32,
    owner: Option<NodeId>,
}

/// Pre-order walk accumulating absolute offsets from parent-relative fragment
/// geometry. Backgrounds (block fills) are collected before text so paint order
/// is backgrounds-under-text within each page. `owner` threads the nearest
/// ancestor with a source DOM node down the tree (CORE-111 attribution).
fn collect(
    frag: &Fragment,
    parent_x: f32,
    parent_y: f32,
    owner: Option<NodeId>,
    backgrounds: &mut Vec<(f32, f32, f32, f32, Color, Option<NodeId>)>,
    borders: &mut Vec<(
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        f32,
        Color,
        Option<NodeId>,
    )>,
    texts: &mut Vec<TextItem>,
    images: &mut Vec<(f32, f32, f32, f32, [u8; 32], Option<NodeId>)>,
) {
    let abs_x = parent_x + frag.offset.x.to_f32();
    let abs_y = parent_y + frag.offset.y.to_f32();
    // The owner for THIS fragment's own paint and its children's default:
    // its source when it has one, else the inherited owner.
    let my_owner = frag.source.or(owner);

    match &frag.content {
        FragmentContent::Background(color) => {
            backgrounds.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                *color,
                my_owner,
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
                my_owner,
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
                owner: my_owner,
            });
        }
        FragmentContent::Image(run) => {
            images.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                run.key,
                my_owner,
            ));
        }
        FragmentContent::None => {}
    }

    for child in &frag.children {
        collect(
            child,
            abs_x,
            abs_y,
            my_owner,
            backgrounds,
            borders,
            texts,
            images,
        );
    }
}

/// Add one link annotation for `pl`. In tagged mode the annotation is bound
/// into the structure tree and its identifier returned (CORE-111); untagged
/// mode keeps the plain add and returns `None`.
#[allow(clippy::too_many_arguments)]
fn add_link_annotation(
    pdf_page: &mut krilla::page::Page,
    page_w: f32,
    page_h: f32,
    rotated: bool,
    orientation: &Option<crate::paged::PageOrientation>,
    pl: &crate::layout::PageLink,
    tagged: bool,
) -> Option<krilla::tagging::Identifier> {
    let (x0, y0, x1, y1) = (
        pl.x.get(),
        pl.y.get(),
        pl.x.get() + pl.w.get(),
        pl.y.get() + pl.h.get(),
    );
    let mapped: Vec<(f32, f32)> = if rotated {
        let (sx, ky, kx, sy, tx, ty): (f64, f64, f64, f64, f64, f64) = match orientation {
            Some(crate::paged::PageOrientation::RotateRight) => {
                (0.0, 1.0, -1.0, 0.0, page_h as f64, 0.0)
            }
            Some(crate::paged::PageOrientation::RotateLeft) => {
                (0.0, -1.0, 1.0, 0.0, 0.0, page_w as f64)
            }
            _ => (-1.0, 0.0, 0.0, -1.0, page_w as f64, page_h as f64),
        };
        [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
            .iter()
            .map(|&(x, y)| ((sx * x + kx * y + tx) as f32, (ky * x + sy * y + ty) as f32))
            .collect()
    } else {
        vec![(x0 as f32, y0 as f32), (x1 as f32, y1 as f32)]
    };
    let min_x = mapped.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let min_y = mapped.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_x = mapped.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let max_y = mapped.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let rect = Rect::from_xywh(min_x, min_y, max_x - min_x, max_y - min_y)?;
    let target = match &pl.target {
        crate::layout::LinkTarget::Url(u) => krilla::annotation::Target::Action(
            krilla::action::Action::Link(krilla::action::LinkAction::new(u.clone())),
        ),
        crate::layout::LinkTarget::Page(p) => krilla::annotation::Target::Destination(
            krilla::destination::Destination::Xyz(krilla::destination::XyzDestination::new(
                *p,
                krilla::geom::Point::from_xy(0.0, 0.0),
            )),
        ),
    };
    let annotation =
        krilla::annotation::Annotation::from(krilla::annotation::LinkAnnotation::new(rect, target));
    if tagged {
        Some(pdf_page.add_tagged_annotation(annotation))
    } else {
        pdf_page.add_annotation(annotation);
        None
    }
}
