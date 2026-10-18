// SPDX-License-Identifier: AGPL-3.0-only

//! Markdown input (CORE-513, `docs/specifications/markdown-input.spec.md`).
//!
//! Markdown never reaches layout directly. `convert` renders the source to
//! one HTML document string with comrak; the caller feeds that string to
//! `Dom::parse`. This module is not called on the HTML input path.
//!
//! Frontmatter uses a restricted YAML reader (spec Gap 4): a top-level
//! mapping of scalar values and string lists (flow `[a, b]` or block `- a`).
//! Anything else is a frontmatter error that names the file line.

use anyhow::{anyhow, Result};
use comrak::{markdown_to_html, Options};

use crate::diagnostics::Diagnostic;

/// Result of a conversion.
pub struct MarkdownDoc {
    /// Complete HTML document for `Dom::parse`.
    pub html: String,
    /// Events with code prefix `markdown-` (serialized with
    /// `"source": "markdown"`).
    pub diagnostics: Vec<Diagnostic>,
}

/// Embedded default stylesheet (D4). No `@page` rule: geometry stays with the
/// CLI flags.
pub const DEFAULT_CSS: &str = "\
body { font-family: serif; font-size: 11pt; line-height: 1.4; }
h1, h2, h3, h4, h5, h6 { font-family: sans-serif; break-after: avoid; line-height: 1.2; }
h1 { font-size: 22pt; margin: 18pt 0 8pt 0; }
h2 { font-size: 17pt; margin: 16pt 0 6pt 0; }
h3 { font-size: 14pt; margin: 14pt 0 5pt 0; }
h4 { font-size: 12pt; margin: 12pt 0 4pt 0; }
h5, h6 { font-size: 11pt; margin: 10pt 0 4pt 0; }
p { margin: 0 0 8pt 0; }
a { color: #1a4f9c; }
pre, code { font-family: monospace; }
code { font-size: 0.9em; }
pre { font-size: 9pt; line-height: 1.3; white-space: pre-wrap; overflow-wrap: anywhere; margin: 0 0 10pt 0; padding: 6pt; background-color: #f4f4f4; }
pre code { font-size: 1em; }
blockquote { margin: 0 0 8pt 0; padding: 0 0 0 10pt; border-left: 3pt solid #cccccc; color: #444444; }
table { border-collapse: collapse; margin: 0 0 10pt 0; }
th, td { border: 0.5pt solid #999999; padding: 3pt 6pt; }
th { background-color: #eeeeee; font-weight: bold; }
thead { display: table-header-group; }
tr { break-inside: avoid; }
hr { border: 0; border-top: 0.5pt solid #999999; margin: 12pt 0; }
img { max-width: 100%; }
ul, ol { margin: 0 0 8pt 0; }
";

#[derive(Default)]
struct Front {
    title: Option<String>,
    authors: Vec<String>,
    description: Option<String>,
    keywords: Vec<String>,
    lang: Option<String>,
}

/// Convert Markdown source to a full HTML document.
///
/// `css` holds author sheets (file contents) appended after the default
/// sheet; `default_css` false omits the embedded sheet.
pub fn convert(source: &str, css: &[String], default_css: bool) -> Result<MarkdownDoc> {
    let text = source.strip_prefix('\u{feff}').unwrap_or(source);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut diagnostics = Vec::new();

    let (front, body, body_line) = split_frontmatter(&text, &mut diagnostics)?;

    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.strikethrough = true;
    opts.extension.tasklist = true;
    opts.extension.autolink = true;
    opts.extension.footnotes = true;
    opts.extension.math_dollars = true;
    // `Some("")` turns on GitHub-style heading ids with `-1`, `-2` suffixes.
    opts.extension.header_id_prefix = Some(String::new());
    opts.parse.smart = false;
    opts.render.r#unsafe = true; // raw HTML passes through (D3)

    let rendered = markdown_to_html(body, &opts);
    let rendered = task_boxes(&rendered);
    let (rendered, unresolved) = footnote_floats(&rendered);
    for label in unresolved {
        diagnostics.push(md_diag(
            "markdown-unresolved-footnote",
            &format!("footnote reference `{label}` has no definition; left as text"),
            0,
        ));
    }
    let _ = body_line;
    let rendered = rendered
        .replace(" align=\"right\"", " style=\"text-align:right\"")
        .replace(" align=\"center\"", " style=\"text-align:center\"")
        .replace(" align=\"left\"", " style=\"text-align:left\"");
    let (rendered, math_errors) = translate_math(&rendered);
    for why in math_errors {
        diagnostics.push(md_diag(
            "markdown-math-untranslated",
            &format!("math rendered as source text: {why}"),
            0,
        ));
    }

    let mut html = String::with_capacity(rendered.len() + 2048);
    html.push_str("<!doctype html><html");
    if let Some(lang) = &front.lang {
        html.push_str(" lang=\"");
        html.push_str(&esc_attr(lang));
        html.push('"');
    }
    html.push_str("><head><meta charset=\"utf-8\">");
    if let Some(t) = &front.title {
        html.push_str("<title>");
        html.push_str(&esc_text(t));
        html.push_str("</title>");
    }
    if !front.authors.is_empty() {
        meta(&mut html, "author", &front.authors.join(", "));
    }
    if let Some(d) = &front.description {
        meta(&mut html, "description", d);
    }
    if !front.keywords.is_empty() {
        meta(&mut html, "keywords", &front.keywords.join(", "));
    }
    if default_css {
        style(&mut html, DEFAULT_CSS);
    }
    for sheet in css {
        style(&mut html, sheet);
    }
    html.push_str("</head><body>");
    html.push_str(&rendered);
    html.push_str("</body></html>");

    Ok(MarkdownDoc { html, diagnostics })
}

/// Gap 1: a checkbox `<input>` has no layout box, so task items get a glyph
/// in `<span class="task">` (checked: U+2611, unchecked: U+2610).
fn task_boxes(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<input type=\"checkbox\"") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find("/>").map(|e| e + 2).unwrap_or(tail.len());
        let checked = tail[..end].contains("checked");
        out.push_str(if checked {
            "<span class=\"task\">\u{2611}</span>"
        } else {
            "<span class=\"task\">\u{2610}</span>"
        });
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// B9: comrak renders `[^id]` as `<sup class="footnote-ref"><a ...>N</a></sup>`
/// plus a trailing `<section class="footnotes">` list. Replace each reference
/// with an inline `<span class="footnote">` holding the definition content
/// (the engine's `float: footnote` form) and drop the trailing list. Returns
/// the new HTML and the labels of references that stayed literal text.
fn footnote_floats(html: &str) -> (String, Vec<String>) {
    const SEC: &str = "<section class=\"footnotes\"";
    let mut unresolved = Vec::new();
    let (body, defs) = match html.find(SEC) {
        Some(i) => {
            let tail = &html[i..];
            let end = tail.find("</section>").map(|e| e + "</section>".len()).unwrap_or(tail.len());
            let mut defs: Vec<(String, String)> = Vec::new();
            let mut rest = &tail[..end];
            while let Some(p) = rest.find("<li id=\"") {
                let after = &rest[p + 8..];
                let Some(q) = after.find('"') else { break };
                let id = after[..q].to_string();
                let Some(gt) = after.find('>') else { break };
                let content = &after[gt + 1..];
                let Some(close) = content.find("</li>") else { break };
                defs.push((id, footnote_inline(&content[..close])));
                rest = &content[close..];
            }
            let mut b = String::with_capacity(html.len());
            b.push_str(&html[..i]);
            b.push_str(&tail[end..]);
            (b, defs)
        }
        None => (html.to_string(), Vec::new()),
    };

    const REF: &str = "<sup class=\"footnote-ref\"><a href=\"#";
    let mut out = String::with_capacity(body.len());
    let mut rest = body.as_str();
    while let Some(i) = rest.find(REF) {
        out.push_str(&rest[..i]);
        let tail = &rest[i + REF.len()..];
        let Some(end) = tail.find("</sup>") else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let target = tail.split('"').next().unwrap_or("");
        // Repeated references get ids `fn-1` for every call; the definition
        // is keyed by the base id.
        match defs.iter().find(|(id, _)| id == target) {
            Some((_, content)) => {
                out.push_str("<span class=\"footnote\" style=\"float: footnote\">");
                out.push_str(content);
                out.push_str("</span>");
            }
            None => out.push_str(&rest[i..i + REF.len() + end + "</sup>".len()]),
        }
        rest = &tail[end + "</sup>".len()..];
    }
    out.push_str(rest);

    // Unresolved references remain as literal `[^label]` text outside code.
    let scan = strip_blocks(&strip_blocks(&out, "pre"), "code");
    let mut s = scan.as_str();
    while let Some(a) = s.find("[^") {
        let t = &s[a + 2..];
        if let Some(b) = t.find(']') {
            let label = &t[..b];
            if !label.is_empty() && !label.contains(|c: char| c.is_whitespace() || c == '<' || c == '[') {
                unresolved.push(label.to_string());
            }
            s = &t[b..];
        } else {
            break;
        }
    }
    (out, unresolved)
}

/// Remove every `<tag ...>...</tag>` block (used to skip code when scanning
/// for literal `[^label]` text).
fn strip_blocks(html: &str, tag: &str) -> String {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = String::with_capacity(html.len());
    let mut s = html;
    while let Some(a) = s.find(&open) {
        out.push_str(&s[..a]);
        match s[a..].find(&close) {
            Some(c) => s = &s[a + c + close.len()..],
            None => return out,
        }
    }
    out.push_str(s);
    out
}

/// Flatten a definition `<li>` body to inline content: drop the back-reference
/// link and join paragraphs with a space (a `<span>` cannot hold block tags).
fn footnote_inline(li: &str) -> String {
    let mut s = li.to_string();
    while let Some(a) = s.find("<a href=\"#fnref") {
        match s[a..].find("</a>") {
            Some(c) => s.replace_range(a..a + c + 4, ""),
            None => break,
        }
    }
    s.replace("</p>\n<p>", " ")
        .replace("<p>", "")
        .replace("</p>", "")
        .trim()
        .to_string()
}

/// B10: comrak's math spans become `<math>` through the TeX translator
/// (`texmath.rs`). A formula the translator rejects becomes
/// `<code class="math">` holding the source; the reason is returned so the
/// caller can emit one diagnostic per failure.
fn translate_math(html: &str) -> (String, Vec<String>) {
    const OPEN: &str = "<span data-math-style=\"";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut errors = Vec::new();
    while let Some(i) = rest.find(OPEN) {
        out.push_str(&rest[..i]);
        let tail = &rest[i + OPEN.len()..];
        let (Some(q), Some(gt)) = (tail.find('"'), tail.find('>')) else {
            out.push_str(&rest[i..]);
            return (out, errors);
        };
        let display = &tail[..q] == "display";
        let after = &tail[gt + 1..];
        let Some(close) = after.find("</span>") else {
            out.push_str(&rest[i..]);
            return (out, errors);
        };
        let escaped = &after[..close];
        let tex = unescape_html(escaped);
        match crate::texmath::tex_to_mathml(&tex, display) {
            Ok(ml) => out.push_str(&ml),
            Err(why) => {
                out.push_str("<code class=\"math\">");
                out.push_str(escaped);
                out.push_str("</code>");
                errors.push(why);
            }
        }
        rest = &after[close + "</span>".len()..];
    }
    out.push_str(rest);
    (out, errors)
}

/// Undo the entity escaping comrak applies to span text.
fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn md_diag(code: &str, message: &str, line: usize) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: "warning".to_string(),
        message: message.to_string(),
        line,
        column: 0,
    }
}

fn meta(out: &mut String, name: &str, content: &str) {
    out.push_str("<meta name=\"");
    out.push_str(name);
    out.push_str("\" content=\"");
    out.push_str(&esc_attr(content));
    out.push_str("\">");
}

fn style(out: &mut String, css: &str) {
    out.push_str("<style>");
    out.push_str(&css.replace("</style", "<\\/style"));
    out.push_str("</style>");
}

fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;")
}

/// Split a leading `---` block off `text`. Returns the parsed fields, the
/// remaining body, and the 1-based file line where the body starts.
fn split_frontmatter<'a>(
    text: &'a str,
    diags: &mut Vec<Diagnostic>,
) -> Result<(Front, &'a str, usize)> {
    let mut front = Front::default();
    let mut lines = text.split('\n');
    if lines.next() != Some("---") {
        return Ok((front, text, 1));
    }
    let mut offset = 4; // "---\n"
    let mut block: Vec<(usize, &str)> = Vec::new();
    let mut closed_at = None;
    for (idx, line) in lines.enumerate() {
        let file_line = idx + 2;
        offset += line.len() + 1;
        if line.trim_end() == "---" || line.trim_end() == "..." {
            closed_at = Some(file_line);
            break;
        }
        block.push((file_line, line));
    }
    let Some(closed) = closed_at else {
        // Never closed: the leading `---` is a thematic break.
        return Ok((front, text, 1));
    };
    let mut cur_key: Option<String> = None;
    let mut cur_list: Option<Vec<String>> = None;
    let mut entries: Vec<(usize, String, Value)> = Vec::new();
    let mut key_line = 0usize;

    let flush = |key: &mut Option<String>,
                 list: &mut Option<Vec<String>>,
                 line: usize,
                 entries: &mut Vec<(usize, String, Value)>| {
        if let Some(k) = key.take() {
            let v = match list.take() {
                Some(l) => Value::List(l),
                None => Value::Scalar(String::new()),
            };
            entries.push((line, k, v));
        }
    };

    for (ln, raw) in block {
        let t = raw.trim_end();
        if t.trim().is_empty() || t.trim_start().starts_with('#') {
            continue;
        }
        let indented = t.starts_with(' ') || t.starts_with('\t');
        let trimmed = t.trim_start();
        if let Some(item) = trimmed.strip_prefix("- ").or_else(|| (trimmed == "-").then_some("")) {
            match cur_list.as_mut() {
                Some(l) if indented || cur_key.is_some() => l.push(unquote(item.trim(), ln)?),
                _ => return Err(anyhow!("frontmatter line {ln}: unexpected list item")),
            }
            continue;
        }
        if indented {
            return Err(anyhow!("frontmatter line {ln}: unsupported nested or continued value"));
        }
        flush(&mut cur_key, &mut cur_list, key_line, &mut entries);
        let Some(colon) = find_key_colon(t) else {
            return Err(anyhow!("frontmatter line {ln}: expected `key: value`"));
        };
        let key = t[..colon].trim();
        if key.is_empty() {
            return Err(anyhow!("frontmatter line {ln}: empty key"));
        }
        let rest = t[colon + 1..].trim();
        key_line = ln;
        if rest.is_empty() {
            cur_key = Some(key.to_string());
            cur_list = Some(Vec::new());
        } else if let Some(inner) = rest.strip_prefix('[') {
            let Some(inner) = inner.strip_suffix(']') else {
                return Err(anyhow!("frontmatter line {ln}: unterminated `[` list"));
            };
            let mut items = Vec::new();
            for part in inner.split(',') {
                let p = part.trim();
                if !p.is_empty() {
                    items.push(unquote(p, ln)?);
                }
            }
            entries.push((ln, key.to_string(), Value::List(items)));
        } else if rest.starts_with('{') || rest.starts_with('|') || rest.starts_with('>') {
            return Err(anyhow!("frontmatter line {ln}: unsupported value form"));
        } else {
            entries.push((ln, key.to_string(), Value::Scalar(unquote(rest, ln)?)));
        }
    }
    flush(&mut cur_key, &mut cur_list, key_line, &mut entries);

    for (ln, key, value) in entries {
        match key.as_str() {
            "title" => front.title = non_empty(value.joined()),
            "author" => front.authors = value.items(),
            "description" => front.description = non_empty(value.joined()),
            "keywords" => front.keywords = value.items(),
            "lang" => front.lang = non_empty(value.joined()),
            other => diags.push(md_diag(
                "markdown-unknown-frontmatter-key",
                &format!("unknown frontmatter key `{other}` ignored"),
                ln,
            )),
        }
    }
    Ok((front, &text[offset.min(text.len())..], closed + 1))
}

enum Value {
    Scalar(String),
    List(Vec<String>),
}

impl Value {
    fn items(self) -> Vec<String> {
        match self {
            Value::Scalar(s) => non_empty(s).into_iter().collect(),
            Value::List(l) => l.into_iter().filter(|s| !s.trim().is_empty()).collect(),
        }
    }
    fn joined(self) -> String {
        match self {
            Value::Scalar(s) => s,
            Value::List(l) => l.join(", "),
        }
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Index of the `:` that ends the key (outside quotes), requiring a space or
/// end of line after it.
fn find_key_colon(t: &str) -> Option<usize> {
    let b = t.as_bytes();
    (0..b.len()).find(|&i| b[i] == b':' && (i + 1 == b.len() || b[i + 1] == b' '))
}

fn unquote(s: &str, ln: usize) -> Result<String> {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(rest) = s.strip_prefix(q) {
            return match rest.strip_suffix(q) {
                Some(inner) if !inner.contains(q) || q == '"' => Ok(inner.replace("\\\"", "\"")),
                _ => Err(anyhow!("frontmatter line {ln}: unterminated quoted string")),
            };
        }
    }
    Ok(s.to_string())
}
