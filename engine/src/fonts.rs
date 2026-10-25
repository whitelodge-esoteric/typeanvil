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
///
/// Each face has a candidate list, tried in order; the first readable path
/// wins (see `bundled_path`). The macOS Arial set stays first so existing
/// macOS hosts and the dev container (which mounts macOS fonts) render
/// byte-identically to pre-CORE-247 output. On Linux hosts without Arial —
/// i.e. the shipped container image — the bundled open-source set (Liberation
/// Sans, metric-compatible with Arial) is the fallback, so a Linux render
/// never reads empty font bytes (CORE-247).
const BUNDLED_PATH_CANDIDATES: [&[&str]; 4] = [
    &[
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/typeanvil/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ],
    &[
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/usr/share/fonts/typeanvil/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ],
    &[
        "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
        "/usr/share/fonts/typeanvil/LiberationSans-Italic.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Italic.ttf",
    ],
    &[
        "/System/Library/Fonts/Supplemental/Arial Bold Italic.ttf",
        "/usr/share/fonts/typeanvil/LiberationSans-BoldItalic.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-BoldItalic.ttf",
    ],
];

/// The first readable path for a bundled face: the first candidate that
/// exists as a file. Falls back to the last candidate when none exist so
/// callers can produce a clear error instead of silently reading empty
/// bytes. Deterministic for a given host (candidates are ordered and the
/// existence check is stable for a process lifetime in practice).
pub fn bundled_path(id: u32) -> &'static str {
    let candidates = BUNDLED_PATH_CANDIDATES[id as usize];
    candidates
        .iter()
        .find(|p| std::path::Path::new(p).is_file())
        .copied()
        .unwrap_or(candidates[candidates.len() - 1])
}

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
    let faces = (0..4)
        .map(|i| RegisteredFace {
            source: FaceSource::Path(bundled_path(i as u32).to_string(), 0),
            // Name table (nameID 6) of the resolved file; the Arial constant
            // is only a fallback for an unreadable file (CORE-476).
            postscript_name: std::fs::read(bundled_path(i as u32))
                .ok()
                .and_then(|b| postscript_name_of(&b))
                .unwrap_or_else(|| BUNDLED_POSTSCRIPT[i].to_string()),
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
        reg.custom.entry(key.0.clone()).or_default().push(id);
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
        let up_to_500: Vec<_> = faces
            .iter()
            .filter(|(w, _)| *w > target && *w <= 500.0)
            .cloned()
            .collect();
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
pub fn resolve_font(families: &[FamilySpec], weight: f32, style: FontStyle) -> FontResolution {
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
        Some(p) => FontResolution {
            primary: p,
            fallbacks,
        },
        None => FontResolution {
            primary: bundled_fallback(weight, style),
            fallbacks: Vec::new(),
        },
    }
}

/// True when a named family resolves to at least one face (an `@font-face`
/// rule or an installed system family). Render report (CORE-571): a `false`
/// first family means the text used a later family or the bundled fallback.
pub fn family_available(name: &str) -> bool {
    !family_candidates(&FamilySpec::Name(name.to_string())).is_empty()
}

/// Identity of the bundled face set this process renders with (CORE-571):
/// `<regular PostScript name>@<FNV-1a 64 of the four face files, hex>`.
/// Stable for one host; changes when the bundled files change.
pub fn bundle_fingerprint() -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &id in BUNDLED_FACES.iter() {
        h ^= content_hash(face_bytes(id));
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{}@{:016x}", face_postscript_name(FACE_REGULAR), h)
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
            system_family_candidates(&key)
        }
        // Generic families resolve through the engine-owned table (spec
        // Behavior 5, CORE-361), not fontdb's generic slots: on Linux fontdb
        // reads those from fontconfig aliases, which picked FreeSerif and
        // uninstalled faces in the dev container. First installed family
        // wins; otherwise the bundled set. `sans-serif` IS the bundled set
        // (fixed ids 0..4), so sans documents stay byte-identical.
        FamilySpec::SansSerif => BUNDLED_FACES.to_vec(),
        FamilySpec::Serif => generic_table_candidates(GENERIC_SERIF),
        FamilySpec::Monospace => generic_table_candidates(GENERIC_MONOSPACE),
        FamilySpec::Cursive => generic_table_candidates(GENERIC_CURSIVE),
        FamilySpec::Fantasy => generic_table_candidates(GENERIC_FANTASY),
    }
}

/// The bundled faces (fixed ids 0..4).
const BUNDLED_FACES: [FaceId; 4] = [FACE_REGULAR, FACE_BOLD, FACE_ITALIC, FACE_BOLD_ITALIC];

/// Ordered concrete families per generic (spec Behavior 5). Liberation
/// Serif / Mono are metric-compatible with Times New Roman / Courier New,
/// so macOS and the Linux images lay text out alike.
const GENERIC_SERIF: &[&str] = &["times new roman", "liberation serif"];
const GENERIC_MONOSPACE: &[&str] = &["courier new", "liberation mono"];
const GENERIC_CURSIVE: &[&str] = &["comic sans ms"];
const GENERIC_FANTASY: &[&str] = &["papyrus", "impact"];

/// The first installed family of a generic's table, else the bundled set.
/// Lowercase names; system faces only (an @font-face rule does not redefine
/// a generic, css-fonts-4 §4.2).
fn generic_table_candidates(families: &[&str]) -> Vec<FaceId> {
    families
        .iter()
        .map(|name| system_family_candidates(name))
        .find(|c| !c.is_empty())
        .unwrap_or_else(|| BUNDLED_FACES.to_vec())
}

/// Every system face of one family name (lowercase key). Arial/Helvetica
/// map to the bundled faces, which keep their FIXED ids 0..4 so existing
/// documents render identically; never re-register system Arial.
fn system_family_candidates(key: &str) -> Vec<FaceId> {
    if key == "arial" || key == "helvetica" {
        return BUNDLED_FACES.to_vec();
    }
    let faces: Vec<&fontdb::FaceInfo> = FONTDB
        .faces()
        .filter(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.to_ascii_lowercase() == key)
        })
        .collect();
    // css-fonts-4 §5.2 matches font-stretch BEFORE style and weight. The
    // engine always wants `normal` width, so when the family has any
    // normal-width face, condensed/expanded faces are not candidates
    // (e.g. Papyrus vs Papyrus-Condensed, both 400 upright — CORE-361).
    let has_normal = faces.iter().any(|f| f.stretch == fontdb::Stretch::Normal);
    faces
        .into_iter()
        .filter(|f| !has_normal || f.stretch == fontdb::Stretch::Normal)
        .map(register_system_face)
        .fold(Vec::new(), |mut out, id| {
            if !out.contains(&id) {
                out.push(id);
            }
            out
        })
}

/// Preferred system families for per-glyph fallback (CORE-386), tried in
/// order before the scan of every other installed face. Lowercase names.
/// Broad-coverage symbol/Unicode faces come first on each platform.
const FALLBACK_FAMILIES: &[&str] = &[
    "arial unicode ms",
    "apple symbols",
    "menlo",
    "dejavu sans",
    "liberation sans",
    "freesans",
    "noto sans symbols",
    "noto sans symbols 2",
    "noto sans math",
];

fn face_covers(info: &fontdb::FaceInfo, ch: char) -> bool {
    FONTDB
        .with_face_data(info.id, |data, index| {
            read_fonts::FontRef::from_index(data, index)
                .ok()
                .and_then(|f| f.cmap().ok().and_then(|c| c.map_codepoint(ch)))
                .map(|g| g.to_u32() != 0)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// The first installed face that has a glyph for `ch` (CORE-386): the
/// preferred families in `FALLBACK_FAMILIES` order, then every other
/// upright normal-width face sorted by PostScript name (deterministic).
/// Cached per codepoint. `None` when no installed face covers it.
pub fn fallback_face_for(ch: char) -> Option<FaceId> {
    static CACHE: LazyLock<Mutex<HashMap<char, Option<FaceId>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if let Some(r) = CACHE.lock().unwrap().get(&ch) {
        return *r;
    }
    let usable = |f: &&fontdb::FaceInfo| {
        f.style == fontdb::Style::Normal && f.stretch == fontdb::Stretch::Normal
    };
    let mut found: Option<fontdb::FaceInfo> = None;
    'pref: for fam in FALLBACK_FAMILIES {
        let mut faces: Vec<&fontdb::FaceInfo> = FONTDB
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n.to_ascii_lowercase() == *fam))
            .filter(usable)
            .collect();
        faces.sort_by_key(|f| ((f.weight.0 as i32 - 400).abs(), f.post_script_name.clone()));
        for f in faces {
            if face_covers(f, ch) {
                found = Some(f.clone());
                break 'pref;
            }
        }
    }
    if found.is_none() {
        let mut faces: Vec<&fontdb::FaceInfo> = FONTDB.faces().filter(usable).collect();
        faces.sort_by_key(|f| ((f.weight.0 as i32 - 400).abs(), f.post_script_name.clone()));
        found = faces.into_iter().find(|f| face_covers(f, ch)).cloned();
    }
    let id = found.map(|f| register_system_face(&f));
    CACHE.lock().unwrap().insert(ch, id);
    id
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

/// True when the face came from an author `@font-face` rule (inline bytes).
pub fn is_font_face_bytes(id: FaceId) -> bool {
    with_registry(|reg| {
        matches!(
            reg.faces.get(id.0 as usize).map(|f| &f.source),
            Some(FaceSource::Bytes(_))
        )
    })
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
    FONTDB
        .faces()
        .filter(|f| {
            f.families
                .iter()
                .any(|(n, _)| n.to_ascii_lowercase() == key)
        })
        .cloned()
        .collect()
}

/// Register one fontdb face (public wrapper for css.rs's `local()` path).
pub fn register_system_face_pub(info: &fontdb::FaceInfo) -> FaceId {
    register_system_face(info)
}

/// Legacy path helper (bundled faces only; tests + the PDF fallback).
/// Returns the first READABLE bundled path for the face (macOS Arial when
/// present; the container image's Liberation set otherwise — CORE-247).
pub fn face_path(face: FaceId) -> &'static str {
    bundled_path(face.0 as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_ids_are_stable() {
        assert_eq!(FACE_REGULAR, FaceId(0));
        // The candidate order is part of the contract: macOS Arial first, so
        // macOS hosts and the dev container render as before; Liberation
        // second, so a Linux host without Arial (CI, the shipped image)
        // falls back (CORE-247).
        for (id, candidates) in BUNDLED_PATH_CANDIDATES.iter().enumerate() {
            // CORE-528: third candidate is the Debian/Ubuntu fonts-liberation path.
            assert_eq!(candidates.len(), 3, "face {id} candidate count");
            assert!(
                candidates[2].starts_with("/usr/share/fonts/truetype/liberation/LiberationSans-"),
                "face {id}: fonts-liberation must be the third candidate"
            );
            assert!(
                candidates[0].starts_with("/System/Library/Fonts/Supplemental/Arial"),
                "face {id}: macOS Arial must be the first candidate"
            );
            assert!(
                candidates[1].starts_with("/usr/share/fonts/typeanvil/LiberationSans-"),
                "face {id}: Liberation Sans must be the second candidate"
            );
            // The resolved path is one of the candidates and exists on this
            // host, whichever font set the host has.
            let path = face_path(FaceId(id as u32));
            assert!(
                candidates.contains(&path),
                "face {id} resolved to {path}, not a candidate"
            );
            assert!(
                std::path::Path::new(path).is_file(),
                "face {id} resolved path {path} is not a file"
            );
        }
    }

    // CORE-247: the bundled fallback must never resolve to empty font bytes.
    // Every bundled face resolves to a readable path (macOS Arial, or the
    // container image's Liberation set on Linux) and its bytes parse as a
    // font. This is the invariant the container image relies on.
    #[test]
    fn bundled_faces_resolve_to_readable_bytes() {
        for id in 0..4 {
            let face = FaceId(id);
            let path = face_path(face);
            let bytes = std::fs::read(path)
                .unwrap_or_else(|e| panic!("bundled face {id} unreadable at {path}: {e}"));
            assert!(
                !bytes.is_empty(),
                "bundled face {id} resolved to empty bytes at {path}"
            );
            assert!(
                read_fonts::FontRef::new(&bytes).is_ok(),
                "bundled face {id} bytes at {path} do not parse as a font"
            );
        }
    }

    // System-face resolution depends on the host's font database (fontdb
    // scans /System/Library/Fonts on macOS). These tests assert macOS
    // system faces and are meaningless elsewhere (e.g. the Linux dev
    // container, CORE-168).
    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_georgia_picks_system_face() {
        let specs = vec![FamilySpec::Name("Georgia".into()), FamilySpec::Serif];
        let r = resolve_font(&specs, 400.0, FontStyle::Normal);
        assert_eq!(face_postscript_name(r.primary), "Georgia");
        // the serif slot resolves through the generic table (CORE-361)
        assert!(!r.fallbacks.is_empty());
        assert_eq!(face_postscript_name(r.fallbacks[0]), "TimesNewRomanPSMT");
    }

    #[cfg(target_os = "macos")]
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

    /// The PostScript name a generic resolves to, or None for the bundled set.
    fn generic_ps(spec: FamilySpec, weight: f32, style: FontStyle) -> Option<String> {
        let r = resolve_font(&[spec], weight, style);
        if BUNDLED_FACES.contains(&r.primary) {
            None
        } else {
            Some(face_postscript_name(r.primary).to_string())
        }
    }

    /// Whether any installed face belongs to `family` (lowercase).
    fn installed(family: &str) -> bool {
        FONTDB
            .faces()
            .any(|f| f.families.iter().any(|(n, _)| n.to_ascii_lowercase() == family))
    }

    // CORE-361: generics follow the engine-owned table (spec Behavior 5).
    // The expected face is derived from what is installed, so the test holds
    // on macOS (Times New Roman / Courier New), in the dev container
    // (Liberation Serif / Mono) and on a bare host (bundled fallback).
    #[test]
    fn generics_follow_the_generic_table() {
        let expect = |table: &[&str], ps: &[&str]| -> Option<String> {
            table
                .iter()
                .position(|f| installed(f))
                .map(|i| ps[i].to_string())
        };
        assert_eq!(
            generic_ps(FamilySpec::Serif, 400.0, FontStyle::Normal),
            expect(GENERIC_SERIF, &["TimesNewRomanPSMT", "LiberationSerif"])
        );
        assert_eq!(
            generic_ps(FamilySpec::Monospace, 400.0, FontStyle::Normal),
            expect(GENERIC_MONOSPACE, &["CourierNewPSMT", "LiberationMono"])
        );
        assert_eq!(
            generic_ps(FamilySpec::Serif, 700.0, FontStyle::Italic),
            expect(
                GENERIC_SERIF,
                &["TimesNewRomanPS-BoldItalicMT", "LiberationSerif-BoldItalic"]
            )
        );
    }

    // sans-serif stays the bundled set with its fixed ids, so sans documents
    // render byte-identically to pre-CORE-361 output.
    #[test]
    fn sans_serif_is_the_bundled_set() {
        let r = resolve_font(&[FamilySpec::SansSerif], 400.0, FontStyle::Normal);
        assert_eq!(r.primary, FACE_REGULAR);
        let r = resolve_font(&[FamilySpec::SansSerif], 700.0, FontStyle::Italic);
        assert_eq!(r.primary, FACE_BOLD_ITALIC);
    }

    // A generic whose table has no installed family falls back to the
    // bundled set (never an empty candidate list).
    #[test]
    fn generic_table_without_installed_family_is_bundled() {
        assert_eq!(
            generic_table_candidates(&["no such family qz"]),
            BUNDLED_FACES.to_vec()
        );
    }

    // css-fonts-4 §5.2: normal width is matched before weight/style, so a
    // family's condensed cut never wins over its normal cut (CORE-361).
    #[cfg(target_os = "macos")]
    #[test]
    fn normal_width_face_beats_condensed_cut() {
        let r = resolve_font(&[FamilySpec::Name("Papyrus".into())], 400.0, FontStyle::Normal);
        assert_eq!(face_postscript_name(r.primary), "Papyrus");
        let r = resolve_font(
            &[FamilySpec::Name("Helvetica Neue".into())],
            700.0,
            FontStyle::Normal,
        );
        assert_eq!(face_postscript_name(r.primary), "HelveticaNeue-Bold");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_generics_are_distinct_faces() {
        let ps = |s| generic_ps(s, 400.0, FontStyle::Normal);
        assert_eq!(ps(FamilySpec::Serif).as_deref(), Some("TimesNewRomanPSMT"));
        assert_eq!(ps(FamilySpec::Monospace).as_deref(), Some("CourierNewPSMT"));
        assert_eq!(ps(FamilySpec::Cursive).as_deref(), Some("ComicSansMS"));
        assert_eq!(ps(FamilySpec::Fantasy).as_deref(), Some("Papyrus"));
    }
}
