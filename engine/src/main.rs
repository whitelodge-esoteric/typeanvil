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
//!     -o <output.pdf>
//! ```

use std::path::PathBuf;
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
    let subcommand = args
        .next()
        .ok_or_else(|| anyhow!("missing subcommand (expected `render`)"))?;
    match subcommand.as_str() {
        "render" => render(args.collect()),
        other => bail!("unknown subcommand `{other}` (expected `render`)"),
    }
}

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
}

fn render(args: Vec<String>) -> Result<()> {
    let opts = parse_render_args(args)?;

    let html = std::fs::read_to_string(&opts.input)
        .with_context(|| format!("reading input {}", opts.input.display()))?;

    let dom = Dom::parse(&html).context("parsing HTML")?;
    let stylesheet = extract_stylesheet(&dom);

    let geometry = PageGeometry {
        width: opts.page_width,
        height: opts.page_height,
        margin_top: opts.margin_top,
        margin_right: opts.margin_right,
        margin_bottom: opts.margin_bottom,
        margin_left: opts.margin_left,
    };

    let laid_out = layout::layout(&dom, &stylesheet, geometry);
    let meta = typeanvil::metadata::extract_metadata(&dom, opts.title, opts.author);
    let bytes = pdf::render_with_metadata(&laid_out, &meta).context("rendering PDF")?;

    std::fs::write(&opts.output, &bytes)
        .with_context(|| format!("writing output {}", opts.output.display()))?;
    Ok(())
}

/// Collect and parse all `<style>` element text into a single stylesheet.
fn extract_stylesheet(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
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
