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
//! wall-clock), document metadata fields are input-derived only, `producer`
//! is the constant `TypeAnvil <version>` (CORE-389), and `creation_date` /
//! `creator` are never set (so no `CreationDate` appears anywhere) — and we
//! always embed the exact same font bytes. Identical input therefore yields identical output bytes.

use anyhow::{anyhow, Context, Result};
use krilla::color::rgb;
use krilla::destination::XyzDestination;
use krilla::geom::{Point, Rect};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::tagging::{Artifact, ArtifactType, ContentTag, SpanTag};
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

/// The Artifact tag for one piece of page chrome painted as a background
/// (CORE-379). PDF 1.7 makes a BBox mandatory on Background artifacts (ISO
/// 32000-1 table 330) and krilla panics without one. krilla writes the BBox
/// in page space and ignores the current transform, so on a rotated page the
/// painted `rect` is mapped through the same `page_xform` the fill is drawn
/// under. With no usable rect the paint is still page chrome, so it falls
/// back to a Layout artifact, which needs no BBox.
fn background_artifact(
    rect: Option<Rect>,
    page_xform: Option<krilla::geom::Transform>,
) -> ContentTag<'static> {
    let bbox = match (rect, page_xform) {
        (Some(r), Some(t)) => r.transform(t),
        (r, None) => r,
        (None, Some(_)) => None,
    };
    match bbox {
        Some(b) => ContentTag::Artifact(Artifact::new(ArtifactType::Background, Some(b))),
        None => ContentTag::Artifact(Artifact::with_kind(ArtifactType::Layout)),
    }
}

/// The artifact kind for everything a page-margin box paints (CORE-380):
/// its margin row decides — top row (incl. the top corners) → Header,
/// bottom row → Footer, the side columns → PaginationOther.
fn margin_artifact_kind(name: crate::paged::MarginBoxName) -> ArtifactType {
    match name.row() {
        crate::paged::MarginRow::Top => ArtifactType::Header,
        crate::paged::MarginRow::Bottom => ArtifactType::Footer,
        crate::paged::MarginRow::Left | crate::paged::MarginRow::Right => {
            ArtifactType::PaginationOther
        }
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
    record_draw_part(draws, page, ident, owner, crate::frag::FragPart::Content);
}

/// [`record_draw`] for a draw that paints a structural part (list marker,
/// footnote body) of its owner (CORE-374).
fn record_draw_part(
    draws: &mut Vec<crate::tags::DrawRef>,
    page: usize,
    ident: krilla::tagging::Identifier,
    owner: Option<NodeId>,
    part: crate::frag::FragPart,
) {
    if let Some(source) = owner {
        draws.push(crate::tags::DrawRef {
            page,
            ident,
            source: Some(source),
            part,
        });
    }
}

fn to_krilla_color(c: Color) -> rgb::Color {
    rgb::Color::new(c.r, c.g, c.b)
}

/// CORE-522: parsed usvg tree for an SVG image whose content the vector
/// replay can draw completely; `None` means embed the PNG raster.
fn vector_tree(img: &crate::images::StoredImage) -> Option<resvg::usvg::Tree> {
    let src = img.svg_source.as_deref()?;
    let tree = crate::svg_vector::parse(src)?;
    crate::svg_vector::is_vector_safe(&tree).then_some(tree)
}

/// Fill for a CSS color: the RGB channels plus the color's own alpha as the
/// fill opacity (CORE-153). PDF `ca` transparency composites over whatever is
/// beneath — the `@page` fill for the canvas background, the page/canvas for
/// element boxes — so no pre-blending happens here.
fn solid_fill(c: Color) -> Fill {
    let opacity = NormalizedF32::new(c.opacity()).unwrap_or(NormalizedF32::ONE);
    Fill {
        paint: to_krilla_color(c).into(),
        opacity,
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
/// All fields are input-derived only. The engine also always sets the constant
/// `producer` (`TypeAnvil <version>`, CORE-389) and never `creation_date` or
/// `creator`, so identical input stays byte-identical.
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

        // `page-orientation` (CORE-66, CORE-155): the laid-out content is
        // unrotated; the emitter maps content coords into the FINAL page box.
        // A 90° rotation swaps the box (landscape <-> portrait); a 180°
        // rotation keeps it.
        let rotated = page.page_orientation.is_some();
        let (pb_w, pb_h) = match page.page_orientation {
            Some(crate::paged::PageOrientation::RotateLeft)
            | Some(crate::paged::PageOrientation::RotateRight) => (page_h, page_w),
            _ => (page_w, page_h),
        };

        let settings = PageSettings::from_wh(pb_w, pb_h)
            .ok_or_else(|| anyhow!("invalid page size {pb_w}x{pb_h}"))?;
        let mut pdf_page = document.start_page_with(settings);
        let mut surface = pdf_page.surface();

        // Rotation applies to the WHOLE page (backgrounds + content), so it
        // is pushed before any drawing and popped at the end. rotate-right:
        // (x, y) -> (H - y, x); rotate-left: (x, y) -> (y, W - x); 180°:
        // (x, y) -> (W - x, H - y).
        let mut page_xform: Option<krilla::geom::Transform> = None;
        if let Some(orient) = page.page_orientation {
            let (sx, ky, kx, sy, tx, ty) = match orient {
                crate::paged::PageOrientation::RotateRight => (0.0, 1.0, -1.0, 0.0, page_h, 0.0),
                crate::paged::PageOrientation::RotateLeft => (0.0, -1.0, 1.0, 0.0, 0.0, page_w),
                crate::paged::PageOrientation::RotateTop
                | crate::paged::PageOrientation::RotateBottom => {
                    (-1.0, 0.0, 0.0, -1.0, page_w, page_h)
                }
            };
            let xform = krilla::geom::Transform::from_row(sx, ky, kx, sy, tx, ty);
            surface.push_transform(&xform);
            page_xform = Some(xform);
        }

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
                        surface.start_tagged(background_artifact(Some(rect), page_xform));
                        surface.draw_path(&path);
                        surface.end_tagged();
                    } else {
                        surface.draw_path(&path);
                    }
                }
            }
        }

        // css-page-3 §3.1 page-margin boxes, phase 1: boxes with a NEGATIVE
        // z-index paint behind the document canvas and all document content —
        // the canvas, page borders and every content fragment behave as one
        // `z-index: 0` stacking context, so a negative-z margin box "may only
        // paint in front of the page background or behind the document
        // canvas". Paint order is z-index ascending, then the spec's default
        // clockwise tree order.
        {
            for mb in page
                .margin_boxes_in_paint_order()
                .into_iter()
                .filter(|m| m.z_index < 0)
            {
                paint_margin_box(&mut surface, layout, &mb.fragment, tagged, margin_artifact_kind(mb.name))?;
            }
        }

        // Document canvas background (CORE-144): the html/body background
        // propagates to the canvas and paints over the page CONTENT area —
        // under all content, but ABOVE the `@page` box fill so page margins
        // keep the page box's own background (page-box-006's white/blue
        // margins). The donor box painted nothing of its own (layout
        // suppressed it). Always an Artifact, like the page-box fill.
        if let Some(bg) = page.canvas_background {
            let (ox, oy) = (
                page.content_origin.x.to_f32(),
                page.content_origin.y.to_f32(),
            );
            let (cw, ch) = (page.content_size.0.to_f32(), page.content_size.1.to_f32());
            if let Some(rect) = Rect::from_xywh(ox, oy, cw, ch) {
                let mut pb = krilla::geom::PathBuilder::new();
                pb.push_rect(rect);
                if let Some(path) = pb.finish() {
                    surface.set_fill(Some(solid_fill(bg)));
                    if tagged {
                        surface.start_tagged(background_artifact(Some(rect), page_xform));
                        surface.draw_path(&path);
                        surface.end_tagged();
                    } else {
                        surface.draw_path(&path);
                    }
                }
            }
        }

        // Page-box outline (CORE-153): a ring painted OUTSIDE the page border
        // box (the page area) by `outline-offset`. Four filled rects form the
        // ring (deterministic, no stroke state); it spans distance
        // [offset, offset+width] beyond the page area, matching css-ui-3 §2.2
        // (positive offset draws the outline further out — page-box-010's
        // 40px offset from a 50px margin lands on the page edge). The ring
        // lies in the margin area, so it never overlaps laid-out content.
        if let Some((ow, oc, oo)) = page.outline {
            let ow = ow.to_f32();
            if ow > 0.0 {
                let ox = page.content_origin.x.to_f32();
                let oy = page.content_origin.y.to_f32();
                let cw = page.content_size.0.to_f32();
                let ch = page.content_size.1.to_f32();
                let o = oo.to_f32();
                let w = ow;
                // Inner boundary = page area expanded by `offset`; outer =
                // expanded by `offset + width`.
                let (ix0, iy0) = (ox - o, oy - o);
                let (ix1, iy1) = (ox + cw + o, oy + ch + o);
                let (ex0, ey0) = (ix0 - w, iy0 - w);
                let ew = (ix1 + w) - ex0; // outer width = cw + 2o + 2w
                // The ring's outer rect — its Background artifact BBox.
                let outline_box = Rect::from_xywh(ex0, ey0, ew, (iy1 + w) - ey0);
                let sides: [(f32, f32, f32, f32); 4] = [
                    (ex0, ey0, ew, w), // top
                    (ex0, iy1, ew, w), // bottom
                    (ex0, iy0, w, iy1 - iy0), // left
                    (ix1, iy0, w, iy1 - iy0), // right
                ];
                let mut pb = krilla::geom::PathBuilder::new();
                for (px, py, pw, ph) in sides {
                    if pw <= 0.0 || ph <= 0.0 {
                        continue;
                    }
                    if let Some(rect) = Rect::from_xywh(px, py, pw, ph) {
                        pb.push_rect(rect);
                    }
                }
                if let Some(path) = pb.finish() {
                    surface.set_fill(Some(solid_fill(oc)));
                    if tagged {
                        surface.start_tagged(background_artifact(outline_box, page_xform));
                        surface.draw_path(&path);
                        surface.end_tagged();
                    } else {
                        surface.draw_path(&path);
                    }
                }
            }
        }

        // CORE-165 page-chrome rings, painted ABOVE the page/canvas fills and
        // BELOW all content (css-page-3 §3.1: page bg < canvas < page borders
        // < document contents). Both rings sit at the page area rect's outer
        // edge:
        // - @page border ring: from `@page { border }` (current-color
        //   resolved in paged.rs).
        // - html root border ring: the root element's own border — the root
        //   box fragments per page and its border repeats at each fragment's
        //   edge (page-box-000's ref `html { border: 20px solid green }`).
        // When both declare the same geometry they overlap identically (the
        // reftest pairs author the same border on one side each).
        let ring_rects = |ox: f32,
                          oy: f32,
                          cw: f32,
                          ch: f32,
                          bw: f32|
         -> [(f32, f32, f32, f32); 4] {
            [
                (ox, oy, cw, bw),             // visually-top (y-up: oy is bottom)
                (ox, oy + ch - bw, cw, bw),   // visually-bottom
                (ox, oy, bw, ch),             // left
                (ox + cw - bw, oy, bw, ch),   // right
            ]
        };
        // NOTE on coordinates: content_origin/size are in the PDF emitter's
        // y-down page space (the canvas fill above uses them directly and
        // Chromium-compare passes), so the ring bands map directly.
        if let Some((bw, bc)) = page.page_border {
            let bwf = bw.to_f32();
            if bwf > 0.0 && !page.page_chrome_hidden {
                let (ox, oy) = (
                    page.content_origin.x.to_f32(),
                    page.content_origin.y.to_f32(),
                );
                let (cw, ch) = (page.content_size.0.to_f32(), page.content_size.1.to_f32());
                // The ring's outer rect (the page area) — its artifact BBox.
                let ring_box = Rect::from_xywh(ox, oy, cw, ch);
                let mut pb = krilla::geom::PathBuilder::new();
                for (rx, ry, rw, rh) in ring_rects(ox, oy, cw, ch, bwf.min(cw / 2.0).min(ch / 2.0)) {
                    if rw <= 0.0 || rh <= 0.0 {
                        continue;
                    }
                    if let Some(rect) = Rect::from_xywh(rx, ry, rw, rh) {
                        pb.push_rect(rect);
                    }
                }
                if let Some(path) = pb.finish() {
                    surface.set_fill(Some(solid_fill(bc)));
                    if tagged {
                        surface.start_tagged(background_artifact(ring_box, page_xform));
                        surface.draw_path(&path);
                        surface.end_tagged();
                    } else {
                        surface.draw_path(&path);
                    }
                }
            }
        }
        if let Some((bw, bc, _pads)) = page.root_border {
            let bwf = bw.to_f32();
            if bwf > 0.0 {
                let (ox, oy) = (
                    page.content_origin.x.to_f32(),
                    page.content_origin.y.to_f32(),
                );
                let (cw, ch) = (page.content_size.0.to_f32(), page.content_size.1.to_f32());
                // The ring's outer rect (the page area) — its artifact BBox.
                let ring_box = Rect::from_xywh(ox, oy, cw, ch);
                let mut pb = krilla::geom::PathBuilder::new();
                for (rx, ry, rw, rh) in ring_rects(ox, oy, cw, ch, bwf.min(cw / 2.0).min(ch / 2.0)) {
                    if rw <= 0.0 || rh <= 0.0 {
                        continue;
                    }
                    if let Some(rect) = Rect::from_xywh(rx, ry, rw, rh) {
                        pb.push_rect(rect);
                    }
                }
                if let Some(path) = pb.finish() {
                    surface.set_fill(Some(solid_fill(bc)));
                    if tagged {
                        surface.start_tagged(background_artifact(ring_box, page_xform));
                        surface.draw_path(&path);
                        surface.end_tagged();
                    } else {
                        surface.draw_path(&path);
                    }
                }
            }
        }

        // Build the document's paint groups (CORE-202): each in-flow
        // flex/grid item with a computed z-index opens its own stacking-
        // context group, painted as one unit in (z-index, tree order). A
        // document with no such items degrades to today's flat five-pass
        // paint exactly. Each collected item carries its OWNER: the nearest
        // ancestor fragment with a source DOM node (CORE-111); unsourced
        // items are artifacts or page chrome.
        let mut root_group = PaintGroup::new(None);
        collect(&page.root, 0.0, 0.0, None, &mut root_group);
        paint_group(&mut surface, layout, &root_group, tagged, &mut draws, page_idx)?;


        // css-page-3 §3.1 page-margin boxes, phase 2: z-index `auto`/0 and
        // positive boxes paint IN FRONT of all document content. Each box is
        // painted as one unit, in (z-index, clockwise tree order) — the
        // default order is @top-left-corner first, then clockwise.
        {
            for mb in page
                .margin_boxes_in_paint_order()
                .into_iter()
                .filter(|m| m.z_index >= 0)
            {
                paint_margin_box(&mut surface, layout, &mb.fragment, tagged, margin_artifact_kind(mb.name))?;
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
                let alt = dom.map(|d| link_alt_text(d, pl));
                let ident = add_link_annotation(
                    &mut pdf_page,
                    page_w,
                    page_h,
                    rotated,
                    &page.page_orientation,
                    pl,
                    true,
                    alt,
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
                    part: crate::frag::FragPart::Content,
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
                None,
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
    // A tagged render with no headings gets one entry, the document title →
    // page 1, because PDF/UA-1 requires an outline (tagged-pdf.spec.md
    // Behavior 7a, CORE-382). Untagged renders keep "no headings, no
    // /Outlines" (pdf-outlines.spec.md §9).
    let outline = build_outline(layout).or_else(|| {
        let title = meta.title.as_ref().filter(|_| tagged && !layout.pages.is_empty())?;
        let mut outline = Outline::new();
        outline.push_child(OutlineNode::new(
            title.clone(),
            XyzDestination::new(0, Point::from_xy(0.0, 0.0)),
        ));
        Some(outline)
    });
    if let Some(outline) = outline {
        document.set_outline(outline);
    }

    // Document metadata (CORE-105, CORE-389): always written. Document fields
    // are set only when non-empty; `producer` is the constant
    // `TypeAnvil <version>` (deterministic). `creation_date` and `creator`
    // are never set.
    {
        let mut m = Metadata::new().producer(format!(
            "TypeAnvil {}",
            env!("CARGO_PKG_VERSION")
        ));
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

/// Build a krilla [`Outline`] from the heading structure (CORE-128), nesting
/// entries by level (an `h2` after an `h1` becomes the `h1`'s child, etc.).
/// Each entry targets its heading's first fragment (page + y, PDF top-left
/// origin — krilla converts to PDF coordinates); collapse state comes from
/// `bookmark-state` via `with_open` (leaf nodes ignore it). Deeper-than-6
/// levels cannot occur (levels are 1..=6 by construction). Returns `None`
/// when the document has no entries.
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
        let y = h.y.unwrap_or(0.0);
        let dest = XyzDestination::new(h.page_index, Point::from_xy(0.0, y as f32));
        let node = OutlineNode::new(h.title.clone(), dest).with_open(h.state_open);
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

/// Draw a text item's shaped glyphs, splitting at every change of fallback
/// face (CORE-386). Glyphs with `face: None` use the run's own font. Each
/// segment keeps the full run text with its glyphs' original ranges, so
/// ToUnicode stays correct.
fn draw_shaped(
    surface: &mut krilla::surface::Surface<'_>,
    t: &TextItem,
    font: &Font,
) -> Result<()> {
    let glyphs = to_krilla_glyphs(&t.glyphs, t.font_size, t.expansion, t.protrude_right);
    let mut x = t.x - t.protrude_left;
    let mut i = 0usize;
    while i < glyphs.len() {
        let seg_face = t.glyphs[i].face;
        let seg_color = t.glyphs[i].color;
        let mut j = i;
        while j < glyphs.len()
            && t.glyphs[j].face == seg_face
            && t.glyphs[j].color == seg_color
        {
            j += 1;
        }
        // CORE-530: an inline element's own colour overrides the run's.
        surface.set_fill(Some(solid_fill(seg_color.unwrap_or(t.color))));
        let seg_font: &Font = match seg_face {
            Some(f) => font_for(f)?,
            None => font,
        };
        let _ = surface.draw_glyphs(
            Point::from_xy(x, t.y),
            &glyphs[i..j],
            seg_font.clone(),
            &t.text,
            t.font_size,
            false,
        );
        let adv: f32 = glyphs[i..j].iter().map(|g| g.x_advance).sum();
        x += adv * t.font_size;
        i = j;
    }
    Ok(())
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
    /// Structural part painted (list marker / footnote body), CORE-374.
    part: crate::frag::FragPart,
}

/// Flat pre-order collection for page-margin boxes (CORE-179 path, unchanged).
/// Margin boxes are their own stacking contexts painted as one unit, so they
/// use the five flat lists directly rather than the document-content paint
/// groups. `owner` threads the nearest sourced ancestor (CORE-111).
fn collect_flat(
    frag: &Fragment,
    parent_x: f32,
    parent_y: f32,
    owner: Option<NodeId>,
    backgrounds: &mut Vec<(f32, f32, f32, f32, Color, Option<NodeId>)>,
    borders: &mut Vec<BorderItem>,
    texts: &mut Vec<TextItem>,
    images: &mut Vec<(f32, f32, f32, f32, [u8; 32], Option<NodeId>)>,
    bg_images: &mut Vec<BgImageItem>,
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
                b.top_color,
                b.right_color,
                b.bottom_color,
                b.left_color,
                my_owner,
                b.styles,
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
                part: frag.part,
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
        FragmentContent::BackgroundImage(run) => {
            bg_images.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                run.key,
                my_owner,
                run.clone(),
            ));
        }
        FragmentContent::None => {}
    }

    for child in &frag.children {
        collect_flat(
            child,
            abs_x,
            abs_y,
            my_owner,
            backgrounds,
            borders,
            texts,
            images,
            bg_images,
        );
    }
}

/// Paint ONE page-margin box as a unit (css-page-3 §3.1).
///
/// A margin box is its own stacking context, so its background, border and
/// content paint together, in tree order, before the next box starts. The
/// page-wide passes (all backgrounds, then all borders, then all text) would
/// instead let a LATER box's background hide an EARLIER box's text — the
/// overlap case the `paint-order-00*` fixtures measure.
///
/// Coordinates are page-absolute: `collect` is rooted at (0, 0) here, and a
/// margin box fragment already carries its page-absolute offset.
fn paint_margin_box(
    surface: &mut krilla::surface::Surface<'_>,
    layout: &Layout,
    fragment: &Fragment,
    tagged: bool,
    kind: ArtifactType,
) -> Result<()> {
    // Everything a margin box paints is page chrome, never document content,
    // even when a draw resolves to a DOM owner: a running copy must not enter
    // the structure tree once per page (CORE-380). No draws are recorded.
    let chrome = || ContentTag::Artifact(Artifact::with_kind(kind));
    let mut backgrounds: Vec<(f32, f32, f32, f32, Color, Option<NodeId>)> = Vec::new();
    let mut bg_images: Vec<BgImageItem> = Vec::new();
    let mut borders: Vec<BorderItem> = Vec::new();
    let mut images: Vec<(f32, f32, f32, f32, [u8; 32], Option<NodeId>)> = Vec::new();
    let mut texts: Vec<TextItem> = Vec::new();
    collect_flat(
        fragment,
        0.0,
        0.0,
        None,
        &mut backgrounds,
        &mut borders,
        &mut texts,
        &mut images,
        &mut bg_images,
    );

    // css-backgrounds-3 §2.1 within the box: background colour, then
    // background images, then borders, then images, then text.
    for (x, y, w, h, color, owner) in &backgrounds {
        if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            continue;
        }
        if *w <= 0.0 || *h <= 0.0 {
            continue;
        }
        let rect =
            Rect::from_xywh(*x, *y, *w, *h).ok_or_else(|| anyhow!("invalid background rect"))?;
        let mut pb = krilla::geom::PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            surface.set_fill(Some(solid_fill(*color)));
            if tagged {
                surface.start_tagged(chrome());
                surface.draw_path(&path);
                surface.end_tagged();
            } else {
                surface.draw_path(&path);
            }
        }
    }

    for (x, y, w, h, key, owner, run) in &bg_images {
        if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            continue;
        }
        if *w <= 0.0 || *h <= 0.0 {
            continue;
        }
        let Some(stored) = layout.images.get(key) else {
            continue;
        };
        let crate::images::ImageEntry::Loaded(img) = stored else {
            continue;
        };
        let tile_w = run.tile_w.to_f32();
        let tile_h = run.tile_h.to_f32();
        if tile_w <= 0.0 || tile_h <= 0.0 {
            continue;
        }
        let raw = krilla::Data::from(img.original.clone());
        let kimg = match img.kind {
            crate::images::ImageKind::Png => krilla::image::Image::from_png(raw, false),
            crate::images::ImageKind::Jpeg => krilla::image::Image::from_jpeg(raw, false),
            crate::images::ImageKind::Svg => krilla::image::Image::from_png(raw, false),
        };
        let Ok(kimg) = kimg else {
            continue;
        };
        let Some(rect) = Rect::from_xywh(*x, *y, *w, *h) else {
            continue;
        };
        let mut clip = krilla::geom::PathBuilder::new();
        clip.push_rect(rect);
        let Some(clip_path) = clip.finish() else {
            continue;
        };
        surface.push_clip_path(&clip_path, &FillRule::NonZero);
        for (ox, oy) in run.tile_origins(*w as f64, *h as f64) {
            let tx = *x + ox as f32;
            let ty = *y + oy as f32;
            let Some(size) = krilla::geom::Size::from_wh(tile_w, tile_h) else {
                continue;
            };
            surface.push_transform(&krilla::geom::Transform::from_row(1.0, 0.0, 0.0, 1.0, tx, ty));
            if tagged {
                surface.start_tagged(chrome());
                surface.draw_image(kimg.clone(), size);
                surface.end_tagged();
            } else {
                surface.draw_image(kimg.clone(), size);
            }
            surface.pop();
        }
        surface.pop();
    }

    for (x, y, w, h, t, r, b, l, ct, cr, cb, cl, owner, st) in &borders {
        let _ = owner;
        // CORE-201: each side strokes with its OWN colour.
        let mut sides: Vec<(f32, f32, f32, f32, Color)> = Vec::new();
        if *t > 0.0 {
            for (a, b2, c, d) in dash_rects(st[0], *x, *y, *w, *t, true) {
                sides.push((a, b2, c, d, *ct));
            }
        }
        if *b > 0.0 {
            for (a, b2, c, d) in dash_rects(st[2], *x, *y + h - b, *w, *b, true) {
                sides.push((a, b2, c, d, *cb));
            }
        }
        if *l > 0.0 {
            for (a, b2, c, d) in dash_rects(st[3], *x, *y, *l, *h, false) {
                sides.push((a, b2, c, d, *cl));
            }
        }
        if *r > 0.0 {
            for (a, b2, c, d) in dash_rects(st[1], *x + w - r, *y, *r, *h, false) {
                sides.push((a, b2, c, d, *cr));
            }
        }
        if !tagged {
            for (px, py, pw, ph, side_color) in &sides {
                if pw <= &0.0 || ph <= &0.0 {
                    continue;
                }
                if let Some(rect) = Rect::from_xywh(*px, *py, *pw, *ph) {
                    let mut pb = krilla::geom::PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        surface.set_fill(Some(solid_fill(*side_color)));
                        surface.draw_path(&path);
                    }
                }
            }
        } else if sides.is_empty() {
            continue;
        } else {
            surface.start_tagged(chrome());
            for (px, py, pw, ph, side_color) in &sides {
                if *pw <= 0.0 || *ph <= 0.0 {
                    continue;
                }
                if let Some(rect) = Rect::from_xywh(*px, *py, *pw, *ph) {
                    let mut pb = krilla::geom::PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        surface.set_fill(Some(solid_fill(*side_color)));
                        surface.draw_path(&path);
                    }
                }
            }
            surface.end_tagged();
        }
    }

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
        if let Some(tree) = vector_tree(img) {
            surface.push_transform(&krilla::geom::Transform::from_row(1.0, 0.0, 0.0, 1.0, *x, *y));
            if tagged {
                surface.start_tagged(chrome());
                crate::svg_vector::draw_svg(surface, &tree, size);
                surface.end_tagged();
            } else {
                crate::svg_vector::draw_svg(surface, &tree, size);
            }
            surface.pop();
            continue;
        }
        let raw = krilla::Data::from(img.original.clone());
        let kimg = match img.kind {
            crate::images::ImageKind::Png => krilla::image::Image::from_png(raw, false),
            crate::images::ImageKind::Jpeg => krilla::image::Image::from_jpeg(raw, false),
            crate::images::ImageKind::Svg => krilla::image::Image::from_png(raw, false),
        };
        let Ok(kimg) = kimg else {
            continue;
        };
        surface.push_transform(&krilla::geom::Transform::from_row(1.0, 0.0, 0.0, 1.0, *x, *y));
        if tagged {
            surface.start_tagged(chrome());
            surface.draw_image(kimg, size);
            surface.end_tagged();
        } else {
            surface.draw_image(kimg, size);
        }
        surface.pop();
    }

    for t in &texts {
        let font = font_for(t.font_face)?;
        surface.set_fill(Some(solid_fill(t.color)));
        let draw = |surface: &mut krilla::surface::Surface<'_>| {
            if t.glyphs.is_empty() {
                let _ = surface.draw_text(
                    Point::from_xy(t.x, t.y),
                    font.clone(),
                    t.font_size,
                    &t.text,
                    false,
                    TextDirection::Auto,
                );
            } else {
                let _ = draw_shaped(surface, t, font);
            }
        };
        if tagged {
            surface.start_tagged(chrome());
            draw(surface);
            surface.end_tagged();
        } else {
            draw(surface);
        }
    }

    Ok(())
}

/// (x, y, w, h, color, owner) — one solid background fill.
type BackgroundItem = (f32, f32, f32, f32, Color, Option<NodeId>);
/// (x, y, w, h, t, r, b, l, color, owner, styles) — one border box.
type BorderItem = (
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    f32,
    Color,
    Color,
    Color,
    Color,
    Option<NodeId>,
    [crate::css::BorderLineStyle; 4],
);

/// Split one border side into the rectangles that paint it (CORE-531).
/// `horizontal` sides run along x, vertical sides along y; `t` is the side's
/// thickness. Solid sides give one rect. Dashed sides give dashes of 3t with
/// gaps of ~3t; dotted sides give square dots of t with gaps of t. The run
/// length is adjusted so a dash/dot starts and ends flush with the edge.
pub fn dash_rects(
    style: crate::css::BorderLineStyle,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    horizontal: bool,
) -> Vec<(f32, f32, f32, f32)> {
    use crate::css::BorderLineStyle as S;
    let (len, t) = if horizontal { (w, h) } else { (h, w) };
    let (dash, gap) = match style {
        S::Solid => return vec![(x, y, w, h)],
        S::Dashed => (3.0 * t, 3.0 * t),
        S::Dotted => (t, t),
    };
    if len.is_nan() || t.is_nan() || len <= 0.0 || t <= 0.0 || len <= dash {
        return vec![(x, y, w, h)];
    }
    // n dashes and n-1 gaps fill `len` exactly; scale the gap to fit.
    let n = (((len + gap) / (dash + gap)).round()).max(2.0);
    let g = (len - n * dash) / (n - 1.0);
    let mut out = Vec::new();
    let mut i = 0.0f32;
    while i < n {
        let off = i * (dash + g);
        out.push(if horizontal {
            (x + off, y, dash, h)
        } else {
            (x, y + off, w, dash)
        });
        i += 1.0;
    }
    out
}
/// (x, y, w, h, store key, owner) — one replaced image or background image.
type ImageItem = (f32, f32, f32, f32, [u8; 32], Option<NodeId>);
type BgImageItem = (f32, f32, f32, f32, [u8; 32], Option<NodeId>, crate::frag::BackgroundImageRun);

/// One in-flow stacking-context paint group (CORE-202).
///
/// A group's own flat lists carry the CONTEXT's ordinary content in tree
/// order; its `subgroups` are keyed child contexts (flex/grid items with
/// `z-index`), each painted as a unit in (z_index, tree order).
struct PaintGroup {
    /// `Some(z)` for a keyed child context; `None` for the root (ordinary
    /// in-flow fragments never get a subgroup at all).
    z_index: Option<i32>,
    /// CSS `transform` (CORE-481) in PAGE coordinates; the whole group paints
    /// under it.
    transform: Option<crate::transform::Affine>,
    /// CSS `opacity` (CORE-518): the whole group composites at this alpha.
    opacity: f32,
    backgrounds: Vec<BackgroundItem>,
    bg_images: Vec<BgImageItem>,
    borders: Vec<BorderItem>,
    images: Vec<ImageItem>,
    texts: Vec<TextItem>,
    /// Keyed child contexts in tree (construction) order; the emitter sorts
    /// them by (z_index, tree order) when painting.
    subgroups: Vec<PaintGroup>,
}

impl PaintGroup {
    fn new(z_index: Option<i32>) -> PaintGroup {
        PaintGroup {
            z_index,
            transform: None,
            opacity: 1.0,
            backgrounds: Vec::new(),
            bg_images: Vec::new(),
            borders: Vec::new(),
            images: Vec::new(),
            texts: Vec::new(),
            subgroups: Vec::new(),
        }
    }
}

/// True when `frag` carries any renderable payload of its own.
fn has_paint(frag: &Fragment) -> bool {
    !matches!(&frag.content, FragmentContent::None)
}

/// Push ONE fragment's own payload into a group (CORE-202). Identical to the
/// old flat-list `collect` body: coordinates and owner propagation unchanged.
fn push_content(
    frag: &Fragment,
    parent_x: f32,
    parent_y: f32,
    abs_x: f32,
    abs_y: f32,
    owner: Option<NodeId>,
    group: &mut PaintGroup,
) {
    match &frag.content {
        FragmentContent::Background(color) => {
            group.backgrounds.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                *color,
                owner,
            ));
        }
        FragmentContent::Border(b) => {
            group.borders.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                b.top.to_f32(),
                b.right.to_f32(),
                b.bottom.to_f32(),
                b.left.to_f32(),
                b.top_color,
                b.right_color,
                b.bottom_color,
                b.left_color,
                owner,
                b.styles,
            ));
        }
        FragmentContent::Text(run) => {
            // The run's baseline is parent-relative; rebase off the same parent.
            group.texts.push(TextItem {
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
                owner,
                part: frag.part,
            });
        }
        FragmentContent::Image(run) => {
            group.images.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                run.key,
                owner,
            ));
        }
        FragmentContent::BackgroundImage(run) => {
            group.bg_images.push((
                abs_x,
                abs_y,
                frag.size.0.to_f32(),
                frag.size.1.to_f32(),
                run.key,
                owner,
                run.clone(),
            ));
        }
        FragmentContent::None => {}
    }
}

/// Pre-order walk accumulating absolute offsets from parent-relative fragment
/// geometry, into a paint-group tree (CORE-202). Each keyed fragment (an
/// in-flow flex/grid item with a computed `z-index`) opens its OWN child
/// group: its own payload and whole subtree paint as one unit, ordered
/// against sibling units by (z_index, tree order). `owner` threads the
/// nearest ancestor with a source DOM node down the tree (CORE-111).
fn collect(
    frag: &Fragment,
    parent_x: f32,
    parent_y: f32,
    owner: Option<NodeId>,
    group: &mut PaintGroup,
) {
    let abs_x = parent_x + frag.offset.x.to_f32();
    let abs_y = parent_y + frag.offset.y.to_f32();
    // The owner for THIS fragment's own paint and its children's default:
    // its source when it has one, else the inherited owner.
    let my_owner = frag.source.or(owner);

    // CORE-375: a repeated table header/footer is page chrome. Paint it, but
    // with no owner anywhere in the subtree, so the tagged emitter marks it a
    // Layout artifact and each row enters the structure tree exactly once.
    if frag.repeated {
        collect_unowned(frag, parent_x, parent_y, group);
        return;
    }

    if frag.z_index.is_some() || frag.transform.is_some() || frag.opacity < 1.0 {
        // A keyed or transformed fragment is its own stacking-context root
        // (CORE-481: `transform` creates one, css-transforms-1 §3). An empty
        // box contributes nothing and skips the subgroup.
        if has_paint(frag) || !frag.children.is_empty() {
            let mut subgroup = PaintGroup::new(frag.z_index);
            subgroup.opacity = frag.opacity;
            subgroup.transform = frag
                .transform
                .map(|t| crate::transform::to_abs(&t, abs_x as f64, abs_y as f64));
            push_content(frag, parent_x, parent_y, abs_x, abs_y, my_owner, &mut subgroup);
            for child in &frag.children {
                collect(child, abs_x, abs_y, my_owner, &mut subgroup);
            }
            // Pushed in tree order; the emitter sorts by z and keeps this
            // order for ties.
            group.subgroups.push(subgroup);
        }
        return;
    }

    push_content(frag, parent_x, parent_y, abs_x, abs_y, my_owner, group);
    for child in &frag.children {
        collect(child, abs_x, abs_y, my_owner, group);
    }
}

/// Collect a subtree with every draw unowned (CORE-375 repeated chrome).
/// Source ids are ignored so nothing inside resolves to a DOM node.
fn collect_unowned(frag: &Fragment, parent_x: f32, parent_y: f32, group: &mut PaintGroup) {
    let abs_x = parent_x + frag.offset.x.to_f32();
    let abs_y = parent_y + frag.offset.y.to_f32();
    push_content(frag, parent_x, parent_y, abs_x, abs_y, None, group);
    for child in &frag.children {
        collect_unowned(child, abs_x, abs_y, group);
    }
}

/// Paint one [`PaintGroup`] as a unit: its own flat content in the page-wide
/// phase order (backgrounds, background images, borders, images, text), then
/// its keyed child contexts in CSS2.1 Appendix E order — negative ascending,
/// zero in tree order, positive ascending.
fn paint_group(
    surface: &mut krilla::surface::Surface<'_>,
    layout: &Layout,
    group: &PaintGroup,
    tagged: bool,
    draws: &mut Vec<crate::tags::DrawRef>,
    page_idx: usize,
) -> Result<()> {
    // CORE-518: group opacity composites the whole subtree as one unit.
    if group.opacity < 1.0 {
        let alpha = NormalizedF32::new(group.opacity).unwrap_or(NormalizedF32::ONE);
        surface.push_opacity(alpha);
        let res = paint_group_transformed(surface, layout, group, tagged, draws, page_idx);
        surface.pop();
        return res;
    }
    paint_group_transformed(surface, layout, group, tagged, draws, page_idx)
}

fn paint_group_transformed(
    surface: &mut krilla::surface::Surface<'_>,
    layout: &Layout,
    group: &PaintGroup,
    tagged: bool,
    draws: &mut Vec<crate::tags::DrawRef>,
    page_idx: usize,
) -> Result<()> {
    // CORE-481: a transformed group paints under its matrix.
    if let Some(m) = group.transform {
        let xf = krilla::geom::Transform::from_row(
            m[0] as f32, m[1] as f32, m[2] as f32, m[3] as f32, m[4] as f32, m[5] as f32,
        );
        surface.push_transform(&xf);
        let res = paint_group_body(surface, layout, group, tagged, draws, page_idx);
        surface.pop();
        return res;
    }
    paint_group_body(surface, layout, group, tagged, draws, page_idx)
}

fn paint_group_body(
    surface: &mut krilla::surface::Surface<'_>,
    layout: &Layout,
    group: &PaintGroup,
    tagged: bool,
    draws: &mut Vec<crate::tags::DrawRef>,
    page_idx: usize,
) -> Result<()> {
    for (x, y, w, h, color, owner) in &group.backgrounds {
        // A degenerate (zero/negative) rect is skipped, not fatal
        // (CORE-139: grid auto-tracks can size to 0 for empty cells; a
        // background on such a cell must not abort the whole render).
        // NaN fails every comparison, so test it explicitly — a NaN
        // coordinate (abspos + break-before combo, page-margin-004) must
        // skip like a degenerate rect, never abort the render.
        if !w.is_finite() || !h.is_finite() || !x.is_finite() || !y.is_finite() {
            continue;
        }
        if *w <= 0.0 || *h <= 0.0 {
            continue;
        }
        let rect =
            Rect::from_xywh(*x, *y, *w, *h).ok_or_else(|| anyhow!("invalid background rect"))?;
        let mut pb = krilla::geom::PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            surface.set_fill(Some(solid_fill(*color)));
            if tagged {
                let ident = surface.start_tagged(tag_for_owner(*owner));
                surface.draw_path(&path);
                surface.end_tagged();
                record_draw(draws, page_idx, ident, *owner);
            } else {
                surface.draw_path(&path);
            }
        }
    }

    // Tiled background images: after solid backgrounds, before borders
    // (css-backgrounds-3 §2.1 order). Each tile is drawn at the image's
    // natural size from the box's top-left (default background-position
    // 0% 0%), clipped to the border box so an edge tile never bleeds into
    // a neighbouring box. Broken images carry no payload and paint nothing.
    for (x, y, w, h, key, owner, run) in &group.bg_images {
        if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            continue;
        }
        if *w <= 0.0 || *h <= 0.0 {
            continue;
        }
        let Some(stored) = layout.images.get(key) else {
            continue;
        };
        let crate::images::ImageEntry::Loaded(img) = stored else {
            continue;
        };
        let tile_w = run.tile_w.to_f32();
        let tile_h = run.tile_h.to_f32();
        if tile_w <= 0.0 || tile_h <= 0.0 {
            continue;
        }
        let raw = krilla::Data::from(img.original.clone());
        let kimg = match img.kind {
            crate::images::ImageKind::Png => krilla::image::Image::from_png(raw, false),
            crate::images::ImageKind::Jpeg => krilla::image::Image::from_jpeg(raw, false),
            // CORE-131: SVG is rasterized to PNG at intern time; the
            // `original` bytes already carry the raster.
            crate::images::ImageKind::Svg => krilla::image::Image::from_png(raw, false),
        };
        let Ok(kimg) = kimg else {
            continue;
        };
        // Clip once to the border box: a partial edge tile shows the
        // tile's own top-left portion at natural scale — exactly what
        // clipped repeat produces.
        let Some(rect) = Rect::from_xywh(*x, *y, *w, *h) else {
            continue;
        };
        let mut clip = krilla::geom::PathBuilder::new();
        clip.push_rect(rect);
        let Some(clip_path) = clip.finish() else {
            continue;
        };
        surface.push_clip_path(&clip_path, &FillRule::NonZero);
        for (ox, oy) in run.tile_origins(*w as f64, *h as f64) {
            let tx = *x + ox as f32;
            let ty = *y + oy as f32;
            let Some(size) = krilla::geom::Size::from_wh(tile_w, tile_h) else {
                continue;
            };
            surface.push_transform(&krilla::geom::Transform::from_row(
                1.0, 0.0, 0.0, 1.0, tx, ty,
            ));
            if tagged {
                let ident = surface.start_tagged(tag_for_owner(*owner));
                surface.draw_image(kimg.clone(), size);
                surface.end_tagged();
                record_draw(draws, page_idx, ident, *owner);
            } else {
                surface.draw_image(kimg.clone(), size);
            }
            surface.pop();
        }
        surface.pop();
    }

    // Borders after backgrounds, before text: stroke each side whose
    // width > 0 as a thin filled rect (deterministic, no stroke state).
    for (x, y, w, h, t, r, b, l, ct, cr, cb, cl, owner, st) in &group.borders {
        // CORE-201: each side strokes with its OWN colour.
        let mut sides: Vec<(f32, f32, f32, f32, Color)> = Vec::new();
        if *t > 0.0 {
            for (a, b2, c, d) in dash_rects(st[0], *x, *y, *w, *t, true) {
                sides.push((a, b2, c, d, *ct));
            }
        }
        if *b > 0.0 {
            for (a, b2, c, d) in dash_rects(st[2], *x, *y + h - b, *w, *b, true) {
                sides.push((a, b2, c, d, *cb));
            }
        }
        if *l > 0.0 {
            for (a, b2, c, d) in dash_rects(st[3], *x, *y, *l, *h, false) {
                sides.push((a, b2, c, d, *cl));
            }
        }
        if *r > 0.0 {
            for (a, b2, c, d) in dash_rects(st[1], *x + w - r, *y, *r, *h, false) {
                sides.push((a, b2, c, d, *cr));
            }
        }
        if !tagged {
            for (px, py, pw, ph, side_color) in &sides {
                if pw <= &0.0 || ph <= &0.0 {
                    continue;
                }
                if let Some(rect) = Rect::from_xywh(*px, *py, *pw, *ph) {
                    let mut pb = krilla::geom::PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        surface.set_fill(Some(solid_fill(*side_color)));
                        surface.draw_path(&path);
                    }
                }
            }
        } else if !sides.iter().any(|(px, py, pw, ph, _)| {
            // CORE-396: open the sequence only when at least one side
            // paints (positive, finite rect), else it would be empty.
            *pw > 0.0 && *ph > 0.0 && Rect::from_xywh(*px, *py, *pw, *ph).is_some()
        }) {
            continue;
        } else {
            // One marked-content sequence wraps ALL border sides of the
            // box — they are one logical graphic.
            let ident = surface.start_tagged(tag_for_owner(*owner));
            for (px, py, pw, ph, side_color) in &sides {
                if *pw <= 0.0 || *ph <= 0.0 {
                    continue;
                }
                if let Some(rect) = Rect::from_xywh(*px, *py, *pw, *ph) {
                    let mut pb = krilla::geom::PathBuilder::new();
                    pb.push_rect(rect);
                    if let Some(path) = pb.finish() {
                        surface.set_fill(Some(solid_fill(*side_color)));
                        surface.draw_path(&path);
                    }
                }
            }
            surface.end_tagged();
            record_draw(draws, page_idx, ident, *owner);
        }
    }

    // Images after backgrounds/borders, before text (CORE-106). The
    // fragment rect IS the used box: CSS sizing was applied at layout time,
    // so draw at exactly that size. Broken images carry no Image payload
    // bytes and are skipped here; their alt text rides the normal text pass
    // as a child line of the placeholder fragment. Each krilla `Image` is
    // built once per unique content key per page; krilla dedupes embedded
    // objects by its own content hash.
    for (x, y, w, h, key, owner) in &group.images {
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
        if let Some(tree) = vector_tree(img) {
            surface.push_transform(&krilla::geom::Transform::from_row(
                1.0, 0.0, 0.0, 1.0, *x, *y,
            ));
            if tagged {
                let ident = surface.start_tagged(tag_for_owner(*owner));
                crate::svg_vector::draw_svg(surface, &tree, size);
                surface.end_tagged();
                record_draw(draws, page_idx, ident, *owner);
            } else {
                crate::svg_vector::draw_svg(surface, &tree, size);
            }
            surface.pop();
            continue;
        }
        let raw = krilla::Data::from(img.original.clone());
        let kimg = match img.kind {
            // CORE-131: SVG is rasterized to PNG at intern time (see
            // images.rs); `original` already carries the PNG raster.
            // CORE-522: the vector path above handles supported trees.
            crate::images::ImageKind::Png => krilla::image::Image::from_png(raw, false),
            crate::images::ImageKind::Jpeg => krilla::image::Image::from_jpeg(raw, false),
            crate::images::ImageKind::Svg => krilla::image::Image::from_png(raw, false),
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
            record_draw(draws, page_idx, ident, *owner);
        } else {
            surface.draw_image(kimg, size);
        }
        surface.pop();
    }

    for t in &group.texts {
        let font = font_for(t.font_face)?;
        surface.set_fill(Some(solid_fill(t.color)));
        // Unsourced text is a Layout artifact, like an unsourced fill — never
        // drawn outside marked content (CORE-380).
        if tagged {
            // CORE-396: a run with no glyphs and no text paints nothing, so
            // it must not open an (empty) marked-content sequence.
            if t.glyphs.is_empty() && t.text.is_empty() {
                continue;
            }
            // CORE-376: a line ending in the hyphen the breaker inserted
            // carries ActualText without it, so readers rejoin the word.
            let actual: Option<&str> = if t.owner.is_some()
                && t.glyphs.last().is_some_and(|g| g.synthetic)
            {
                t.text.strip_suffix('-')
            } else if t.owner.is_some() && t.glyphs.iter().any(|g| g.spaced) {
                // CORE-539: letter-spacing widens every glyph gap, so
                // extractors read each letter as a word. ActualText hands
                // them the source string.
                Some(t.text.as_str())
            } else {
                None
            };
            let tag = match actual {
                Some(a) => ContentTag::Span(SpanTag::empty().with_actual_text(Some(a))),
                None => tag_for_owner(t.owner),
            };
            let ident = surface.start_tagged(tag);
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
                // (scale every advance by 1+expansion) applied at draw time.
                draw_shaped(surface, t, font)?;
            }
            surface.end_tagged();
            record_draw_part(draws, page_idx, ident, t.owner, t.part);
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
                draw_shaped(surface, t, font)?;
            }
        }
    }

    // Keyed child contexts (CORE-202): negative ascending, then zero in tree
    // order, then positive ascending. The stable sort keeps tree order for
    // equal z (tree order is already the construction order in `subgroups`).
    let mut neg: Vec<usize> = Vec::new();
    let mut zero: Vec<usize> = Vec::new();
    let mut pos: Vec<usize> = Vec::new();
    for (i, sg) in group.subgroups.iter().enumerate() {
        match sg.z_index {
            Some(z) if z < 0 => neg.push(i),
            Some(0) => zero.push(i),
            Some(_) => pos.push(i),
            None => zero.push(i),
        }
    }
    neg.sort_by_key(|&i| group.subgroups[i].z_index.unwrap_or(0));
    pos.sort_by_key(|&i| group.subgroups[i].z_index.unwrap_or(0));
    for &i in neg.iter().chain(zero.iter()).chain(pos.iter()) {
        paint_group(surface, layout, &group.subgroups[i], tagged, draws, page_idx)?;
    }

    Ok(())
}

/// Add one link annotation for `pl`. In tagged mode the annotation is bound
/// into the structure tree and its identifier returned (CORE-111); untagged
/// mode keeps the plain add and returns `None`.
#[allow(clippy::too_many_arguments)]
/// Alternate text for a link annotation (tagged-pdf.spec.md Behavior 6,
/// CORE-381): the `<a>` element's text with whitespace collapsed, else its
/// `title`, else its `href`.
fn link_alt_text(dom: &crate::dom::Dom, pl: &crate::layout::PageLink) -> String {
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = collapse(&dom.text_content(pl.node));
    if !text.is_empty() {
        return text;
    }
    // CORE-391: image-only link → the first descendant image's `alt`.
    let mut stack = vec![pl.node];
    while let Some(n) = stack.pop() {
        if let Some(e) = dom.nodes[n].kind.element() {
            if e.tag == "img" {
                let alt = e.attr("alt").map(collapse).unwrap_or_default();
                if !alt.is_empty() {
                    return alt;
                }
            }
        }
        stack.extend(dom.nodes[n].children.iter().rev());
    }
    let el = dom.nodes[pl.node].kind.element();
    let attr = |name: &str| el.and_then(|e| e.attr(name)).map(collapse).unwrap_or_default();
    let title = attr("title");
    if !title.is_empty() {
        return title;
    }
    attr("href")
}

fn add_link_annotation(
    pdf_page: &mut krilla::page::Page,
    page_w: f32,
    page_h: f32,
    rotated: bool,
    orientation: &Option<crate::paged::PageOrientation>,
    pl: &crate::layout::PageLink,
    tagged: bool,
    alt: Option<String>,
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
    // `alt` becomes the annotation's `/Contents` (PDF/UA-1 §7.18.5). Only the
    // tagged path passes one, so untagged output is unchanged (CORE-381).
    let annotation = krilla::annotation::Annotation::new_link(
        krilla::annotation::LinkAnnotation::new(rect, target),
        alt,
    );
    if tagged {
        Some(pdf_page.add_tagged_annotation(annotation))
    } else {
        pdf_page.add_annotation(annotation);
        None
    }
}
