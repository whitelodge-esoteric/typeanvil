// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal DOM: parse HTML with html5ever, then lower markup5ever's `RcDom`
//! into our own arena-based tree so the rest of the engine never touches
//! markup5ever types.

use anyhow::Result;
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, ParseOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Index into [`Dom::nodes`].
pub type NodeId = usize;

/// A single parsed element.
#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    /// All attributes in source order (`id`/`class` included), so paged-media
    /// features like `target-counter(attr(href), page)` can read arbitrary
    /// attributes deterministically.
    pub attrs: Vec<(String, String)>,
}

impl Element {
    /// The value of the named attribute, if present.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The kind of a DOM node we care about for the skeleton.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// The synthetic document root.
    Root,
    Element(Element),
    /// A run of text (whitespace-collapsed at layout time).
    Text(String),
}

impl NodeKind {
    /// The element payload, if this node is an element.
    pub fn element(&self) -> Option<&Element> {
        match self {
            NodeKind::Element(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

/// An owned DOM tree.
#[derive(Debug, Default)]
pub struct Dom {
    pub nodes: Vec<Node>,
    pub root: NodeId,
    /// 1-based HTML source `(line, column)` of each ELEMENT's start tag `<`,
    /// indexed by [`NodeId`] (CORE-571 render report). Column counts
    /// characters. `(0, 0)` = unknown (text nodes, the root, synthetic nodes
    /// such as implied `<html>`/`<body>` wrappers or anonymous table boxes).
    /// May be shorter than `nodes`; read it through [`Dom::source_pos`].
    pub source_lines: Vec<(u32, u32)>,
}

/// Element-creation lines from the parse plus the start-tag offsets already
/// claimed, so two same-named tags on one line get distinct columns.
struct SourceMap {
    lines: std::collections::HashMap<usize, u64>,
    line_starts: Vec<usize>,
    claimed: std::cell::RefCell<std::collections::BTreeSet<usize>>,
}

impl SourceMap {
    /// Find the first unclaimed `<tag` start on `line` (html5ever reports
    /// the line where the start tag ENDS, so a tag split over lines is
    /// searched on up to 8 preceding lines too). Returns 1-based
    /// `(line, char column)`.
    fn locate(&self, html: &str, line: u64, tag: &str) -> Option<(u32, u32)> {
        let line = line as usize;
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        let first = line.saturating_sub(8).max(1);
        let mut claimed = self.claimed.borrow_mut();
        for l in (first..=line).rev() {
            let start = self.line_starts[l - 1];
            let end = self.line_starts.get(l).copied().unwrap_or(html.len());
            let text = &html[start..end];
            let mut search = 0;
            while let Some(rel) = text[search..].find('<') {
                let at = search + rel;
                let after = &text[at + 1..];
                let matches = after.len() >= tag.len()
                    && after.is_char_boundary(tag.len())
                    && after[..tag.len()].eq_ignore_ascii_case(tag)
                    && after[tag.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_ascii_whitespace() || c == '>' || c == '/');
                if matches && claimed.insert(start + at) {
                    let col = text[..at].chars().count() + 1;
                    return Some((l as u32, col as u32));
                }
                search = at + 1;
            }
        }
        None
    }
}

/// html5ever reports only the tokenizer's current line, so this sink wraps
/// `RcDom` and records the line at which each element was created.
struct LineSink {
    inner: RcDom,
    line: std::cell::Cell<u64>,
    lines: std::cell::RefCell<std::collections::HashMap<usize, u64>>,
}

impl html5ever::interface::TreeSink for LineSink {
    type Handle = Handle;
    type Output = (RcDom, std::collections::HashMap<usize, u64>);
    type ElemName<'a>
        = html5ever::ExpandedName<'a>
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        (self.inner, self.lines.into_inner())
    }
    fn parse_error(&self, msg: std::borrow::Cow<'static, str>) {
        self.inner.parse_error(msg)
    }
    fn get_document(&self) -> Handle {
        self.inner.get_document()
    }
    fn elem_name<'a>(&'a self, target: &'a Handle) -> html5ever::ExpandedName<'a> {
        self.inner.elem_name(target)
    }
    fn create_element(
        &self,
        name: html5ever::QualName,
        attrs: Vec<html5ever::Attribute>,
        flags: html5ever::interface::ElementFlags,
    ) -> Handle {
        let h = self.inner.create_element(name, attrs, flags);
        let key = std::rc::Rc::as_ptr(&h) as usize;
        self.lines.borrow_mut().insert(key, self.line.get());
        h
    }
    fn create_comment(&self, text: html5ever::tendril::StrTendril) -> Handle {
        self.inner.create_comment(text)
    }
    fn create_pi(
        &self,
        target: html5ever::tendril::StrTendril,
        data: html5ever::tendril::StrTendril,
    ) -> Handle {
        self.inner.create_pi(target, data)
    }
    fn append(&self, parent: &Handle, child: html5ever::interface::NodeOrText<Handle>) {
        self.inner.append(parent, child)
    }
    fn append_based_on_parent_node(
        &self,
        element: &Handle,
        prev_element: &Handle,
        child: html5ever::interface::NodeOrText<Handle>,
    ) {
        self.inner.append_based_on_parent_node(element, prev_element, child)
    }
    fn append_doctype_to_document(
        &self,
        name: html5ever::tendril::StrTendril,
        public_id: html5ever::tendril::StrTendril,
        system_id: html5ever::tendril::StrTendril,
    ) {
        self.inner.append_doctype_to_document(name, public_id, system_id)
    }
    fn get_template_contents(&self, target: &Handle) -> Handle {
        self.inner.get_template_contents(target)
    }
    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        self.inner.same_node(x, y)
    }
    fn set_quirks_mode(&self, mode: html5ever::interface::QuirksMode) {
        self.inner.set_quirks_mode(mode)
    }
    fn append_before_sibling(
        &self,
        sibling: &Handle,
        new_node: html5ever::interface::NodeOrText<Handle>,
    ) {
        self.inner.append_before_sibling(sibling, new_node)
    }
    fn add_attrs_if_missing(&self, target: &Handle, attrs: Vec<html5ever::Attribute>) {
        self.inner.add_attrs_if_missing(target, attrs)
    }
    fn remove_from_parent(&self, target: &Handle) {
        self.inner.remove_from_parent(target)
    }
    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        self.inner.reparent_children(node, new_parent)
    }
    fn is_mathml_annotation_xml_integration_point(&self, handle: &Handle) -> bool {
        self.inner.is_mathml_annotation_xml_integration_point(handle)
    }
    fn set_current_line(&self, line_number: u64) {
        self.line.set(line_number);
    }
    fn maybe_clone_an_option_into_selectedcontent(&self, option: &Handle) {
        self.inner.maybe_clone_an_option_into_selectedcontent(option)
    }
}

impl Dom {
    /// Parse an HTML document string into our arena tree.
    pub fn parse(html: &str) -> Result<Dom> {
        let sink = LineSink {
            inner: RcDom::default(),
            line: std::cell::Cell::new(1),
            lines: std::cell::RefCell::new(std::collections::HashMap::new()),
        };
        let (rc_dom, lines) = parse_document(sink, ParseOpts::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())?;

        let mut line_starts = vec![0];
        for (i, b) in html.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        let map = SourceMap {
            lines,
            line_starts,
            claimed: std::cell::RefCell::new(std::collections::BTreeSet::new()),
        };

        let mut dom = Dom {
            nodes: Vec::new(),
            root: 0,
            source_lines: Vec::new(),
        };
        let root = dom.push(NodeKind::Root, None);
        dom.root = root;
        // rc_dom.document is the Document node; walk its children.
        let doc: Handle = rc_dom.document.clone();
        for child in doc.children.borrow().iter() {
            dom.lower(child, root, &map, html);
        }
        Ok(dom)
    }

    /// The 1-based HTML source `(line, column)` of an element's start tag,
    /// or `(0, 0)` when unknown (CORE-571).
    pub fn source_pos(&self, id: NodeId) -> (u32, u32) {
        self.source_lines.get(id).copied().unwrap_or((0, 0))
    }

    fn push(&mut self, kind: NodeKind, parent: Option<NodeId>) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            parent,
            children: Vec::new(),
        });
        id
    }

    /// Recursively lower a markup5ever handle under `parent`.
    fn lower(&mut self, handle: &Handle, parent: NodeId, lines: &SourceMap, html: &str) {
        match &handle.data {
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                let mut id = None;
                let mut classes = Vec::new();
                let mut all_attrs = Vec::new();
                for attr in attrs.borrow().iter() {
                    let key = attr.name.local.as_ref();
                    let val = attr.value.to_string();
                    match key {
                        "id" => id = Some(val.clone()),
                        "class" => {
                            classes = val.split_whitespace().map(|s| s.to_string()).collect();
                        }
                        _ => {}
                    }
                    all_attrs.push((key.to_string(), val));
                }
                let node = self.push(
                    NodeKind::Element(Element {
                        tag,
                        id,
                        classes,
                        attrs: all_attrs,
                    }),
                    Some(parent),
                );
                self.nodes[parent].children.push(node);
                let key = std::rc::Rc::as_ptr(handle) as usize;
                if let Some(&line) = lines.lines.get(&key) {
                    if let Some(pos) = lines.locate(html, line, &name.local) {
                        if self.source_lines.len() <= node {
                            self.source_lines.resize(node + 1, (0, 0));
                        }
                        self.source_lines[node] = pos;
                    }
                }
                for child in handle.children.borrow().iter() {
                    self.lower(child, node, lines, html);
                }
            }
            NodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                let node = self.push(NodeKind::Text(text), Some(parent));
                self.nodes[parent].children.push(node);
            }
            // Document / Doctype / Comment / PI: descend but do not materialize.
            _ => {
                for child in handle.children.borrow().iter() {
                    self.lower(child, parent, lines, html);
                }
            }
        }
    }

    /// Find the first element with the given tag name (depth-first), if any.
    pub fn find_tag(&self, tag: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| match &n.kind {
            NodeKind::Element(e) => e.tag == tag,
            _ => false,
        })
    }

    /// Collect the concatenated text content of a node's subtree.
    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        match &self.nodes[id].kind {
            NodeKind::Text(t) => out.push_str(t),
            _ => {
                for &c in &self.nodes[id].children {
                    self.collect_text(c, out);
                }
            }
        }
    }
}
