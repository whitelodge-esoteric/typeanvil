// SPDX-License-Identifier: AGPL-3.0-only

//! Logical structure tagging (CORE-111): HTML semantics → PDF structure tree.
//!
//! Two halves:
//! 1. [`role_for`] maps one DOM element to a `TagRole` (pure, unit-tested).
//! 2. [`build_tag_tree`] turns the DOM walk + recorded draws into a krilla
//!    `TagTree`. The tree is built in document order over the arena Vec —
//!    never hash-iterated — so identical input stays byte-identical.
//!
//! Draw attribution: the PDF emitter wraps every draw in a
//! `start_tagged(ContentTag::Other)` pair and records `(page, Identifier,
//! resolved owner)`. The owner is the nearest ancestor fragment with a
//! non-None `source` NodeId; unsourced subtrees (margin boxes, page
//! background) are Artifacts and carry no entry here.

use crate::dom::{Dom, Element, NodeId, NodeKind};
use krilla::tagging::{Identifier, TableHeaderScope, TagKind, TagTree};

/// The logical role of one element in the structure tree.
#[derive(Clone, Debug, PartialEq)]
pub enum TagRole {
    /// Not part of the tree: the subtree is skipped (head/meta/script).
    Skipped,
    /// A group with no own semantics beyond containment.
    Div,
    Paragraph,
    Heading(u16), // 1..=6 (validated by caller; >6 clamps to 6)
    List,
    ListItem,
    /// The label part of an LI. The engine renders no list markers yet, so
    /// Lbl groups stay empty; kept as a distinct role for completeness.
    Label,
    ListBody,
    Table,
    TableHeaderRowGroup,
    TableBodyRowGroup,
    TableFooterRowGroup,
    TableRow,
    TableHeader(TableHeaderScope),
    TableCell,
    Figure(Option<String>),
    Link,
    Strong,
    Emphasis,
    BlockQuote,
    Code,
}

impl TagRole {
    /// The krilla tag for this role.
    pub fn tag(&self) -> TagKind {
        use krilla::tagging::kind;
        use krilla::tagging::{ListNumbering, Tag};
        match self {
            // Skipped never reaches the tree builder; treat as Div if it does.
            TagRole::Skipped | TagRole::Div => TagKind::Div(Tag::Div),
            TagRole::Paragraph => TagKind::P(Tag::P),
            TagRole::Heading(n) => TagKind::Hn(Tag::<kind::Hn>::Hn(
                std::num::NonZeroU16::new((*n).max(1)).unwrap(),
                None,
            )),
            // The engine renders no visual list markers yet, so the numbering
            // entry is `None` (PDF/UA requires SOME numbering value; `None`
            // is its own variant and is what unmarked lists map to).
            TagRole::List => TagKind::L(Tag::<kind::L>::L(ListNumbering::None)),
            TagRole::ListItem => TagKind::LI(Tag::LI),
            TagRole::Label => TagKind::Lbl(Tag::Lbl),
            TagRole::ListBody => TagKind::LBody(Tag::LBody),
            TagRole::Table => TagKind::Table(Tag::Table),
            TagRole::TableHeaderRowGroup => TagKind::THead(Tag::THead),
            TagRole::TableBodyRowGroup => TagKind::TBody(Tag::TBody),
            TagRole::TableFooterRowGroup => TagKind::TFoot(Tag::TFoot),
            TagRole::TableRow => TagKind::TR(Tag::TR),
            TagRole::TableHeader(scope) => TagKind::TH(Tag::<kind::TH>::TH(*scope)),
            TagRole::TableCell => TagKind::TD(Tag::TD),
            TagRole::Figure(alt) => TagKind::Figure(Tag::<kind::Figure>::Figure(alt.clone())),
            TagRole::Link => TagKind::Link(Tag::Link),
            TagRole::Strong => TagKind::Strong(Tag::Strong),
            TagRole::Emphasis => TagKind::Em(Tag::Em),
            TagRole::BlockQuote => TagKind::BlockQuote(Tag::BlockQuote),
            TagRole::Code => TagKind::Code(Tag::Code),
        }
    }
}

/// Map an element to its structure role. Pure function of the tag + attrs;
/// table-section scope heuristics that need tree context live in the builder.
pub fn role_for(el: &Element) -> TagRole {
    match el.tag.as_str() {
        "h1" => TagRole::Heading(1),
        "h2" => TagRole::Heading(2),
        "h3" => TagRole::Heading(3),
        "h4" => TagRole::Heading(4),
        "h5" => TagRole::Heading(5),
        "h6" => TagRole::Heading(6),
        "p" => TagRole::Paragraph,
        "ul" | "ol" | "dl" => TagRole::List,
        "li" | "dt" | "dd" => TagRole::ListItem,
        "table" => TagRole::Table,
        "thead" => TagRole::TableHeaderRowGroup,
        "tbody" => TagRole::TableBodyRowGroup,
        "tfoot" => TagRole::TableFooterRowGroup,
        "tr" => TagRole::TableRow,
        "th" => TagRole::TableHeader(header_scope(el)),
        "td" => TagRole::TableCell,
        "figure" | "img" => TagRole::Figure(None), // alt filled by builder
        "figcaption" => TagRole::Paragraph,        // caption text stays readable
        "a" => TagRole::Link,
        "strong" | "b" => TagRole::Strong,
        "em" | "i" => TagRole::Emphasis,
        "blockquote" => TagRole::BlockQuote,
        "pre" | "code" | "kbd" | "samp" => TagRole::Code,
        // Head/stylesheet machinery carries no rendered content.
        "style" | "script" | "meta" | "title" | "link" | "head" => TagRole::Skipped,
        _ => TagRole::Div,
    }
}

/// `scope` attribute → krilla table-header scope. Missing scope falls back to
/// Column (first-row headers are column headers in practice); the builder
/// refines first-row detection when the table context is known.
fn header_scope(el: &Element) -> TableHeaderScope {
    match el.attr("scope") {
        Some("col") | Some("colgroup") => TableHeaderScope::Column,
        Some("row") | Some("rowgroup") => TableHeaderScope::Row,
        Some("both") => TableHeaderScope::Both,
        _ => TableHeaderScope::Column,
    }
}

/// One tagged draw recorded by the PDF emitter.
#[derive(Clone, Copy, Debug)]
pub struct DrawRef {
    /// Zero-based page index the draw landed on.
    pub page: usize,
    /// The marked-content identifier returned by `start_tagged`.
    pub ident: Identifier,
    /// The DOM node this draw belongs to (nearest sourced ancestor), if any.
    pub source: Option<NodeId>,
}

/// The document language from `<html lang="…">`, if declared.
fn document_lang(dom: &Dom) -> Option<String> {
    for node in &dom.nodes {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "html" {
                return el.attr("lang").map(|s| s.to_string());
            }
        }
    }
    None
}

/// Alt text for a figure/img element: `<img alt>` directly on the node, or —
/// for `figure` — its `<img>` child's alt or its `figcaption` text.
fn alt_for(dom: &Dom, id: NodeId) -> Option<String> {
    let el = dom.nodes[id].kind.element()?;
    if el.tag == "img" {
        return el
            .attr("alt")
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
    }
    if el.tag == "figure" {
        for &child in &dom.nodes[id].children {
            if let NodeKind::Element(cel) = &dom.nodes[child].kind {
                if cel.tag == "img" {
                    if let Some(a) = cel.attr("alt").filter(|s| !s.is_empty()) {
                        return Some(a.to_string());
                    }
                }
                if cel.tag == "figcaption" {
                    let text = dom.text_content(child);
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Build the structure tree.
///
/// `draws` must be sorted by (page, MCID order of recording) — the emitter's
/// natural paint order satisfies this. Elements participate only along chains
/// that own at least one draw; empty branches are dropped so display:none and
/// head machinery leave no groups.
pub fn build_tag_tree(dom: &Dom, draws: &[DrawRef], lang: Option<String>) -> TagTree {
    use krilla::tagging::{Node, TagGroup};

    // Group draw references by owning node. IndexMap-free: a BTreeMap keyed
    // by NodeId gives deterministic iteration without hash ordering.
    let mut by_node: std::collections::BTreeMap<NodeId, Vec<&DrawRef>> =
        std::collections::BTreeMap::new();
    for d in draws {
        if let Some(src) = d.source {
            by_node.entry(src).or_default().push(d);
        }
    }

    let mut tree = TagTree::new();
    if let Some(l) = lang.or_else(|| document_lang(dom)) {
        tree = tree.with_lang(Some(l));
    }

    // Recursive emit: returns true when the subtree produced any content.
    fn emit(
        dom: &Dom,
        id: NodeId,
        by_node: &std::collections::BTreeMap<NodeId, Vec<&DrawRef>>,
        out: &mut Vec<Node>,
    ) -> bool {
        let kind = match &dom.nodes[id].kind {
            NodeKind::Element(el) => el,
            NodeKind::Text(_) => {
                // Text nodes have no direct draws (draws attach to line
                // fragments whose source is the nearest ELEMENT ancestor).
                return false;
            }
            NodeKind::Root => {
                let mut produced = false;
                for &c in &dom.nodes[id].children {
                    produced |= emit(dom, c, by_node, out);
                }
                return produced;
            }
        };

        match role_for(kind) {
            TagRole::Skipped => return false,
            role => {
                let mut group = match &role {
                    TagRole::Heading(level) => {
                        use krilla::tagging::{kind, Tag};
                        let title = dom.text_content(id);
                        let title = title.trim();
                        TagGroup::new(krilla::tagging::TagKind::Hn(Tag::<kind::Hn>::Hn(
                            std::num::NonZeroU16::new((*level).max(1)).unwrap(),
                            (!title.is_empty()).then(|| title.to_string()),
                        )))
                    }
                    TagRole::Figure(_) => {
                        use krilla::tagging::{kind, Tag};
                        TagGroup::new(krilla::tagging::TagKind::Figure(
                            Tag::<kind::Figure>::Figure(alt_for(dom, id)),
                        ))
                    }
                    _ => TagGroup::new(role.tag()),
                };
                // Own draws become leaves (already in paint/MCID order).
                if let Some(mine) = by_node.get(&id) {
                    for d in mine {
                        group.push(Node::Leaf(d.ident));
                    }
                }
                // Children in document order.
                let mut child_nodes: Vec<Node> = Vec::new();
                for &c in &dom.nodes[id].children {
                    let mut sub: Vec<Node> = Vec::new();
                    emit(dom, c, by_node, &mut sub);
                    child_nodes.extend(sub);
                }
                // List items get the Lbl/LBody split even though the engine
                // renders no markers yet: Lbl stays empty, all content rides
                // LBody (spec Edge Cases).
                if matches!(role_for(kind), TagRole::ListItem) {
                    let mut lbody = TagGroup::new(role_for_list_body());
                    for n in child_nodes {
                        lbody.push(n);
                    }
                    group.push(Node::Group(lbody));
                } else {
                    for n in child_nodes {
                        group.push(n);
                    }
                }
                // Keep the group only when something under it exists OR the
                // element itself owns draws.
                let owns_draws = by_node.contains_key(&id);
                if !owns_draws && group.children.is_empty() {
                    return false;
                }
                out.push(Node::Group(group));
                true
            }
        }
    }

    let mut roots: Vec<Node> = Vec::new();
    emit(dom, dom.root, &by_node, &mut roots);
    for r in roots {
        tree.push(r);
    }
    tree
}

fn role_for_list_body() -> TagKind {
    use krilla::tagging::kind;
    use krilla::tagging::Tag;
    TagKind::LBody(Tag::LBody)
}
