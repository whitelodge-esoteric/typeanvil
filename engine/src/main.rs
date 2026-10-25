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
//! ```
//!
//! `--diagnostics json` prints one schema-versioned JSON document (see
//! `docs/specifications/diagnostics.spec.md`, CORE-112) to stdout after the
//! PDF is written; `--diagnostics text` prints one line per event to stderr.
//! Without the flag, output is byte-identical to a plain render.
//!
//! `typeanvil mcp` serves the MCP tool contract over stdio and
//! `typeanvil call <tool>` runs one JSON request (CORE-575,
//! `docs/specifications/mcp-server.spec.md`).

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};

use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::pipeline::{self, parse_length, RenderInput};
use typeanvil::safety::{FetchPolicy, LimitError, Policy};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        // CORE-573: `--fail-on-assert` with failing assertions exits 3. The
        // PDF and diagnostics were already written.
        Err(e) if e.downcast_ref::<AssertionsFailed>().is_some() => {
            eprintln!("typeanvil: {e}");
            ExitCode::from(3)
        }
        // CORE-578: a resource limit stopped the render (exit 4, no PDF).
        // stderr carries the stable code: `typeanvil: error: [LIMIT_...] ...`.
        Err(e) if e.downcast_ref::<LimitError>().is_some() => {
            eprintln!("typeanvil: error: {e}");
            ExitCode::from(4)
        }
        // CORE-576: template / data / schema errors (exit 5, no PDF). One
        // stderr line per error: `typeanvil: error: [CODE] <where>: <msg>`.
        Err(e) if e.downcast_ref::<typeanvil::template::TemplateErrors>().is_some() => {
            let errs = e.downcast_ref::<typeanvil::template::TemplateErrors>().unwrap();
            for t in &errs.0 {
                eprintln!(
                    "typeanvil: error: [{}] {}: {}",
                    t.code,
                    typeanvil::template::location(t),
                    t.message
                );
            }
            ExitCode::from(5)
        }
        Err(e) => {
            eprintln!("typeanvil: error: {e:#}");
            // CORE-513 B14: usage errors for the Markdown flags exit 2.
            if e.downcast_ref::<UsageError>().is_some() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn run() -> Result<ExitCode> {
    let mut args = std::env::args().skip(1);
    let first = args
        .next()
        .ok_or_else(|| anyhow!("missing subcommand (expected `render`)"))?;
    match first.as_str() {
        // First-argument-only flags (CORE-134): wrapper scripts (npm shim,
        // Homebrew formula, release-CI tag check) rely on the exit codes.
        "--version" | "-V" => {
            println!("typeanvil {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        "--help" | "-h" => {
            print!("{HELP}");
            Ok(ExitCode::SUCCESS)
        }
        "render" => render(args.collect()).map(|()| ExitCode::SUCCESS),
        // CORE-572: capability manifest generated from the diagnostics tables.
        "capabilities" => {
            let rest: Vec<String> = args.collect();
            match rest.as_slice() {
                [flag] if flag == "--json" => {
                    print!("{}", typeanvil::capabilities::to_json());
                    Ok(ExitCode::SUCCESS)
                }
                _ => Err(UsageError("usage: typeanvil capabilities --json".to_string()).into()),
            }
        }
        // CORE-576: publish a template's data schema.
        "template-schema" => {
            let rest: Vec<String> = args.collect();
            let [tmpl] = rest.as_slice() else {
                return Err(
                    UsageError("usage: typeanvil template-schema <template>".to_string()).into()
                );
            };
            let path = PathBuf::from(tmpl);
            match typeanvil::template::load_schema(&path, None)? {
                Some(s) => {
                    println!("{}", serde_json::to_string_pretty(&s).expect("JSON serializes"));
                    Ok(ExitCode::SUCCESS)
                }
                None => bail!("no schema for {}", path.display()),
            }
        }
        // CORE-575: MCP server over stdio.
        "mcp" => {
            let rest: Vec<String> = args.collect();
            let roots = parse_root_args(&rest)?;
            let server = typeanvil::mcp::Server::new(roots);
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            server
                .serve(stdin.lock(), stdout.lock())
                .context("mcp stdio transport")?;
            Ok(ExitCode::SUCCESS)
        }
        // CORE-575: JSON request mode — one request on stdin, one result on
        // stdout. Exit 0 = success result, 1 = tool error result.
        "call" => {
            let rest: Vec<String> = args.collect();
            let (tool, roots) = match rest.split_first() {
                Some((tool, more)) if !tool.starts_with('-') => {
                    (tool.clone(), parse_root_args(more)?)
                }
                _ => {
                    return Err(UsageError(
                        "usage: typeanvil call <tool> [--root <dir>]... < request.json"
                            .to_string(),
                    )
                    .into())
                }
            };
            let mut body = String::new();
            std::io::stdin()
                .read_to_string(&mut body)
                .context("reading request from stdin")?;
            let server = typeanvil::mcp::Server::new(roots);
            let (value, is_error) = server.call_json(&tool, &body);
            println!("{}", serde_json::to_string(&value).expect("JSON value serializes"));
            Ok(if is_error { ExitCode::FAILURE } else { ExitCode::SUCCESS })
        }
        other => bail!(
            "unknown subcommand `{other}` (expected `render`, `capabilities`, `template-schema`, `mcp` or `call`)"
        ),
    }
}

/// `--root <dir>` (repeatable): restrict readable and writable paths.
fn parse_root_args(args: &[String]) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| UsageError("flag `--root` needs a value".to_string()))?;
                roots.push(PathBuf::from(v));
            }
            other => return Err(UsageError(format!("unknown argument `{other}`")).into()),
        }
    }
    Ok(roots)
}

/// A command-line usage error (exit code 2).
#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// CORE-573: one or more assertions failed under `--fail-on-assert` (exit 3).
#[derive(Debug)]
struct AssertionsFailed(usize);

impl std::fmt::Display for AssertionsFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} assertion(s) failed", self.0)
    }
}

impl std::error::Error for AssertionsFailed {}

const HELP: &str = "\
typeanvil — AI-first HTML and Markdown to PDF typesetting engine

Usage: typeanvil render <input.html|input.md> [flags] -o <output.pdf>
       typeanvil capabilities --json
       typeanvil template-schema <template>
       typeanvil mcp [--root <dir>]...
       typeanvil call <tool> [--root <dir>]... < request.json
       typeanvil --version
       typeanvil --help

Subcommands:
  render        Render an HTML or Markdown document to a paged PDF
                (.md, .markdown, .mdown select Markdown)
  capabilities  Print the CSS/input capability manifest as JSON (--json)
  template-schema  Print a template's data JSON Schema
  mcp           Serve the MCP tools (render, validate, inspect_page,
                capabilities, explain) over stdio
  call          Run one MCP tool: JSON request on stdin, JSON result on stdout

Flags (render):
  -o, --output <path>       Output PDF path (required)
      --page-width <len>    Page width (in|pt|px|cm|mm) (required)
      --page-height <len>   Page height (required)
      --margin-<side> <len> Top/right/bottom/left page margins
      --base-url <url>      Base URL for resolving document URLs
      --title <text>        PDF metadata title
      --author <text>       PDF metadata author
      --no-tagged           Omit the logical structure tree (tagged PDF is the default)
      --tagged              Emit a logical structure tree (default; accepted for compatibility)
      --ua                  Tagged PDF + PDF/UA-1 validation (overrides --no-tagged)
      --diagnostics <mode>  css diagnostics: json (stdout) | text (stderr)
      --from <format>       Input format: html | markdown (default: by extension)
      --css <file>          Markdown only: append an author stylesheet (repeatable)
      --no-default-css      Markdown only: omit the embedded default stylesheet
      --assert <file.json>  Evaluate layout assertions; verdicts join the
                            diagnostics JSON (implies --diagnostics json)
      --fail-on-assert      Exit 3 when any assertion fails (needs --assert)
      --data <file.json>    Treat the input as a MiniJinja template and render
                            it with this JSON data (escaped by default)
      --schema <file.json>  Validate --data against this JSON Schema (default:
                            the template's sibling <name>.schema.json, if any).
                            Template or data errors write no PDF and exit 5

Safety flags (render; untrusted input, default off):
      --fetch <mode>        none | local | allow-list | unrestricted (default).
                            local: reads only under --base-url (else the
                            input's directory); blocked reads are reported
      --fetch-hosts <list>  Comma-separated hosts for --fetch allow-list
      --max-pages <n>       Stop with LIMIT_PAGE_COUNT above n pages
      --timeout <seconds>   Stop with LIMIT_TIMEOUT after the time budget
      --max-input-bytes <n> Stop with LIMIT_INPUT_SIZE above n source bytes
      --max-image-pixels <n> Stop with LIMIT_IMAGE_SIZE for a larger image
                            A stopped render writes no PDF and exits 4

Flags (mcp, call):
      --root <dir>          Allow file access only under <dir> (repeatable)

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
    base_url: String,
    title: Option<String>,
    author: Option<String>,
    /// Emit a logical structure tree (CORE-111).
    tagged: bool,
    /// Additionally run krilla's PDF/UA-1 validator (implies tagged).
    ua: bool,
    /// CORE-112: emit machine-readable diagnostics (`json` or `text`).
    diagnostics: Option<DiagnosticsMode>,
    /// CORE-513: resolved input format (`--from` or the file extension).
    markdown: bool,
    /// CORE-513: `--css` files, in order (Markdown only).
    css_files: Vec<PathBuf>,
    /// CORE-513: `--no-default-css` (Markdown only).
    no_default_css: bool,
    /// CORE-573: parsed `--assert` file.
    assertions: Option<Vec<typeanvil::assertions::Assertion>>,
    /// CORE-573: `--fail-on-assert`.
    fail_on_assert: bool,
    /// CORE-578: `--fetch` (Unrestricted when absent).
    fetch: FetchPolicy,
    max_pages: Option<usize>,
    timeout: Option<std::time::Duration>,
    max_input_bytes: Option<usize>,
    max_image_pixels: Option<u64>,
    /// CORE-576: `--data` (the input is a template) and `--schema`.
    data: Option<PathBuf>,
    schema: Option<PathBuf>,
}

/// Output format for `--diagnostics`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticsMode {
    Json,
    Text,
}

fn render(args: Vec<String>) -> Result<()> {
    let opts = parse_render_args(args)?;

    let source = std::fs::read_to_string(&opts.input)
        .with_context(|| format!("reading input {}", opts.input.display()))?;
    // CORE-576: data template mode. Validate, then expand, before any render
    // work; an error writes no PDF (exit 5).
    let source = match &opts.data {
        None => source,
        Some(data_path) => match expand_template(&opts, &source, data_path) {
            Ok(html) => html,
            Err(errs) => {
                if opts.diagnostics == Some(DiagnosticsMode::Json) {
                    print!("{}", typeanvil::template::errors_json(&errs.0));
                }
                return Err(errs.into());
            }
        },
    };
    let mut extra_css = Vec::new();
    for f in &opts.css_files {
        extra_css.push(
            std::fs::read_to_string(f).with_context(|| format!("reading --css {}", f.display()))?,
        );
    }

    // `--base-url` (CORE-106 contract): relative image paths resolve against
    // it; absent, they resolve against the process working directory.
    let base_url = if opts.base_url.is_empty() {
        None
    } else {
        Some(PathBuf::from(opts.base_url.as_str()))
    };

    // CORE-578: the confinement root is `--base-url` when given, else the
    // input file's directory. Without any safety flag the policy is
    // unrestricted and the render is the legacy one.
    let root = base_url.clone().unwrap_or_else(|| {
        opts.input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let policy = Policy {
        fetch: opts.fetch.clone(),
        roots: Vec::new(),
        max_pages: opts.max_pages,
        deadline: opts.timeout.map(|t| std::time::Instant::now() + t),
        max_image_pixels: opts.max_image_pixels,
    }
    .with_root(&root);

    // CORE-575: the same pipeline the MCP `render` tool calls.
    let result = pipeline::run(RenderInput {
        source,
        input: opts.input.clone(),
        markdown: opts.markdown,
        extra_css,
        default_css: !opts.no_default_css,
        geometry: PageGeometry {
            width: opts.page_width,
            height: opts.page_height,
            margin_top: opts.margin_top,
            margin_right: opts.margin_right,
            margin_bottom: opts.margin_bottom,
            margin_left: opts.margin_left,
        },
        base_url,
        title: opts.title,
        author: opts.author,
        tagged: opts.tagged,
        ua: opts.ua,
        rewrite: None,
        policy,
        max_input_bytes: opts.max_input_bytes,
    });
    let done = match result {
        Ok(d) => d,
        Err(e) => {
            // Partial diagnostics: blocked fetches seen before the stop.
            if let Some(le) = e.downcast_ref::<LimitError>() {
                for msg in &le.blocked {
                    eprintln!("typeanvil: warning: [fetch-blocked] {msg}");
                }
            }
            return Err(e);
        }
    };
    // CORE-578: blocked fetches are never silent — one stderr line each,
    // and a `fetch-blocked` event in `--diagnostics` output.
    for msg in &done.blocked {
        eprintln!("typeanvil: warning: [fetch-blocked] {msg}");
    }

    std::fs::write(&opts.output, &done.pdf)
        .with_context(|| format!("writing output {}", opts.output.display()))?;

    // Diagnostics (CORE-112): analyze the stylesheet only when asked. The
    // render above is untouched either way — byte-stable PDFs, zero cost.
    if let Some(mode) = opts.diagnostics {
        let mut events = done.md_events;
        events.extend(typeanvil::diagnostics::analyze(done.stylesheet.source()));
        for msg in &done.blocked {
            events.push(typeanvil::diagnostics::Diagnostic {
                code: "fetch-blocked".to_string(),
                severity: "warning".to_string(),
                message: msg.clone(),
                line: 0,
                column: 0,
            });
        }
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
        // CORE-571: layout-outcome events + summary from the final layout.
        let report =
            typeanvil::render_report::analyze_layout(&done.dom, &done.stylesheet, &done.layout);
        // CORE-573: assertions read the same final layout and report.
        let outcomes = opts.assertions.as_ref().map(|list| {
            typeanvil::assertions::evaluate(&done.dom, &done.stylesheet, &done.layout, &report, list)
        });
        match mode {
            DiagnosticsMode::Json => {
                let block = outcomes.as_deref().map(typeanvil::assertions::to_json_block);
                print!(
                    "{}",
                    typeanvil::render_report::to_json_with(&events, &report, block.as_deref())
                )
            }
            DiagnosticsMode::Text => {
                for d in &events {
                    eprintln!(
                        "{}:{} {} [{}]: {}",
                        d.line, d.column, d.severity, d.code, d.message
                    );
                }
                for e in &report.events {
                    eprintln!("{}", typeanvil::render_report::to_text(e));
                }
                for o in outcomes.iter().flatten() {
                    eprintln!("{}", typeanvil::assertions::to_text(o));
                }
            }
        }
        if opts.fail_on_assert {
            let failed = outcomes.iter().flatten().filter(|o| !o.passed).count();
            if failed > 0 {
                return Err(AssertionsFailed(failed).into());
            }
        }
    }
    Ok(())
}

/// CORE-576: load data + schema, validate, expand.
fn expand_template(
    opts: &RenderArgs,
    source: &str,
    data_path: &std::path::Path,
) -> std::result::Result<String, typeanvil::template::TemplateErrors> {
    use typeanvil::template::{self as t, TemplateError, TemplateErrors};
    let text = std::fs::read_to_string(data_path).map_err(|e| {
        TemplateErrors::from(TemplateError {
            code: "TEMPLATE_DATA_INVALID_JSON",
            pointer: None,
            line: None,
            message: format!("reading {}: {e}", data_path.display()),
        })
    })?;
    let data = t::parse_data(&text)?;
    if let Some(schema) = t::load_schema(&opts.input, opts.schema.as_deref())? {
        let errs = t::validate_data(&schema, &data);
        if !errs.is_empty() {
            return Err(TemplateErrors(errs));
        }
    }
    t::expand(source, &data)
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
    // CORE-378: tagging is the default; `--no-tagged` opts out and `--ua`
    // (which implies tagging) wins over it in any argument order.
    let mut no_tagged = false;
    let mut ua = false;
    let mut diagnostics: Option<DiagnosticsMode> = None;
    let mut from: Option<String> = None;
    let mut css_files: Vec<PathBuf> = Vec::new();
    let mut no_default_css = false;
    let mut assert_file: Option<PathBuf> = None;
    let mut fail_on_assert = false;
    let mut fetch_mode: Option<String> = None;
    let mut fetch_hosts: Option<String> = None;
    let mut max_pages: Option<usize> = None;
    let mut timeout: Option<std::time::Duration> = None;
    let mut max_input_bytes: Option<usize> = None;
    let mut max_image_pixels: Option<u64> = None;
    let mut data: Option<PathBuf> = None;
    let mut schema: Option<PathBuf> = None;

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
            "--tagged" => no_tagged = false,
            "--no-tagged" => no_tagged = true,
            "--ua" => ua = true,
            "--diagnostics" => {
                let value = next_value(&mut it, &arg)?;
                diagnostics = Some(match value.as_str() {
                    "json" => DiagnosticsMode::Json,
                    "text" => DiagnosticsMode::Text,
                    other => bail!("invalid --diagnostics mode `{other}` (expected json|text)"),
                });
            }
            "--from" => from = Some(next_value(&mut it, &arg)?),
            "--css" => css_files.push(PathBuf::from(next_value(&mut it, &arg)?)),
            "--no-default-css" => no_default_css = true,
            "--assert" => assert_file = Some(PathBuf::from(next_value(&mut it, &arg)?)),
            "--fail-on-assert" => fail_on_assert = true,
            "--data" => data = Some(PathBuf::from(next_value(&mut it, &arg)?)),
            "--schema" => schema = Some(PathBuf::from(next_value(&mut it, &arg)?)),
            // CORE-578: fetch policy and limits.
            "--fetch" => fetch_mode = Some(next_value(&mut it, &arg)?),
            "--fetch-hosts" => fetch_hosts = Some(next_value(&mut it, &arg)?),
            "--max-pages" => max_pages = Some(parse_count(&next_value(&mut it, &arg)?, &arg)?),
            "--timeout" => {
                let v = next_value(&mut it, &arg)?;
                let secs: f64 = v
                    .parse()
                    .ok()
                    .filter(|s: &f64| s.is_finite() && *s > 0.0)
                    .ok_or_else(|| UsageError(format!("invalid --timeout `{v}` (seconds > 0)")))?;
                timeout = Some(std::time::Duration::from_secs_f64(secs));
            }
            "--max-input-bytes" => {
                max_input_bytes = Some(parse_count(&next_value(&mut it, &arg)?, &arg)?)
            }
            "--max-image-pixels" => {
                max_image_pixels = Some(parse_count(&next_value(&mut it, &arg)?, &arg)? as u64)
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

    let input = input.ok_or_else(|| anyhow!("missing input HTML path"))?;
    // CORE-513 D2: `--from` overrides the extension.
    let markdown = match from.as_deref() {
        Some("markdown") => true,
        Some("html") => false,
        Some(other) => {
            return Err(UsageError(format!(
                "invalid --from format `{other}` (expected html|markdown)"
            ))
            .into())
        }
        None => pipeline::is_markdown_path(&input.to_string_lossy()),
    };
    if !markdown && (!css_files.is_empty() || no_default_css) {
        return Err(UsageError(
            "--css and --no-default-css apply to Markdown input only".to_string(),
        )
        .into());
    }

    // CORE-576: templates are HTML; `--schema` needs `--data`.
    if data.is_some() && markdown {
        return Err(UsageError("--data applies to HTML templates only".to_string()).into());
    }
    if schema.is_some() && data.is_none() {
        return Err(UsageError("--schema requires --data <file.json>".to_string()).into());
    }

    // CORE-573: validate the assertions file BEFORE rendering (usage error,
    // exit 2, no PDF). `--assert` implies `--diagnostics json`.
    if fail_on_assert && assert_file.is_none() {
        return Err(UsageError("--fail-on-assert requires --assert <file.json>".to_string()).into());
    }
    let assertions = match &assert_file {
        None => None,
        Some(f) => {
            let text = std::fs::read_to_string(f).map_err(|e| {
                UsageError(format!("--assert {}: {e}", f.display()))
            })?;
            let list = typeanvil::assertions::parse(&text)
                .map_err(|e| UsageError(format!("--assert {}: {e}", f.display())))?;
            Some(list)
        }
    };
    let diagnostics = match (diagnostics, &assertions) {
        (None, Some(_)) => Some(DiagnosticsMode::Json),
        (d, _) => d,
    };

    // CORE-578: no flag = unrestricted (the legacy contract).
    let fetch = match fetch_mode.as_deref() {
        None if fetch_hosts.is_some() => {
            return Err(UsageError("--fetch-hosts requires --fetch allow-list".to_string()).into())
        }
        None => FetchPolicy::Unrestricted,
        Some(m) => {
            if m != "allow-list" && fetch_hosts.is_some() {
                return Err(
                    UsageError("--fetch-hosts requires --fetch allow-list".to_string()).into()
                );
            }
            FetchPolicy::parse(m, fetch_hosts.as_deref()).map_err(UsageError)?
        }
    };

    Ok(RenderArgs {
        input,
        assertions,
        fail_on_assert,
        markdown,
        css_files,
        no_default_css,
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
        tagged: ua || !no_tagged,
        ua,
        diagnostics,
        fetch,
        max_pages,
        timeout,
        max_input_bytes,
        max_image_pixels,
        data,
        schema,
    })
}

/// A positive integer flag value (usage error otherwise).
fn parse_count(v: &str, flag: &str) -> Result<usize> {
    v.parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| UsageError(format!("invalid {flag} `{v}` (a positive integer)")).into())
}

fn next_value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    it.next()
        .ok_or_else(|| anyhow!("flag `{flag}` needs a value"))
}
