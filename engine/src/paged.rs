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

/// One piece of generated content (the `content` property / margin-box
/// `content`). A `content` value is a sequence of these, concatenated.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentPiece {
    /// A quoted literal string.
    Literal(String),
    /// `string(name)` — the running string's per-page value.
    StringRef(String),
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

/// One parsed `@page` rule.
#[derive(Clone, Debug)]
pub struct PageRule {
    /// The page name, or `None` for the default page.
    pub name: Option<String>,
    /// The pseudo-class selector.
    pub pseudo: PagePseudo,
    /// Explicit `size` (width, height) in points, if set.
    pub size: Option<(Scalar, Scalar)>,
    /// Individually-set margins (any subset), in points.
    pub margin_top: Option<Scalar>,
    pub margin_right: Option<Scalar>,
    pub margin_bottom: Option<Scalar>,
    pub margin_left: Option<Scalar>,
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
        margin_top: None,
        margin_right: None,
        margin_bottom: None,
        margin_left: None,
        margin_boxes: Vec::new(),
        order,
    };

    // Walk the body, extracting flat `prop: value;` declarations and nested
    // `@margin-box { ... }` sub-rules.
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut decl = String::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
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
            i = end + 1;
            continue;
        }
        if c == ';' {
            apply_page_decl(&mut rule, &decl);
            decl.clear();
        } else {
            decl.push(c);
        }
        i += 1;
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
        "margin" => {
            if let Some(m) = parse_margin_shorthand(value) {
                rule.margin_top = Some(m.top);
                rule.margin_right = Some(m.right);
                rule.margin_bottom = Some(m.bottom);
                rule.margin_left = Some(m.left);
            }
        }
        "margin-top" => rule.margin_top = parse_length(value),
        "margin-right" => rule.margin_right = parse_length(value),
        "margin-bottom" => rule.margin_bottom = parse_length(value),
        "margin-left" => rule.margin_left = parse_length(value),
        _ => {}
    }
}

/// Parse a `size` value: `A4`/`Letter`/`Legal` keyword, one length (square),
/// or two lengths (width height). Keywords resolve to portrait points.
fn parse_size(value: &str) -> Option<(Scalar, Scalar)> {
    let v = value.trim();
    match v.to_ascii_lowercase().as_str() {
        "a4" => return Some((Scalar(595.0), Scalar(842.0))),
        "letter" => return Some((Scalar(612.0), Scalar(792.0))),
        "legal" => return Some((Scalar(612.0), Scalar(1008.0))),
        _ => {}
    }
    let parts: Vec<&str> = v.split_whitespace().collect();
    match parts.as_slice() {
        [one] => {
            let s = parse_length(one)?;
            Some((s, s))
        }
        [w, h] => Some((parse_length(w)?, parse_length(h)?)),
        _ => None,
    }
}

/// Parse a CSS `margin` shorthand (1–4 lengths) into `PageMargins`.
fn parse_margin_shorthand(value: &str) -> Option<PageMargins> {
    let parts: Vec<Scalar> = value.split_whitespace().filter_map(parse_length).collect();
    let m = match parts.as_slice() {
        [a] => PageMargins {
            top: *a,
            right: *a,
            bottom: *a,
            left: *a,
        },
        [v, h] => PageMargins {
            top: *v,
            right: *h,
            bottom: *v,
            left: *h,
        },
        [t, h, b] => PageMargins {
            top: *t,
            right: *h,
            bottom: *b,
            left: *h,
        },
        [t, r, b, l] => PageMargins {
            top: *t,
            right: *r,
            bottom: *b,
            left: *l,
        },
        _ => return None,
    };
    Some(m)
}

/// Parse a CSS absolute length (`in`/`pt`/`px`/`cm`/`mm`) into points.
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
            "string" => pieces.push(ContentPiece::StringRef(args.trim().to_string())),
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
pub fn resolve_page_spec(
    rules: &[PageRule],
    page_name: Option<&str>,
    global_index: usize,
    cli: &PageGeometry,
) -> PageSpec {
    let mut size = (cli.width, cli.height);
    let mut margins = PageMargins {
        top: cli.margin_top,
        right: cli.margin_right,
        bottom: cli.margin_bottom,
        left: cli.margin_left,
    };
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
            size = s;
        }
        if let Some(v) = r.margin_top {
            margins.top = v;
        }
        if let Some(v) = r.margin_right {
            margins.right = v;
        }
        if let Some(v) = r.margin_bottom {
            margins.bottom = v;
        }
        if let Some(v) = r.margin_left {
            margins.left = v;
        }
        for mb in &r.margin_boxes {
            match boxes.iter_mut().find(|(n, _)| *n == mb.name) {
                Some((_, c)) => *c = mb.content.clone(),
                None => boxes.push((mb.name, mb.content.clone())),
            }
        }
    }

    PageSpec {
        size,
        margins,
        margin_boxes: boxes,
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
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
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
        assert_eq!(r.size, Some((Scalar(612.0), Scalar(792.0))));
        assert_eq!(r.margin_top, Some(Scalar(72.0)));
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
