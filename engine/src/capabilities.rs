// SPDX-License-Identifier: AGPL-3.0-only

//! Capability manifest (CORE-572): `typeanvil capabilities --json`.
//!
//! The manifest is GENERATED from the same tables the diagnostics scanner
//! (`diagnostics.rs`) uses to decide "not consumed", so the two cannot
//! disagree. Curated tables here only add notes and alternatives; they
//! never decide a status on their own for properties, at-rules, or
//! `display` values.
//!
//! Spec: `docs/specifications/capability-manifest.spec.md`.

use std::fmt::Write as _;

use crate::diagnostics::{
    DISPLAY_FALLBACKS, SUPPORTED_AT_RULES, SUPPORTED_PAGE_DESCRIPTORS, SUPPORTED_PROPERTIES,
};

/// Manifest schema version. Breaking changes bump it.
pub const SCHEMA: u32 = 1;

/// Support status of one capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Supported,
    Partial,
    Unsupported,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Supported => "supported",
            Status::Partial => "partial",
            Status::Unsupported => "unsupported",
        }
    }
}

/// One manifest entry. `id` is `<kind>:<name>`, e.g. `property:color`,
/// `display:inline-flex`, `at-rule:import`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    pub kind: &'static str,
    pub name: String,
    pub status: Status,
    pub note: String,
    pub alternative: Option<&'static str>,
}

impl Capability {
    pub fn id(&self) -> String {
        format!("{}:{}", self.kind, self.name)
    }
}

/// Unsupported properties commonly written by authors and LLMs, with a
/// recommended alternative. Status is NOT taken from here: an entry that is
/// also in `SUPPORTED_PROPERTIES` is a test failure
/// (`curated_unsupported_are_not_consumed`). Sorted by name.
///
/// Only properties with no reader anywhere in `engine/src` belong here
/// (verified by grep, 2026-10-24).
pub(crate) const UNSUPPORTED_PROPERTY_HINTS: &[(&str, &str, Option<&str>)] = &[
    (
        "border-radius",
        "corners are always square",
        Some("use an SVG or `<img>` for rounded shapes"),
    ),
    (
        "box-shadow",
        "shadows are not painted",
        Some("use a `border` for an outline effect"),
    ),
    (
        "text-decoration",
        "underline and strike-through are not drawn",
        Some("use `border-bottom` on an inline element for an underline"),
    ),
    (
        "text-indent",
        "first-line indentation is not applied",
        Some("use `padding-left` or `margin-left` on the block"),
    ),
    ("word-spacing", "word spacing is not adjusted", None),
];

/// Unsupported at-rules with notes. Same rule: status comes from
/// `SUPPORTED_AT_RULES`. Sorted by name.
pub(crate) const UNSUPPORTED_AT_RULE_HINTS: &[(&str, &str, Option<&str>)] = &[
    (
        "import",
        "imported stylesheets are not fetched",
        Some("inline the stylesheet in a `<style>` element"),
    ),
    ("keyframes", "animation has no effect in a PDF", None),
    (
        "supports",
        "the rule and its contents are ignored",
        Some("write the declarations without the `@supports` wrapper"),
    ),
];

/// Selector features. The engine matches selectors with the `selectors`
/// crate that stylo uses, so these are documented, not table-derived.
const SELECTOR_FEATURES: &[(&str, Status, &str)] = &[
    ("attribute", Status::Supported, "attribute selectors via stylo"),
    ("class", Status::Supported, "class selectors via stylo"),
    ("combinators", Status::Supported, "descendant, child, and sibling combinators via stylo"),
    ("id", Status::Supported, "id selectors via stylo"),
    (
        "structural-pseudo-classes",
        Status::Supported,
        ":first-child, :nth-of-type and related via stylo",
    ),
    ("type", Status::Supported, "element type selectors via stylo"),
];

/// Input formats accepted by `render --from`.
const INPUT_FORMATS: &[(&str, &str)] = &[
    ("html", "HTML documents (default for other extensions)"),
    ("markdown", "CommonMark + GFM (.md, .markdown, .mdown)"),
    (
        "template",
        "MiniJinja HTML template + JSON data (--data; schema via template-schema)",
    ),
];

fn hint<'a>(
    table: &'a [(&'a str, &'a str, Option<&'static str>)],
    name: &str,
) -> Option<&'a (&'a str, &'a str, Option<&'static str>)> {
    table.iter().find(|(n, _, _)| *n == name)
}

/// The status the diagnostics scanner implies for an element property.
pub fn property_status(name: &str) -> Status {
    let name = name.to_ascii_lowercase();
    if SUPPORTED_PROPERTIES.binary_search(&name.as_str()).is_ok() {
        Status::Supported
    } else {
        Status::Unsupported
    }
}

/// Look up an entry by id (`property:text-indent`). Properties and at-rules
/// not in any table resolve to an `unsupported` entry (the manifest's
/// `default_status`), mirroring the scanner.
pub fn lookup(id: &str) -> Option<Capability> {
    let (kind, name) = id.split_once(':')?;
    if let Some(c) = manifest().into_iter().find(|c| c.kind == kind && c.name == name) {
        return Some(c);
    }
    match kind {
        "property" | "at-rule" => Some(Capability {
            kind: if kind == "property" { "property" } else { "at-rule" },
            name: name.to_string(),
            status: Status::Unsupported,
            note: "not consumed by the engine".to_string(),
            alternative: None,
        }),
        _ => None,
    }
}

/// Build the full manifest, sorted by `(kind, name)`.
pub fn manifest() -> Vec<Capability> {
    let mut out = Vec::new();
    for p in SUPPORTED_PROPERTIES {
        out.push(Capability {
            kind: "property",
            name: (*p).to_string(),
            status: Status::Supported,
            note: "consumed by the cascade or a paged/break pass".to_string(),
            alternative: None,
        });
    }
    for (name, note, alt) in UNSUPPORTED_PROPERTY_HINTS {
        if property_status(name) == Status::Unsupported {
            out.push(Capability {
                kind: "property",
                name: (*name).to_string(),
                status: Status::Unsupported,
                note: (*note).to_string(),
                alternative: *alt,
            });
        }
    }
    for d in SUPPORTED_PAGE_DESCRIPTORS {
        out.push(Capability {
            kind: "page-descriptor",
            name: (*d).to_string(),
            status: Status::Supported,
            note: "honored inside `@page`".to_string(),
            alternative: None,
        });
    }
    for (value, msg) in DISPLAY_FALLBACKS {
        out.push(Capability {
            kind: "display",
            name: (*value).to_string(),
            status: Status::Partial,
            note: (*msg).to_string(),
            alternative: None,
        });
    }
    for r in SUPPORTED_AT_RULES {
        out.push(Capability {
            kind: "at-rule",
            name: (*r).to_string(),
            status: Status::Supported,
            note: "recognized at-rule".to_string(),
            alternative: None,
        });
    }
    for (name, note, alt) in UNSUPPORTED_AT_RULE_HINTS {
        if !SUPPORTED_AT_RULES.contains(name) {
            out.push(Capability {
                kind: "at-rule",
                name: (*name).to_string(),
                status: Status::Unsupported,
                note: (*note).to_string(),
                alternative: *alt,
            });
        }
    }
    for (name, status, note) in SELECTOR_FEATURES {
        out.push(Capability {
            kind: "selector",
            name: (*name).to_string(),
            status: *status,
            note: (*note).to_string(),
            alternative: None,
        });
    }
    for (name, note) in INPUT_FORMATS {
        out.push(Capability {
            kind: "input-format",
            name: (*name).to_string(),
            status: Status::Supported,
            note: (*note).to_string(),
            alternative: None,
        });
    }
    out.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
    out.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    out
}

/// Diagnostics hint: ` (capability property:X; alternative: Y)` for an
/// entry that carries an alternative, empty otherwise.
pub(crate) fn diagnostic_suffix(kind: &str, name: &str) -> String {
    let table = match kind {
        "property" => UNSUPPORTED_PROPERTY_HINTS,
        "at-rule" => UNSUPPORTED_AT_RULE_HINTS,
        _ => return String::new(),
    };
    match hint(table, name) {
        Some((_, _, Some(alt))) => format!(" (capability {kind}:{name}; alternative: {alt})"),
        _ => String::new(),
    }
}

/// Serialize the manifest as JSON. Deterministic: sorted entries, fixed key
/// order, no version-dependent fields.
pub fn to_json() -> String {
    let entries = manifest();
    let mut out = String::new();
    let _ = writeln!(out, "{{\n  \"schema\": {SCHEMA},");
    out.push_str("  \"default_status\": {\"property\": \"unsupported\", \"at-rule\": \"unsupported\"},\n");
    out.push_str("  \"capabilities\": [\n");
    for (i, c) in entries.iter().enumerate() {
        let comma = if i + 1 == entries.len() { "" } else { "," };
        let alt = match c.alternative {
            Some(a) => format!(", \"alternative\": {}", crate::diagnostics::json_string(a)),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "    {{\"id\": {}, \"kind\": {}, \"name\": {}, \"status\": {}, \"note\": {}{}}}{}",
            crate::diagnostics::json_string(&c.id()),
            crate::diagnostics::json_string(c.kind),
            crate::diagnostics::json_string(&c.name),
            crate::diagnostics::json_string(c.status.as_str()),
            crate::diagnostics::json_string(&c.note),
            alt,
            comma
        );
    }
    out.push_str("  ]\n}\n");
    out
}
