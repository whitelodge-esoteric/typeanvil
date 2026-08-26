// SPDX-License-Identifier: AGPL-3.0-only

//! PDF document metadata extraction from the parsed DOM (CORE-105).
//!
//! Values come only from the document (or explicit CLI overrides) — never
//! from the wall clock or environment — so identical input stays
//! byte-identical.

use crate::dom::{Dom, NodeKind};
use crate::pdf::DocumentMetadata;

/// Extract Title/Author/Subject/Keywords from the DOM.
///
/// `--title` / `--author` overrides replace the document-derived values when
/// present and non-empty; an empty override falls back to the document.
/// Other fields are not CLI-overridable.
pub fn extract_metadata(
    dom: &Dom,
    title_override: Option<String>,
    author_override: Option<String>,
) -> DocumentMetadata {
    let title = title_override.and_then(non_empty).or_else(|| {
        // First `<title>` element's text, trimmed; empty → None.
        dom.find_tag("title").and_then(|id| {
            let t = dom.text_content(id);
            let t = t.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
    });
    // Single author → a one-element list (or empty when absent).
    let authors = author_override
        .and_then(non_empty)
        .or_else(|| meta_content(dom, "author"))
        .into_iter()
        .collect();
    DocumentMetadata {
        title,
        authors,
        subject: meta_content(dom, "description"),
        keywords: meta_content(dom, "keywords")
            .map(|c| {
                c.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Content of the first `<meta name="...">` element (exact lowercase name
/// match, per the spec's known limitation), trimmed; `None` when absent or
/// empty.
fn meta_content(dom: &Dom, name: &str) -> Option<String> {
    dom.nodes.iter().find_map(|node| match &node.kind {
        NodeKind::Element(el) if el.tag == "meta" && el.attr("name") == Some(name) => {
            non_empty(el.attr("content")?.to_string())
        }
        _ => None,
    })
}

/// Trim a value; treat empty/whitespace-only as absent.
fn non_empty(s: String) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}
