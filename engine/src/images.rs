// SPDX-License-Identifier: AGPL-3.0-only

//! Image interning for `<img>` elements (CORE-106).
//!
//! The engine decodes nothing itself: krilla 0.8 parses PNG/JPEG headers and
//! embeds the canonical byte stream. What we need at LAYOUT time is only the
//! intrinsic pixel size (for CSS2.1 replaced-element sizing) and a stable
//! cache key so each unique byte stream interns exactly once. At EMIT time
//! [`crate::pdf`] builds one krilla `Image` per interned entry.
//!
//! Determinism: the store is a `BTreeMap` keyed by the SHA-256 of the source
//! bytes — never a path, never insertion order, no timestamps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// A successfully interned raster image.
#[derive(Clone, Debug)]
pub struct StoredImage {
    /// Pixel width from the codec header.
    pub width_px: u32,
    /// Pixel height from the codec header.
    pub height_px: u32,
    /// Codec/format of `original`.
    pub kind: ImageKind,
    /// The canonical encoded bytes (embedded as-is — no re-encode).
    pub original: Vec<u8>,
}

/// Supported image formats (spec Non-Goal 2: everything else is broken).
/// `Svg` is the CORE-131 rasterizer bridge: the source SVG is rasterized
/// once at intern time and `original` carries the PNG raster bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Svg,
}

/// An interned image that failed to load; renders as an alt-text
/// placeholder box (spec Behavior 7).
#[derive(Clone, Debug)]
pub struct BrokenImage {
    pub alt: Option<String>,
}

/// One entry in the store.
#[derive(Clone, Debug)]
pub enum ImageEntry {
    Loaded(StoredImage),
    Broken(BrokenImage),
}

/// Process-wide image store for one render. Keyed by content hash.
#[derive(Clone, Default, Debug)]
pub struct ImageStore {
    entries: BTreeMap<[u8; 32], ImageEntry>,
}

impl ImageStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve + sniff an image source, interning it once. Returns the key.
    ///
    /// - `data:` URIs (`image/png;base64`, `image/jpeg;base64`) decode to
    ///   bytes directly.
    /// - Any other source resolves as a file path relative to `base_url`
    ///   (absolute paths pass through).
    ///
    /// Broken sources still intern (as [`ImageEntry::Broken`]) so repeated
    /// broken references share one placeholder decision; this returns
    /// `Ok` unless the caller misuses the API.
    pub fn intern(
        &mut self,
        src: &str,
        base_url: Option<&Path>,
        alt: Option<String>,
    ) -> anyhow::Result<[u8; 32]> {
        let bytes = load_bytes(src, base_url);
        self.intern_bytes(bytes.unwrap_or_else(|| src.as_bytes().to_vec()), alt)
    }

    /// Intern already-loaded image bytes (CORE-131 inline `<svg>`: the DOM
    /// subtree is serialized to SVG text and fed here). Keyed by content
    /// hash like [`ImageStore::intern`]; empty/undecodable bytes intern as
    /// [`ImageEntry::Broken`].
    pub fn intern_bytes(
        &mut self,
        bytes: Vec<u8>,
        alt: Option<String>,
    ) -> anyhow::Result<[u8; 32]> {
        let key = sha256(&bytes);
        self.entries
            .entry(key)
            .or_insert_with(|| match sniff(&bytes) {
                Some((w, h, kind, stored)) => ImageEntry::Loaded(StoredImage {
                    width_px: w,
                    height_px: h,
                    kind,
                    original: stored,
                }),
                None => ImageEntry::Broken(BrokenImage { alt }),
            });
        Ok(key)
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<&ImageEntry> {
        self.entries.get(key)
    }

    /// Number of interned entries.
    pub fn entries_len(&self) -> usize {
        self.entries.len()
    }

    /// Deterministic iteration over (key, entry), key order.
    pub fn entries_iter(&self) -> impl Iterator<Item = (&[u8; 32], &ImageEntry)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

/// Read the raw bytes of a source. File paths resolve against `base_url`.
fn load_bytes(src: &str, base_url: Option<&Path>) -> Option<Vec<u8>> {
    if let Some(rest) = src.strip_prefix("data:") {
        return decode_data_uri(rest);
    }
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = resolve_path(trimmed, base_url);
    std::fs::read(path).ok()
}

/// Resolve a document-relative path against `--base-url`. Absolute paths
/// pass through unchanged.
fn resolve_path(src: &str, base_url: Option<&Path>) -> PathBuf {
    let p = Path::new(src);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match base_url {
        Some(base) => base.join(p),
        None => p.to_path_buf(),
    }
}

/// Decode the payload of a `data:` URI (the part after `data:`):
/// `image/png;base64,<payload>` with optional percent-encoding.
fn decode_data_uri(rest: &str) -> Option<Vec<u8>> {
    let (mime_and_params, payload) = rest.split_once(',')?;
    let mime = mime_and_params
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase();
    let is_base64 = mime_and_params
        .split(';')
        .any(|p| p.trim().eq_ignore_ascii_case("base64"));

    // Undo percent-encoding first (both plain and base64 URIs may be
    // percent-encoded). Minimal decoder: %XX pairs.
    let decoded: Vec<u8> = if payload.contains('%') {
        percent_decode(payload.as_bytes())
    } else {
        payload.as_bytes().to_vec()
    };

    match mime.as_str() {
        "image/png" | "image/jpeg" | "image/jpg" => {
            if is_base64 {
                decode_base64(&decoded)
            } else {
                // Plain (unencoded) payloads are rare but legal for tiny
                // images; treat the bytes as-is only when they sniff OK.
                sniff_ok_then_keep(decoded)
            }
        }
        _ => None,
    }
}

fn sniff_ok_then_keep(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if !bytes.is_empty() && (is_png(&bytes) || is_jpeg(&bytes)) {
        Some(bytes)
    } else {
        None
    }
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]);
            let lo = hex_val(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

const B64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode_base64(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut table = [255u8; 256];
    for (i, &c) in B64_ALPHABET.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut nbits = 0;
    for &b in bytes {
        if b == b'=' || b == b'\n' || b == b'\r' || b == b' ' || b == b'\t' {
            continue;
        }
        let v = table[b as usize];
        if v == 255 {
            return None;
        }
        acc = (acc << 6) | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Some(out)
}

/// Sniff a byte stream into (width, height, kind, stored bytes) via codec
/// headers. PNG/JPEG embed the source stream as-is; SVG rasterizes once and
/// stores the PNG raster. The layout-time contract is unchanged:
/// `width_px`/`height_px` are CSS pixels at the 96 DPI baseline
/// (1px = 0.75pt), same for every format.
fn sniff(b: &[u8]) -> Option<(u32, u32, ImageKind, Vec<u8>)> {
    if is_png(b) {
        png_dimensions(b).map(|(w, h)| (w, h, ImageKind::Png, b.to_vec()))
    } else if is_jpeg(b) {
        jpeg_dimensions(b).map(|(w, h)| (w, h, ImageKind::Jpeg, b.to_vec()))
    } else if is_svg(b) {
        // `rasterize_svg` returns (intrinsic_w_px, intrinsic_h_px, png_bytes):
        // the raster becomes `original` at emit time; the intrinsic CSS px
        // drive replaced-element sizing exactly like a PNG/JPEG header.
        rasterize_svg(b).map(|(w, h, png)| (w, h, ImageKind::Svg, png))
    } else {
        None
    }
}

fn is_png(b: &[u8]) -> bool {
    b.len() >= 8 && b[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
}

fn is_jpeg(b: &[u8]) -> bool {
    b.len() >= 4 && b[0] == 0xFF && b[1] == 0xD8
}

/// PNG IHDR: width/height are big-endian u32s at fixed offsets.
fn png_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
    let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
    Some((w, h))
}

/// JPEG SOFn scan: walk markers until a Start-of-Frame carries dimensions.
fn jpeg_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2; // skip SOI
    while i + 9 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        // Standalone markers without length payloads.
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if i + 4 > b.len() {
            return None;
        }
        let seg_len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
        // SOF0-SOF15 except DHT (C4), JPG (C8), DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            if i + 9 <= b.len() {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h));
            }
            return None;
        }
        i += 2 + seg_len;
    }
    None
}

/// SHA-256 (pure implementation; the engine keeps zero C FFI and this is the
/// only consumer in the crate). FIPS 180-4.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut bb, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & bb) ^ (a & c) ^ (bb & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = bb;
            bb = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(bb);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// --- SVG rasterizer bridge (CORE-131) --------------------------------------

/// Sniff SVG by content prefix: an XML prolog or an `<svg` element tag.
/// Deliberately lax — a mis-sniff costs one usvg parse failure and the
/// image falls back to Broken.
fn is_svg(b: &[u8]) -> bool {
    let t = b.trim_ascii();
    t.starts_with(b"<?xml") || t.starts_with(b"<svg")
}

/// Rasterize an SVG byte stream to PNG at a fixed 1× scale (96 DPI: 1 SVG
/// user unit = 1 CSS px), returning `(intrinsic_w_px, intrinsic_h_px, png)`.
///
/// Determinism contract (CORE-105): no timestamps, no environment reads, a
/// fixed rasterization scale and a fixed default font family — identical
/// input bytes give identical raster bytes on every run and machine.
fn rasterize_svg(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let text = std::str::from_utf8(b).ok()?;

    let mut opts = resvg::usvg::Options::default();
    // Deterministic text: one fixed family lookup, our bundled Arial via a
    // process-wide fontdb (system fonts + the four bundled faces). usvg
    // matches fontdb faces deterministically (weight/style match is a pure
    // function of the database, same registry model as fonts.rs).
    opts.fontdb = svg_fontdb();
    opts.font_family = "Arial".into();

    let tree = resvg::usvg::Tree::from_str(text, &opts).ok()?;
    let size = tree.size();
    let (w, h) = (size.width().round() as u32, size.height().round() as u32);
    if w == 0 || h == 0 || w > 16_384 || h > 16_384 {
        return None;
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    // 1×: identity transform — the tree renders at its own intrinsic size.
    // Transparent background (alpha preserved into PNG).
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().ok()?;
    Some((w, h, png))
}

/// Process-wide font database for SVG text rasterization. System fonts plus
/// the four bundled Arial faces so `default_font_family` resolves even on a
/// bare machine. Same model as fonts.rs (a LazyLock fontdb), kept separate
/// so the SVG path can never mutate the text-engine registry.
static SVG_FONTDB: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for path in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold Italic.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            db.load_font_data(bytes);
        }
    }
    db
});

fn svg_fontdb() -> std::sync::Arc<fontdb::Database> {
    std::sync::Arc::new(SVG_FONTDB.clone())
}
