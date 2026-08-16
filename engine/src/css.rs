//! CSS cascade.
//!
//! ## Path taken: hand-rolled cascade (sanctioned fallback), not stylo.
//!
//! The issue's headline spike was binding Servo's `stylo` crate for a
//! web-grade cascade. `stylo` (0.20) requires implementing the `TElement` /
//! `TNode` / `TDocument` trait family over our DOM, plus atom interning,
//! restyle snapshots, and a `SharedStyleContext` — a deep, multi-day binding
//! surface that would dominate a *walking skeleton*. The Architecture note
//! flags exactly this as the biggest wrap risk and documents a sanctioned
//! fallback: a hand-rolled cascade on `cssparser` (+ selector matching).
//!
//! We take the fallback, but keep the seam clean: [`ComputedStyle`] is the
//! output contract and [`cascade`] is the single entry point. Swapping stylo in
//! later means reimplementing `cascade` to produce the same `ComputedStyle`,
//! with zero changes to layout or PDF code.
//!
//! Supported today: inline `<style>` rules; element / `.class` / `#id` /
//! descendant selectors; specificity ordering; inheritance of `color`,
//! `font-size`, `font-family`; non-inherited `display`, `background-color`,
//! `margin`, `padding`.

use cssparser::{Parser, ParserInput, Token};

use crate::dom::{Dom, NodeId, NodeKind};
use crate::geom::{px_to_pt, Scalar};

/// An sRGB color, 8 bits per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Display {
    Block,
    Inline,
    None,
}

/// Fully computed style for one element. This is the cascade's output contract;
/// layout and PDF read only this.
#[derive(Clone, Debug)]
pub struct ComputedStyle {
    pub color: Color,
    pub background_color: Option<Color>,
    pub font_size: Scalar,
    pub font_family: String,
    pub display: Display,
    pub margin_top: Scalar,
    pub margin_right: Scalar,
    pub margin_bottom: Scalar,
    pub margin_left: Scalar,
    pub padding_top: Scalar,
    pub padding_right: Scalar,
    pub padding_bottom: Scalar,
    pub padding_left: Scalar,
}

impl ComputedStyle {
    /// The initial / root style (the base of inheritance).
    fn initial() -> ComputedStyle {
        ComputedStyle {
            color: Color::BLACK,
            background_color: None,
            font_size: px_to_pt(16.0),
            font_family: "sans-serif".to_string(),
            display: Display::Inline,
            margin_top: Scalar::ZERO,
            margin_right: Scalar::ZERO,
            margin_bottom: Scalar::ZERO,
            margin_left: Scalar::ZERO,
            padding_top: Scalar::ZERO,
            padding_right: Scalar::ZERO,
            padding_bottom: Scalar::ZERO,
            padding_left: Scalar::ZERO,
        }
    }

    /// Derive a child style from a parent, resetting non-inherited properties to
    /// their initial values and carrying inherited ones (color/font).
    fn inherit_from(parent: &ComputedStyle, default_display: Display) -> ComputedStyle {
        ComputedStyle {
            // Inherited.
            color: parent.color,
            font_size: parent.font_size,
            font_family: parent.font_family.clone(),
            // Not inherited: reset.
            background_color: None,
            display: default_display,
            margin_top: Scalar::ZERO,
            margin_right: Scalar::ZERO,
            margin_bottom: Scalar::ZERO,
            margin_left: Scalar::ZERO,
            padding_top: Scalar::ZERO,
            padding_right: Scalar::ZERO,
            padding_bottom: Scalar::ZERO,
            padding_left: Scalar::ZERO,
        }
    }
}

// --- Selector model -------------------------------------------------------

/// One compound selector piece: `tag.class#id` (any part optional).
#[derive(Clone, Debug, Default)]
struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

/// A selector is a descendant chain of compound selectors, e.g. `body p.lead`.
#[derive(Clone, Debug)]
struct Selector {
    parts: Vec<Compound>,
}

impl Selector {
    /// CSS specificity as (id, class, type) packed for ordering.
    fn specificity(&self) -> (u32, u32, u32) {
        let mut a = 0;
        let mut b = 0;
        let mut c = 0;
        for p in &self.parts {
            if p.id.is_some() {
                a += 1;
            }
            b += p.classes.len() as u32;
            if p.tag.is_some() {
                c += 1;
            }
        }
        (a, b, c)
    }
}

#[derive(Clone, Debug)]
struct Declaration {
    property: String,
    value: String,
}

#[derive(Clone, Debug)]
struct Rule {
    selectors: Vec<Selector>,
    declarations: Vec<Declaration>,
}

/// A parsed stylesheet.
#[derive(Debug, Default)]
pub struct Stylesheet {
    rules: Vec<Rule>,
}

impl Stylesheet {
    /// Parse CSS source text into rules. Tolerant: unparseable rules are skipped.
    pub fn parse(css: &str) -> Stylesheet {
        let mut rules = Vec::new();
        for (prelude, block) in split_rules(css) {
            let selectors = parse_selector_list(&prelude);
            if selectors.is_empty() {
                continue;
            }
            let declarations = parse_declarations(&block);
            if declarations.is_empty() {
                continue;
            }
            rules.push(Rule {
                selectors,
                declarations,
            });
        }
        Stylesheet { rules }
    }
}

/// Split a stylesheet into `(prelude, block)` pairs at top-level `{ }`.
/// Comments are stripped first. Nested braces are not expected at this layer
/// (no at-rules in the skeleton) but are handled defensively.
fn split_rules(css: &str) -> Vec<(String, String)> {
    let css = strip_comments(css);
    let mut out = Vec::new();
    let mut prelude = String::new();
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch == '{' {
            // Consume until matching close brace (depth-tracked).
            let mut depth = 1;
            let mut block = String::new();
            i += 1;
            while i < bytes.len() && depth > 0 {
                let c = bytes[i] as char;
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                block.push(c);
                i += 1;
            }
            out.push((prelude.trim().to_string(), block));
            prelude.clear();
        } else {
            prelude.push(ch);
            i += 1;
        }
    }
    out
}

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

fn parse_selector_list(prelude: &str) -> Vec<Selector> {
    prelude
        .split(',')
        .filter_map(|s| parse_selector(s.trim()))
        .collect()
}

fn parse_selector(s: &str) -> Option<Selector> {
    if s.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for token in s.split_whitespace() {
        let compound = parse_compound(token)?;
        parts.push(compound);
    }
    if parts.is_empty() {
        None
    } else {
        Some(Selector { parts })
    }
}

/// Parse `tag.class1.class2#id` (in any order after an optional leading tag).
fn parse_compound(s: &str) -> Option<Compound> {
    let mut compound = Compound::default();
    let mut chars = s.chars().peekable();
    // Optional leading type selector or universal.
    if let Some(&c) = chars.peek() {
        if c == '*' {
            chars.next();
        } else if c.is_ascii_alphabetic() {
            let mut tag = String::new();
            while let Some(&c) = chars.peek() {
                if c == '.' || c == '#' {
                    break;
                }
                tag.push(c);
                chars.next();
            }
            compound.tag = Some(tag.to_ascii_lowercase());
        }
    }
    // Then any sequence of `.class` / `#id`.
    while let Some(&c) = chars.peek() {
        match c {
            '.' => {
                chars.next();
                let mut cls = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '.' || c == '#' {
                        break;
                    }
                    cls.push(c);
                    chars.next();
                }
                if cls.is_empty() {
                    return None;
                }
                compound.classes.push(cls);
            }
            '#' => {
                chars.next();
                let mut id = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '.' || c == '#' {
                        break;
                    }
                    id.push(c);
                    chars.next();
                }
                if id.is_empty() {
                    return None;
                }
                compound.id = Some(id);
            }
            _ => return None, // unsupported combinator/pseudo in skeleton
        }
    }
    Some(compound)
}

/// Parse `prop: value; prop: value` inside a declaration block using cssparser's
/// tokenizer to reconstruct trimmed value strings.
fn parse_declarations(block: &str) -> Vec<Declaration> {
    let mut out = Vec::new();
    for chunk in block.split(';') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some(colon) = chunk.find(':') else {
            continue;
        };
        let property = chunk[..colon].trim().to_ascii_lowercase();
        let value = chunk[colon + 1..].trim().to_string();
        if property.is_empty() || value.is_empty() {
            continue;
        }
        out.push(Declaration { property, value });
    }
    out
}

// --- Matching -------------------------------------------------------------

fn compound_matches(compound: &Compound, dom: &Dom, id: NodeId) -> bool {
    let NodeKind::Element(el) = &dom.nodes[id].kind else {
        return false;
    };
    if let Some(tag) = &compound.tag {
        if !el.tag.eq_ignore_ascii_case(tag) {
            return false;
        }
    }
    if let Some(sel_id) = &compound.id {
        if el.id.as_deref() != Some(sel_id.as_str()) {
            return false;
        }
    }
    for cls in &compound.classes {
        if !el.classes.iter().any(|c| c == cls) {
            return false;
        }
    }
    true
}

/// Descendant-combinator match: the last compound must match `id`, and each
/// earlier compound must match some ancestor in order.
fn selector_matches(selector: &Selector, dom: &Dom, id: NodeId) -> bool {
    let parts = &selector.parts;
    let last = parts.len() - 1;
    if !compound_matches(&parts[last], dom, id) {
        return false;
    }
    // Walk ancestors, greedily matching remaining compounds from right to left.
    let mut remaining = last;
    let mut cur = dom.nodes[id].parent;
    while remaining > 0 {
        let Some(anc) = cur else {
            return false;
        };
        if compound_matches(&parts[remaining - 1], dom, anc) {
            remaining -= 1;
        }
        cur = dom.nodes[anc].parent;
    }
    true
}

// --- Value parsing --------------------------------------------------------

fn parse_color(value: &str) -> Option<Color> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    // A small set of named colors + rgb() via cssparser tokenizer.
    match v.to_ascii_lowercase().as_str() {
        "black" => return Some(Color { r: 0, g: 0, b: 0 }),
        "white" => {
            return Some(Color {
                r: 255,
                g: 255,
                b: 255,
            })
        }
        "red" => return Some(Color { r: 255, g: 0, b: 0 }),
        "green" => return Some(Color { r: 0, g: 128, b: 0 }),
        "blue" => return Some(Color { r: 0, g: 0, b: 255 }),
        "transparent" => return None,
        _ => {}
    }
    parse_rgb_func(v)
}

fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim();
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some(Color {
                r: r * 17,
                g: g * 17,
                b: b * 17,
            })
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color { r, g, b })
        }
        _ => None,
    }
}

/// Parse `rgb(r, g, b)` using cssparser's tokenizer.
fn parse_rgb_func(v: &str) -> Option<Color> {
    let mut input = ParserInput::new(v);
    let mut parser = Parser::new(&mut input);
    let name = parser.expect_function().ok()?.to_string();
    if !name.eq_ignore_ascii_case("rgb") && !name.eq_ignore_ascii_case("rgba") {
        return None;
    }
    let comps: Option<Vec<u8>> = parser
        .parse_nested_block(|p| {
            let mut vals = Vec::new();
            while vals.len() < 3 {
                let n = match p.next() {
                    Ok(Token::Number { value, .. }) => *value,
                    Ok(_) => continue, // skip commas/whitespace
                    Err(_) => break,
                };
                vals.push(n.clamp(0.0, 255.0) as u8);
            }
            Ok::<_, cssparser::ParseError<()>>(vals)
        })
        .ok();
    let comps = comps?;
    if comps.len() < 3 {
        return None;
    }
    Some(Color {
        r: comps[0],
        g: comps[1],
        b: comps[2],
    })
}

/// Parse a CSS length to points. Supports `px`, `pt`, `in`. Unitless → treated
/// as `px` (skeleton simplification).
fn parse_length(value: &str) -> Option<Scalar> {
    let v = value.trim();
    let (num, unit) = split_number_unit(v)?;
    match unit.as_str() {
        "px" | "" => Some(px_to_pt(num)),
        "pt" => Some(Scalar(num)),
        "in" => Some(Scalar(num * 72.0)),
        "em" => None, // resolved by caller relative to font-size; unsupported here
        _ => None,
    }
}

fn split_number_unit(v: &str) -> Option<(f64, String)> {
    let end = v
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(v.len());
    let num: f64 = v[..end].trim().parse().ok()?;
    let unit = v[end..].trim().to_ascii_lowercase();
    Some((num, unit))
}

fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration) {
    match decl.property.as_str() {
        "color" => {
            if let Some(c) = parse_color(&decl.value) {
                style.color = c;
            }
        }
        "background-color" | "background" => {
            style.background_color = parse_color(&decl.value);
        }
        "font-size" => {
            if let Some(l) = parse_length(&decl.value) {
                style.font_size = l;
            }
        }
        "font-family" => {
            let fam = decl
                .value
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if !fam.is_empty() {
                style.font_family = fam;
            }
        }
        "display" => {
            style.display = match decl.value.trim().to_ascii_lowercase().as_str() {
                "block" => Display::Block,
                "none" => Display::None,
                "inline" => Display::Inline,
                _ => style.display,
            };
        }
        "margin" => {
            if let Some(l) = parse_length(&decl.value) {
                style.margin_top = l;
                style.margin_right = l;
                style.margin_bottom = l;
                style.margin_left = l;
            }
        }
        "margin-top" => set_len(&mut style.margin_top, &decl.value),
        "margin-right" => set_len(&mut style.margin_right, &decl.value),
        "margin-bottom" => set_len(&mut style.margin_bottom, &decl.value),
        "margin-left" => set_len(&mut style.margin_left, &decl.value),
        "padding" => {
            if let Some(l) = parse_length(&decl.value) {
                style.padding_top = l;
                style.padding_right = l;
                style.padding_bottom = l;
                style.padding_left = l;
            }
        }
        "padding-top" => set_len(&mut style.padding_top, &decl.value),
        "padding-right" => set_len(&mut style.padding_right, &decl.value),
        "padding-bottom" => set_len(&mut style.padding_bottom, &decl.value),
        "padding-left" => set_len(&mut style.padding_left, &decl.value),
        _ => {}
    }
}

fn set_len(slot: &mut Scalar, value: &str) {
    if let Some(l) = parse_length(value) {
        *slot = l;
    }
}

/// UA default `display` for the handful of block-level tags we care about.
fn ua_display(tag: &str) -> Display {
    matches!(
        tag,
        "html"
            | "body"
            | "div"
            | "p"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
    )
    .then_some(Display::Block)
    .unwrap_or(Display::Inline)
}

/// UA default margins (px) for common tags, approximating browser defaults so
/// stacked blocks get visible separation.
fn ua_margins(tag: &str) -> (f64, f64) {
    // (top+bottom em-ish px, using font-size-independent px for the skeleton)
    match tag {
        "h1" => (21.0, 21.0),
        "h2" => (19.0, 19.0),
        "h3" => (18.0, 18.0),
        "p" => (16.0, 16.0),
        "ul" | "ol" | "blockquote" => (16.0, 16.0),
        _ => (0.0, 0.0),
    }
}

/// The cascade entry point. Produces a `ComputedStyle` per DOM node id
/// (indexed by `NodeId`). Text nodes inherit their parent's style.
///
/// This is the swap boundary for stylo: reimplement this function to produce
/// the same `Vec<ComputedStyle>` and nothing downstream changes.
pub fn cascade(dom: &Dom, stylesheet: &Stylesheet) -> Vec<ComputedStyle> {
    let mut styles = vec![ComputedStyle::initial(); dom.nodes.len()];
    // Depth-first from root so parents are computed before children.
    let root = dom.root;
    let root_style = ComputedStyle::initial();
    compute_node(dom, stylesheet, root, &root_style, &mut styles);
    styles
}

fn compute_node(
    dom: &Dom,
    stylesheet: &Stylesheet,
    id: NodeId,
    parent_style: &ComputedStyle,
    out: &mut [ComputedStyle],
) {
    let style = match &dom.nodes[id].kind {
        NodeKind::Root => parent_style.clone(),
        NodeKind::Text(_) => parent_style.clone(),
        NodeKind::Element(el) => {
            let default_display = ua_display(&el.tag);
            let mut style = ComputedStyle::inherit_from(parent_style, default_display);
            // UA margins.
            let (mt, mb) = ua_margins(&el.tag);
            style.margin_top = px_to_pt(mt);
            style.margin_bottom = px_to_pt(mb);

            // Collect matching (specificity, source-order) declarations.
            let mut matched: Vec<(&Selector, usize, usize)> = Vec::new(); // (sel, rule_idx, decl group)
            for (ri, rule) in stylesheet.rules.iter().enumerate() {
                for selector in &rule.selectors {
                    if selector_matches(selector, dom, id) {
                        matched.push((selector, ri, ri));
                    }
                }
            }
            // Sort by specificity then source order (stable).
            matched.sort_by(|a, b| {
                a.0.specificity()
                    .cmp(&b.0.specificity())
                    .then(a.1.cmp(&b.1))
            });
            for (_, ri, _) in matched {
                for decl in &stylesheet.rules[ri].declarations {
                    apply_declaration(&mut style, decl);
                }
            }
            style
        }
    };
    out[id] = style.clone();
    let children = dom.nodes[id].children.clone();
    for child in children {
        compute_node(dom, stylesheet, child, &style, out);
    }
}
