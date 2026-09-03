// SPDX-License-Identifier: AGPL-3.0-only

//! Structured machine-readable diagnostics (CORE-112).
//!
//! Iteration one covers the stylesheet surface: this module scans the SAME
//! CSS text the cascade consumes (`Stylesheet::source`) and reports
//! properties the engine does not consume, at-rules it does not know,
//! documented display fallbacks, and structurally malformed declarations.
//!
//! Design constraints from the spec
//! (`docs/specifications/diagnostics.spec.md`):
//!
//! - Zero render-path cost: `analyze` runs only when `--diagnostics` is
//!   passed; nothing here is touched by a plain render.
//! - Determinism: events come out in ascending source order; no hash-map
//!   iteration anywhere.
//! - Char safety (the CORE-83 lesson): every scan step iterates code points,
//!   never raw bytes — a multi-byte character must never split or mojibake.

use std::fmt::Write as _;

/// Broken-anchor events collected during layout (CORE-128): elements that
/// carry a `bookmark-*` declaration but resolve to no destination. Appended
/// by `layout::collect_headings`; drained by `main.rs` when `--diagnostics`
/// is active, and only then (zero cost on a plain render). A `Vec` with
/// single-threaded layout keeps this deterministic.
static BROKEN_ANCHORS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Record one `bookmark-anchor-unresolved` event: `element` describes the
/// node (tag + id/class if present). Called from layout; the message is
/// appended to the diagnostics stream when `--diagnostics` runs.
pub fn report_bookmark_anchor_unresolved(element: String) {
    if let Ok(mut v) = BROKEN_ANCHORS.lock() {
        v.push(element);
    }
}

/// Drain the collected broken-anchor events (in report order). Empty when no
/// render produced any (or this is a plain render that never drained).
pub fn take_broken_anchors() -> Vec<String> {
    if let Ok(mut v) = BROKEN_ANCHORS.lock() {
        std::mem::take(&mut *v)
    } else {
        Vec::new()
    }
}

/// One diagnostic event. Schema 1 fields only — see the spec's JSON schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable event code, e.g. `"unsupported-property"`.
    pub code: String,
    /// `"warning"` in schema 1.
    pub severity: String,
    /// Human sentence.
    pub message: String,
    /// 1-based line of the event start in the ORIGINAL source.
    pub line: usize,
    /// 1-based column (in characters) of the event start.
    pub column: usize,
}

/// The engine's consumed-property set (spec Appendix A): everything read by
/// `css.rs::convert`, the break pass, the border pass, the paged pass, or
/// `@font-face` descriptors. MUST stay sorted (binary search); lowercase.
const SUPPORTED_PROPERTIES: &[&str] = &[
    "align-items",
    "align-self",
    "background-color",
    "bookmark-label",
    "bookmark-level",
    "bookmark-state",
    "border",
    "border-bottom",
    "border-bottom-color",
    "border-bottom-width",
    "border-color",
    "border-left",
    "border-left-color",
    "border-left-width",
    "border-right",
    "border-right-color",
    "border-right-width",
    "border-top",
    "border-top-color",
    "border-top-width",
    "border-width",
    "bottom",
    "break-after",
    "break-before",
    "break-inside",
    "clear",
    "color",
    "column-count",
    "column-gap",
    "column-span",
    "column-width",
    "content",
    "counter-increment",
    "counter-reset",
    "display",
    "float",
    "flex",
    "flex-basis",
    "flex-direction",
    "flex-grow",
    "flex-shrink",
    "flex-wrap",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "gap",
    "height",
    "hyphens",
    "justify-content",
    "left",
    "line-height",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "min-width",
    "order",
    "orphans",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "padding-top",
    "page",
    "page-break-after",
    "page-break-before",
    "page-break-inside",
    "position",
    "right",
    "row-gap",
    "src",
    "string-set",
    "text-align",
    "top",
    "widows",
    "width",
    "z-index",
];

/// At-rules the engine understands.
const SUPPORTED_AT_RULES: &[&str] = &["font-face", "media", "page"];

/// `@page`-level descriptors (css-page-3 §3/§4): `size` sets the page box,
/// the four margin longhands + shorthand set the page margins, `background`
/// paints the page background.
const SUPPORTED_PAGE_DESCRIPTORS: &[&str] = &[
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "size",
];

/// `@page` margin-box names (css-page-3 §5). Their bodies carry margin-box
/// descriptors that overlap the element property set imperfectly; the spec's
/// Edge Cases section says skip them rather than risk false positives.
const MARGIN_BOXES: &[&str] = &[
    "@top-left-corner",
    "@top-left",
    "@top-center",
    "@top-right",
    "@top-right-corner",
    "@bottom-left-corner",
    "@bottom-left",
    "@bottom-center",
    "@bottom-right",
    "@bottom-right-corner",
];

/// `display` values the engine maps to a documented fallback in
/// `css.rs::convert` instead of honoring literally.
const DISPLAY_FALLBACKS: &[(&str, &str)] = &[
    ("inline-table", "inline-table falls back to block"),
    (
        "table-column",
        "table-column boxes are unsupported and fall back to block",
    ),
    (
        "table-column-group",
        "table-column-group falls back to block",
    ),
    ("table-caption", "table-caption falls back to block"),
    (
        "inline-flex",
        "inline-flex is treated as a block-level flex container",
    ),
];

/// Scan CSS source and return diagnostic events sorted by source position.
///
/// Comments (`/* */`) and quoted strings are transparent to the scanner: a
/// property-like token inside either never produces an event.
pub fn analyze(css: &str) -> Vec<Diagnostic> {
    let mut events = Vec::new();
    let mut scanner = Scanner::new(css);
    scan_block(&mut scanner, &mut events);
    events.sort_by_key(|d| (d.line, d.column));
    // Identical positions cannot occur for distinct declarations, but keep
    // the order fully determined anyway.
    events.dedup();
    events
}

/// Serialize the versioned JSON document. Hand-rolled emitter: deterministic
/// key order, no serde dependency, escaping covers everything our messages
/// can contain (quotes, backslashes, control chars).
pub fn to_json(events: &[Diagnostic]) -> String {
    let mut out = String::with_capacity(256 + events.len() * 160);
    out.push_str("{\n  \"schema\": 1,\n  \"diagnostics\": [");
    if events.is_empty() {
        out.push_str("],\n");
    } else {
        out.push('\n');
        for (idx, d) in events.iter().enumerate() {
            let comma = if idx + 1 == events.len() { "" } else { "," };
            let _ = writeln!(
                out,
                "    {{\"code\": {}, \"severity\": {}, \"message\": {}, \"line\": {}, \"column\": {}}}{}",
                json_string(&d.code),
                json_string(&d.severity),
                json_string(&d.message),
                d.line,
                d.column,
                comma
            );
        }
        out.push_str("  ],\n");
    }
    let _ = writeln!(out, "  \"counts\": {{\"warnings\": {}}}", events.len());
    out.push('}');
    out
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// --- Scanner ---------------------------------------------------------------

/// A cursor over the source that skips comments and quoted strings and maps
/// byte offsets to line/column coordinates. All matching happens on `&str`
/// slices at code-point boundaries.
struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Scanner { src, pos: 0 }
    }

    /// The unconsumed remainder.
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    /// Advance one code point.
    fn bump(&mut self) {
        let ch = self.rest().chars().next().expect("not at end");
        self.pos += ch.len_utf8();
    }

    /// Line/column (1-based, counted in characters) of an absolute byte
    /// offset.
    fn coords_of(&self, offset: usize) -> (usize, usize) {
        let mut line = 1usize;
        let mut col = 1usize;
        for ch in self.src[..offset].chars() {
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Skip whitespace, comments, and quoted strings.
    fn skip_noise(&mut self) {
        loop {
            let rest = self.rest();
            if rest.is_empty() {
                return;
            }
            let first = rest.chars().next().expect("non-empty rest");
            if first.is_whitespace() {
                self.bump();
            } else if rest.starts_with("/*") {
                match rest[2..].find("*/") {
                    Some(end) => self.pos += 2 + end + 2,
                    // Unterminated comment eats the rest (matches the
                    // engine's own strip_comments behavior).
                    None => self.pos = self.src.len(),
                }
            } else if first == '"' || first == '\'' {
                let inner = &rest[first.len_utf8()..];
                // A string ends at the matching quote character.
                let quote = first;
                match inner.find(quote) {
                    Some(e) => self.pos += first.len_utf8() + e + quote.len_utf8(),
                    None => self.pos = self.src.len(),
                }
            } else {
                return;
            }
        }
    }
}

/// Walk style rules and at-rules until the source ends or an outer closing
/// brace begins.
fn scan_block(scanner: &mut Scanner, events: &mut Vec<Diagnostic>) {
    loop {
        scanner.skip_noise();
        if scanner.at_end() {
            return;
        }
        let rest = scanner.rest();
        let brace_rel = rest.find('{');
        let close_rel = rest.find('}');
        match (brace_rel, close_rel) {
            // No further block: any trailing garbage ends the scan.
            (None, _) => return,
            // A closing brace belongs to an outer scope (@media body): stop
            // here and let the caller consume it.
            (Some(b), Some(c)) if c < b => return,
            (Some(b), _) => {
                let prelude_start = scanner.pos;
                let prelude = rest[..b].trim();
                let open_brace_abs = scanner.pos + b;
                let Some(body_end) = find_matching_brace(scanner.src, open_brace_abs) else {
                    push_unterminated(scanner, events, open_brace_abs);
                    return;
                };

                if prelude.starts_with('@') {
                    handle_at_rule(
                        scanner,
                        events,
                        prelude,
                        prelude_start,
                        open_brace_abs,
                        body_end,
                    );
                } else {
                    // Style rule: check each declaration in its body.
                    let body_offset = open_brace_abs + 1;
                    let body = &scanner.src[body_offset..body_end];
                    scan_declarations(body, body_offset, scanner, events);
                }
                scanner.pos = body_end + 1;
            }
        }
    }
}

/// Handle one `@…` block: emit `unknown-at-rule` for unsupported keywords,
/// recurse into `@media`, scan `@page` bodies with margin-box awareness.
fn handle_at_rule(
    scanner: &Scanner,
    events: &mut Vec<Diagnostic>,
    prelude: &str,
    prelude_start: usize,
    open_brace: usize,
    body_end: usize,
) {
    let keyword = prelude
        .split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or("");
    let kw_lower = keyword.to_ascii_lowercase();

    // Margin-box sub-block inside @page: recognized construct, skipped
    // wholesale per spec Edge Cases.
    if MARGIN_BOXES.contains(&kw_lower.as_str()) {
        return;
    }

    let Some(name) = kw_lower.strip_prefix('@') else {
        return;
    };
    if !SUPPORTED_AT_RULES.contains(&name) {
        let (line, col) = scanner.coords_of(prelude_start);
        events.push(Diagnostic {
            code: "unknown-at-rule".to_string(),
            severity: "warning".to_string(),
            message: format!("at-rule `{keyword}` is not supported and was ignored"),
            line,
            column: col,
        });
        return;
    }
    match name {
        "media" | "page" => {
            // `open_brace` is the at-rule's own opening brace, already located
            // by scan_block — no re-derivation needed.
            let mut sub = Scanner {
                src: scanner.src,
                pos: open_brace + 1,
            };
            if name == "media" {
                scan_block(&mut sub, events);
            } else {
                let body = &scanner.src[open_brace + 1..body_end];
                scan_page_body(body, open_brace + 1, scanner, events);
            }
        }
        // @font-face descriptors (font-family/src/font-weight/font-style)
        // are all consumed by fonts.rs; nothing further to check.
        _ => {}
    }
}

/// Check each declaration in a style-rule body against the supported set.
fn scan_declarations(
    body: &str,
    body_offset: usize,
    scanner: &Scanner,
    events: &mut Vec<Diagnostic>,
) {
    let mut start = 0usize;
    while start <= body.len() {
        match body[start..].find(';') {
            Some(rel) => {
                emit_decl_check(
                    scanner,
                    events,
                    &body[start..start + rel],
                    body_offset + start,
                    false,
                );
                start += rel + 1;
            }
            None => {
                emit_decl_check(scanner, events, &body[start..], body_offset + start, false);
                return;
            }
        }
    }
}

/// Check a single declaration fragment if it is non-empty.
fn emit_decl_check(
    scanner: &Scanner,
    events: &mut Vec<Diagnostic>,
    raw: &str,
    raw_offset: usize,
    in_page_body: bool,
) {
    if !raw.trim().is_empty() {
        check_declaration(scanner, events, raw, raw_offset, in_page_body);
    }
}

/// Scan an `@page` body: plain declarations are checked against the supported
/// set; margin-box blocks are skipped entirely.
fn scan_page_body(body: &str, body_offset: usize, scanner: &Scanner, events: &mut Vec<Diagnostic>) {
    let mut i = 0usize;
    while i < body.len() {
        match body.as_bytes()[i] {
            b'@' => {
                // Margin-box (or unknown nested at-rule): find its block.
                let rel_end = body[i..]
                    .find('{')
                    .and_then(|b| find_matching_brace(body, i + b));
                let name = body[i..]
                    .find('{')
                    .map(|b| body[i..i + b].trim())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !MARGIN_BOXES.contains(&name.as_str()) && !name.is_empty() {
                    let (line, col) = scanner.coords_of(body_offset + i);
                    events.push(Diagnostic {
                        code: "unknown-at-rule".to_string(),
                        severity: "warning".to_string(),
                        message: format!("at-rule `{name}` is not supported and was ignored"),
                        line,
                        column: col,
                    });
                }
                match rel_end {
                    Some(end) => i = end + 1,
                    None => return,
                }
            }
            b'}' => return,
            _ => {
                // Plain declaration up to ';', the next '@', or the block's
                // '}'. Only ';' is consumed here — '@' and '}' must stay
                // visible to the branches above.
                match body[i..].find(|c| c == ';' || c == '@' || c == '}') {
                    Some(rel) => {
                        emit_decl_check(scanner, events, &body[i..i + rel], body_offset + i, true);
                        i += if body.as_bytes()[i + rel] == b';' {
                            rel + 1
                        } else {
                            rel
                        };
                    }
                    None => {
                        emit_decl_check(scanner, events, &body[i..], body_offset + i, true);
                        return;
                    }
                }
            }
        }
    }
}

/// Check a single declaration fragment (`prop: value`) against the supported
/// set. Emits `unsupported-property`, `display-fallback`, or
/// `malformed-declaration`. `raw_offset` points at the fragment's first
/// character (leading whitespace included); reported columns point at the
/// property name itself.
fn check_declaration(
    scanner: &Scanner,
    events: &mut Vec<Diagnostic>,
    raw: &str,
    raw_offset: usize,
    in_page_body: bool,
) {
    let prop_col_offset = raw_offset + (raw.len() - raw.trim_start().len());

    let Some((prop_raw, value)) = raw.split_once(':') else {
        let (line, col) = scanner.coords_of(prop_col_offset);
        events.push(Diagnostic {
            code: "malformed-declaration".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "declaration `{}` has no property-value separator",
                raw.trim()
            ),
            line,
            column: col,
        });
        return;
    };

    let prop = prop_raw.trim().to_ascii_lowercase();
    let value_clean = value
        .trim()
        .trim_end_matches("!important")
        .trim()
        .to_ascii_lowercase();

    if prop.is_empty() {
        let (line, col) = scanner.coords_of(prop_col_offset);
        events.push(Diagnostic {
            code: "malformed-declaration".to_string(),
            severity: "warning".to_string(),
            message: "declaration has an empty property name".to_string(),
            line,
            column: col,
        });
        return;
    }
    // Custom properties (--*) and vendor prefixes (-webkit-…) are legal CSS
    // the engine deliberately ignores — never reported.
    if prop.starts_with('-') {
        return;
    }

    if prop == "display" {
        for (needle, msg) in DISPLAY_FALLBACKS {
            if value_clean.split_whitespace().any(|tok| tok == *needle) {
                let (line, col) = scanner.coords_of(prop_col_offset);
                events.push(Diagnostic {
                    code: "display-fallback".to_string(),
                    severity: "warning".to_string(),
                    message: (*msg).to_string(),
                    line,
                    column: col,
                });
                break;
            }
        }
        return;
    }

    let supported = if in_page_body {
        SUPPORTED_PAGE_DESCRIPTORS
    } else {
        SUPPORTED_PROPERTIES
    };
    if supported.binary_search(&prop.as_str()).is_err() {
        let (line, col) = scanner.coords_of(prop_col_offset);
        events.push(Diagnostic {
            code: "unsupported-property".to_string(),
            severity: "warning".to_string(),
            message: format!("property `{prop}` is not supported and was ignored"),
            line,
            column: col,
        });
    }
}

fn push_unterminated(scanner: &Scanner, events: &mut Vec<Diagnostic>, offset: usize) {
    let (line, col) = scanner.coords_of(offset);
    events.push(Diagnostic {
        code: "malformed-declaration".to_string(),
        severity: "warning".to_string(),
        message: "unterminated block at end of stylesheet".to_string(),
        line,
        column: col,
    });
}

/// Byte offset of the '}' matching the '{' at `open`. None when unterminated.
fn find_matching_brace(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    debug_assert_eq!(bytes[open], b'{');
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
