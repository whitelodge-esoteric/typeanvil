// SPDX-License-Identifier: AGPL-3.0-only

//! Typeanvil CLI — walking skeleton.
//!
//! Contract (driven by the Python harness, `harness/engine.py`):
//!
//! ```text
//! typeanvil render <input.html> \
//!     --page-width 5in --page-height 3in \
//!     --margin-top 0.5in --margin-right 0.5in \
//!     --margin-bottom 0.5in --margin-left 0.5in \
//!     --base-url http://127.0.0.1:PORT/ \
//!     --title "Q3 Report" --author "A. Author" \
//!     [--diagnostics <json|text>] \
//!     -o <output.pdf>
//!
//! `--diagnostics json` prints one schema-versioned JSON document (see
//! `docs/specifications/diagnostics.spec.md`, CORE-112) to stdout after the
//! PDF is written; `--diagnostics text` prints one line per event to stderr.
//! Without the flag, output is byte-identical to a plain render.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::{dom, layout, pdf};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("typeanvil: error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args
        .next()
        .ok_or_else(|| anyhow!("missing subcommand (expected `render`)"))?;
    match first.as_str() {
        // First-argument-only flags (CORE-134): wrapper scripts (npm shim,
        // Homebrew formula, release-CI tag check) rely on the exit codes.
        "--version" | "-V" => {
            println!("typeanvil {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }
        "render" => render(args.collect()),
        other => bail!("unknown subcommand `{other}` (expected `render`)"),
    }
}

const HELP: &str = "\
typeanvil — AI-first HTML to PDF typesetting engine

Usage: typeanvil render <input.html> [flags] -o <output.pdf>
       typeanvil --version
       typeanvil --help

Subcommands:
  render        Render an HTML document to a paged PDF

Flags (render):
  -o, --output <path>       Output PDF path (required)
      --page-width <len>    Page width (in|pt|px|cm|mm) (required)
      --page-height <len>   Page height (required)
      --margin-<side> <len> Top/right/bottom/left page margins
      --base-url <url>      Base URL for resolving document URLs
      --title <text>        PDF metadata title
      --author <text>       PDF metadata author
      --tagged              Emit a logical structure tree (tagged PDF)
      --ua                  Tagged PDF + PDF/UA-1 validation
      --diagnostics <mode>  css diagnostics: json (stdout) | text (stderr)
  -h, --help                Print this help
  -V, --version             Print version

Flags are honored as the first argument only for --help and --version.
";

/// Parsed CLI options for `render`.
struct RenderArgs {
    input: PathBuf,
    output: PathBuf,
    page_width: Scalar,
    page_height: Scalar,
    margin_top: Scalar,
    margin_right: Scalar,
    margin_bottom: Scalar,
    margin_left: Scalar,
    #[allow(dead_code)]
    base_url: String,
    title: Option<String>,
    author: Option<String>,
    /// Emit a logical structure tree (CORE-111).
    tagged: bool,
    /// Additionally run krilla's PDF/UA-1 validator (implies tagged).
    ua: bool,
    /// CORE-112: emit machine-readable diagnostics (`json` or `text`).
    diagnostics: Option<DiagnosticsMode>,
}

/// Output format for `--diagnostics`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticsMode {
    Json,
    Text,
}

fn render(args: Vec<String>) -> Result<()> {
    let opts = parse_render_args(args)?;

    let html = std::fs::read_to_string(&opts.input)
        .with_context(|| format!("reading input {}", opts.input.display()))?;

    let dom = Dom::parse(&html).context("parsing HTML")?;
    let stylesheet = extract_stylesheet(&dom, &opts.input);

    let geometry = PageGeometry {
        width: opts.page_width,
        height: opts.page_height,
        margin_top: opts.margin_top,
        margin_right: opts.margin_right,
        margin_bottom: opts.margin_bottom,
        margin_left: opts.margin_left,
    };

    // `--base-url` (CORE-106 contract): relative image paths resolve against
    // it; absent, they resolve against the process working directory.
    let base_url = if opts.base_url.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(opts.base_url.as_str()))
    };
    let laid_out = layout::layout_with_images(&dom, &stylesheet, geometry, base_url.as_deref());
    let meta = typeanvil::metadata::extract_metadata(&dom, opts.title, opts.author);

    let bytes = pdf::render_with_options(
        &laid_out,
        Some(&dom),
        &meta,
        opts.tagged,
        opts.ua,
    )
    .context("rendering PDF")?;

    std::fs::write(&opts.output, &bytes)
        .with_context(|| format!("writing output {}", opts.output.display()))?;

    // Diagnostics (CORE-112): analyze the stylesheet only when asked. The
    // render above is untouched either way — byte-stable PDFs, zero cost.
    if let Some(mode) = opts.diagnostics {
        let mut events = typeanvil::diagnostics::analyze(stylesheet.source());
        // CORE-128: broken bookmark anchors collected during layout join the
        // stream as trailing warning events (no source position — 0,0).
        for msg in typeanvil::diagnostics::take_broken_anchors() {
            events.push(typeanvil::diagnostics::Diagnostic {
                code: "bookmark-anchor-unresolved".to_string(),
                severity: "warning".to_string(),
                message: msg,
                line: 0,
                column: 0,
            });
        }
        match mode {
            DiagnosticsMode::Json => print!("{}", typeanvil::diagnostics::to_json(&events)),
            DiagnosticsMode::Text => {
                for d in &events {
                    eprintln!(
                        "{}:{} {} [{}]: {}",
                        d.line, d.column, d.severity, d.code, d.message
                    );
                }
            }
        }
    }
    Ok(())
}

/// Collect and parse `<style>` element text and `<link rel="stylesheet">`
/// stylesheets into a single stylesheet, in document order (css-cascade-5:
/// link sheets interleave with style elements at their source position).
///
/// Link hrefs resolve against the input file's directory (relative) or, for
/// root-absolute paths (`/...`), by walking UP from the input directory until
/// the joined path exists (the WPT checkout root — the harness serves
/// `/fonts/...` from there). `http(s)://` hrefs are skipped (offline
/// contract). Font `url()` sources inside the collected CSS are rewritten to
/// resolved absolute filesystem paths before parse, so `@font-face`
/// registration works without `--base-url`.
fn extract_stylesheet(dom: &Dom, input: &Path) -> Stylesheet {
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
fn resolve_asset_path(input_dir: &Path, href: &str) -> Option<PathBuf> {
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
fn resolve_font_urls(css: &str, input_dir: &Path) -> String {
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

fn parse_render_args(args: Vec<String>) -> Result<RenderArgs> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut page_width: Option<Scalar> = None;
    let mut page_height: Option<Scalar> = None;
    let mut margin_top: Option<Scalar> = None;
    let mut margin_right: Option<Scalar> = None;
    let mut margin_bottom: Option<Scalar> = None;
    let mut margin_left: Option<Scalar> = None;
    let mut base_url = String::new();
    let mut title: Option<String> = None;
    let mut author: Option<String> = None;
    let mut tagged = false;
    let mut ua = false;
    let mut diagnostics: Option<DiagnosticsMode> = None;

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output = Some(PathBuf::from(next_value(&mut it, &arg)?));
            }
            "--page-width" => page_width = Some(parse_length(&next_value(&mut it, &arg)?)?),
            "--page-height" => page_height = Some(parse_length(&next_value(&mut it, &arg)?)?),
            "--margin-top" => margin_top = Some(parse_length(&next_value(&mut it, &arg)?)?),
            "--margin-right" => margin_right = Some(parse_length(&next_value(&mut it, &arg)?)?),
            "--margin-bottom" => margin_bottom = Some(parse_length(&next_value(&mut it, &arg)?)?),
            "--margin-left" => margin_left = Some(parse_length(&next_value(&mut it, &arg)?)?),
            "--base-url" => base_url = next_value(&mut it, &arg)?,
            "--title" => title = Some(next_value(&mut it, &arg)?),
            "--author" => author = Some(next_value(&mut it, &arg)?),
            "--tagged" => tagged = true,
            "--ua" => {
                tagged = true;
                ua = true;
            }
            "--diagnostics" => {
                let value = next_value(&mut it, &arg)?;
                diagnostics = Some(match value.as_str() {
                    "json" => DiagnosticsMode::Json,
                    "text" => DiagnosticsMode::Text,
                    other => bail!("invalid --diagnostics mode `{other}` (expected json|text)"),
                });
            }
            other if other.starts_with('-') => bail!("unknown flag `{other}`"),
            _ => {
                if input.is_none() {
                    input = Some(PathBuf::from(arg));
                } else {
                    bail!("unexpected positional argument `{arg}`");
                }
            }
        }
    }

    Ok(RenderArgs {
        input: input.ok_or_else(|| anyhow!("missing input HTML path"))?,
        output: output.ok_or_else(|| anyhow!("missing -o output path"))?,
        page_width: page_width.ok_or_else(|| anyhow!("missing --page-width"))?,
        page_height: page_height.ok_or_else(|| anyhow!("missing --page-height"))?,
        margin_top: margin_top.unwrap_or(Scalar::ZERO),
        margin_right: margin_right.unwrap_or(Scalar::ZERO),
        margin_bottom: margin_bottom.unwrap_or(Scalar::ZERO),
        margin_left: margin_left.unwrap_or(Scalar::ZERO),
        base_url,
        title,
        author,
        tagged,
        ua,
        diagnostics,
    })
}

fn next_value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    it.next()
        .ok_or_else(|| anyhow!("flag `{flag}` needs a value"))
}

/// Parse a CSS absolute length (`in`, `pt`, `px`, `cm`, `mm`) into points.
fn parse_length(s: &str) -> Result<Scalar> {
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
        let ss = extract_stylesheet(&dom, &dir.join("page.html"));
        let src = ss.source();
        let a = src.find("a { color: blue }").unwrap();
        let p = src.find("p { color: red }").unwrap();
        let b = src.find("b { color: green }").unwrap();
        assert!(a < p && p < b, "sheets must interleave in document order: {src}");
        // The non-stylesheet link contributed nothing.
        assert!(!src.contains("mailto"), "author link leaked into CSS: {src}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
