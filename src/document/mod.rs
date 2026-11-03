//! The native (non-wasm) document API: parse, read, build, serialize,
//! parser policy.
//!
//! Direction (moxml-rs#2, `docs/data-plane.md`): the **primary surface
//! is row/batch-shaped**. [`Document::element_rows`] reads the whole
//! tree in one pass as flat [`ElementRow`]s (document order; names,
//! namespaces, attributes and the ordered content run ride the same
//! row), and [`ElementSpec`] writes a whole tree in one call
//! ([`Document::build`]). [`NodeId`] handles ([`Document::root`],
//! [`Document::name`], [`Document::children`], [`Document::append_element`],
//! ...) are the convenience layer over the same arena, never the only
//! path.
//!
//! Engine: parsing and serialization go through the [`Engine`] trait.
//! The only backend today is [`ReferenceEngine`], a dependency-free
//! XML 1.0 reader/writer. The leptris backend is blocked on engine
//! reach, not on this API: the `leptris` crate's safe surface exposes no
//! namespace URI/prefix, text/CDATA node, or mutation faces (they exist
//! only behind its private `ffi` module and its wasm32 `wasmface`), and
//! it links a system `libleptris` that a plain `cargo test` here cannot
//! assume. See `docs/native-document.md`.

mod error;
mod parse;
mod write;

pub use error::{Error, Result};
pub use write::SerializeOptions;

/// The XML namespace bound to the reserved `xml` prefix.
pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// Parser safety policy. The default is the safe one: DOCTYPE is
/// rejected outright, so neither XXE nor entity-expansion attacks can
/// start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserPolicy {
    /// Accept a DOCTYPE with an *internal* subset (general internal
    /// entities only). External entities, external subsets and
    /// parameter entities are rejected whatever this says.
    pub allow_doctype: bool,
    /// Cap on total bytes produced by entity expansion (only relevant
    /// when `allow_doctype`). Exceeding it is [`Error::EntityLimit`].
    pub max_entity_expansion: usize,
    /// Cap on element nesting.
    pub max_depth: usize,
}

impl Default for ParserPolicy {
    fn default() -> Self {
        ParserPolicy {
            allow_doctype: false,
            max_entity_expansion: 1 << 20,
            max_depth: 256,
        }
    }
}

/// A parse/serialize backend.
pub trait Engine {
    /// Parses UTF-8 XML bytes under `policy`.
    fn parse(&self, xml: &[u8], policy: &ParserPolicy) -> Result<Document>;
    /// Serializes a document deterministically.
    fn serialize(&self, doc: &Document, opts: &SerializeOptions) -> Result<String>;
}

/// The dependency-free reference backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceEngine;

impl Engine for ReferenceEngine {
    fn parse(&self, xml: &[u8], policy: &ParserPolicy) -> Result<Document> {
        parse::parse(xml, policy)
    }
    fn serialize(&self, doc: &Document, opts: &SerializeOptions) -> Result<String> {
        write::serialize(doc, opts)
    }
}

/// An element or attribute name with its resolved namespace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QName {
    pub prefix: Option<String>,
    pub local: String,
    /// Namespace URI; `None` = no namespace.
    pub namespace: Option<String>,
}

impl QName {
    /// A name in no namespace.
    pub fn local(local: &str) -> QName {
        QName {
            prefix: None,
            local: local.into(),
            namespace: None,
        }
    }

    /// A name in a namespace, written with `prefix` (`None` = default namespace).
    pub fn ns(prefix: Option<&str>, local: &str, uri: &str) -> QName {
        QName {
            prefix: prefix.map(String::from),
            local: local.into(),
            namespace: Some(uri.into()),
        }
    }

    /// `prefix:local` or `local`.
    pub fn qualified(&self) -> String {
        match &self.prefix {
            Some(p) => format!("{p}:{}", self.local),
            None => self.local.clone(),
        }
    }
}

/// A (non-`xmlns`) attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: QName,
    pub value: String,
}

impl Attribute {
    pub fn new(name: QName, value: &str) -> Attribute {
        Attribute {
            name,
            value: value.into(),
        }
    }
}

/// A namespace declaration (`xmlns` / `xmlns:p`) on an element.
/// `uri == ""` with `prefix == None` is `xmlns=""` (undeclare default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceDecl {
    pub prefix: Option<String>,
    pub uri: String,
}

// ---- the arena -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) usize);

#[derive(Debug, Clone)]
pub(crate) struct ElementData {
    pub name: QName,
    pub namespaces: Vec<(Option<String>, String)>,
    pub attributes: Vec<Attribute>,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Element(ElementData),
    Text(String),
    CData(String),
    Comment(String),
    Pi { target: String, data: String },
}

/// Maximum nesting a built document may reach (parse uses its policy).
const MAX_BUILD_DEPTH: usize = 1024;

/// A parsed or built document: one root element.
#[derive(Debug, Clone)]
pub struct Document {
    pub(crate) nodes: Vec<Node>,
    pub(crate) root: NodeId,
}

// ---- the batch read plane --------------------------------------------

/// One entry of an element's ordered content run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentRow {
    Text(String),
    CData(String),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
    /// The child element's `ordinal` in the row list.
    Element(usize),
}

/// Row contract: one row per element in document (pre-)order.
/// `ordinal` is the row index (root = 0); `parent`/`depth` locate it;
/// `content` is the ordered children, text and CDATA kept distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementRow {
    pub ordinal: usize,
    pub parent: Option<usize>,
    pub depth: usize,
    pub name: QName,
    pub namespaces: Vec<NamespaceDecl>,
    pub attributes: Vec<Attribute>,
    pub content: Vec<ContentRow>,
}

// ---- the batch write plane -------------------------------------------

/// Owned description of a subtree, built fluently and turned into a
/// document in one call ([`Document::build`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSpec {
    pub name: QName,
    pub namespaces: Vec<NamespaceDecl>,
    pub attributes: Vec<Attribute>,
    pub content: Vec<ContentSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentSpec {
    Text(String),
    CData(String),
    Comment(String),
    ProcessingInstruction { target: String, data: String },
    Element(ElementSpec),
}

impl ElementSpec {
    pub fn new(name: QName) -> ElementSpec {
        ElementSpec {
            name,
            namespaces: vec![],
            attributes: vec![],
            content: vec![],
        }
    }
    /// Declares `xmlns[:prefix]="uri"` on this element.
    pub fn declare(mut self, prefix: Option<&str>, uri: &str) -> Self {
        self.namespaces.push(NamespaceDecl {
            prefix: prefix.map(String::from),
            uri: uri.into(),
        });
        self
    }
    pub fn attr(mut self, name: QName, value: &str) -> Self {
        self.attributes.push(Attribute::new(name, value));
        self
    }
    pub fn text(mut self, text: &str) -> Self {
        self.content.push(ContentSpec::Text(text.into()));
        self
    }
    pub fn cdata(mut self, text: &str) -> Self {
        self.content.push(ContentSpec::CData(text.into()));
        self
    }
    pub fn comment(mut self, text: &str) -> Self {
        self.content.push(ContentSpec::Comment(text.into()));
        self
    }
    pub fn child(mut self, child: ElementSpec) -> Self {
        self.content.push(ContentSpec::Element(child));
        self
    }
}

pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

pub(crate) fn is_name_start(c: char) -> bool {
    c == '_' || c.is_alphabetic() || (c as u32) >= 0x80 && !c.is_whitespace()
}

pub(crate) fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '-' || c == '.'
}

/// An NCName (no colon).
pub(crate) fn valid_name(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(), Some(c) if is_name_start(c)) && it.all(is_name_char)
}

impl Document {
    // -- parse / serialize (default engine) ---------------------------

    /// Parses with the default (safe) policy.
    pub fn parse(xml: &[u8]) -> Result<Document> {
        Document::parse_with(xml, &ParserPolicy::default())
    }

    /// Parses under an explicit policy.
    pub fn parse_with(xml: &[u8], policy: &ParserPolicy) -> Result<Document> {
        ReferenceEngine.parse(xml, policy)
    }

    /// Serializes with default options (no declaration, no indent).
    pub fn to_xml(&self) -> Result<String> {
        self.to_xml_with(&SerializeOptions::default())
    }

    pub fn to_xml_with(&self, opts: &SerializeOptions) -> Result<String> {
        ReferenceEngine.serialize(self, opts)
    }

    // -- batch write ---------------------------------------------------

    /// Builds a document from a spec tree in one call.
    pub fn build(spec: ElementSpec) -> Result<Document> {
        let mut doc = Document::with_root(spec.name.clone())?;
        let root = doc.root;
        doc.fill(root, spec, 1)?;
        Ok(doc)
    }

    fn fill(&mut self, id: NodeId, spec: ElementSpec, depth: usize) -> Result<()> {
        for d in spec.namespaces {
            self.declare_namespace(id, d.prefix.as_deref(), &d.uri)?;
        }
        for a in spec.attributes {
            self.set_attribute(id, a.name, &a.value)?;
        }
        for c in spec.content {
            match c {
                ContentSpec::Text(t) => {
                    self.append_text(id, &t)?;
                }
                ContentSpec::CData(t) => {
                    self.append_cdata(id, &t)?;
                }
                ContentSpec::Comment(t) => {
                    self.push_child(id, Node::Comment(t))?;
                }
                ContentSpec::ProcessingInstruction { target, data } => {
                    self.push_child(id, Node::Pi { target, data })?;
                }
                ContentSpec::Element(child) => {
                    if depth >= MAX_BUILD_DEPTH {
                        return Err(Error::DepthLimit);
                    }
                    let cid = self.append_element(id, child.name.clone())?;
                    self.fill(cid, child, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    // -- batch read ----------------------------------------------------

    /// Reads the whole tree as flat rows in document order, one pass.
    pub fn element_rows(&self) -> Vec<ElementRow> {
        // Pass 1: pre-order, assigning ordinals.
        let mut order: Vec<(NodeId, Option<usize>, usize)> = Vec::new();
        let mut ordinal_of = vec![usize::MAX; self.nodes.len()];
        let mut stack = vec![(self.root, None, 0usize)];
        while let Some((id, parent, depth)) = stack.pop() {
            let Node::Element(e) = &self.nodes[id.0] else {
                continue;
            };
            let ordinal = order.len();
            ordinal_of[id.0] = ordinal;
            order.push((id, parent, depth));
            for c in e.children.iter().rev() {
                stack.push((*c, Some(ordinal), depth + 1));
            }
        }
        // Pass 2: materialize.
        order
            .into_iter()
            .enumerate()
            .map(|(ordinal, (id, parent, depth))| {
                let Node::Element(e) = &self.nodes[id.0] else {
                    unreachable!("pass 1 only records elements")
                };
                let content = e
                    .children
                    .iter()
                    .map(|c| match &self.nodes[c.0] {
                        Node::Text(t) => ContentRow::Text(t.clone()),
                        Node::CData(t) => ContentRow::CData(t.clone()),
                        Node::Comment(t) => ContentRow::Comment(t.clone()),
                        Node::Pi { target, data } => ContentRow::ProcessingInstruction {
                            target: target.clone(),
                            data: data.clone(),
                        },
                        Node::Element(_) => ContentRow::Element(ordinal_of[c.0]),
                    })
                    .collect();
                ElementRow {
                    ordinal,
                    parent,
                    depth,
                    name: e.name.clone(),
                    namespaces: e
                        .namespaces
                        .iter()
                        .map(|(p, u)| NamespaceDecl {
                            prefix: p.clone(),
                            uri: u.clone(),
                        })
                        .collect(),
                    attributes: e.attributes.clone(),
                    content,
                }
            })
            .collect()
    }

    // -- node-handle convenience layer ----------------------------------

    /// A new document whose root is an empty element.
    pub fn with_root(name: QName) -> Result<Document> {
        check_qname(&name)?;
        Ok(Document {
            nodes: vec![Node::Element(ElementData {
                name,
                namespaces: vec![],
                attributes: vec![],
                children: vec![],
                parent: None,
            })],
            root: NodeId(0),
        })
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    fn element(&self, id: NodeId) -> Result<&ElementData> {
        match self.nodes.get(id.0) {
            Some(Node::Element(e)) => Ok(e),
            _ => Err(Error::InvalidNode),
        }
    }

    fn element_mut(&mut self, id: NodeId) -> Result<&mut ElementData> {
        match self.nodes.get_mut(id.0) {
            Some(Node::Element(e)) => Ok(e),
            _ => Err(Error::InvalidNode),
        }
    }

    pub fn name(&self, id: NodeId) -> Result<&QName> {
        Ok(&self.element(id)?.name)
    }

    pub fn attributes(&self, id: NodeId) -> Result<&[Attribute]> {
        Ok(&self.element(id)?.attributes)
    }

    pub fn namespace_declarations(&self, id: NodeId) -> Result<Vec<NamespaceDecl>> {
        Ok(self
            .element(id)?
            .namespaces
            .iter()
            .map(|(p, u)| NamespaceDecl {
                prefix: p.clone(),
                uri: u.clone(),
            })
            .collect())
    }

    pub fn parent(&self, id: NodeId) -> Result<Option<NodeId>> {
        Ok(self.element(id)?.parent)
    }

    /// Ordered content as rows (child elements are handles).
    pub fn children(&self, id: NodeId) -> Result<Vec<Child>> {
        Ok(self
            .element(id)?
            .children
            .iter()
            .map(|c| match &self.nodes[c.0] {
                Node::Element(_) => Child::Element(*c),
                Node::Text(t) => Child::Text(t.clone()),
                Node::CData(t) => Child::CData(t.clone()),
                Node::Comment(t) => Child::Comment(t.clone()),
                Node::Pi { target, data } => Child::ProcessingInstruction {
                    target: target.clone(),
                    data: data.clone(),
                },
            })
            .collect())
    }

    /// Concatenated direct Text and CDATA content.
    pub fn text(&self, id: NodeId) -> Result<String> {
        let mut s = String::new();
        for c in &self.element(id)?.children {
            if let Node::Text(t) | Node::CData(t) = &self.nodes[c.0] {
                s.push_str(t);
            }
        }
        Ok(s)
    }

    pub fn append_element(&mut self, parent: NodeId, name: QName) -> Result<NodeId> {
        check_qname(&name)?;
        self.element(parent)?;
        if self.depth_of(parent) + 1 >= MAX_BUILD_DEPTH {
            return Err(Error::DepthLimit);
        }
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node::Element(ElementData {
            name,
            namespaces: vec![],
            attributes: vec![],
            children: vec![],
            parent: Some(parent),
        }));
        self.element_mut(parent)?.children.push(id);
        Ok(id)
    }

    fn depth_of(&self, mut id: NodeId) -> usize {
        let mut d = 0;
        while let Some(Node::Element(e)) = self.nodes.get(id.0) {
            match e.parent {
                Some(p) => {
                    d += 1;
                    id = p;
                }
                None => break,
            }
        }
        d
    }

    fn push_child(&mut self, parent: NodeId, node: Node) -> Result<NodeId> {
        self.element(parent)?;
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        self.element_mut(parent)?.children.push(id);
        Ok(id)
    }

    pub fn append_text(&mut self, parent: NodeId, text: &str) -> Result<NodeId> {
        self.push_child(parent, Node::Text(text.into()))
    }

    pub fn append_cdata(&mut self, parent: NodeId, text: &str) -> Result<NodeId> {
        self.push_child(parent, Node::CData(text.into()))
    }

    /// Sets (or replaces, keeping position) an attribute by expanded name.
    pub fn set_attribute(&mut self, id: NodeId, name: QName, value: &str) -> Result<()> {
        check_qname(&name)?;
        let e = self.element_mut(id)?;
        let key = (&name.namespace, &name.local);
        if let Some(a) = e
            .attributes
            .iter_mut()
            .find(|a| (&a.name.namespace, &a.name.local) == key)
        {
            a.name = name;
            a.value = value.into();
        } else {
            e.attributes.push(Attribute::new(name, value));
        }
        Ok(())
    }

    /// Declares `xmlns[:prefix]` on an element (replacing a prior
    /// declaration of the same prefix).
    pub fn declare_namespace(&mut self, id: NodeId, prefix: Option<&str>, uri: &str) -> Result<()> {
        if let Some(p) = prefix {
            if !valid_name(p) || p == "xmlns" || uri.is_empty() {
                return Err(Error::InvalidName(p.into()));
            }
        }
        let e = self.element_mut(id)?;
        let p = prefix.map(String::from);
        if let Some(d) = e.namespaces.iter_mut().find(|(q, _)| *q == p) {
            d.1 = uri.into();
        } else {
            e.namespaces.push((p, uri.into()));
        }
        Ok(())
    }
}

/// A child in the handle layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Child {
    Element(NodeId),
    Text(String),
    CData(String),
    Comment(String),
    ProcessingInstruction { target: String, data: String },
}

fn check_qname(n: &QName) -> Result<()> {
    if !valid_name(&n.local) || n.prefix.as_deref().is_some_and(|p| !valid_name(p)) {
        return Err(Error::InvalidName(n.qualified()));
    }
    if n.prefix.is_some() && n.namespace.is_none() {
        return Err(Error::InvalidName(n.qualified()));
    }
    Ok(())
}
