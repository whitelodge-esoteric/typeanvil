// SPDX-License-Identifier: AGPL-3.0-only

//! Servo stylo DOM trait family over Typeanvil's arena DOM.
//!
//! Implements the `TDocument` / `TNode` / `TElement` / `TShadowRoot` traits
//! (from `style::dom`) plus `selectors::Element` for the [`crate::dom::Dom`]
//! tree, so the stylo engine can match selectors and cascade against it.
//!
//! Design notes:
//!
//! - [`TyElement`] is a `Copy` value of `(NodeId, &TyBackend)`. Node ids are
//!   assigned in pre-order at parse time, so parents always precede children;
//!   `cascade` resolves elements in index order and reads the parent's
//!   computed values out of the per-element [`ElementData`] stored in
//!   [`TyBackend`].
//! - Atom interning happens eagerly at backend construction: tag names go into
//!   the `web_atoms` local-name set, ids into the `stylo_atoms` set, classes
//!   into `AtomIdent`. `web_atoms::ns!` is not public API (CORE-56 ground
//!   truth), so namespaces are built with `Namespace::from(URL)` directly.
//! - The element-data map sits behind `UnsafeCell`; the `AtomicRefCell` inside
//!   `ElementDataWrapper` provides the (debug) borrow checks, mirroring how
//!   Servo embeds this in its DOM.
//! - Minimal surface per CORE-56 scope: element/class/id/descendant
//!   selectors, inheritance of color/font-*, non-inherited display/box
//!   properties. No shadow DOM (always `None`), no attribute selectors
//!   (`attr_matches` → `false`), no style attribute, no animations.

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

use app_units::Au;
use dom::ElementState;
use euclid::default::Size2D as UntypedSize2D;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::matching::{ElementSelectorFlags, MatchingContext};
use selectors::{Element as SelectorsElement, OpaqueElement};
use servo_arc::{Arc, ArcBorrow};
use style::data::{ElementDataMut, ElementDataRef, ElementDataWrapper};
use style::dom::{LayoutIterator, NodeInfo, TDocument, TElement, TNode, TShadowRoot};
use style::properties::PropertyDeclarationBlock;
use style::selector_parser::{AttrValue, Lang, PseudoElement, SelectorImpl};
use style::shared_lock::{Locked, SharedRwLock};
use style::stylesheets::UrlExtraData;
use style::stylist::CascadeData;
use style::values::computed::Display;
use style::values::AtomIdent;
use style::LocalName;
use style::Namespace;
use stylo_atoms::Atom;
use style_traits::dom::OpaqueNode;

use crate::dom::{Dom, NodeId, NodeKind};

/// The HTML namespace URL. All elements in our (html5ever-parsed) tree live
/// in this namespace.
const NS_HTML: &str = "http://www.w3.org/1999/xhtml";

/// The per-document state the trait family operates over.
///
/// Owns the element-data arena (populated as elements get styled), the
/// interned atoms for ids / tags / classes, and a reference to the shared
/// stylesheet lock (needed by `TDocument::shared_lock`).
pub struct TyBackend<'a> {
    /// The DOM tree being styled.
    pub dom: &'a Dom,
    /// The shared lock used by the stylesheets.
    pub lock: &'a SharedRwLock,
    /// The single namespace every element lives in (HTML).
    html_ns: string_cache::Atom<web_atoms::NamespaceStaticSet>,
    /// Per-element parsed `style=""` declarations (CORE-126). `None` for
    /// elements without the attribute (the common case — zero cost).
    style_attrs: HashMap<NodeId, Arc<Locked<PropertyDeclarationBlock>>>,
    /// Element tag names, interned into the web_atoms local-name set.
    local_names: HashMap<NodeId, string_cache::Atom<web_atoms::LocalNameStaticSet>>,
    /// Element ids, interned into the stylo atoms set.
    ids: HashMap<NodeId, Atom>,
    /// Element classes, interned as `AtomIdent`s (in document order).
    classes: HashMap<NodeId, Vec<AtomIdent>>,
    /// Per-element stylo style data (lazily allocated during the cascade).
    data: UnsafeCell<HashMap<NodeId, ElementDataWrapper>>,
}

impl<'a> TyBackend<'a> {
    /// Build the backend, interning every element's tag/id/classes up front,
    /// and parsing every `style=""` attribute through stylo's real style-
    /// attribute parser (CORE-126). Empty/absent attributes store nothing.
    pub fn new(dom: &'a Dom, lock: &'a SharedRwLock) -> Self {
        let mut local_names = HashMap::new();
        let mut ids = HashMap::new();
        let mut classes = HashMap::new();
        let mut style_attrs = HashMap::new();
        for (id, node) in dom.nodes.iter().enumerate() {
            let NodeKind::Element(el) = &node.kind else { continue };
            local_names.insert(id, string_cache::Atom::from(el.tag.as_str()));
            if let Some(id_attr) = &el.id {
                ids.insert(id, Atom::from(id_attr.as_str()));
            }
            classes.insert(
                id,
                el.classes.iter().map(|c| AtomIdent::from(c.as_str())).collect(),
            );
            // Inline style seam (CORE-126): stylo parses the full declaration
            // list (every property it supports), so inline declarations now
            // participate in the cascade like a browser. WPT print-reftest
            // refs use inline geometry/backgrounds pervasively.
            if let Some(style_attr) = el.attr("style") {
                let trimmed = style_attr.trim();
                if !trimmed.is_empty() {
                    let url_data =
                        UrlExtraData(Arc::new(url::Url::parse("http://localhost/").unwrap()));
                    let block = style::properties::parse_style_attribute(
                        trimmed,
                        &url_data,
                        None,
                        style::context::QuirksMode::NoQuirks,
                        style::stylesheets::CssRuleType::Style,
                    );
                    style_attrs.insert(id, Arc::new(lock.wrap(block)));
                }
            }
        }
        TyBackend {
            dom,
            lock,
            html_ns: string_cache::Atom::from(NS_HTML),
            style_attrs,
            local_names,
            ids,
            classes,
            data: UnsafeCell::new(HashMap::new()),
        }
    }

    fn data(&self) -> &HashMap<NodeId, ElementDataWrapper> {
        // Safety: `&self` is the only handle; access is single-threaded (the
        // cascade is one thread) and the wrappers do their own borrow checks.
        unsafe { &*self.data.get() }
    }

    fn data_mut(&self) -> &mut HashMap<NodeId, ElementDataWrapper> {
        // Safety: as above; no other reference to the map is live.
        unsafe { &mut *self.data.get() }
    }

    /// The element data wrapper for a node id, creating it if absent.
    fn wrapper(&self, id: NodeId) -> &ElementDataWrapper {
        self.data_mut().entry(id).or_default()
    }
}

// ---------------------------------------------------------------------------
// NodeInfo
// ---------------------------------------------------------------------------

impl<'a> NodeInfo for TyNode<'a> {
    fn is_element(&self) -> bool {
        matches!(self.backend.dom.nodes[self.id].kind, NodeKind::Element(_))
    }

    fn is_text_node(&self) -> bool {
        matches!(self.backend.dom.nodes[self.id].kind, NodeKind::Text(_))
    }
}

// ---------------------------------------------------------------------------
// TyNode
// ---------------------------------------------------------------------------

/// A stylo view of one DOM node.
#[derive(Clone, Copy)]
pub struct TyNode<'a> {
    id: NodeId,
    backend: &'a TyBackend<'a>,
}

impl<'a> TyNode<'a> {
    pub fn new(id: NodeId, backend: &'a TyBackend<'a>) -> Self {
        TyNode { id, backend }
    }
}

impl<'a> PartialEq for TyNode<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> fmt::Debug for TyNode<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TyNode({})", self.id)
    }
}

impl<'a> TNode for TyNode<'a> {
    type ConcreteElement = TyElement<'a>;
    type ConcreteDocument = TyDocument<'a>;
    type ConcreteShadowRoot = TyShadowRoot<'a>;

    fn parent_node(&self) -> Option<Self> {
        self.backend.dom.nodes[self.id]
            .parent
            .map(|p| TyNode::new(p, self.backend))
    }

    fn first_child(&self) -> Option<Self> {
        self.backend.dom.nodes[self.id]
            .children
            .first()
            .map(|&c| TyNode::new(c, self.backend))
    }

    fn last_child(&self) -> Option<Self> {
        self.backend.dom.nodes[self.id]
            .children
            .last()
            .map(|&c| TyNode::new(c, self.backend))
    }

    fn prev_sibling(&self) -> Option<Self> {
        self.sibling(-1)
    }

    fn next_sibling(&self) -> Option<Self> {
        self.sibling(1)
    }

    fn owner_doc(&self) -> Self::ConcreteDocument {
        TyDocument::new(self.backend)
    }

    fn is_in_document(&self) -> bool {
        true
    }

    fn traversal_parent(&self) -> Option<Self::ConcreteElement> {
        let mut cur = self.backend.dom.nodes[self.id].parent;
        while let Some(p) = cur {
            if matches!(self.backend.dom.nodes[p].kind, NodeKind::Element(_)) {
                return Some(TyElement::new(p, self.backend));
            }
            cur = self.backend.dom.nodes[p].parent;
        }
        None
    }

    fn opaque(&self) -> OpaqueNode {
        OpaqueNode(self.id)
    }

    fn debug_id(self) -> usize {
        self.id
    }

    fn as_element(&self) -> Option<Self::ConcreteElement> {
        match self.backend.dom.nodes[self.id].kind {
            NodeKind::Element(_) => Some(TyElement::new(self.id, self.backend)),
            _ => None,
        }
    }

    fn as_document(&self) -> Option<Self::ConcreteDocument> {
        (self.id == self.backend.dom.root).then(|| TyDocument::new(self.backend))
    }

    fn as_shadow_root(&self) -> Option<Self::ConcreteShadowRoot> {
        None
    }
}

impl<'a> TyNode<'a> {
    /// The adjacent sibling in direction `delta` (-1 or +1).
    fn sibling(&self, delta: isize) -> Option<Self> {
        let node = &self.backend.dom.nodes[self.id];
        let parent = node.parent?;
        let siblings = &self.backend.dom.nodes[parent].children;
        let pos = siblings.iter().position(|&c| c == self.id)? as isize;
        let next = pos + delta;
        (next >= 0 && (next as usize) < siblings.len()).then(|| {
            TyNode::new(siblings[next as usize], self.backend)
        })
    }

    /// The nearest ELEMENT sibling in direction `delta` (-1 or +1), skipping
    /// text nodes. `selectors` drives `:nth-of-type` / `:nth-last-of-type` /
    /// adjacent-sibling matching through this; returning a raw text sibling
    /// would abort the walk, so any whitespace between element siblings broke
    /// structural pseudo-classes (CORE-155).
    fn sibling_element(&self, delta: isize) -> Option<TyElement<'a>> {
        let mut cur = self.sibling(delta);
        while let Some(n) = cur {
            if let Some(el) = n.as_element() {
                return Some(el);
            }
            cur = n.sibling(delta);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// TyElement
// ---------------------------------------------------------------------------

/// A stylo view of one element.
#[derive(Clone, Copy)]
pub struct TyElement<'a> {
    id: NodeId,
    backend: &'a TyBackend<'a>,
}

impl<'a> TyElement<'a> {
    pub fn new(id: NodeId, backend: &'a TyBackend<'a>) -> Self {
        TyElement { id, backend }
    }

    fn tag(&self) -> &str {
        match &self.backend.dom.nodes[self.id].kind {
            NodeKind::Element(el) => &el.tag,
            _ => unreachable!("TyElement over a non-element node"),
        }
    }
}

impl<'a> PartialEq for TyElement<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> Eq for TyElement<'a> {}

impl<'a> Hash for TyElement<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<'a> fmt::Debug for TyElement<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TyElement({}, <{}>)", self.id, self.tag())
    }
}

impl<'a> TElement for TyElement<'a> {
    type ConcreteNode = TyNode<'a>;
    type TraversalChildrenIterator = std::vec::IntoIter<TyNode<'a>>;

    fn as_node(&self) -> Self::ConcreteNode {
        TyNode::new(self.id, self.backend)
    }

    fn traversal_children(&self) -> LayoutIterator<Self::TraversalChildrenIterator> {
        let children: Vec<TyNode<'a>> = self.backend.dom.nodes[self.id]
            .children
            .iter()
            .map(|&c| TyNode::new(c, self.backend))
            .collect();
        LayoutIterator(children.into_iter())
    }

    fn is_html_element(&self) -> bool {
        true
    }

    fn is_mathml_element(&self) -> bool {
        false
    }

    fn is_svg_element(&self) -> bool {
        false
    }

    fn style_attribute(&self) -> Option<ArcBorrow<'_, Locked<PropertyDeclarationBlock>>> {
        self.backend
            .style_attrs
            .get(&self.id)
            .map(|arc| arc.borrow_arc())
    }

    fn animation_rule(
        &self,
        _context: &style::context::SharedStyleContext,
    ) -> Option<Arc<Locked<PropertyDeclarationBlock>>> {
        None
    }

    fn transition_rule(
        &self,
        _context: &style::context::SharedStyleContext,
    ) -> Option<Arc<Locked<PropertyDeclarationBlock>>> {
        None
    }

    fn state(&self) -> ElementState {
        ElementState::empty()
    }

    fn has_part_attr(&self) -> bool {
        false
    }

    fn exports_any_part(&self) -> bool {
        false
    }

    fn id(&self) -> Option<&Atom> {
        self.backend.ids.get(&self.id)
    }

    fn each_class<F>(&self, mut callback: F)
    where
        F: FnMut(&AtomIdent),
    {
        if let Some(classes) = self.backend.classes.get(&self.id) {
            for class in classes {
                callback(class);
            }
        }
    }

    fn each_custom_state<F>(&self, _callback: F)
    where
        F: FnMut(&AtomIdent),
    {
    }

    fn each_attr_name<F>(&self, _callback: F)
    where
        F: FnMut(&LocalName),
    {
    }

    fn has_dirty_descendants(&self) -> bool {
        false
    }

    fn has_snapshot(&self) -> bool {
        false
    }

    fn handled_snapshot(&self) -> bool {
        false
    }

    unsafe fn set_handled_snapshot(&self) {}

    unsafe fn set_dirty_descendants(&self) {}

    unsafe fn unset_dirty_descendants(&self) {}

    fn store_children_to_process(&self, _n: isize) {}

    fn did_process_child(&self) -> isize {
        0
    }

    unsafe fn ensure_data(&self) -> ElementDataMut<'_> {
        self.backend.wrapper(self.id).borrow_mut()
    }

    unsafe fn clear_data(&self) {
        self.backend.data_mut().remove(&self.id);
    }

    fn has_data(&self) -> bool {
        self.backend.data().contains_key(&self.id)
    }

    fn borrow_data(&self) -> Option<ElementDataRef<'_>> {
        self.backend
            .data()
            .get(&self.id)
            .map(|wrapper| wrapper.borrow())
    }

    fn mutate_data(&self) -> Option<ElementDataMut<'_>> {
        self.backend
            .data()
            .get(&self.id)
            .map(|wrapper| wrapper.borrow_mut())
    }

    fn skip_item_display_fixup(&self) -> bool {
        false
    }

    fn may_have_animations(&self) -> bool {
        false
    }

    fn has_animations(&self, _context: &style::context::SharedStyleContext) -> bool {
        false
    }

    fn has_css_animations(
        &self,
        _context: &style::context::SharedStyleContext,
        _pseudo_element: Option<PseudoElement>,
    ) -> bool {
        false
    }

    fn has_css_transitions(
        &self,
        _context: &style::context::SharedStyleContext,
        _pseudo_element: Option<PseudoElement>,
    ) -> bool {
        false
    }

    fn shadow_root(&self) -> Option<<Self::ConcreteNode as TNode>::ConcreteShadowRoot> {
        None
    }

    fn containing_shadow(&self) -> Option<<Self::ConcreteNode as TNode>::ConcreteShadowRoot> {
        None
    }

    fn lang_attr(&self) -> Option<AttrValue> {
        None
    }

    fn match_element_lang(&self, _override_lang: Option<Option<AttrValue>>, _value: &Lang) -> bool {
        false
    }

    fn is_html_document_body_element(&self) -> bool {
        if self.tag() != "body" {
            return false;
        }
        self.as_node()
            .parent_node()
            .is_some_and(|p| matches!(p.as_element(), Some(e) if e.tag() == "html"))
    }

    fn synthesize_presentational_hints_for_legacy_attributes<V>(
        &self,
        _visited_handling: selectors::matching::VisitedHandlingMode,
        _hints: &mut V,
    ) where
        V: selectors::sink::Push<style::applicable_declarations::ApplicableDeclarationBlock>,
    {
    }

    fn local_name(&self) -> &string_cache::Atom<web_atoms::LocalNameStaticSet> {
        self.backend
            .local_names
            .get(&self.id)
            .expect("element tag not interned")
    }

    fn namespace(&self) -> &string_cache::Atom<web_atoms::NamespaceStaticSet> {
        &self.backend.html_ns
    }

    fn query_container_size(
        &self,
        _display: &Display,
    ) -> UntypedSize2D<Option<Au>> {
        UntypedSize2D::new(None, None)
    }

    fn has_selector_flags(&self, _flags: ElementSelectorFlags) -> bool {
        false
    }

    fn relative_selector_search_direction(&self) -> ElementSelectorFlags {
        ElementSelectorFlags::empty()
    }

    fn get_attr(&self, attr: &LocalName, namespace: &Namespace) -> Option<String> {
        if namespace.0 != self.backend.html_ns {
            return None;
        }
        // The seam only carries id/class; the style attribute is not stored.
        match attr.to_string().as_str() {
            "id" => self.backend.dom.nodes[self.id]
                .kind
                .as_element_attr(|el| el.id.clone())
                .flatten(),
            "class" => self
                .backend
                .dom
                .nodes
                .get(self.id)
                .and_then(|n| match &n.kind {
                    NodeKind::Element(el) => {
                        Some(if el.classes.is_empty() {
                            None
                        } else {
                            Some(el.classes.join(" "))
                        })
                    }
                    _ => None,
                })
                .flatten(),
            _ => None,
        }
    }
}

impl<'a> SelectorsElement for TyElement<'a> {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        // Node ids are unique per document and non-null once offset (the root
        // has id 0). Identity only; never dereferenced.
        OpaqueElement::from_non_null_ptr(
            NonNull::new((self.id as usize + 1) as *mut ()).expect("id + 1 is never null"),
        )
    }

    fn parent_element(&self) -> Option<Self> {
        self.as_node().parent_element()
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.as_node().sibling_element(-1)
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.as_node().sibling_element(1)
    }

    fn first_element_child(&self) -> Option<Self> {
        self.backend.dom.nodes[self.id]
            .children
            .iter()
            .find_map(|&c| match &self.backend.dom.nodes[c].kind {
                NodeKind::Element(_) => Some(TyElement::new(c, self.backend)),
                _ => None,
            })
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(
        &self,
        local_name: &string_cache::Atom<web_atoms::LocalNameStaticSet>,
    ) -> bool {
        self.local_name() == local_name
    }

    fn has_namespace(&self, ns: &string_cache::Atom<web_atoms::NamespaceStaticSet>) -> bool {
        self.namespace() == ns
    }

    fn is_same_type(&self, other: &Self) -> bool {
        self.has_local_name(other.local_name()) && self.has_namespace(other.namespace())
    }

    fn attr_matches(
        &self,
        _ns: &NamespaceConstraint<&Namespace>,
        _local_name: &LocalName,
        _operation: &AttrSelectorOperation<&AttrValue>,
    ) -> bool {
        // Attribute selectors are out of CORE-56 scope.
        false
    }

    fn match_non_ts_pseudo_class(
        &self,
        _pc: &style::selector_parser::NonTSPseudoClass,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {}

    fn is_link(&self) -> bool {
        false
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(
        &self,
        id: &<Self::Impl as selectors::parser::SelectorImpl>::Identifier,
        case_sensitivity: CaseSensitivity,
    ) -> bool {
        let Some(actual) = self.backend.ids.get(&self.id) else {
            return false;
        };
        match case_sensitivity {
            CaseSensitivity::CaseSensitive => actual == &id.0,
            CaseSensitivity::AsciiCaseInsensitive => actual.eq_ignore_ascii_case(&id.0),
        }
    }

    fn has_class(
        &self,
        name: &<Self::Impl as selectors::parser::SelectorImpl>::Identifier,
        case_sensitivity: CaseSensitivity,
    ) -> bool {
        self.backend.classes.get(&self.id).is_some_and(|classes| {
            classes.iter().any(|class| match case_sensitivity {
                CaseSensitivity::CaseSensitive => &class.0 == &name.0,
                CaseSensitivity::AsciiCaseInsensitive => class.0.eq_ignore_ascii_case(&name.0),
            })
        })
    }

    fn has_custom_state(&self, _name: &AtomIdent) -> bool {
        false
    }

    fn imported_part(&self, _name: &AtomIdent) -> Option<AtomIdent> {
        None
    }

    fn is_part(&self, _name: &AtomIdent) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.backend.dom.nodes[self.id].children.iter().all(|&c| {
            match &self.backend.dom.nodes[c].kind {
                NodeKind::Element(_) => false,
                NodeKind::Text(t) => t.trim().is_empty(),
                NodeKind::Root => true,
            }
        })
    }

    fn is_root(&self) -> bool {
        matches!(
            self.as_node().parent_node().map(|p| &self.backend.dom.nodes[p.id].kind),
            Some(NodeKind::Root)
        )
    }

    fn add_element_unique_hashes(&self, filter: &mut BloomFilter) -> bool {
        let mut added = false;
        if let Some(id) = self.id() {
            filter.insert_hash(id.get_hash());
            added = true;
        }
        self.each_class(|class| {
            filter.insert_hash(class.get_hash());
            added = true;
        });
        added
    }
}


// ---------------------------------------------------------------------------
// TyDocument / TyShadowRoot
// ---------------------------------------------------------------------------

/// A stylo view of the document (the synthetic root node).
#[derive(Clone, Copy)]
pub struct TyDocument<'a> {
    backend: &'a TyBackend<'a>,
}

impl<'a> TyDocument<'a> {
    fn new(backend: &'a TyBackend<'a>) -> Self {
        TyDocument { backend }
    }
}

impl<'a> TDocument for TyDocument<'a> {
    type ConcreteNode = TyNode<'a>;

    fn as_node(&self) -> Self::ConcreteNode {
        TyNode::new(self.backend.dom.root, self.backend)
    }

    fn is_html_document(&self) -> bool {
        true
    }

    fn quirks_mode(&self) -> selectors::matching::QuirksMode {
        selectors::matching::QuirksMode::NoQuirks
    }

    fn shared_lock(&self) -> &SharedRwLock {
        self.backend.lock
    }
}

/// A stylo view of a shadow root. Never exists in our tree.
// `TyBackend` holds an `UnsafeCell`, so equality is backend-pointer identity
// (there is exactly one backend per cascade) and `Debug` prints a placeholder.
#[derive(Clone, Copy)]
pub struct TyShadowRoot<'a> {
    backend: &'a TyBackend<'a>,
}

impl<'a> PartialEq for TyShadowRoot<'a> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.backend, other.backend)
    }
}

impl<'a> fmt::Debug for TyShadowRoot<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TyShadowRoot")
    }
}

impl<'a> TShadowRoot for TyShadowRoot<'a> {
    type ConcreteNode = TyNode<'a>;

    fn as_node(&self) -> Self::ConcreteNode {
        TyNode::new(self.backend.dom.root, self.backend)
    }

    fn host(&self) -> <Self::ConcreteNode as TNode>::ConcreteElement {
        TyElement::new(self.backend.dom.root, self.backend)
    }

    fn style_data<'b>(&self) -> Option<&'b CascadeData>
    where
        Self: 'b,
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Small helpers on NodeKind
// ---------------------------------------------------------------------------

impl NodeKind {
    fn as_element_attr<T>(&self, f: impl FnOnce(&crate::dom::Element) -> T) -> Option<T> {
        match self {
            NodeKind::Element(el) => Some(f(el)),
            _ => None,
        }
    }
}
