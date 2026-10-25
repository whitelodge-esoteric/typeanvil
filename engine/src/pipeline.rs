// SPDX-License-Identifier: AGPL-3.0-only

//! The document render pipeline shared by the CLI `render` command and the
//! MCP / JSON request tools (CORE-575).
//!
//! Both front ends call [`run`], so a `render` through MCP makes the same PDF
//! bytes as the CLI for the same input and options (mcp-tool-contract
//! Behavior 4). The steps and their order are the CLI's, unchanged:
//! read → (Markdown → HTML) → parse → inline iframes → collect stylesheets →
//! layout → metadata → PDF.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::css::Stylesheet;
use crate::diagnostics::Diagnostic;
use crate::dom::{self, Dom};
use crate::geom::{PageGeometry, Scalar};
use crate::layout::Layout;

/// Everything one render needs. `input` is the document's file path (real
/// or nominal): relative `<link>`, `<iframe>` and font references resolve
/// against its directory.
pub struct RenderInput {
    pub source: String,
    pub input: PathBuf,
    pub markdown: bool,
    /// Author stylesheets. Markdown: appended after the default sheet (the
    /// CLI `--css` files). HTML: appended after the document's own sheets.
    pub extra_css: Vec<String>,
    /// Markdown only: include the embedded default stylesheet.
    pub default_css: bool,
    pub geometry: PageGeometry,
    /// Image base directory (the CLI `--base-url`); `None` = working dir.
    pub base_url: Option<PathBuf>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub tagged: bool,
    pub ua: bool,
    /// Called after HTML parse, before stylesheet collection. The MCP tools
    /// use it to point `assets` references at their materialized files.
    pub rewrite: Option<Box<dyn Fn(&mut Dom)>>,
}

/// The finished render: the PDF plus the intermediate state the report,
/// inspect and explain tools read.
pub struct Rendered {
    pub dom: Dom,
    pub stylesheet: Stylesheet,
    pub layout: Layout,
    pub pdf: Vec<u8>,
    /// Markdown converter events (empty for HTML input).
    pub md_events: Vec<Diagnostic>,
}

/// Parse and lay out the document, then write the PDF.
pub fn run(input: RenderInput) -> Result<Rendered> {
    let mut md_events = Vec::new();
    let (html, html_extra) = if input.markdown {
        let doc = crate::markdown::convert(&input.source, &input.extra_css, input.default_css)
            .context("converting Markdown")?;
        md_events = doc.diagnostics;
        (doc.html, Vec::new())
    } else {
        (input.source, input.extra_css)
    };

    let mut dom = Dom::parse(&html).context("parsing HTML")?;
    if let Some(rewrite) = &input.rewrite {
        rewrite(&mut dom);
    }
    inline_iframe_sources(&mut dom, &input.input);
    let stylesheet = extract_stylesheet(&dom, &input.input, &html_extra);

    let layout =
        crate::layout::layout_with_images(&dom, &stylesheet, input.geometry, input.base_url.as_deref());
    let meta = crate::metadata::extract_metadata(&dom, input.title, input.author);
    let pdf = crate::pdf::render_with_options(&layout, Some(&dom), &meta, input.tagged, input.ua)
        .context("rendering PDF")?;
    Ok(Rendered {
        dom,
        stylesheet,
        layout,
        pdf,
        md_events,
    })
}

/// True for the Markdown file extensions (`.md`, `.markdown`, `.mdown`).
pub fn is_markdown_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| ["md", "markdown", "mdown"].iter().any(|m| e.eq_ignore_ascii_case(m)))
        .unwrap_or(false)
}

/// CORE-482: load each `<iframe src>` file and store its text as the
/// element's `srcdoc` attribute, so the library lays the nested document out
/// without touching the filesystem. Unreadable or remote sources stay
/// as plain boxes.
pub fn inline_iframe_sources(dom: &mut Dom, input: &Path) {
    let input_dir = input.parent().unwrap_or(Path::new("."));
    for node in dom.nodes.iter_mut() {
        if let dom::NodeKind::Element(el) = &mut node.kind {
            if el.tag != "iframe" || el.attr("srcdoc").is_some() {
                continue;
            }
            let Some(src) = el.attr("src") else { continue };
            let Some(path) = resolve_asset_path(input_dir, src) else {
                continue;
            };
            if let Ok(text) = std::fs::read_to_string(path) {
                el.attrs.push(("srcdoc".to_string(), text));
            }
        }
    }
}

/// Collect and parse `<style>` element text and `<link rel="stylesheet">`
/// stylesheets into a single stylesheet, in document order (css-cascade-5:
/// link sheets interleave with style elements at their source position).
/// `extra` sheets follow the document's own, in order.
///
/// Link hrefs resolve against the input file's directory (relative) or, for
/// root-absolute paths (`/...`), by walking UP from the input directory until
/// the joined path exists (the WPT checkout root — the harness serves
/// `/fonts/...` from there). `http(s)://` hrefs are skipped (offline
/// contract). Font `url()` sources inside the collected CSS are rewritten to
/// resolved absolute filesystem paths before parse, so `@font-face`
/// registration works without `--base-url`.
pub fn extract_stylesheet(dom: &Dom, input: &Path, extra: &[String]) -> Stylesheet {
    let input_dir = input.parent().unwrap_or(Path::new("."));
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            } else if el.tag == "link" {
                if let Some(sheet) = read_link_stylesheet(el, input_dir) {
                    css.push_str(&sheet);
                    css.push('\n');
                }
            }
        }
    }
    for sheet in extra {
        css.push_str(sheet);
        css.push('\n');
    }
    let css = resolve_font_urls(&css, input_dir);
    Stylesheet::parse(&css)
}

/// Read the stylesheet a `<link rel="stylesheet" href="...">` points to.
/// Returns `None` when the element is not a stylesheet link, has no href, or
/// the file cannot be resolved/read (skipped deterministically, like broken
/// font sources). `rel` is matched case-insensitively on whitespace tokens;
/// the `type` attribute is intentionally not required.
fn read_link_stylesheet(el: &dom::Element, input_dir: &Path) -> Option<String> {
    let rel = el.attr("rel")?;
    if !rel
        .split_ascii_whitespace()
        .any(|t| t.eq_ignore_ascii_case("stylesheet"))
    {
        return None;
    }
    let href = el.attr("href")?.trim();
    let path = resolve_asset_path(input_dir, href)?;
    std::fs::read_to_string(path).ok()
}

/// Resolve a `<link href>` / font `url()` source to a filesystem path.
///
/// - Relative paths resolve against `input_dir` (the input file's directory).
/// - Root-absolute paths (`/...`) resolve by walking UP from `input_dir`,
///   joining the un-slashed path at each ancestor (the input dir itself
///   first) until it exists — this locates the WPT checkout root without
///   `--base-url`.
/// - `http(s)://` and other non-file schemes return `None` (offline contract).
pub fn resolve_asset_path(input_dir: &Path, href: &str) -> Option<PathBuf> {
    let href = href.trim();
    if href.is_empty()
        || href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("data:")
        || href.starts_with("blob:")
        || href.starts_with("about:")
        || href.starts_with("file:")
        || href.starts_with('#')
    {
        return None;
    }
    if let Some(rest) = href.strip_prefix('/') {
        let rel = Path::new(rest);
        let mut dir = Some(input_dir);
        while let Some(d) = dir {
            let candidate = d.join(rel);
            if candidate.exists() {
                return Some(candidate);
            }
            dir = d.parent();
        }
        None
    } else {
        let candidate = input_dir.join(href);
        candidate.exists().then_some(candidate)
    }
}

/// Rewrite `url(...)` font sources in the stylesheet to resolved absolute
/// filesystem paths, so `@font-face` registration (css.rs `read_font_file`
/// reads the path verbatim) works without `--base-url`. Only file-like paths
/// are rewritten; `data:`, `http(s):`, fragments and `var(...)` are left
/// untouched. `format(...)` hints and other trailing `src` tokens are
/// preserved (only the path inside `url(...)` is replaced).
pub fn resolve_font_urls(css: &str, input_dir: &Path) -> String {
    let mut out = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Match `url(` case-sensitively, mirroring the consumer in css.rs
        // `parse_one_font_face` (which strips a lowercase `url(` prefix).
        if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"url(" {
            let start = i + 4;
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            // The argument: a quoted string or an unquoted URL (no spaces).
            let (path_start, path_end, mut close) = if j < bytes.len()
                && (bytes[j] == b'\'' || bytes[j] == b'"')
            {
                let q = bytes[j];
                let mut k = j + 1;
                while k < bytes.len() && bytes[k] != q {
                    k += 1;
                }
                (j + 1, k, k + 1)
            } else {
                let mut k = j;
                while k < bytes.len()
                    && bytes[k] != b')'
                    && !bytes[k].is_ascii_whitespace()
                {
                    k += 1;
                }
                (j, k, k)
            };
            while close < bytes.len() && bytes[close].is_ascii_whitespace() {
                close += 1;
            }
            if close < bytes.len() && bytes[close] == b')' {
                let raw = &css[path_start..path_end];
                if let Some(abs) = resolve_asset_path(input_dir, raw) {
                    out.push_str("url('");
                    out.push_str(&abs.to_string_lossy());
                    out.push_str("')");
                    i = close + 1;
                    continue;
                }
            }
        }
        // Copy one UTF-8 code point (byte-index safe; `url(` is ASCII so it
        // can only begin at a char boundary).
        let ch = css[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Parse a CSS absolute length (`in`, `pt`, `px`, `cm`, `mm`) into points.
pub fn parse_length(s: &str) -> Result<Scalar> {
    let s = s.trim();
    let end = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_str, unit) = s.split_at(end);
    let num: f64 = num_str
        .trim()
        .parse()
        .with_context(|| format!("invalid length `{s}`"))?;
    let pt = match unit.trim().to_ascii_lowercase().as_str() {
        "in" => num * 72.0,
        "pt" | "" => num,
        "px" => num * 0.75,
        "cm" => num * 72.0 / 2.54,
        "mm" => num * 72.0 / 25.4,
        other => bail!("unsupported length unit `{other}` in `{s}`"),
    };
    Ok(Scalar(pt))
}

#[cfg(test)]
mod link_tests {
    use super::*;

    /// A unique scratch directory per test (tests run in parallel threads in
    /// one process; the `name` keeps each test's tree distinct).
    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "typeanvil-link-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn resolve_root_absolute_walks_up() {
        let dir = tmp_dir("walkup");
        let nested = dir.join("css").join("css-page");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        std::fs::write(dir.join("fonts").join("ahem.css"), "x").unwrap();

        // Root-absolute `/fonts/ahem.css` is found by walking up from the
        // input dir to the checkout root (`dir`).
        assert_eq!(
            resolve_asset_path(&nested, "/fonts/ahem.css"),
            Some(dir.join("fonts").join("ahem.css"))
        );

        // Relative resolves against the input dir itself.
        assert!(resolve_asset_path(&nested, "local.css").is_none());
        std::fs::write(nested.join("local.css"), "y").unwrap();
        assert_eq!(
            resolve_asset_path(&nested, "local.css"),
            Some(nested.join("local.css"))
        );

        // Non-file schemes and fragments never resolve.
        assert!(resolve_asset_path(&nested, "http://e/x.css").is_none());
        assert!(resolve_asset_path(&nested, "https://e/x.css").is_none());
        assert!(resolve_asset_path(&nested, "data:text/css,x").is_none());
        assert!(resolve_asset_path(&nested, "#frag").is_none());
        assert!(resolve_asset_path(&nested, "").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_font_urls_rewrites_file_paths_only() {
        let dir = tmp_dir("urls");
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        std::fs::write(dir.join("fonts").join("Ahem.ttf"), "x").unwrap();

        let css = "@font-face { font-family: A; src: url('/fonts/Ahem.ttf') format('truetype'); }\n\
                   .x { background: url(data:image/png;base64,AA==); list-style: url(http://e/x.png); }";
        let out = resolve_font_urls(css, &dir);

        let abs = dir.join("fonts").join("Ahem.ttf");
        assert!(
            out.contains(&format!("url('{}')", abs.to_string_lossy())),
            "root-absolute font url must be rewritten to an absolute path: {out}"
        );
        // The original leading-slash form is gone.
        assert!(!out.contains("'/fonts/"), "stale root-relative url kept: {out}");
        // Non-file sources are preserved verbatim.
        assert!(out.contains("data:image/png;base64,AA=="));
        assert!(out.contains("http://e/x.png"));
        assert!(out.contains("format('truetype')"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_stylesheet_interleaves_links_and_styles() {
        let dir = tmp_dir("extract");
        std::fs::write(dir.join("sheet.css"), "p { color: red }").unwrap();
        let html = "<!DOCTYPE html><html><head>\
            <style>a { color: blue }</style>\
            <link rel=\"STYLESHEET\" href=\"sheet.css\">\
            <link rel=\"author\" href=\"mailto:x@y\">\
            <style>b { color: green }</style>\
            </head><body>x</body></html>";
        let dom = Dom::parse(html).unwrap();
        let ss = extract_stylesheet(&dom, &dir.join("page.html"), &["i { color: navy }".to_string()]);
        let src = ss.source();
        let a = src.find("a { color: blue }").unwrap();
        let p = src.find("p { color: red }").unwrap();
        let b = src.find("b { color: green }").unwrap();
        let i = src.find("i { color: navy }").unwrap();
        assert!(a < p && p < b && b < i, "sheets must interleave in document order, extra last: {src}");
        // The non-stylesheet link contributed nothing.
        assert!(!src.contains("mailto"), "author link leaked into CSS: {src}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
