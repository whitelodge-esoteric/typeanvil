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
}

impl Dom {
    /// Parse an HTML document string into our arena tree.
    pub fn parse(html: &str) -> Result<Dom> {
        let rc_dom = parse_document(RcDom::default(), ParseOpts::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())?;

        let mut dom = Dom {
            nodes: Vec::new(),
            root: 0,
        };
        let root = dom.push(NodeKind::Root, None);
        dom.root = root;
        // rc_dom.document is the Document node; walk its children.
        let doc: Handle = rc_dom.document.clone();
        for child in doc.children.borrow().iter() {
            dom.lower(child, root);
        }
        Ok(dom)
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
    fn lower(&mut self, handle: &Handle, parent: NodeId) {
        match &handle.data {
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                let mut id = None;
                let mut classes = Vec::new();
                for attr in attrs.borrow().iter() {
                    let key = attr.name.local.as_ref();
                    let val = attr.value.to_string();
                    match key {
                        "id" => id = Some(val),
                        "class" => {
                            classes = val.split_whitespace().map(|s| s.to_string()).collect();
                        }
                        _ => {}
                    }
                }
                let node = self.push(
                    NodeKind::Element(Element { tag, id, classes }),
                    Some(parent),
                );
                self.nodes[parent].children.push(node);
                for child in handle.children.borrow().iter() {
                    self.lower(child, node);
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
                    self.lower(child, parent);
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
