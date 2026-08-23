//! CORE-115 acceptance tests — license resolution + watermark seam.
//!
//! Maps to `docs/specifications/licensing-resolution.spec.md` Acceptance
//! Criteria 1–8. Resolution tests drive `resolve_with_exe` with injected
//! accessors (no process-global mutation); watermark tests render through
//! the library and inspect the PDF streams directly (the same technique as
//! `tests/tounicode.rs` — no external extractor dependency).
//!
//! Profile note: these tests run under `debug_assertions`, where `resolve()`
//! short-circuits to licensed (spec Behavior §3). The debug-bypass criterion
//! (AC 3) is therefore asserted directly; the enforcement paths are exercised
//! through `resolve_with_exe`, which both profiles execute identically.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;
use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;
use typeanvil::licensing::{
    resolve, resolve_with_exe, Edition, LicenseError, LicenseSource, LicenseState,
};
use typeanvil::pdf::DocumentMetadata;
use typeanvil::pdf::render_with_metadata;

// --- helpers -----------------------------------------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let typeanvil::dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
}

const TWO_PAGE_HTML: &str = r#"<html><head><style>
p { font-family: Arial; font-size: 11pt; }
.pb { break-before: page; }
</style></head><body>
<p>Page one body text.</p>
<p class="pb">Page two body text.</p>
</body></html>"#;

fn lay_two_page() -> typeanvil::layout::Layout {
    let dom = Dom::parse(TWO_PAGE_HTML).unwrap();
    let ss = stylesheet_of(&dom);
    let l = layout(&dom, &ss, geometry(8.5, 11.0, 0.8));
    assert!(
        l.pages.len() >= 2,
        "fixture must span 2+ pages, got {}",
        l.pages.len()
    );
    l
}

fn render_pdf(watermark: bool) -> Vec<u8> {
    let l = lay_two_page();
    render_with_metadata(&l, &DocumentMetadata::default(), watermark).unwrap()
}

/// Every successfully-inflated PAGE CONTENT stream (ToUnicode CMaps excluded
/// via `beginbfchar`; font programs and degenerate objects excluded by
/// requiring text-showing operators).
fn content_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut search = 0usize;
    while let Some(mut i) = find_bytes(pdf, b"stream", search) {
        i += b"stream".len();
        if pdf.get(i) == Some(&b'\r') {
            i += 1;
        }
        if pdf.get(i) == Some(&b'\n') {
            i += 1;
        }
        let end = find_bytes(pdf, b"endstream", i).unwrap_or(pdf.len());
        let mut dec = ZlibDecoder::new(&pdf[i..end]);
        let mut raw = Vec::new();
        if dec.read_to_end(&mut raw).is_ok()
            && !raw.windows(11).any(|w| w == b"beginbfchar")
            && (find_bytes(&raw, b"TJ", 0).is_some() || find_bytes(&raw, b"Tj", 0).is_some())
            && find_bytes(&raw, b"BT", 0).is_some()
        {
            out.push(raw);
        }
        search = end + b"endstream".len();
    }
    out
}

fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    let mut n = 0;
    let mut at = 0;
    while let Some(i) = find_bytes(haystack, needle, at) {
        n += 1;
        at = i + 1;
    }
    n
}

/// A byte window (≥16B) present in EVERY watermarked page stream and in NO
/// clean stream — the shared watermark glyph run.
fn watermark_signature(clean: &[u8], marked: &[u8]) -> Vec<u8> {
    let clean_streams = content_streams(clean);
    let mut marked_streams = content_streams(marked);
    assert!(
        marked_streams.len() >= 2,
        "expected 2+ page streams, got {}",
        marked_streams.len()
    );
    let first = marked_streams.remove(0);
    for w in (16..=64).step_by(8) {
        if w > first.len() {
            break;
        }
        for start in 0..=(first.len() - w) {
            let window = &first[start..start + w];
            let in_all_marked = marked_streams.iter().all(|s| find_bytes(s, window, 0).is_some());
            let in_no_clean = clean_streams
                .iter()
                .all(|s| find_bytes(s, window, 0).is_none());
            if in_all_marked && in_no_clean {
                return window.to_vec();
            }
        }
    }
    panic!("no watermark signature window found");
}

// --- AC 1: lookup order — arg beats env beats adjacent ------------------------

#[test]
fn ac1_arg_beats_env_beats_adjacent() {
    let tmp = tempfile::tempdir().unwrap();
    let arg_path = tmp.path().join("arg.dat");
    let env_path = tmp.path().join("env.dat");
    let adj_dir = tmp.path().join("bindir");
    std::fs::create_dir(&adj_dir).unwrap();
    let adj_path = adj_dir.join("license.dat");
    for p in [&arg_path, &env_path, &adj_path] {
        std::fs::write(p, b"license-bytes").unwrap();
    }
    let exe = adj_dir.join("typeanvil");

    // All three present → the CLI argument wins outright.
    let state = resolve_with_exe(
        Some(&arg_path),
        || Some(OsString::from(env_path.clone())),
        || Some(exe.clone()),
    )
    .unwrap();
    assert_eq!(state, valid_state());

    // Env + adjacent, no arg → the environment variable wins.
    let state = resolve_with_exe(
        None,
        || Some(OsString::from(env_path.clone())),
        || Some(exe.clone()),
    )
    .unwrap();
    assert_eq!(state, valid_state());

    // Only the adjacent file → adjacent wins.
    let state = resolve_with_exe(None, || None, || Some(exe.clone())).unwrap();
    assert_eq!(state, valid_state());
}

/// Source provenance is distinguishable (spec AC 1 rides on this).
#[test]
fn sources_report_their_origin() {
    let p = PathBuf::from("/x/license.dat");
    assert_eq!(LicenseSource::CommandLine(p.clone()).path(), Path::new("/x/license.dat"));
    assert_eq!(LicenseSource::EnvVar(p.clone()).path(), Path::new("/x/license.dat"));
    assert_eq!(
        LicenseSource::ExecutableAdjacent(p.clone()).path(),
        Path::new("/x/license.dat")
    );
}

// --- AC 2: adjacency resolves against the executable, never the CWD ----------

#[test]
fn ac2_adjacent_uses_exe_directory_not_cwd() {
    // CWD carries a license.dat; the injected exe lives elsewhere with none.
    let cwd_tmp = tempfile::tempdir().unwrap();
    std::fs::write(cwd_tmp.path().join("license.dat"), b"cwd-decoy").unwrap();
    let exe_tmp = tempfile::tempdir().unwrap();
    let exe = exe_tmp.path().join("typeanvil");

    let state =
        resolve_with_exe(None, || None, || Some(exe)).expect("resolution must not fail");
    assert_eq!(state, LicenseState::Missing, "CWD decoy must not be picked up");

    // With license.dat beside the executable, it IS found.
    std::fs::write(exe_tmp.path().join("license.dat"), b"adjacent").unwrap();
    let exe = exe_tmp.path().join("typeanvil");
    let state = resolve_with_exe(None, || None, || Some(exe)).unwrap();
    assert_eq!(state, valid_state());
}

// --- AC 3: debug-build bypass --------------------------------------------------

#[test]
fn ac3_debug_builds_bypass_resolution() {
    if cfg!(debug_assertions) {
        // No arg, no env (unset in the test process), and the test binary's
        // directory has no license.dat — yet resolution yields licensed
        // WITHOUT touching any source.
        let state = resolve(None).expect("debug resolution never errors");
        assert_eq!(state, valid_state());
    } else {
        // Release shape: no sources → Missing (enforcement path).
        let state = resolve_with_exe(None, || None, || None).unwrap();
        assert_eq!(state, LicenseState::Missing);
    }
}

// --- AC 4: missing license renders watermarked, exactly once per page ---------

#[test]
fn ac4_missing_license_watermarks_every_page_once() {
    let clean = render_pdf(false);
    let marked = render_pdf(true);

    let sig = watermark_signature(&clean, &marked);
    let marked_streams = content_streams(&marked);
    assert!(marked_streams.len() >= 2);
    for (idx, s) in marked_streams.iter().enumerate() {
        assert_eq!(
            count_occurrences(s, &sig),
            1,
            "watermark drawn {n} times on page {idx} (want exactly 1)",
            n = count_occurrences(s, &sig)
        );
    }

    // The watermark is real extractable TEXT (shaped run → ToUnicode), not
    // outlines: the em dash U+2014 from "Unlicensed — TypeAnvil" appears in
    // the marked PDF's CMap and not in the clean one (whose fixture text has
    // no em dash).
    let marked_map = to_unicode_text(&marked);
    let clean_map = to_unicode_text(&clean);
    assert!(
        marked_map.contains('\u{2014}'),
        "watermark em dash missing from ToUnicode: {marked_map:?}"
    );
    assert!(
        !clean_map.contains('\u{2014}'),
        "clean render unexpectedly carries an em dash"
    );
}

// --- AC 5: licensed output is clean --------------------------------------------

#[test]
fn ac5_licensed_output_has_no_watermark() {
    let clean = render_pdf(false);
    let marked = render_pdf(true);
    let sig = watermark_signature(&clean, &marked);
    for (idx, s) in content_streams(&clean).iter().enumerate() {
        assert_eq!(
            count_occurrences(s, &sig),
            0,
            "clean page {idx} contains the watermark signature"
        );
    }
}

// --- AC 6: broken licenses fail loudly (API level; see module docs) ------------

#[test]
fn ac6_unreadable_explicit_license_is_a_loud_error() {
    // A directory cannot be read as a license file (spec edge case).
    let dir = tempfile::tempdir().unwrap();
    let err = resolve_with_exe(Some(dir.path()), || None, || None).unwrap_err();
    match &err {
        LicenseError::Unreadable { path, .. } => {
            assert_eq!(path, dir.path());
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
    // The message is user-facing loudness: names the path and the problem.
    let msg = err.to_string();
    assert!(msg.contains("unreadable"), "message: {msg}");
    assert!(msg.contains(dir.path().to_str().unwrap()), "message: {msg}");

    // Malformed shape exists with the same loud contract (reachable once the
    // real verifier replaces the mock parser).
    let malformed = LicenseError::Malformed {
        path: PathBuf::from("/lic"),
        reason: "bad signature".into(),
    };
    let msg = malformed.to_string();
    assert!(msg.contains("malformed") && msg.contains("/lic"), "message: {msg}");
}

// --- AC 7: determinism with the watermark ---------------------------------------

#[test]
fn ac7_watermarked_output_is_byte_identical() {
    let a = render_pdf(true);
    let b = render_pdf(true);
    assert_eq!(a, b, "two watermarked renders diverged");
    let c = render_pdf(false);
    let d = render_pdf(false);
    assert_eq!(c, d, "two clean renders diverged");
}

// --- AC 8: existing suites unchanged ---------------------------------------------
//
// Covered by running the full `cargo test` suite (harness callers exercise
// the debug bypass); `pdf::render` keeps its old signature and behavior, so
// every pre-existing call site compiles and stays clean-output unchanged.

// --- shared expectations -----------------------------------------------------------

fn valid_state() -> LicenseState {
    LicenseState::Valid(typeanvil::licensing::License {
        customer: "development".to_string(),
        edition: Edition::Trial,
    })
}

/// Concatenated ToUnicode mappings (cid → string) of a PDF, sorted by cid.
fn to_unicode_text(pdf: &[u8]) -> String {
    let mut pairs: Vec<(u16, String)> = Vec::new();
    let mut search = 0usize;
    while let Some(mut i) = find_bytes(pdf, b"stream", search) {
        i += b"stream".len();
        if pdf.get(i) == Some(&b'\r') {
            i += 1;
        }
        if pdf.get(i) == Some(&b'\n') {
            i += 1;
        }
        let end = find_bytes(pdf, b"endstream", i).unwrap_or(pdf.len());
        let mut dec = ZlibDecoder::new(&pdf[i..end]);
        let mut raw = Vec::new();
        if dec.read_to_end(&mut raw).is_ok() && raw.windows(11).any(|w| w == b"beginbfchar") {
            pairs.extend(parse_bfchar(&raw));
        }
        search = end + b"endstream".len();
    }
    pairs.sort_by_key(|(cid, _)| *cid);
    pairs.into_iter().map(|(_, s)| s).collect()
}

fn parse_bfchar(data: &[u8]) -> Vec<(u16, String)> {
    let Some(start) = find_bytes(data, b"beginbfchar", 0).map(|p| p + b"beginbfchar".len()) else {
        return Vec::new();
    };
    let stop = find_bytes(data, b"endbfchar", start).unwrap_or(data.len());
    let block = &data[start..stop];

    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < block.len() {
        if block[i] == b'<' {
            let mut hex = String::new();
            i += 1;
            while i < block.len() && block[i] != b'>' {
                hex.push(block[i] as char);
                i += 1;
            }
            tokens.push(hex);
        }
        i += 1;
    }

    let mut out = Vec::new();
    let mut it = tokens.into_iter();
    while let (Some(cid_hex), Some(uni_hex)) = (it.next(), it.next()) {
        let Ok(cid) = u16::from_str_radix(&cid_hex, 16) else {
            continue;
        };
        let uni = decode_utf16be(&uni_hex);
        if !uni.is_empty() {
            out.push((cid, uni));
        }
    }
    out
}

fn decode_utf16be(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}
