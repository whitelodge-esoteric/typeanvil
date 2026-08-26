// SPDX-License-Identifier: AGPL-3.0-only

//! Paged-media CSS: `@page` rules, margin boxes, running strings, counters,
//! and generated content — the wedge features layered on top of the CORE-51
//! fragment tree.
//!
//! stylo's servo build compiles neither `@page` nor the paged-media longhands,
//! so — exactly like `css::breaks` — this module runs a small, deterministic
//! author-CSS pass that parses only what it needs and never fails on the rest.
//! [`parse_page_rules`] extracts `@page` blocks (default + named, with
//! `:first`/`:left`/`:right` pseudos, `size`, margin longhands, and margin-box
//! sub-rules); [`resolve_page_spec`] picks the winning rule for one page by
//! name-in-effect then pseudo-by-index, falling back to the CLI geometry.
//!
//! Generated content ([`ContentPiece`]) is shared between margin boxes and the
//! element `content` property (see `css::ComputedStyle::content`, which the TOC
//! uses for `leader('.') target-counter(attr(href), page)`). Resolution to a
//! concrete string happens at fragmentainer-build time, once running strings,
//! the page counter, and the `target-counter` page map are known.
//!
//! Determinism: parsing is a single left-to-right scan; rule selection is a
//! total order over (name, pseudo-specificity, source order); running strings
//! and counters are threaded in document order. No hash-order dependence.

use crate::geom::{PageGeometry, Scalar};
use crate::css::Color;

/// A `@page` pseudo-class we support (css-page-3 subset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagePseudo {
    /// `@page :first` — the first page only.
    First,
    /// `@page :left` — verso pages (even 1-based index).
    Left,
    /// `@page :right` — recto pages (odd 1-based index; page 1 is right).
    Right,
    /// No pseudo — matches every page (of the given name).
    None,
}

/// The name of a page margin box (css-page-3 §7). The four side boxes
/// (`@left-*`/`@right-*`) are parsed and positioned but rendered as a single
/// horizontal line (no rotation — a documented non-goal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginBoxName {
    TopLeftCorner,
    TopLeft,
    TopCenter,
    TopRight,
    TopRightCorner,
    BottomLeftCorner,
    BottomLeft,
    BottomCenter,
    BottomRight,
    BottomRightCorner,
    LeftTop,
    LeftMiddle,
    LeftBottom,
    RightTop,
    RightMiddle,
    RightBottom,
}

impl MarginBoxName {
    /// Parse an at-keyword like `top-center` (without the leading `@`).
    fn parse(kw: &str) -> Option<MarginBoxName> {
        use MarginBoxName::*;
        Some(match kw.trim().to_ascii_lowercase().as_str() {
            "top-left-corner" => TopLeftCorner,
            "top-left" => TopLeft,
            "top-center" => TopCenter,
            "top-right" => TopRight,
            "top-right-corner" => TopRightCorner,
            "bottom-left-corner" => BottomLeftCorner,
            "bottom-left" => BottomLeft,
            "bottom-center" => BottomCenter,
            "bottom-right" => BottomRight,
            "bottom-right-corner" => BottomRightCorner,
            "left-top" => LeftTop,
            "left-middle" => LeftMiddle,
            "left-bottom" => LeftBottom,
            "right-top" => RightTop,
            "right-middle" => RightMiddle,
            "right-bottom" => RightBottom,
            _ => return None,
        })
    }

    /// Which margin row/column this box sits in.
    pub fn row(self) -> MarginRow {
        use MarginBoxName::*;
        match self {
            TopLeftCorner | TopLeft | TopCenter | TopRight | TopRightCorner => MarginRow::Top,
            BottomLeftCorner | BottomLeft | BottomCenter | BottomRight | BottomRightCorner => {
                MarginRow::Bottom
            }
            LeftTop | LeftMiddle | LeftBottom => MarginRow::Left,
            RightTop | RightMiddle | RightBottom => MarginRow::Right,
        }
    }

    /// Horizontal alignment within the box's slot (start / center / end).
    pub fn align(self) -> MarginAlign {
        use MarginBoxName::*;
        match self {
            TopLeftCorner | TopLeft | BottomLeftCorner | BottomLeft | LeftTop | RightTop => {
                MarginAlign::Start
            }
            TopCenter | BottomCenter | LeftMiddle | RightMiddle => MarginAlign::Center,
            TopRight | TopRightCorner | BottomRight | BottomRightCorner | LeftBottom
            | RightBottom => MarginAlign::End,
        }
    }
}

/// The page-margin band a margin box lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginRow {
    Top,
    Bottom,
    Left,
    Right,
}

/// Horizontal alignment of a margin box's single text line within its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginAlign {
    Start,
    Center,
    End,
}

/// The css-gcpm-3 §7 keyword of a `string(name, keyword)` reference.
/// Semantics probed against Prince 16.2 (see
/// `docs/research/css-gcpm/prince-string-keywords-probe.md`): with A = this
/// page's assignments and C = the carried value entering the page,
/// `first`/default → first of A else C; `last` → last of A else C;
/// `start` → C always (a top-of-page element does NOT count);
/// `first-except` → empty when A is non-empty, else C.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StringKeyword {
    #[default]
    First,
    Start,
    Last,
    FirstExcept,
}

impl StringKeyword {
    fn parse(s: &str) -> StringKeyword {
        match s.trim().to_ascii_lowercase().as_str() {
            "start" => StringKeyword::Start,
            "last" => StringKeyword::Last,
            "first-except" => StringKeyword::FirstExcept,
            // "first" and anything malformed fall back to the default
            // (spec §Behavior 1).
            _ => StringKeyword::First,
        }
    }
}

/// One piece of generated content (the `content` property / margin-box
/// `content`). A `content` value is a sequence of these, concatenated.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentPiece {
    /// A quoted literal string.
    Literal(String),
    /// `string(name[, keyword])` — the running string's value for this page,
    /// with css-gcpm-3 §7 keyword semantics (default `first`; probed against
    /// Prince 16.2, see docs/research/css-gcpm/prince-string-keywords-probe.md).
    StringRef(String, StringKeyword),
    /// `counter(page)` — the decimal page counter.
    CounterPage,
    /// `counter(pages)` — the total page count, resolved by a bounded
    /// second layout pass (paged-media-css spec §8; see `layout.rs`).
    CounterPages,
    /// `counter(name)` — a named counter (decimal).
    CounterRef(String),
    /// `target-counter(attr(href), page)` — the page of the element the
    /// `href` fragment points at. Carries the attribute name to read (`href`).
    TargetCounter { attr: String },
    /// `leader('.')` — fill to the content edge with a repeating character.
    Leader(char),
}

/// One parsed margin-box declaration inside an `@page` rule.
#[derive(Clone, Debug)]
pub struct MarginBoxDecl {
    pub name: MarginBoxName,
    /// The margin box's `content` value, as an ordered piece list.
    pub content: Vec<ContentPiece>,
}

/// Page margins in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageMargins {
    pub top: Scalar,
    pub right: Scalar,
    pub bottom: Scalar,
    pub left: Scalar,
}

/// A `size` declaration inside an `@page` rule (css-page-3 §4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SizeDecl {
    /// An absolute size (width, height) in points; a single value is square.
    Abs(Scalar, Scalar),
    /// `size: portrait` — the default page size, oriented tall (width < height).
    Portrait,
    /// `size: landscape` — the default page size, oriented wide.
    Landscape,
}

/// The `page-orientation` property (css-page-3): how the laid-out content is
/// rotated within the page box. Parsed and carried on the spec; the PDF
/// emitter applies the rotation (see `pdf.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageOrientation {
    RotateLeft,
    RotateRight,
    RotateTop,
    RotateBottom,
}

/// One margin/size value inside an `@page` rule: lengths, percentages
/// (relative to the page box), and the `auto`/`inherit` keywords.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageLength {
    /// An absolute length in points.
    Abs(Scalar),
    /// A percentage as a 0..=1 fraction of the page box size (width for
    /// left/right, height for top/bottom).
    Percent(f64),
    /// `auto` — participates in page-area centering (css-page-3 §4.1).
    Auto,
    /// `inherit` — resolves to the initial value (0) in the page context.
    Inherit,
}

/// One parsed `@page` rule.
#[derive(Clone, Debug)]
pub struct PageRule {
    /// The page name, or `None` for the default page.
    pub name: Option<String>,
    /// The pseudo-class selector.
    pub pseudo: PagePseudo,
    /// Explicit `size` (width, height) or an orientation keyword.
    pub size: Option<SizeDecl>,
    /// Page-area `width` override (css-page-3 §4.1 overconstrained sizing).
    pub width: Option<PageLength>,
    /// Page-area `height` override.
    pub height: Option<PageLength>,
    /// Individually-set margins (any subset), as parsed values.
    pub margin_top: Option<PageLength>,
    pub margin_right: Option<PageLength>,
    pub margin_bottom: Option<PageLength>,
    pub margin_left: Option<PageLength>,
    /// Page box background color (paints the whole page, under content).
    pub background: Option<Color>,
    /// `page-orientation` — how the content rotates within the page box.
    pub page_orientation: Option<PageOrientation>,
    /// Margin-box declarations, in source order.
    pub margin_boxes: Vec<MarginBoxDecl>,
    /// Source order, so later equal-specificity rules win.
    order: u32,
}

/// A fully-resolved page spec for one fragmentainer: geometry plus the margin
/// boxes to draw, with content still symbolic (resolved at build time).
#[derive(Clone, Debug)]
pub struct PageSpec {
    pub size: (Scalar, Scalar),
    pub margins: PageMargins,
    pub background: Option<Color>,
    pub page_orientation: Option<PageOrientation>,
    pub margin_boxes: Vec<(MarginBoxName, Vec<ContentPiece>)>,
}

impl PageSpec {
    /// The page geometry this spec implies (for `content_rect`).
    pub fn geometry(&self) -> PageGeometry {
        PageGeometry {
            width: self.size.0,
            height: self.size.1,
            margin_top: self.margins.top,
            margin_right: self.margins.right,
            margin_bottom: self.margins.bottom,
            margin_left: self.margins.left,
        }
    }
}

/// Running-string state: named string -> its current value, threaded in
/// document order. Kept as a sorted `Vec` (not a `HashMap`) so any iteration is
/// deterministic.
#[derive(Clone, Debug, Default)]
pub struct RunningStrings {
    entries: Vec<(String, String)>,
}

impl RunningStrings {
    pub fn new() -> RunningStrings {
        RunningStrings {
            entries: Vec::new(),
        }
    }

    /// Assign `value` to the named string (the last assignment on a page wins).
    pub fn set(&mut self, name: &str, value: String) {
        match self.entries.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value,
            None => self.entries.push((name.to_string(), value)),
        }
    }

    /// The current value of the named string, or empty before any assignment.
    pub fn get(&self, name: &str) -> &str {
        self.entries
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

// --- @page parsing ---------------------------------------------------------

/// Parse every `@page` rule out of the author stylesheet. Unknown declarations
/// and unparsable blocks are skipped without failing.
pub fn parse_page_rules(css: &str) -> Vec<PageRule> {
    let css = strip_comments(css);
    let mut rules = Vec::new();
    let mut order = 0u32;
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next `@page` at-keyword.
        let Some(at) = css[i..].find("@page") else {
            break;
        };
        let start = i + at;
        // The prelude runs from just after `@page` to the opening brace.
        let after_kw = start + "@page".len();
        let Some(brace_rel) = css[after_kw..].find('{') else {
            break;
        };
        let brace = after_kw + brace_rel;
        let prelude = css[after_kw..brace].trim();
        // Match the block braces (margin boxes nest one level of braces).
        let Some(block_end) = matching_brace(&css, brace) else {
            break;
        };
        let body = &css[brace + 1..block_end];

        if let Some(rule) = parse_one_page_rule(prelude, body, order) {
            rules.push(rule);
        }
        order += 1;
        i = block_end + 1;
    }
    rules
}

/// Parse the `@page` prelude (`landscape :first`, `:left`, etc.) into
/// (name, pseudo).
fn parse_prelude(prelude: &str) -> (Option<String>, PagePseudo) {
    let mut name = None;
    let mut pseudo = PagePseudo::None;
    for tok in prelude.split_whitespace() {
        if let Some(p) = tok.strip_prefix(':') {
            pseudo = match p.to_ascii_lowercase().as_str() {
                "first" => PagePseudo::First,
                "left" => PagePseudo::Left,
                "right" => PagePseudo::Right,
                _ => pseudo,
            };
        } else if !tok.is_empty() {
            name = Some(tok.to_string());
        }
    }
    (name, pseudo)
}

fn parse_one_page_rule(prelude: &str, body: &str, order: u32) -> Option<PageRule> {
    let (name, pseudo) = parse_prelude(prelude);
    let mut rule = PageRule {
        name,
        pseudo,
        size: None,
        width: None,
        height: None,
        margin_top: None,
        margin_right: None,
        margin_bottom: None,
        margin_left: None,
        background: None,
        page_orientation: None,
        margin_boxes: Vec::new(),
        order,
    };

    // Walk the body, extracting flat `prop: value;` declarations and nested
    // `@margin-box { ... }` sub-rules. Char-safe iteration (CORE-83): a
    // byte-wise loop would slice `body` mid-codepoint and mangle any
    // multi-byte UTF-8 in the declarations.
    let mut i = 0;
    let mut decl = String::new();
    let mut char_iter = body.char_indices();
    while let Some((ci, c)) = char_iter.next() {
        i = ci;
        if c == '@' {
            // A margin-box sub-rule. Read its keyword up to '{'.
            let Some(brace_rel) = body[i..].find('{') else {
                break;
            };
            let brace = i + brace_rel;
            let kw = body[i + 1..brace].trim();
            let Some(end) = matching_brace(body, brace) else {
                break;
            };
            let inner = &body[brace + 1..end];
            if let Some(name) = MarginBoxName::parse(kw) {
                if let Some(content) = parse_margin_box_content(inner) {
                    rule.margin_boxes.push(MarginBoxDecl { name, content });
                }
            }
            decl.clear();
            // Skip past the sub-rule body (the char iterator is at `ci`, so
            // advance it to just past `end`).
            while let Some((nci, _)) = char_iter.next() {
                if nci >= end {
                    break;
                }
                let _ = nci;
            }
            continue;
        }
        if c == ';' {
            apply_page_decl(&mut rule, &decl);
            decl.clear();
        } else {
            decl.push(c);
        }
    }
    apply_page_decl(&mut rule, &decl);

    Some(rule)
}

/// Extract the `content: ...;` declaration from a margin-box body.
fn parse_margin_box_content(body: &str) -> Option<Vec<ContentPiece>> {
    for decl in body.split(';') {
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        if prop.trim().eq_ignore_ascii_case("content") {
            return Some(parse_content(value));
        }
    }
    None
}

/// Apply one `prop: value` declaration to a page rule.
fn apply_page_decl(rule: &mut PageRule, decl: &str) {
    let Some((prop, value)) = decl.split_once(':') else {
        return;
    };
    let prop = prop.trim().to_ascii_lowercase();
    let value = value.trim();
    match prop.as_str() {
        "size" => rule.size = parse_size(value),
        "width" => rule.width = parse_page_length(value),
        "height" => rule.height = parse_page_length(value),
        "margin" => {
            if let Some((t, r, b, l)) = parse_margin_shorthand(value) {
                rule.margin_top = Some(t);
                rule.margin_right = Some(r);
                rule.margin_bottom = Some(b);
                rule.margin_left = Some(l);
            }
        }
        "margin-top" => rule.margin_top = parse_page_length(value),
        "margin-right" => rule.margin_right = parse_page_length(value),
        "margin-bottom" => rule.margin_bottom = parse_page_length(value),
        "margin-left" => rule.margin_left = parse_page_length(value),
        "background" | "background-color" => rule.background = crate::css::parse_css_color(value),
        "page-orientation" => rule.page_orientation = parse_page_orientation(value),
        _ => {}
    }
}

/// Parse a `size` value: css-page size keywords, orientation keywords
/// (`landscape`/`portrait`), one length (square), or two lengths
/// (width height). A keyword + orientation pair orients the keyword's size.
fn parse_size(value: &str) -> Option<SizeDecl> {
    let v = value.trim();
    let parts: Vec<&str> = v.split_whitespace().collect();
    let mut lens: Vec<Scalar> = Vec::new();
    let mut orient: Option<SizeDecl> = None;
    for p in &parts {
        match p.to_ascii_lowercase().as_str() {
            "landscape" => orient = Some(SizeDecl::Landscape),
            "portrait" => orient = Some(SizeDecl::Portrait),
            // Predefined page sizes (css-page-3 §6.3). Values are computed
            // with the SAME mm→pt formula as `parse_length` (mm * 72 / 25.4)
            // so `size: a5` renders byte-identically to `size: 148mm 210mm`
            // (the WPT references spell the keywords out as mm).
            "a5" => lens = vec![Scalar(148.0 * 72.0 / 25.4), Scalar(210.0 * 72.0 / 25.4)],
            "a4" => lens = vec![Scalar(210.0 * 72.0 / 25.4), Scalar(297.0 * 72.0 / 25.4)],
            "a3" => lens = vec![Scalar(297.0 * 72.0 / 25.4), Scalar(420.0 * 72.0 / 25.4)],
            "b5" => lens = vec![Scalar(176.0 * 72.0 / 25.4), Scalar(250.0 * 72.0 / 25.4)],
            "b4" => lens = vec![Scalar(250.0 * 72.0 / 25.4), Scalar(353.0 * 72.0 / 25.4)],
            "jis-b5" => lens = vec![Scalar(182.0 * 72.0 / 25.4), Scalar(257.0 * 72.0 / 25.4)],
            "jis-b4" => lens = vec![Scalar(257.0 * 72.0 / 25.4), Scalar(364.0 * 72.0 / 25.4)],
            "letter" => lens = vec![Scalar(8.5 * 72.0), Scalar(11.0 * 72.0)],
            "legal" => lens = vec![Scalar(8.5 * 72.0), Scalar(14.0 * 72.0)],
            "ledger" => lens = vec![Scalar(11.0 * 72.0), Scalar(17.0 * 72.0)],
            _ => lens.push(parse_length(p)?),
        }
    }
    let abs = match lens.as_slice() {
        [] => None,
        [s] => Some((*s, *s)),
        [w, h] => Some((*w, *h)),
        _ => return None,
    };
    match (abs, orient) {
        (None, o) => o,
        (Some((w, h)), None) => Some(SizeDecl::Abs(w, h)),
        (Some((w, h)), Some(SizeDecl::Landscape)) => {
            Some(SizeDecl::Abs(Scalar(w.get().max(h.get())), Scalar(w.get().min(h.get()))))
        }
        (Some((w, h)), Some(SizeDecl::Portrait)) => {
            Some(SizeDecl::Abs(Scalar(w.get().min(h.get())), Scalar(w.get().max(h.get()))))
        }
        _ => None,
    }
}

/// Parse the `page-orientation` property.
fn parse_page_orientation(value: &str) -> Option<PageOrientation> {
    match value.trim().to_ascii_lowercase().as_str() {
        "rotate-left" => Some(PageOrientation::RotateLeft),
        "rotate-right" => Some(PageOrientation::RotateRight),
        "rotate-top" => Some(PageOrientation::RotateTop),
        "rotate-bottom" => Some(PageOrientation::RotateBottom),
        _ => None,
    }
}

/// Parse a CSS `margin` shorthand (1–4 values) into `(top, right, bottom, left)`
/// as [`PageLength`] values (lengths, percentages, `auto`, `inherit`).
fn parse_margin_shorthand(value: &str) -> Option<(PageLength, PageLength, PageLength, PageLength)> {
    let parts: Vec<PageLength> = value.split_whitespace().filter_map(parse_page_length).collect();
    match parts.as_slice() {
        [a] => Some((*a, *a, *a, *a)),
        [v, h] => Some((*v, *h, *v, *h)),
        [t, h, b] => Some((*t, *h, *b, *h)),
        [t, r, b, l] => Some((*t, *r, *b, *l)),
        _ => None,
    }
}

/// Parse one margin/size value: `auto`, `inherit`, a percentage, or an
/// absolute length.
fn parse_page_length(s: &str) -> Option<PageLength> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("auto") {
        return Some(PageLength::Auto);
    }
    if s.eq_ignore_ascii_case("inherit") {
        return Some(PageLength::Inherit);
    }
    if let Some(pct) = s.strip_suffix('%') {
        let n: f64 = pct.trim().parse().ok()?;
        return Some(PageLength::Percent(n / 100.0));
    }
    parse_length(s).map(PageLength::Abs)
}

/// Parse a CSS absolute length (`in`/`pt`/`px`/`cm`/`mm`/`em`) into points.
/// `em` resolves against the page context's default font-size (16px = 12pt,
/// css-page-3 §3).
pub fn parse_length(s: &str) -> Option<Scalar> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let end = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num_str, unit) = s.split_at(end);
    let num: f64 = num_str.trim().parse().ok()?;
    let pt = match unit.trim().to_ascii_lowercase().as_str() {
        "in" => num * 72.0,
        "pt" | "" => num,
        "px" => num * 0.75,
        "cm" => num * 72.0 / 2.54,
        "mm" => num * 72.0 / 25.4,
        "em" => num * 12.0,
        _ => return None,
    };
    Some(Scalar(pt))
}

/// Parse a `content` value into an ordered piece list. Understands quoted
/// literals, `string(name)`, `counter(page)`/`counter(pages)`/`counter(name)`,
/// `target-counter(attr(href), page)`, and `leader('.')`. Unknown tokens are
/// skipped.
pub fn parse_content(value: &str) -> Vec<ContentPiece> {
    let mut pieces = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut lit = String::new();
            while i < chars.len() && chars[i] != quote {
                lit.push(chars[i]);
                i += 1;
            }
            i += 1; // closing quote
            pieces.push(ContentPiece::Literal(lit));
            continue;
        }
        // Read an identifier followed by an optional `( ... )`.
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
        {
            i += 1;
        }
        let ident: String = chars[start..i].iter().collect();
        if ident.is_empty() {
            i += 1;
            continue;
        }
        // Optional argument list.
        let mut args = String::new();
        if i < chars.len() && chars[i] == '(' {
            let mut depth = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '(' {
                    depth += 1;
                    if depth == 1 {
                        i += 1;
                        continue;
                    }
                } else if ch == ')' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                args.push(ch);
                i += 1;
            }
        }
        match ident.to_ascii_lowercase().as_str() {
            "string" => {
                // string(name[, keyword]) — split on the first comma; a
                // missing or malformed keyword falls back to First (spec
                // §Behavior 1).
                let (name, kw) = match args.split_once(',') {
                    Some((n, k)) => (n.trim(), StringKeyword::parse(k)),
                    None => (args.trim(), StringKeyword::First),
                };
                pieces.push(ContentPiece::StringRef(name.to_string(), kw));
            }
            "counter" => {
                let name = args.split(',').next().unwrap_or("").trim();
                if name.eq_ignore_ascii_case("page") {
                    pieces.push(ContentPiece::CounterPage);
                } else if name.eq_ignore_ascii_case("pages") {
                    pieces.push(ContentPiece::CounterPages);
                } else {
                    pieces.push(ContentPiece::CounterRef(name.to_string()));
                }
            }
            "target-counter" => {
                // target-counter(attr(href), page): grab the attr name.
                let attr = args
                    .find("attr(")
                    .and_then(|p| {
                        let rest = &args[p + 5..];
                        rest.find(')').map(|e| rest[..e].trim().to_string())
                    })
                    .unwrap_or_else(|| "href".to_string());
                pieces.push(ContentPiece::TargetCounter { attr });
            }
            "leader" => {
                let ch = args
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .chars()
                    .next()
                    .unwrap_or('.');
                pieces.push(ContentPiece::Leader(ch));
            }
            _ => {}
        }
    }
    pieces
}

// --- per-page resolution ---------------------------------------------------

/// Resolve the winning [`PageSpec`] for one fragmentainer.
///
/// - `page_name`: the named page in effect (from a `page: <name>` element), or
///   `None` for the default page.
/// - `global_index`: zero-based page index (page 1 == index 0).
/// - `cli`: the CLI geometry, used as the fallback default.
///
/// Selection: collect rules whose name matches (named rules first, then the
/// default page's rules apply as a base), and whose pseudo matches the page
/// parity; later, more-specific matches override earlier ones property by
/// property. Every field starts from the CLI geometry so unspecified props
/// inherit the default.
///
/// Resolution (css-page-3 §4.1): the page box size comes from `size` (with
/// `landscape`/`portrait` oriiented against the running size); the page AREA
/// (the content box) is either the explicit `width`/`height` overrides or the
/// page box minus margins. `auto` margins absorb the leftover space equally
/// (both auto) or entirely (single auto); with no auto margin, the leftover
/// goes to the end (right/bottom) margin, matching the overconstrained case.
pub fn resolve_page_spec(
    rules: &[PageRule],
    page_name: Option<&str>,
    global_index: usize,
    cli: &PageGeometry,
) -> PageSpec {
    let mut size = (cli.width, cli.height);
    let mut width: Option<PageLength> = None;
    let mut height: Option<PageLength> = None;
    let mut margins = (
        PageLength::Abs(cli.margin_top),
        PageLength::Abs(cli.margin_right),
        PageLength::Abs(cli.margin_bottom),
        PageLength::Abs(cli.margin_left),
    );
    let mut background = None;
    let mut page_orientation = None;
    // Margin boxes accumulate by name; later matches replace earlier.
    let mut boxes: Vec<(MarginBoxName, Vec<ContentPiece>)> = Vec::new();

    // Build the ordered list of matching rules, weakest first, so later
    // applications win. Ordering key: (name_specificity, pseudo_specificity,
    // source order). Default-page rules (name None) are weaker than named
    // rules; :none pseudo weaker than :left/:right weaker than :first.
    let mut matching: Vec<&PageRule> = rules
        .iter()
        .filter(|r| rule_matches(r, page_name, global_index))
        .collect();
    matching.sort_by_key(|r| {
        let name_spec = if r.name.is_some() { 1 } else { 0 };
        let pseudo_spec = match r.pseudo {
            PagePseudo::None => 0,
            PagePseudo::Left | PagePseudo::Right => 1,
            PagePseudo::First => 2,
        };
        (name_spec, pseudo_spec, r.order)
    });

    for r in matching {
        if let Some(s) = r.size {
            size = match s {
                SizeDecl::Abs(w, h) => (w, h),
                // Orientation keywords orient the *running* size (the default
                // page size) rather than a fresh pair.
                SizeDecl::Portrait => (
                    Scalar(size.0.get().min(size.1.get())),
                    Scalar(size.0.get().max(size.1.get())),
                ),
                SizeDecl::Landscape => (
                    Scalar(size.0.get().max(size.1.get())),
                    Scalar(size.0.get().min(size.1.get())),
                ),
            };
        }
        if let Some(v) = r.width {
            width = Some(v);
        }
        if let Some(v) = r.height {
            height = Some(v);
        }
        if let Some(v) = r.margin_top {
            margins.0 = v;
        }
        if let Some(v) = r.margin_right {
            margins.1 = v;
        }
        if let Some(v) = r.margin_bottom {
            margins.2 = v;
        }
        if let Some(v) = r.margin_left {
            margins.3 = v;
        }
        if let Some(c) = r.background {
            background = Some(c);
        }
        if let Some(o) = r.page_orientation {
            page_orientation = Some(o);
        }
        for mb in &r.margin_boxes {
            match boxes.iter_mut().find(|(n, _)| *n == mb.name) {
                Some((_, c)) => *c = mb.content.clone(),
                None => boxes.push((mb.name, mb.content.clone())),
            }
        }
    }

    // Resolve margins to concrete values against the final page size.
    let mt = resolve_margin(margins.0, size.1);
    let mr = resolve_margin(margins.1, size.0);
    let mb = resolve_margin(margins.2, size.1);
    let ml = resolve_margin(margins.3, size.0);
    let (mut margin_top, auto_top) = mt;
    let (mut margin_right, auto_right) = mr;
    let (mut margin_bottom, auto_bottom) = mb;
    let (mut margin_left, auto_left) = ml;

    // The page area: explicit width/height overrides, else size minus margins
    // (auto margins contribute 0 at this stage).
    let area_w = resolve_area(width, size.0).unwrap_or(size.0 - margin_left - margin_right);
    let area_h = resolve_area(height, size.1).unwrap_or(size.1 - margin_top - margin_bottom);

    // Leftover space on each axis: distributed to auto margins (equal split
    // when both auto), or to the end margin when overconstrained (css-page-3
    // §4.1: "the used value of the right/bottom margin absorbs the extra").
    let leftover_w = Scalar((size.0 - area_w - margin_left - margin_right).get().max(0.0));
    match (auto_left, auto_right) {
        (true, true) => {
            margin_left += leftover_w * 0.5;
            margin_right += leftover_w * 0.5;
        }
        (true, false) => margin_left += leftover_w,
        (false, true) => margin_right += leftover_w,
        (false, false) => margin_right += leftover_w,
    }
    let leftover_h = Scalar((size.1 - area_h - margin_top - margin_bottom).get().max(0.0));
    match (auto_top, auto_bottom) {
        (true, true) => {
            margin_top += leftover_h * 0.5;
            margin_bottom += leftover_h * 0.5;
        }
        (true, false) => margin_top += leftover_h,
        (false, true) => margin_bottom += leftover_h,
        (false, false) => margin_bottom += leftover_h,
    }

    PageSpec {
        size,
        margins: PageMargins {
            top: margin_top,
            right: margin_right,
            bottom: margin_bottom,
            left: margin_left,
        },
        background,
        page_orientation,
        margin_boxes: boxes,
    }
}

/// Resolve one margin value against the page box dimension (width for
/// left/right, height for top/bottom). Returns the concrete value plus
/// whether it was `auto`.
fn resolve_margin(len: PageLength, page_dim: Scalar) -> (Scalar, bool) {
    match len {
        PageLength::Abs(v) => (v, false),
        PageLength::Percent(f) => (page_dim * f, false),
        PageLength::Auto => (Scalar::ZERO, true),
        // `inherit` in the page context resolves to the initial value (0).
        PageLength::Inherit => (Scalar::ZERO, false),
    }
}

/// Resolve a page-area `width`/`height` override against the page dimension.
/// `None` means no override (the area is size minus margins).
fn resolve_area(len: Option<PageLength>, page_dim: Scalar) -> Option<Scalar> {
    match len? {
        PageLength::Abs(v) => Some(v),
        PageLength::Percent(f) => Some(page_dim * f),
        PageLength::Auto | PageLength::Inherit => None,
    }
}

/// Whether a rule matches the given page name and index parity.
fn rule_matches(rule: &PageRule, page_name: Option<&str>, global_index: usize) -> bool {
    // Name: a named rule matches only when that name is in effect; the default
    // page (name None) always applies as the base.
    match (&rule.name, page_name) {
        (Some(rn), Some(pn)) if rn == pn => {}
        (Some(_), _) => return false,
        (None, _) => {}
    }
    pseudo_matches(rule.pseudo, global_index)
}

/// Whether a page pseudo matches the given zero-based page index.
///
/// css-page-3: page 1 (index 0) is `:first` and `:right` (LTR progression);
/// odd 1-based indices are `:right`, even are `:left`.
fn pseudo_matches(pseudo: PagePseudo, global_index: usize) -> bool {
    let one_based = global_index + 1;
    match pseudo {
        PagePseudo::None => true,
        PagePseudo::First => global_index == 0,
        PagePseudo::Right => one_based % 2 == 1,
        PagePseudo::Left => one_based % 2 == 0,
    }
}

// --- shared parsing helpers ------------------------------------------------

/// Strip `/* ... */` comments (mirrors `css::breaks::strip_comments`).
/// Char-safe: iterates code points, never raw bytes (CORE-83 — a byte-wise
/// loop turned every multi-byte UTF-8 char in the stylesheet into Latin-1
/// mojibake before margin-box content was parsed).
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while !rest.is_empty() {
        if rest.starts_with("/*") {
            match rest.find("*/") {
                Some(end) => rest = &rest[end + 2..],
                // Unterminated comment: drop the remainder (matches the
                // byte-wise loop, which ran off the end of the buffer).
                None => break,
            }
        } else {
            let ch = rest.chars().next().expect("non-empty rest");
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// Given the byte index of a `{`, return the index of its matching `}`,
/// accounting for nested braces (margin boxes).
fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    debug_assert_eq!(bytes[open], b'{');
    let mut depth = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_size_and_margins() {
        let rules = parse_page_rules("@page { size: 8.5in 11in; margin: 1in; }");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert!(r.name.is_none());
        assert_eq!(r.size, Some(SizeDecl::Abs(Scalar(612.0), Scalar(792.0))));
        assert_eq!(r.margin_top, Some(PageLength::Abs(Scalar(72.0))));
    }

    #[test]
    fn parses_orientation_and_margins() {
        let rules = parse_page_rules(
            "@page { size: portrait; margin: 10px 20px 30px 40px; } \
             @page wide { size: landscape; page-orientation: rotate-right; \
             width: 12em; height: 3em; margin: auto; background: yellow; }",
        );
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].size, Some(SizeDecl::Portrait));
        assert_eq!(
            rules[0].margin_left,
            Some(PageLength::Abs(Scalar(30.0))) // 40px = 30pt
        );
        assert_eq!(rules[1].size, Some(SizeDecl::Landscape));
        assert_eq!(rules[1].page_orientation, Some(PageOrientation::RotateRight));
        assert_eq!(rules[1].width, Some(PageLength::Abs(Scalar(144.0)))); // 12em = 144pt
        assert_eq!(rules[1].margin_top, Some(PageLength::Auto));
        assert!(rules[1].background.is_some());
    }

    #[test]
    fn resolves_auto_margins_and_area() {
        // size 20em x 7em (= 240 x 84pt), area 12em x 3em (= 144 x 36pt),
        // margin auto -> 48pt left/right, 24pt top/bottom (centered).
        let rules = parse_page_rules("@page { size: 20em 7em; width: 12em; height: 3em; margin: auto; }");
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let spec = resolve_page_spec(&rules, None, 0, &cli);
        assert_eq!(spec.size, (Scalar(240.0), Scalar(84.0)));
        assert_eq!(spec.margins.left, Scalar(48.0));
        assert_eq!(spec.margins.right, Scalar(48.0));
        assert_eq!(spec.margins.top, Scalar(24.0));
        assert_eq!(spec.margins.bottom, Scalar(24.0));
    }

    #[test]
    fn resolves_orientation_specificity() {
        // page-rule-specificity-001 semantics: :first portrait beats default
        // landscape on page 1; page 2 uses the default landscape.
        let rules = parse_page_rules("@page :first { size: portrait; } @page { size: landscape; }");
        let cli = PageGeometry {
            width: Scalar(360.0),
            height: Scalar(216.0),
            margin_top: Scalar(36.0),
            margin_right: Scalar(36.0),
            margin_bottom: Scalar(36.0),
            margin_left: Scalar(36.0),
        };
        let p1 = resolve_page_spec(&rules, None, 0, &cli);
        assert_eq!(p1.size, (Scalar(216.0), Scalar(360.0))); // portrait of 5x3in
        let p2 = resolve_page_spec(&rules, None, 1, &cli);
        assert_eq!(p2.size, (Scalar(360.0), Scalar(216.0))); // landscape (default)
    }

    #[test]
    fn parses_named_and_pseudo() {
        let rules =
            parse_page_rules("@page landscape { size: 11in 8.5in; } @page :first { margin-top: 2in; }");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name.as_deref(), Some("landscape"));
        assert_eq!(rules[1].pseudo, PagePseudo::First);
    }

    #[test]
    fn parses_margin_boxes() {
        let rules = parse_page_rules(
            "@page { @top-center { content: \"Report\"; } @bottom-right { content: counter(page); } }",
        );
        let r = &rules[0];
        assert_eq!(r.margin_boxes.len(), 2);
        assert_eq!(r.margin_boxes[0].name, MarginBoxName::TopCenter);
        assert_eq!(
            r.margin_boxes[0].content,
            vec![ContentPiece::Literal("Report".to_string())]
        );
        assert_eq!(r.margin_boxes[1].content, vec![ContentPiece::CounterPage]);
    }

    #[test]
    fn parses_toc_content() {
        let pieces = parse_content("leader('.') target-counter(attr(href), page)");
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], ContentPiece::Leader('.'));
        assert_eq!(
            pieces[1],
            ContentPiece::TargetCounter {
                attr: "href".to_string()
            }
        );
    }

    #[test]
    fn pseudo_parity() {
        assert!(pseudo_matches(PagePseudo::First, 0));
        assert!(!pseudo_matches(PagePseudo::First, 1));
        assert!(pseudo_matches(PagePseudo::Right, 0)); // page 1
        assert!(pseudo_matches(PagePseudo::Left, 1)); // page 2
    }
}
