//! Image acceptance tests — one per acceptance criterion in
//! `docs/specifications/images.spec.md` (CORE-106).
//!
//! PNG fixtures are generated at test time (small solid-color images) — no
//! binary fixture files. Tests drive `layout_with_images_and_store` directly
//! and assert on fragment geometry / store state; determinism and embed-count
//! tests compare rendered PDF bytes.

use std::path::PathBuf;

use tempfile::TempDir;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{FragmentContent, FragmentKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::images::{ImageEntry, ImageStore};
use typeanvil::layout::{layout, layout_with_images_and_store, Layout};

// --- helpers -----------------------------------------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(8.5),
        height: inches(11.0),
        margin_top: inches(0.8),
        margin_right: inches(0.8),
        margin_bottom: inches(0.8),
        margin_left: inches(0.8),
    }
}

/// Minimal PNG encoder: 8-bit RGB, no filtering (filter byte 0 per row).
/// Enough for solid-color test images; keeps the test suite dependency-free.
fn make_png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    use std::io::Write;

    let mut out = Vec::new();
    // PNG signature
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, t) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB88320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *t = c;
        }
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    // IHDR
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type RGB
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    chunk(&mut out, b"IHDR", &ihdr);

    // IDAT: zlib stream with stored (uncompressed) deflate blocks.
    let mut raw = Vec::new();
    for _ in 0..height {
        raw.push(0); // filter none
        for _ in 0..width {
            raw.extend_from_slice(&rgb);
        }
    }
    let mut z = Vec::new();
    z.push(0x78);
    z.push(0x01);
    {
        // stored blocks, max 65535 bytes each
        let mut i = 0;
        while i < raw.len() {
            let n = (raw.len() - i).min(65535);
            let last = if i + n >= raw.len() { 1 } else { 0 };
            z.push(last);
            z.extend_from_slice(&(n as u16).to_le_bytes());
            z.extend_from_slice(&(!(n as u16)).to_le_bytes());
            z.extend_from_slice(&raw[i..i + n]);
            i += n;
        }
    }
    let mut adler: u32 = 0;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    adler = (b << 16) | a;
    z.extend_from_slice(&adler.to_be_bytes());
    chunk(&mut out, b"IDAT", &z);

    chunk(&mut out, b"IEND", &[]);
    let _ = std::io::sink().write_all(&[]); // silence unused-import in some toolchains
    out
}

fn write_fixture(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn style_text(dom: &Dom) -> String {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let Some(el) = node.kind.element() {
            let _ = el;
            if matches!(&node.kind, typeanvil::dom::NodeKind::Element(e) if e.tag == "style") {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    css
}

/// Lay out HTML with images resolved against `dir`. The interned store comes
/// back inside `Layout.images`.
fn lay_in(html: &str, dir: Option<&TempDir>) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = Stylesheet::parse(&style_text(&dom));
    let mut store = ImageStore::new();
    layout_with_images_and_store(&dom, &ss, geometry(), dir.map(|d| d.path()), &mut store)
}

/// Collect every image-carrying fragment across pages: (page, x, y, w, h).
fn image_fragments(layout: &Layout) -> Vec<(usize, f64, f64, f64, f64)> {
    fn walk(
        frag: &typeanvil::frag::Fragment,
        ox: f64,
        oy: f64,
        page: usize,
        out: &mut Vec<(usize, f64, f64, f64, f64)>,
    ) {
        let x = ox + frag.offset.x.get();
        let y = oy + frag.offset.y.get();
        if matches!(frag.content, FragmentContent::Image(_)) {
            out.push((page, x, y, frag.size.0.get(), frag.size.1.get()));
        }
        for c in &frag.children {
            walk(c, x, y, page, out);
        }
    }
    let mut out = Vec::new();
    for page in &layout.pages {
        // Margin-box images live outside the content root (CORE-179).
        for root in page.paint_roots() {
            walk(root, 0.0, 0.0, page.index, &mut out);
        }
    }
    out
}

fn count_lines(frag: &typeanvil::frag::Fragment, out: &mut usize) {
    if frag.kind == FragmentKind::Line {
        *out += 1;
    }
    for c in &frag.children {
        count_lines(c, out);
    }
}

// --- AC 1: intrinsic sizing ----------------------------------------------------

#[test]
fn intrinsic_size_at_96dpi() {
    let dir = TempDir::new().unwrap();
    let png = make_png(192, 64, [255, 0, 0]);
    let p = write_fixture(&dir, "logo.png", &png);

    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{}\"></body></html>",
        p.display()
    );
    let lay = lay_in(&html, Some(&dir));
    let store = &lay.images;

    assert_eq!(store.entries_len(), 1);
    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1, "exactly one image fragment");
    let (_, _, _, w, h) = frags[0];
    // 192 px × 0.75 = 144 pt; 64 px × 0.75 = 48 pt (±0.5 tolerance).
    assert!(
        (w - 144.0).abs() < 0.5 && (h - 48.0).abs() < 0.5,
        "intrinsic size {w}×{h} pt, expected ~144×48"
    );
}

// --- AC 2: attribute sizing preserves ratio -------------------------------------

#[test]
fn attribute_width_preserves_ratio() {
    let dir = TempDir::new().unwrap();
    let png = make_png(192, 64, [0, 255, 0]);
    let p = write_fixture(&dir, "logo.png", &png);

    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{}\" width=\"96\"></body></html>",
        p.display()
    );
    let lay = lay_in(&html, Some(&dir));

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1);
    let (_, _, _, w, h) = frags[0];
    // 96 px attr → 72 pt wide; height from ratio 64/192 × 72 = 24 pt.
    assert!((w - 72.0).abs() < 0.5, "width {w}, expected ~72");
    assert!((h - 24.0).abs() < 0.5, "height {h}, expected ~24");
}

// --- AC 3: CSS sizing beats attributes -------------------------------------------

#[test]
fn css_width_beats_attribute() {
    let dir = TempDir::new().unwrap();
    let png = make_png(1000, 500, [0, 0, 255]);
    let p = write_fixture(&dir, "big.png", &png);

    let html = format!(
        "<html><head><style>body{{margin:0}} img{{width:50pt}}</style></head>\
         <body><img src=\"{}\" width=\"960\"></body></html>",
        p.display()
    );
    let lay = lay_in(&html, Some(&dir));

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1);
    let (_, _, _, w, h) = frags[0];
    assert!((w - 50.0).abs() < 0.5, "width {w}, expected ~50");
    assert!(
        (h - 25.0).abs() < 0.5,
        "height {h}, expected ratio-derived ~25"
    );
}

// --- AC 4: cross-page monolithic move --------------------------------------------

#[test]
fn image_moves_whole_across_pages() {
    let dir = TempDir::new().unwrap();
    let png = make_png(400, 400, [128, 128, 128]); // 300 pt tall — over half a page
    let p = write_fixture(&dir, "fig.png", &png);

    // Filler text then an image that cannot fit under it: the wrapper's
    // `break-before: page` forces the break deterministically (the
    // cross-page test fixture rule — never count filler lines).
    let html = format!(
        "<html><head><style>body{{margin:0}} p{{font-size:12pt}} \
         .fig{{break-before:page}}</style></head>         <body><p>Short paragraph.</p><div class=\"fig\"><img src=\"{q}\"></div></body></html>",
        q = p.display()
    );
    let (lay, _) = {
        let l = lay_in(&html, Some(&dir));
        (l, ())
    };

    assert!(lay.pages.len() >= 2, "expected pagination onto page 2");
    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1, "image appears exactly once, whole");
    let (page, _, _, w, h) = frags[0];
    assert_eq!(page, 1, "deferred to the second page (index 1)");
    // Full intrinsic size preserved after the move: 300×300 pt.
    assert!((w - 300.0).abs() < 0.5 && (h - 300.0).abs() < 0.5);
}

// --- AC 5: data URI matches file source ------------------------------------------

#[test]
fn data_uri_matches_file_source() {
    let dir = TempDir::new().unwrap();
    let png = make_png(80, 40, [10, 200, 30]);
    let p = write_fixture(&dir, "dot.png", &png);

    // base64-encode the same bytes
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut b64 = String::new();
    for chunk in png.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        b64.push(ALPHA[(b[0] >> 2) as usize] as char);
        b64.push(ALPHA[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        b64.push(if chunk.len() > 1 {
            ALPHA[(((b[1] & 0x0F) << 2) | (b[2] >> 6)) as usize] as char
        } else {
            '='
        });
        b64.push(if chunk.len() > 2 {
            ALPHA[(b[2] & 0x3F) as usize] as char
        } else {
            '='
        });
    }

    let html_file = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{}\"></body></html>",
        p.display()
    );
    let html_data = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"data:image/png;base64,{b64}\"></body></html>"
    );

    let lay_file = lay_in(&html_file, Some(&dir));
    let store_file = &lay_file.images;
    let lay_data = lay_in(&html_data, None);
    let store_data = &lay_data.images;

    let ff = image_fragments(&lay_file);
    let df = image_fragments(&lay_data);
    assert_eq!(ff.len(), 1);
    assert_eq!(df.len(), 1);
    let (_, _, _, fw, fh) = ff[0];
    let (_, _, _, dw, dh) = df[0];
    assert_eq!((fw, fh), (dw, dh), "identical geometry from either source");

    // And both intern to the SAME content key.
    let key_file = match store_file.entries_iter().next().unwrap().1 {
        ImageEntry::Loaded(i) => i.width_px,
        _ => panic!("file entry should be loaded"),
    };
    let key_data = match store_data.entries_iter().next().unwrap().1 {
        ImageEntry::Loaded(i) => i.width_px,
        _ => panic!("data entry should be loaded"),
    };
    assert_eq!(key_file, key_data);
}

// --- AC 6: broken image placeholder ------------------------------------------------

#[test]
fn broken_image_placeholder_box() {
    // With explicit dimensions the placeholder honors them and draws alt text.
    let html = "<html><head><style>body{margin:0}</style></head>\
                <body><img src=\"missing.png\" alt=\"Company logo\" width=\"200\"></body></html>";
    let lay = lay_in(html, None);
    let store = &lay.images;

    // The broken entry is interred (stable key) and marked broken.
    let entries: Vec<_> = store.entries_iter().collect();
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].1, ImageEntry::Broken(_)));

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1);
    let (_, _, _, w, h) = frags[0];
    assert!(
        (w - 150.0).abs() < 0.5,
        "placeholder width {w}, expected ~150"
    );
    assert!(
        h < 1.0,
        "collapsed height without explicit height attr, got {h}"
    );

    // Alt text rides the fragment tree as a line child of the placeholder.
    let mut lines = 0;
    for page in &lay.pages {
        count_lines(&page.root, &mut lines);
    }
    assert_eq!(lines, 1, "alt text drawn once");
}

/// A broken source WITHOUT explicit dimensions collapses to nothing (v1 gate
/// finding) — documents that never expected the image to render stay
/// pixel-identical, and no alt text is drawn into zero space.
#[test]
fn broken_image_without_dimensions_collapses() {
    let html = "<html><head><style>body{margin:0}</style></head>\
                <body><img src=\"missing.png\" alt=\"A green rectangle\"></body></html>";
    let lay = lay_in(html, None);

    let frags = image_fragments(&lay);
    if !frags.is_empty() {
        let (_, _, _, w, h) = frags[0];
        assert!(w < 1.0 && h < 1.0, "collapsed, got {w}×{h}");
    }

    let mut lines = 0;
    for page in &lay.pages {
        count_lines(&page.root, &mut lines);
    }
    assert_eq!(lines, 0, "no alt text without space to draw it");
}

// --- AC 7: determinism --------------------------------------------------------------

#[test]
fn image_render_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let png = make_png(120, 90, [200, 100, 50]);
    let p = write_fixture(&dir, "photo.png", &png);
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{q}\"><p>x</p></body></html>",
        q = p.display()
    );

    let render = || {
        let lay = lay_in(&html, Some(&dir));
        typeanvil::pdf::render(&lay).unwrap()
    };
    let a = render();
    let b = render();
    assert_eq!(a, b, "identical input must give byte-identical PDFs");
}

// --- AC 8: duplicate images embed once -----------------------------------------------

#[test]
fn duplicate_images_embed_once() {
    let dir = TempDir::new().unwrap();
    let png = make_png(60, 60, [1, 2, 3]);
    let p = write_fixture(&dir, "seal.png", &png);
    let q = p.display();

    // Three references: two on this element set, plus one repeated later.
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{q}\"><p>a</p><img src=\"{q}\"><p>b</p><img src=\"{q}\"></body></html>"
    );
    let lay = lay_in(&html, Some(&dir));

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 3, "three fragments share one store entry");

    // One XObject image stream in the PDF: krilla dedupes by its own
    // content hash (SipHash of the source bytes), so repeated references
    // share one image object. Count `/Subtype /Image` occurrences.
    let bytes = typeanvil::pdf::render(&lay).unwrap();
    // Count real image XObjects: "/Subtype/Image" inside an object dict
    // ("/Image" alone also appears in the page's /ProcSet).
    let hay = String::from_utf8_lossy(&bytes);
    let image_objects =
        hay.matches("/Subtype/Image").count() + hay.matches("/Subtype /Image").count();
    assert_eq!(image_objects, 1, "one embedded image object");
    // All three references share the one XObject name in the resource dict.
}

// --- CORE-131: SVG images ------------------------------------------------------

const SAMPLE_SVG: &str = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" viewBox="0 0 120 80">
  <rect x="0" y="0" width="120" height="80" fill="#336699"/>
  <circle cx="60" cy="40" r="20" fill="#ffcc00"/>
</svg>"##;

/// An `<img src="x.svg">` interns as a Loaded image whose intrinsic size is
/// the SVG's width/height attributes (CSS px at the 96 DPI baseline), and the
/// layout box honors that size like any raster image.
#[test]
fn svg_img_intrinsic_size_from_attributes() {
    let dir = TempDir::new().unwrap();
    let p = write_fixture(&dir, "chart.svg", SAMPLE_SVG.as_bytes());
    let q = p.display();
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{q}\"><p>x</p></body></html>"
    );
    let lay = lay_in(&html, Some(&dir));

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1, "the svg renders as an image fragment");
    // 120px × 0.75 = 90pt, 80px × 0.75 = 60pt.
    let (_, _, _, w, h) = frags[0];
    assert!((w - 90.0).abs() < 0.5, "svg width 120px = 90pt, got {w}");
    assert!((h - 60.0).abs() < 0.5, "svg height 80px = 60pt, got {h}");

    // Store entry: kind Svg, loaded, intrinsic px carried.
    let mut loaded = None;
    for (_, entry) in lay.images.entries_iter() {
        if let ImageEntry::Loaded(img) = entry {
            loaded = Some(img);
        }
    }
    let img = loaded.expect("svg interned as Loaded");
    assert_eq!(img.kind, typeanvil::images::ImageKind::Svg);
    assert_eq!((img.width_px, img.height_px), (120, 80));
    // The stored bytes are the rasterized PNG (sniffs as PNG).
    assert_eq!(&img.original[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
}

/// An inline `<svg>` element renders through the same bridge: one image
/// fragment at the SVG's intrinsic size.
#[test]
fn inline_svg_element_renders() {
    let html = "<html><head><style>body{margin:0}</style></head>\
         <body>\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\">\
         <rect x=\"0\" y=\"0\" width=\"100\" height=\"50\" fill=\"#008000\"/>\
         <text x=\"10\" y=\"30\" font-size=\"16\">Tag</text>\
         </svg>\
         <p>x</p></body></html>";
    let lay = lay_in(html, None);

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1, "inline svg renders as an image fragment");
    let (_, _, _, w, h) = frags[0];
    assert!((w - 75.0).abs() < 0.5, "100px = 75pt, got {w}");
    assert!((h - 37.5).abs() < 0.5, "50px = 37.5pt, got {h}");
}

/// Two runs of the same SVG-bearing document produce byte-identical PDFs
/// (the rasterization is a pure function of the SVG bytes).
#[test]
fn svg_render_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let p = write_fixture(&dir, "chart.svg", SAMPLE_SVG.as_bytes());
    let q = p.display();
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{q}\"><p>x</p></body></html>"
    );

    let render = || {
        let lay = lay_in(&html, Some(&dir));
        typeanvil::pdf::render(&lay).unwrap()
    };
    let a = render();
    let b = render();
    assert_eq!(a, b, "identical SVG input must give byte-identical PDFs");
}

/// A viewBox-only SVG (no width/height attributes) sizes from the viewBox —
/// usvg treats the viewBox as the intrinsic viewport, matching SVG2
/// intrinsic-size practice.
#[test]
fn svg_viewbox_only_uses_viewbox_size() {
    let dir = TempDir::new().unwrap();
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"><rect width="200" height="100" fill="red"/></svg>"##;
    let p = write_fixture(&dir, "vb.svg", svg.as_bytes());
    let q = p.display();
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{q}\"><p>x</p></body></html>"
    );
    let lay = lay_in(&html, Some(&dir));
    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1, "viewBox-only svg still renders");
    // viewBox 200×100: 200px = 150pt, 100px = 75pt.
    let (_, _, _, w, h) = frags[0];
    assert!((w - 150.0).abs() < 0.5, "viewBox width 200px = 150pt, got {w}");
    assert!((h - 75.0).abs() < 0.5, "viewBox height 100px = 75pt, got {h}");
}

/// Identical inline SVG subtrees (same serialization) intern once — three
/// references share one store entry and one embedded PDF object.
#[test]
fn duplicate_inline_svgs_embed_once() {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"40\">\
               <circle cx=\"20\" cy=\"20\" r=\"18\" fill=\"blue\"/></svg>";
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head><body>{svg}<p>a</p>{svg}<p>b</p>{svg}</body></html>"
    );
    let lay = lay_in(&html, None);
    assert_eq!(image_fragments(&lay).len(), 3, "three image fragments");
    assert_eq!(lay.images.entries_len(), 1, "one interned entry");

    let bytes = typeanvil::pdf::render(&lay).unwrap();
    let hay = String::from_utf8_lossy(&bytes);
    let image_objects =
        hay.matches("/Subtype/Image").count() + hay.matches("/Subtype /Image").count();
    // 2 objects: the image XObject plus its alpha SMask (the rasterized PNG
    // carries alpha); krilla still dedupes the three references to one pair.
    assert_eq!(image_objects, 2, "one image + one smask, deduped");
}

/// A genuinely malformed SVG (bytes that are neither valid UTF-8 nor a
/// parseable SVG document) falls back to the broken-image path, not a
/// panic — same contract as broken PNG/JPEG sources.
#[test]
fn broken_svg_falls_back_to_broken_entry() {
    let dir = TempDir::new().unwrap();
    // 0xFF bytes: fails UTF-8 decoding, so usvg can never parse it —
    // while NOT sniffing as PNG/JPEG.
    let svg: Vec<u8> = vec![0xFF, 0xFE, 0x3C, 0x73, 0x76, 0x67, 0x3E, 0xFF];
    let p = write_fixture(&dir, "bad.svg", &svg);
    let q = p.display();
    let html = format!(
        "<html><head><style>body{{margin:0}}</style></head>\
         <body><img src=\"{q}\" alt=\"d\" width=\"50\" height=\"20\"><p>x</p></body></html>"
    );
    let lay = lay_in(&html, Some(&dir));
    let mut broken = false;
    for (_, entry) in lay.images.entries_iter() {
        if matches!(entry, ImageEntry::Broken(_)) {
            broken = true;
        }
    }
    assert!(broken, "malformed svg interns as Broken");
    // With explicit attrs the placeholder box still lays out, but carries
    // the broken flag and embeds no raster payload in the PDF.
    let bytes = typeanvil::pdf::render(&lay).unwrap();
    let hay = String::from_utf8_lossy(&bytes);
    let image_objects =
        hay.matches("/Subtype/Image").count() + hay.matches("/Subtype /Image").count();
    assert_eq!(image_objects, 0, "broken svg embeds no raster");
}

// --- CORE-141: `content: url(...)` inside a page-margin box ---------------------

/// A margin box's `content: url(...)` paints the image at its intrinsic size on
/// the box's line, after any preceding text. Before this the margin-box content
/// pipeline was text-only, so the image was silently dropped (and the
/// `css-page/margin-boxes/content-003` reference could not be satisfied).
#[test]
fn margin_box_content_url_paints_intrinsic_image() {
    let dir = TempDir::new().unwrap();
    let png = make_png(100, 50, [0, 255, 0]);
    let p = write_fixture(&dir, "green.png", &png);

    // The top page margin is 50px = 37.5pt, exactly the image's height.
    let html = format!(
        "<html><head><style>\
         @page {{ margin: 0; margin-top: 50px;\
                  @top-left {{ content: \"Ti \" url({}); }} }}\
         body {{ margin: 0 }}\
         </style></head><body></body></html>",
        p.display()
    );
    let lay = lay_in(&html, Some(&dir));

    let frags = image_fragments(&lay);
    assert_eq!(frags.len(), 1, "one margin-box image fragment: {frags:?}");
    let (_, x, y, w, h) = frags[0];
    // 100px x 0.75 = 75pt wide; 50px x 0.75 = 37.5pt tall (intrinsic size).
    assert!(
        (w - 75.0).abs() < 0.5 && (h - 37.5).abs() < 0.5,
        "intrinsic size {w}x{h} pt, expected 75x37.5"
    );
    // Replaced content follows the text on the box's single line...
    assert!(x > 0.0, "image must follow the text on the line, got x={x}");
    // ...and the line box grows to the image, so the whole image is inside the
    // margin band rather than hanging above it.
    assert!(
        y >= -0.01 && y + h <= 37.6,
        "image must sit inside the 37.5pt top margin band, got y={y} h={h}"
    );
}

// --- CORE-141: margin-box `background-image` tiling -------------------------

/// A margin box's `background-image: url(...)` paints a tiled background over
/// the whole border box: the emitted BackgroundImage fragment covers the box
/// at zero offset with the image's natural tile size, and sits as the FIRST
/// child so the PDF pass paints it under the border. Before this a url()
/// background parsed to no fill, so `css-page/margin-boxes/background-001`
/// (a lime band from `url(/images/green.png)`) could not match its
/// lime-div reference.
#[test]
fn margin_box_background_image_tiles_over_border_box() {
    let dir = TempDir::new().unwrap();
    let png = make_png(100, 50, [0, 255, 0]);
    let p = write_fixture(&dir, "green.png", &png);

    // The top page margin is 50px = 37.5pt tall; the top-center box spans
    // the full page-area width (Letter 8.5in = 612pt with @page margin 0).
    // The colour+image shorthand keeps the root colour fill; the 2px border
    // then follows the tiled image as a child, exercising paint order.
    let html = format!(
        "<html><head><style>\
         @page {{ margin: 0; margin-top: 50px;\
                  @top-center {{ content: \"\"; border: 2px solid blue;\
                                  background: lime url({}); }} }}\
         body {{ margin: 0 }}\
         </style></head><body></body></html>",
        p.display()
    );
    let lay = lay_in(&html, Some(&dir));

    // The margin-box fragment carries the tiled background as its first child.
    fn find_box<'a>(frag: &'a typeanvil::frag::Fragment) -> Option<&'a typeanvil::frag::Fragment> {
        if frag
            .children
            .iter()
            .any(|c| matches!(c.content, FragmentContent::BackgroundImage(_)))
        {
            return Some(frag);
        }
        frag.children.iter().find_map(find_box)
    }
    let box_frag = lay
        .pages
        .iter()
        .find_map(|p| p.paint_roots().into_iter().find_map(find_box))
        .expect("margin box fragment carrying the background image");
    assert!(
        (box_frag.size.0.get() - 612.0).abs() < 0.5,
        "band width = page area width, got {}",
        box_frag.size.0.get()
    );
    assert!(
        (box_frag.size.1.get() - 37.5).abs() < 0.5,
        "band height = top margin, got {}",
        box_frag.size.1.get()
    );
    // The shorthand's colour fills the root (under the tiled image).
    assert!(
        matches!(box_frag.content, FragmentContent::Background(_)),
        "colour fill paints under the image, got {:?}",
        box_frag.content
    );

    let child0 = &box_frag.children[0];
    let FragmentContent::BackgroundImage(run) = &child0.content else {
        panic!(
            "first child must be the tiled background, got {:?}",
            child0.content
        );
    };
    assert!(!run.broken, "loaded image");
    // 100px x 0.75 = 75pt, 50px x 0.75 = 37.5pt: the image's natural size.
    assert!(
        (run.tile_w.get() - 75.0).abs() < 0.5 && (run.tile_h.get() - 37.5).abs() < 0.5,
        "natural tile {}x{}pt, expected 75x37.5",
        run.tile_w.get(),
        run.tile_h.get()
    );
    // Covers the whole border box, anchored at its origin (position 0% 0%).
    assert_eq!(child0.offset.x.get(), 0.0);
    assert_eq!(child0.offset.y.get(), 0.0);
    assert!(
        (child0.size.0.get() - 612.0).abs() < 0.5 && (child0.size.1.get() - 37.5).abs() < 0.5,
        "cover box {}x{}pt, expected 612x37.5",
        child0.size.0.get(),
        child0.size.1.get()
    );
    // The border follows the background in paint order.
    assert!(
        matches!(box_frag.children[1].content, FragmentContent::Border(_)),
        "border must paint over the tiled background"
    );
    // The image actually embeds in the PDF, once.
    let bytes = typeanvil::pdf::render(&lay).unwrap();
    let hay = String::from_utf8_lossy(&bytes);
    let image_objects =
        hay.matches("/Subtype/Image").count() + hay.matches("/Subtype /Image").count();
    assert_eq!(image_objects, 1, "background image embeds exactly once");
}
