// SPDX-License-Identifier: AGPL-3.0-only

//! List markers (CORE-385, css-lists-3): marker text for `<li>` items.
//!
//! stylo's servo build has no `list-style-type` longhand, so the paged pass
//! in `css.rs` records the declared keyword on `ComputedStyle`. This module
//! resolves the effective type, the item ordinal (`<ol start>`, `reversed`,
//! `<li value>`) and the marker string.

use crate::css::{ComputedStyle, Display, ListStyleType};
use crate::dom::{Dom, NodeId, NodeKind};

fn tag(dom: &Dom, id: NodeId) -> Option<&str> {
    match &dom.nodes[id].kind {
        NodeKind::Element(e) => Some(e.tag.as_str()),
        _ => None,
    }
}

fn attr<'a>(dom: &'a Dom, id: NodeId, name: &str) -> Option<&'a str> {
    match &dom.nodes[id].kind {
        NodeKind::Element(e) => e.attr(name),
        _ => None,
    }
}

/// True when `id` is a list item that gets a marker (`li` in block flow).
pub(super) fn is_list_item(dom: &Dom, styles: &[ComputedStyle], id: NodeId) -> bool {
    tag(dom, id) == Some("li") && styles[id].display == Display::Block
}

/// The effective `list-style-type` of an `li`: its own declaration, else the
/// nearest ancestor declaration (inherited), else the UA default (`decimal`
/// in an `ol`; `disc`/`circle`/`square` by `ul` nesting depth).
fn effective_type(dom: &Dom, styles: &[ComputedStyle], li: NodeId) -> ListStyleType {
    let mut cur = Some(li);
    while let Some(n) = cur {
        if let Some(t) = styles[n].list_style_type {
            return t;
        }
        cur = dom.nodes[n].parent;
    }
    let parent = dom.nodes[li].parent;
    if let Some(p) = parent {
        if tag(dom, p) == Some("ol") {
            return match attr(dom, p, "type") {
                Some("a") => ListStyleType::LowerAlpha,
                Some("A") => ListStyleType::UpperAlpha,
                Some("i") => ListStyleType::LowerRoman,
                Some("I") => ListStyleType::UpperRoman,
                _ => ListStyleType::Decimal,
            };
        }
    }
    // Nested-`ul` depth: disc, circle, then square.
    let mut depth = 0;
    let mut cur = parent;
    while let Some(n) = cur {
        if matches!(tag(dom, n), Some("ul") | Some("ol")) {
            depth += 1;
        }
        cur = dom.nodes[n].parent;
    }
    match depth {
        0 | 1 => ListStyleType::Disc,
        2 => ListStyleType::Circle,
        _ => ListStyleType::Square,
    }
}

/// The ordinal of `li` among its parent's list items (css-lists-3 §4.5,
/// HTML 4.9.8: `start`, `reversed`, `value`).
fn ordinal(dom: &Dom, styles: &[ComputedStyle], li: NodeId) -> i64 {
    let Some(parent) = dom.nodes[li].parent else {
        return 1;
    };
    let items: Vec<NodeId> = dom.nodes[parent]
        .children
        .iter()
        .copied()
        .filter(|c| is_list_item(dom, styles, *c))
        .collect();
    let reversed = tag(dom, parent) == Some("ol") && attr(dom, parent, "reversed").is_some();
    let step: i64 = if reversed { -1 } else { 1 };
    let start = tag(dom, parent)
        .filter(|t| *t == "ol")
        .and_then(|_| attr(dom, parent, "start"))
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(if reversed { items.len() as i64 } else { 1 });
    let mut n = start - step;
    for item in items {
        n = match attr(dom, item, "value").and_then(|v| v.trim().parse::<i64>().ok()) {
            Some(v) => v,
            None => n + step,
        };
        if item == li {
            return n;
        }
    }
    1
}

fn alpha(n: i64, upper: bool) -> Option<String> {
    if n < 1 {
        return None;
    }
    let base = if upper { b'A' } else { b'a' };
    let mut n = n;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(base + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).ok()
}

fn roman(n: i64, upper: bool) -> Option<String> {
    if !(1..=3999).contains(&n) {
        return None;
    }
    const T: [(i64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut n = n;
    let mut s = String::new();
    for (v, r) in T {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    Some(if upper { s.to_uppercase() } else { s })
}

/// The marker string for list item `li` including the trailing space, or
/// `None` for `list-style-type: none`.
pub(super) fn marker_text(dom: &Dom, styles: &[ComputedStyle], li: NodeId) -> Option<String> {
    let ty = effective_type(dom, styles, li);
    let n = ordinal(dom, styles, li);
    let body = match ty {
        ListStyleType::None => return None,
        ListStyleType::Disc => return Some("\u{2022} ".to_string()),
        ListStyleType::Circle => return Some("\u{25E6} ".to_string()),
        ListStyleType::Square => return Some("\u{25AA} ".to_string()),
        ListStyleType::Decimal => n.to_string(),
        ListStyleType::DecimalLeadingZero => {
            if (0..10).contains(&n) {
                format!("0{n}")
            } else {
                n.to_string()
            }
        }
        ListStyleType::LowerAlpha => alpha(n, false).unwrap_or_else(|| n.to_string()),
        ListStyleType::UpperAlpha => alpha(n, true).unwrap_or_else(|| n.to_string()),
        ListStyleType::LowerRoman => roman(n, false).unwrap_or_else(|| n.to_string()),
        ListStyleType::UpperRoman => roman(n, true).unwrap_or_else(|| n.to_string()),
    };
    Some(format!("{body}. "))
}
