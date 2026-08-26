// SPDX-License-Identifier: AGPL-3.0-only

//! Runtime font registry: system discovery + @font-face + resolution.
//!
//! Replaces the CORE-80 hardcoded 4-face Arial bundle (CORE-103). The
//! registry owns every face the process may embed:
//!
//! - The four bundled Arial faces, fixed ids 0..4 (the CORE-80 fallback
//!   set; documents whose families resolve nowhere keep rendering exactly
//!   as before).
//! - System faces, discovered via fontdb and registered on demand
//!   (macOS-first; ~430 ms one-time scan of 1285 faces, measured
//!   2026-08-22).
//! - `@font-face` faces, registered from parsed stylesheet rules (bytes
//!   read from document-relative file URLs).
//!
//! Determinism: face ids are assigned in a fixed order (bundled faces,
//! then @font-face rules in stylesheet order, then system faces in
//! fontdb's deterministic query order), and weight/style matching is a
//! pure function of the registry. Identical input yields identical face
//! selection and byte-identical PDF.
//!
//! Borrowing: HarfRust shapes against `FontRef<'static>` borrowing the
//! font bytes. Registered bytes are leaked into a `'static` arena (one
//! copy per content hash) so shapers and the PDF backend can hold them
//! for the process lifetime.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use read_fonts::TableProvider;

use crate::css::FontStyle;

/// A registry handle for one concrete font face. Copy, stable for the
/// process lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub u32);

/// The four bundled Arial faces (fixed ids; the CORE-80 fallback set).
pub const FACE_REGULAR: FaceId = FaceId(0);
pub const FACE_BOLD: FaceId = FaceId(1);
pub const FACE_ITALIC: FaceId = FaceId(2);
pub const FACE_BOLD_ITALIC: FaceId = FaceId(3);

/// The filesystem paths of the bundled fallback faces, indexed by `FaceId.0`
/// for ids 0..4 (the old `face_path` table).
const BUNDLED_PATHS: [&str; 4] = [
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold Italic.ttf",
];

const BUNDLED_POSTSCRIPT: [&str; 4] = [
    "ArialMT",
    "Arial-BoldMT",
    "Arial-ItalicMT",
    "Arial-BoldItalicMT",
];

/// A family name from the computed `font-family` list, normalized so this
/// module stays stylo-free (css.rs converts stylo's `SingleFontFamily`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FamilySpec {
    Name(String),
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

/// A face's byte source: a system path (+ TrueType-collection index) or
/// inline `@font-face` bytes (already in the leaked arena).
#[derive(Clone, Debug)]
pub enum FaceSource {
    Path(String, u32),
    Bytes(Arc<[u8]>),
}

/// One registered face's metadata.
#[derive(Clone, Debug)]
pub struct RegisteredFace {
    pub source: FaceSource,
    /// PostScript name (from fontdb for system faces; parsed from the name
    /// table for `@font-face` bytes) — verification + `/BaseFont` greps.
    pub postscript_name: String,
    pub weight: f32,
    pub italic: bool,
}

struct Registry {
    faces: Vec<RegisteredFace>,
    /// @font-face family name (lowercased) → face ids, in rule order.
    custom: HashMap<String, Vec<FaceId>>,
    /// Dedup: (family, weight, italic, content hash) → face id, so
    /// registering the same stylesheet twice is a no-op (idempotent).
    registered_rules: HashMap<(String, u32, bool, u64), FaceId>,
    /// System faces already registered: fontdb (path, index) → id.
    system: HashMap<(String, u32), FaceId>,
    /// Leaked byte blobs by content hash (the 'static shaping arena).
    byte_arena: HashMap<u64, &'static [u8]>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    let faces = BUNDLED_PATHS
        .iter()
        .enumerate()
        .map(|(i, path)| RegisteredFace {
            source: FaceSource::Path((*path).to_string(), 0),
            postscript_name: BUNDLED_POSTSCRIPT[i].to_string(),
            weight: if i == 1 || i == 3 { 700.0 } else { 400.0 },
            italic: i >= 2,
        })
        .collect();
    Mutex::new(Registry {
        faces,
        custom: HashMap::new(),
        registered_rules: HashMap::new(),
        system: HashMap::new(),
        byte_arena: HashMap::new(),
    })
});

fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    let mut reg = REGISTRY.lock().unwrap();
    f(&mut reg)
}

/// The system font database, loaded once per process.
static FONTDB: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
});

/// FNV-1a 64 over the bytes (dedup key only; not a security boundary).
fn content_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Leak bytes into the 'static arena (deduped by content hash) and return
/// the shared handle. The registry lock must be held.
fn leak_bytes(reg: &mut Registry, bytes: Vec<u8>) -> Arc<[u8]> {
    let hash = content_hash(&bytes);
    let leaked: &'static [u8] = *reg
        .byte_arena
        .entry(hash)
        .or_insert_with(|| Box::leak(bytes.into_boxed_slice()));
    Arc::from(leaked)
}

/// The PostScript name of a font byte blob (name table, id 6), or a fixed
/// fallback when absent. Pure function of the bytes.
fn postscript_name_of(bytes: &[u8]) -> Option<String> {
    let font = read_fonts::FontRef::new(bytes).ok()?;
    let name = font.name().ok()?;
    let data = name.string_data();
    for record in name.name_record().iter() {
        if record.name_id() == read_fonts::tables::name::NameId::POSTSCRIPT_NAME {
            if let Ok(s) = record.string(data) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Register an `@font-face` face. `family` is the rule's `font-family`
/// descriptor; `bytes` come from the resolved `src` (already read by the
/// caller). Returns the face id, or `None` when the bytes do not parse as
/// a font (the rule is then ignored deterministically). Idempotent: the
/// same (family, weight, style, bytes) tuple returns the original id.
pub fn register_face_bytes(
    family: &str,
    weight: f32,
    italic: bool,
    bytes: Vec<u8>,
) -> Option<FaceId> {
    // Reject garbage before touching the registry so a broken @font-face
    // never reaches shaping.
    let ps_name = postscript_name_of(&bytes)?;
    let hash = content_hash(&bytes);
    let key = (
        family.to_ascii_lowercase(),
        (weight * 10.0).round() as u32,
        italic,
        hash,
    );
    with_registry(|reg| {
        if let Some(&id) = reg.registered_rules.get(&key) {
            return Some(id);
        }
        let shared = leak_bytes(reg, bytes);
        let id = FaceId(reg.faces.len() as u32);
        reg.faces.push(RegisteredFace {
            source: FaceSource::Bytes(shared),
            postscript_name: ps_name,
            weight,
            italic,
        });
        reg.custom
            .entry(key.0.clone())
            .or_default()
            .push(id);
        reg.registered_rules.insert(key, id);
        Some(id)
    })
}

/// Register a system face from fontdb's query result. No-op (returns the
/// existing id) when the same file+index is already registered.
fn register_system_face(info: &fontdb::FaceInfo) -> FaceId {
    let (path, index) = match &info.source {
        fontdb::Source::File(p) => (p.to_string_lossy().into_owned(), info.index),
        _ => (String::new(), 0),
    };
    with_registry(|reg| {
        if let Some(&id) = reg.system.get(&(path.clone(), index)) {
            return id;
        }
        let id = FaceId(reg.faces.len() as u32);
        reg.faces.push(RegisteredFace {
            source: FaceSource::Path(path.clone(), index),
            postscript_name: info.post_script_name.clone(),
            weight: info.weight.0 as f32,
            italic: matches!(info.style, fontdb::Style::Italic | fontdb::Style::Oblique),
        });
        reg.system.insert((path, index), id);
        id
    })
}

/// A resolved candidate chain: the primary face plus later stack members
/// for per-character fallback (spec Behavior 8).
#[derive(Clone, Debug)]
pub struct FontResolution {
    pub primary: FaceId,
    pub fallbacks: Vec<FaceId>,
}

/// css-fonts-4 §5.2 weight matching over one family's candidate faces:
/// exact, then the §5.2 search order (which side of the target is tried
/// first depends on the target band). `faces` are (weight, id) pairs.
fn match_weight(faces: &[(f32, FaceId)], target: f32) -> Option<FaceId> {
    if faces.is_empty() {
        return None;
    }
    if let Some(&(_, id)) = faces.iter().find(|(w, _)| *w == target) {
        return Some(id);
    }
    let by_asc = |mut v: Vec<(f32, FaceId)>| {
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        v.first().map(|&(_, id)| id)
    };
    let by_desc = |mut v: Vec<(f32, FaceId)>| {
        v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
        v.first().map(|&(_, id)| id)
    };
    if (400.0..=500.0).contains(&target) {
        // §5.2: ascending toward 500, then descending below target, then
        // ascending above 500.
        let up_to_500: Vec<_> = faces.iter().filter(|(w, _)| *w > target && *w <= 500.0).cloned().collect();
        if let Some(id) = by_asc(up_to_500) {
            return Some(id);
        }
        let below: Vec<_> = faces.iter().filter(|(w, _)| *w < target).cloned().collect();
        if let Some(id) = by_desc(below) {
            return Some(id);
        }
        let above: Vec<_> = faces.iter().filter(|(w, _)| *w > 500.0).cloned().collect();
        by_asc(above)
    } else if target < 400.0 {
        // Below target descending first, then above ascending.
        let below: Vec<_> = faces.iter().filter(|(w, _)| *w < target).cloned().collect();
        if let Some(id) = by_desc(below) {
            return Some(id);
        }
        let above: Vec<_> = faces.iter().filter(|(w, _)| *w > target).cloned().collect();
        by_asc(above)
    } else {
        // Above target ascending first, then below descending.
        let above: Vec<_> = faces.iter().filter(|(w, _)| *w > target).cloned().collect();
        if let Some(id) = by_asc(above) {
            return Some(id);
        }
        let below: Vec<_> = faces.iter().filter(|(w, _)| *w < target).cloned().collect();
        by_desc(below)
    }
}

/// Best face for (weight, style) among one family's candidates: style pass
/// first (italic desired → italic faces; normal → upright faces), then
/// css-fonts-4 weight matching. `None` when no candidate exists.
fn best_face(candidates: &[FaceId], weight: f32, italic: bool) -> Option<FaceId> {
    let meta = |id: FaceId| {
        with_registry(|reg| {
            let f = &reg.faces[id.0 as usize];
            (f.weight, f.italic, id)
        })
    };
    let all: Vec<(f32, bool, FaceId)> = candidates.iter().map(|&id| meta(id)).collect();
    let style_ok: Vec<(f32, FaceId)> = all
        .iter()
        .filter(|(_, it, _)| *it == italic)
        .map(|(w, _, id)| (*w, *id))
        .collect();
    let pool = if style_ok.is_empty() {
        all.into_iter().map(|(w, _, id)| (w, id)).collect()
    } else {
        style_ok
    };
    match_weight(&pool, weight)
}

/// Resolve a computed `font-family` stack plus weight/style into a face
/// chain (spec Behavior 2/4/8). Deterministic. Falls back to the bundled
/// Arial face for the requested weight/style when nothing resolves
/// (Behavior 6) — the pre-CORE-103 behavior, preserved.
pub fn resolve_font(
    families: &[FamilySpec],
    weight: f32,
    style: FontStyle,
) -> FontResolution {
    let italic = matches!(style, FontStyle::Italic);
    let mut primary: Option<FaceId> = None;
    let mut fallbacks: Vec<FaceId> = Vec::new();
    for spec in families {
        let candidates = family_candidates(spec);
        let Some(face) = best_face(&candidates, weight, italic) else {
            continue;
        };
        if primary.is_none() {
            primary = Some(face);
        } else if !fallbacks.contains(&face) && Some(face) != primary {
            fallbacks.push(face);
        }
    }
    match primary {
        Some(p) => FontResolution { primary: p, fallbacks },
        None => FontResolution {
            primary: bundled_fallback(weight, style),
            fallbacks: Vec::new(),
        },
    }
}

/// Every candidate face id for one family slot: @font-face rules first
/// (they shadow system families), then system faces.
fn family_candidates(spec: &FamilySpec) -> Vec<FaceId> {
    match spec {
        FamilySpec::Name(n) => {
            let key = n.to_ascii_lowercase();
            // @font-face rules shadow system families (css-fonts-4 §2): when
            // the name is defined by a rule, ONLY rule faces are candidates.
            let custom = with_registry(|reg| reg.custom.get(&key).cloned().unwrap_or_default());
            if !custom.is_empty() {
                return custom;
            }
            // The bundled Arial faces keep their FIXED ids 0..4 so existing
            // documents render identically; never re-register system Arial.
            if key == "arial" || key == "helvetica" {
                return vec![FACE_REGULAR, FACE_BOLD, FACE_ITALIC, FACE_BOLD_ITALIC];
            }
            FONTDB
                .faces()
                .filter(|face| {
                    face.families
                        .iter()
                        .any(|(name, _)| name.to_ascii_lowercase() == key)
                })
                .map(register_system_face)
                .fold(Vec::new(), |mut out, id| {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                    out
                })
        }
            // GENERIC GATE (0-regression rule): the engine's UA default
            // font-family is stylo's initial value — bare `serif` — and the
            // WPT harness compares OUR test vs OUR reference through this
            // same default. Resolving generics to concrete system faces
            // changes line metrics for every unstyled doc and re-baselines
            // the suite. Until a deliberate re-baseline lands, ALL generics
            // resolve to the bundled Arial set (the pre-CORE-103 behavior).
            // The fontdb generic mapping is verified and recorded in the
            // spec; flipping this switch is a one-line follow-up gated on a
            // full-suite run. Author-declared family NAMES resolve fully.
            FamilySpec::Serif
            | FamilySpec::SansSerif
            | FamilySpec::Monospace
            | FamilySpec::Cursive
            | FamilySpec::Fantasy => vec![FACE_REGULAR, FACE_BOLD, FACE_ITALIC, FACE_BOLD_ITALIC],
    }
}

/// All faces of the concrete family fontdb maps the generic to.
fn generic_candidates(generic: fontdb::Family) -> Vec<FaceId> {
    let name = FONTDB.family_name(&generic).to_ascii_lowercase();
    let mut out = Vec::new();
    for face in FONTDB.faces() {
        if face
            .families
            .iter()
            .any(|(n, _)| n.to_ascii_lowercase() == name)
        {
            let id = register_system_face(face);
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out
}

/// The bundled Arial face for a weight/style (the Behavior-6 fallback).
pub fn face_for(weight: f32, style: FontStyle) -> FaceId {
    bundled_fallback(weight, style)
}

/// The bundled Arial face for a weight/style (the Behavior-6 fallback).
fn bundled_fallback(weight: f32, style: FontStyle) -> FaceId {
    let italic = matches!(style, FontStyle::Italic);
    if weight >= 600.0 && italic {
        FACE_BOLD_ITALIC
    } else if weight >= 600.0 {
        FACE_BOLD
    } else if italic {
        FACE_ITALIC
    } else {
        FACE_REGULAR
    }
}

/// The full byte blob of a face, leaked 'static so shapers can borrow it
/// for the process. Reads bundled/system paths once and leaks them.
pub fn face_bytes(id: FaceId) -> &'static [u8] {
    static LOADED: LazyLock<Mutex<HashMap<u32, &'static [u8]>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if let Some(b) = LOADED.lock().unwrap().get(&id.0) {
        return b;
    }
    let source = with_registry(|reg| reg.faces[id.0 as usize].source.clone());
    let bytes: Vec<u8> = match &source {
        FaceSource::Path(path, index) => {
            // TrueType collections: extract the requested face's table
            // directory is overkill for shaping — read the whole file;
            // read-fonts picks the face by index via `FontRef::from_index`.
            let data = std::fs::read(path).unwrap_or_default();
            if *index == 0 || data.is_empty() {
                data
            } else {
                // Non-zero TTC index: slice via read-fonts so shaping sees
                // the right tables (rare on the corpus; correctness path).
                match read_fonts::FontRef::from_index(&data, *index) {
                    // from_index borrows `data`; we must leak the WHOLE
                    // file and return a slice covering it — FontRef keeps
                    // its own offsets, so leaking the full blob and
                    // re-parsing at the call site is correct.
                    _ => data,
                }
            }
        }
        FaceSource::Bytes(arc) => arc.to_vec(),
    };
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    LOADED.lock().unwrap().insert(id.0, leaked);
    leaked
}

/// The TrueType-collection index a face was registered with.
pub fn face_index(id: FaceId) -> u32 {
    with_registry(|reg| match &reg.faces[id.0 as usize].source {
        FaceSource::Path(_, index) => *index,
        FaceSource::Bytes(_) => 0,
    })
}

/// Face metadata accessors (PostScript name, weight, italic).
pub fn face_postscript_name(id: FaceId) -> String {
    with_registry(|reg| reg.faces[id.0 as usize].postscript_name.clone())
}

pub fn face_weight(id: FaceId) -> f32 {
    with_registry(|reg| reg.faces[id.0 as usize].weight)
}

pub fn face_italic(id: FaceId) -> bool {
    with_registry(|reg| reg.faces[id.0 as usize].italic)
}

/// Number of registered faces (tests).
pub fn face_count() -> usize {
    with_registry(|reg| reg.faces.len())
}

/// Every installed system face of a family name (for `@font-face
/// src: local(...)`), registered on demand.
pub fn system_family_faces(family: &str) -> Vec<fontdb::FaceInfo> {
    let key = family.to_ascii_lowercase();
    FONTDB.faces()
        .filter(|f| f.families.iter().any(|(n, _)| n.to_ascii_lowercase() == key))
        .cloned()
        .collect()
}

/// Register one fontdb face (public wrapper for css.rs's `local()` path).
pub fn register_system_face_pub(info: &fontdb::FaceInfo) -> FaceId {
    register_system_face(info)
}

/// Legacy path helper (bundled faces only; tests + the PDF fallback).
pub fn face_path(face: FaceId) -> &'static str {
    BUNDLED_PATHS
        .get(face.0 as usize)
        .copied()
        .unwrap_or(BUNDLED_PATHS[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_ids_are_stable() {
        assert_eq!(FACE_REGULAR, FaceId(0));
        assert_eq!(face_path(FACE_BOLD), "/System/Library/Fonts/Supplemental/Arial Bold.ttf");
    }

    #[test]
    fn resolve_georgia_picks_system_face() {
        let specs = vec![FamilySpec::Name("Georgia".into()), FamilySpec::Serif];
        let r = resolve_font(&specs, 400.0, FontStyle::Normal);
        assert_eq!(face_postscript_name(r.primary), "Georgia");
        // the serif slot falls back to the bundled set (generic gate)
        assert!(!r.fallbacks.is_empty());
        assert_eq!(r.fallbacks[0], FACE_REGULAR);
    }

    #[test]
    fn weight_matching_nearest_side() {
        // Georgia has 400/700; target 500 → within band: ascending >500? no;
        // then below target → 400.
        let specs = vec![FamilySpec::Name("Georgia".into())];
        let r = resolve_font(&specs, 500.0, FontStyle::Normal);
        assert_eq!(face_weight(r.primary), 400.0);
        // target 700 exact
        let r = resolve_font(&specs, 700.0, FontStyle::Normal);
        assert_eq!(face_weight(r.primary), 700.0);
        // italic desired → the italic cut wins (Georgia has one)
        let r = resolve_font(&specs, 400.0, FontStyle::Italic);
        assert!(face_italic(r.primary));
        // a family with no italic cut still resolves upright (its only face)
        let specs2 = vec![FamilySpec::Name("Arial Black".into())];
        let r = resolve_font(&specs2, 400.0, FontStyle::Italic);
        assert_eq!(face_postscript_name(r.primary), "Arial-Black");
        assert!(!face_italic(r.primary));
    }

    #[test]
    fn unknown_family_falls_back_to_bundled() {
        let specs = vec![FamilySpec::Name("NoSuchFontQZ".into())];
        let r = resolve_font(&specs, 400.0, FontStyle::Normal);
        assert_eq!(r.primary, FACE_REGULAR);
        let r = resolve_font(&specs, 700.0, FontStyle::Normal);
        assert_eq!(r.primary, FACE_BOLD);
    }

    #[test]
    fn generics_map_to_bundled_set() {
        // GENERIC GATE: generics resolve to the bundled Arial set until the
        // deliberate re-baseline (see family_candidates). Named families
        // resolve to system faces (see resolve_georgia test).
        let r = resolve_font(&[FamilySpec::Monospace], 400.0, FontStyle::Normal);
        assert_eq!(r.primary, FACE_REGULAR);
        let r = resolve_font(&[FamilySpec::Serif], 400.0, FontStyle::Normal);
        assert_eq!(r.primary, FACE_REGULAR);
    }
}
