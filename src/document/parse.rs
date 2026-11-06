//! Reference XML 1.0 reader with namespace resolution and the parser
//! policy. Iterative (no recursion on nesting) so depth is a policy
//! number, not a stack-overflow risk.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::error::{Error, Result};
use super::{
    is_name_char, is_name_start, is_xml_char, valid_name, Attribute, Document, ElementData, Node,
    NodeId, ParserPolicy, QName, XML_NS,
};

const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const MAX_ENTITY_NESTING: usize = 16;

pub(super) fn parse(xml: &[u8], policy: &ParserPolicy) -> Result<Document> {
    let src = std::str::from_utf8(xml).map_err(|_| Error::Encoding)?;
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut p = Parser {
        src,
        pos: 0,
        policy,
        entities: HashMap::new(),
        expanded: 0,
        nodes: Vec::new(),
        scope: HashMap::new(),
    };
    p.document()
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    policy: &'a ParserPolicy,
    entities: HashMap<String, String>,
    expanded: usize,
    nodes: Vec<Node>,
    /// In-scope namespace bindings: prefix -> stack of URIs (innermost last).
    scope: HashMap<Option<String>, Vec<Arc<str>>>,
}

impl<'a> Parser<'a> {
    fn err<T>(&self, message: &str) -> Result<T> {
        Err(Error::Malformed {
            at: self.pos,
            message: message.into(),
        })
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn starts(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.starts(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) -> bool {
        let before = self.pos;
        while matches!(
            self.rest().as_bytes().first(),
            Some(b' ' | b'\t' | b'\r' | b'\n')
        ) {
            self.pos += 1;
        }
        self.pos > before
    }

    fn take_until(&mut self, end: &str) -> Result<&'a str> {
        match self.rest().find(end) {
            Some(i) => {
                let s = &self.rest()[..i];
                self.pos += i + end.len();
                Ok(s)
            }
            None => self.err(&format!("expected '{end}'")),
        }
    }

    fn name(&mut self) -> Result<&'a str> {
        let rest = self.rest();
        let mut it = rest.char_indices();
        match it.next() {
            Some((_, c)) if is_name_start(c) || c == ':' => {}
            _ => return self.err("expected a name"),
        }
        let end = it
            .find(|(_, c)| !(is_name_char(*c) || *c == ':'))
            .map_or(rest.len(), |(i, _)| i);
        self.pos += end;
        Ok(&rest[..end])
    }

    fn document(&mut self) -> Result<Document> {
        if self.starts("<?xml") && self.rest()[5..].starts_with(|c: char| c.is_whitespace()) {
            self.take_until("?>")?;
        }
        self.misc(true)?;
        let root = self.element_tree()?;
        self.misc(false)?;
        if self.pos != self.src.len() {
            return self.err("content after the document element");
        }
        Ok(Document {
            nodes: std::mem::take(&mut self.nodes),
            root,
        })
    }

    /// Prolog (`doctype` allowed) or epilog: whitespace, comments, PIs.
    fn misc(&mut self, prolog: bool) -> Result<()> {
        loop {
            self.skip_ws();
            if self.starts("<!--") {
                self.comment()?;
            } else if self.starts("<?") {
                self.pi()?;
            } else if prolog && self.starts("<!DOCTYPE") {
                if !self.policy.allow_doctype {
                    return Err(Error::DoctypeForbidden);
                }
                self.doctype()?;
            } else {
                return Ok(());
            }
        }
    }

    fn comment(&mut self) -> Result<String> {
        self.pos += 4;
        let body = self.take_until("-->")?;
        if body.contains("--") || body.ends_with('-') {
            return self.err("'--' inside a comment");
        }
        self.check_chars(body)?;
        Ok(body.to_string())
    }

    fn pi(&mut self) -> Result<(String, String)> {
        self.pos += 2;
        let target = self.name()?;
        if target.eq_ignore_ascii_case("xml") || target.contains(':') {
            return self.err("reserved or invalid processing instruction target");
        }
        let data = if self.skip_ws() {
            self.take_until("?>")?
        } else {
            self.take_until("?>")?;
            ""
        };
        self.check_chars(data)?;
        Ok((target.to_string(), data.to_string()))
    }

    fn check_chars(&self, s: &str) -> Result<()> {
        if s.chars().all(is_xml_char) {
            Ok(())
        } else {
            self.err("character not allowed in XML 1.0")
        }
    }

    // -- DOCTYPE (only reached when the policy allows it) ----------------

    fn quoted(&mut self) -> Result<&'a str> {
        let q = match self.rest().chars().next() {
            Some(c @ ('"' | '\'')) => c,
            _ => return self.err("expected a quoted literal"),
        };
        self.pos += 1;
        self.take_until(&q.to_string())
    }

    fn doctype(&mut self) -> Result<()> {
        self.pos += "<!DOCTYPE".len();
        self.skip_ws();
        self.name()?;
        self.skip_ws();
        if self.starts("SYSTEM") || self.starts("PUBLIC") {
            return Err(Error::ExternalEntity);
        }
        if self.eat("[") {
            loop {
                self.skip_ws();
                if self.eat("]") {
                    break;
                }
                if self.starts("<!--") {
                    self.comment()?;
                } else if self.starts("<?") {
                    self.pi()?;
                } else if self.starts("<!ENTITY") {
                    self.pos += "<!ENTITY".len();
                    if !self.skip_ws() {
                        return self.err("expected whitespace");
                    }
                    if self.starts("%") {
                        return Err(Error::ExternalEntity);
                    }
                    let name = self.name()?.to_string();
                    if !self.skip_ws() {
                        return self.err("expected whitespace");
                    }
                    if self.starts("SYSTEM") || self.starts("PUBLIC") {
                        return Err(Error::ExternalEntity);
                    }
                    let value = self.quoted()?.to_string();
                    self.skip_ws();
                    if !self.eat(">") {
                        return self.err("expected '>' after entity declaration");
                    }
                    self.entities.entry(name).or_insert(value);
                } else if self.starts("<!") {
                    self.skip_decl()?;
                } else if self.starts("%") {
                    return Err(Error::ExternalEntity);
                } else {
                    return self.err("unexpected content in DOCTYPE");
                }
            }
            self.skip_ws();
        }
        if !self.eat(">") {
            return self.err("expected '>' to close DOCTYPE");
        }
        Ok(())
    }

    /// Skips an ELEMENT/ATTLIST/NOTATION declaration.
    fn skip_decl(&mut self) -> Result<()> {
        let mut quote: Option<char> = None;
        while let Some(c) = self.rest().chars().next() {
            self.pos += c.len_utf8();
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '"' | '\'') => quote = Some(c),
                (None, '>') => return Ok(()),
                _ => {}
            }
        }
        self.err("unterminated declaration")
    }

    // -- character data ----------------------------------------------------

    fn char_ref(&self, body: &str) -> Result<char> {
        let n = if let Some(h) = body.strip_prefix("x") {
            u32::from_str_radix(h, 16)
        } else {
            body.parse::<u32>()
        };
        match n.ok().and_then(char::from_u32).filter(|c| is_xml_char(*c)) {
            Some(c) => Ok(c),
            None => self.err("invalid character reference"),
        }
    }

    /// Expands references in `raw`; `attr` normalizes literal whitespace.
    fn expand(&mut self, raw: &str, attr: bool, depth: usize, out: &mut String) -> Result<()> {
        let mut rest = raw;
        loop {
            let (lit, tail) = match rest.find('&') {
                Some(i) => (&rest[..i], Some(&rest[i + 1..])),
                None => (rest, None),
            };
            self.check_chars(lit)?;
            if attr {
                out.push_str(&lit.replace("\r\n", " ").replace(['\n', '\r', '\t'], " "));
            } else {
                out.push_str(&lit.replace("\r\n", "\n").replace('\r', "\n"));
            }
            let Some(tail) = tail else { return Ok(()) };
            let Some(semi) = tail.find(';') else {
                return self.err("unterminated reference");
            };
            let body = &tail[..semi];
            rest = &tail[semi + 1..];
            if let Some(num) = body.strip_prefix('#') {
                out.push(self.char_ref(num)?);
                continue;
            }
            match body {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {
                    let Some(value) = self.entities.get(body).cloned() else {
                        return Err(Error::UndefinedEntity(body.to_string()));
                    };
                    if depth >= MAX_ENTITY_NESTING {
                        return Err(Error::EntityLimit);
                    }
                    if value.contains('<') {
                        return self.err("markup inside an entity value is not supported");
                    }
                    self.expanded = self.expanded.saturating_add(value.len());
                    if self.expanded > self.policy.max_entity_expansion {
                        return Err(Error::EntityLimit);
                    }
                    self.expand(&value, attr, depth + 1, out)?;
                }
            }
        }
    }

    // -- elements ------------------------------------------------------------

    fn add(&mut self, parent: NodeId, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        if let Some(Node::Element(e)) = self.nodes.get_mut(parent.0) {
            e.children.push(id);
        }
        id
    }

    fn element_tree(&mut self) -> Result<NodeId> {
        if !self.starts("<") || self.starts("</") {
            return self.err("expected the document element");
        }
        // open elements: (id, raw qualified name, scope frame count)
        let mut open: Vec<(NodeId, String)> = Vec::new();
        // prefixes each open element declared, to unwind `self.scope` on close
        let mut frames: Vec<Vec<Option<String>>> = Vec::new();
        let mut root: Option<NodeId> = None;
        loop {
            if self.starts("</") {
                let Some((_, raw)) = open.last() else {
                    return self.err("unexpected end tag");
                };
                self.pos += 2;
                let n = self.name()?;
                if n != raw {
                    return self.err(&format!("end tag '{n}' does not match '{raw}'"));
                }
                self.skip_ws();
                if !self.eat(">") {
                    return self.err("expected '>'");
                }
                open.pop();
                if let Some(f) = frames.pop() {
                    self.leave_scope(f);
                }
                if open.is_empty() {
                    return Ok(root.expect("root set with first element"));
                }
            } else if self.starts("<!--") {
                let Some((parent, _)) = open.last().cloned() else {
                    return self.err("comment before root inside tree");
                };
                let c = self.comment()?;
                self.add(parent, Node::Comment(c));
            } else if self.starts("<![CDATA[") {
                let Some((parent, _)) = open.last().cloned() else {
                    return self.err("CDATA outside the document element");
                };
                self.pos += 9;
                let body = self.take_until("]]>")?;
                self.check_chars(body)?;
                let body = body.replace("\r\n", "\n").replace('\r', "\n");
                self.add(parent, Node::CData(body));
            } else if self.starts("<?") {
                let Some((parent, _)) = open.last().cloned() else {
                    return self.err("PI outside the document element");
                };
                let (target, data) = self.pi()?;
                self.add(parent, Node::Pi { target, data });
            } else if self.starts("<") {
                if open.len() >= self.policy.max_depth {
                    return Err(Error::DepthLimit);
                }
                let (id, raw, empty, frame) = self.start_tag(open.last().map(|o| o.0))?;
                if root.is_none() {
                    root = Some(id);
                }
                if empty {
                    self.leave_scope(frame);
                    if open.is_empty() {
                        return Ok(id);
                    }
                } else {
                    open.push((id, raw));
                    frames.push(frame);
                }
            } else {
                let Some((parent, _)) = open.last().cloned() else {
                    return self.err("text outside the document element");
                };
                let end = self.rest().find('<').unwrap_or(self.rest().len());
                let raw = &self.rest()[..end];
                if raw.contains("]]>") {
                    return self.err("']]>' in character data");
                }
                if end == self.rest().len() {
                    return self.err("unclosed element");
                }
                let mut text = String::new();
                self.expand(raw, false, 0, &mut text)?;
                self.pos += end;
                if !text.is_empty() {
                    self.add(parent, Node::Text(text));
                }
            }
        }
    }

    /// Parses `<name attrs>` / `<name attrs/>`, resolves namespaces,
    /// appends the element and enters its namespace scope. Returns (id, raw
    /// name, empty?, prefixes declared, which the caller must `leave_scope`).
    #[allow(clippy::type_complexity)]
    fn start_tag(
        &mut self,
        parent: Option<NodeId>,
    ) -> Result<(NodeId, String, bool, Vec<Option<String>>)> {
        self.pos += 1;
        let raw = self.name()?.to_string();
        let mut raw_attrs: Vec<(&'a str, String)> = Vec::new();
        let empty;
        loop {
            let had_ws = self.skip_ws();
            if self.eat("/>") {
                empty = true;
                break;
            }
            if self.eat(">") {
                empty = false;
                break;
            }
            if !had_ws {
                return self.err("expected whitespace before attribute");
            }
            let an = self.name()?;
            self.skip_ws();
            if !self.eat("=") {
                return self.err("expected '=' after attribute name");
            }
            self.skip_ws();
            let lit = self.quoted()?;
            if lit.contains('<') {
                return self.err("'<' in attribute value");
            }
            let mut value = String::new();
            self.expand(lit, true, 0, &mut value)?;
            raw_attrs.push((an, value));
        }

        let mut frame: Vec<(Option<String>, String)> = Vec::new();
        let mut plain: Vec<(&str, String)> = Vec::new();
        for (n, v) in raw_attrs {
            if (n == "xmlns" || n.starts_with("xmlns:"))
                && frame
                    .iter()
                    .any(|(p, _)| p.as_deref() == n.strip_prefix("xmlns:"))
            {
                return Err(Error::DuplicateAttribute(n.to_string()));
            }
            if n == "xmlns" {
                frame.push((None, v));
            } else if let Some(p) = n.strip_prefix("xmlns:") {
                if !valid_name(p) || p == "xmlns" || v.is_empty() {
                    return self.err("invalid namespace declaration");
                }
                if (p == "xml") != (v == XML_NS) {
                    return self.err("the xml prefix and namespace are reserved");
                }
                if v == XMLNS_NS {
                    return self.err("the xmlns namespace cannot be declared");
                }
                frame.push((Some(p.to_string()), v));
            } else {
                plain.push((n, v));
            }
        }
        for (p, u) in &frame {
            self.scope
                .entry(p.clone())
                .or_default()
                .push(Arc::from(u.as_str()));
        }
        let declared: Vec<Option<String>> = frame.iter().map(|(p, _)| p.clone()).collect();
        let scope = &self.scope;
        let resolve = |prefix: Option<&str>| -> Option<Arc<str>> {
            let hit = scope
                .get(&prefix.map(String::from))
                .and_then(|stack| stack.last());
            match (hit, prefix) {
                (Some(u), _) if u.is_empty() => None,
                (Some(u), _) => Some(Arc::clone(u)),
                (None, Some("xml")) => Some(Arc::from(XML_NS)),
                _ => None,
            }
        };
        let split = |this: &Self, q: &str| -> Result<(Option<String>, String)> {
            match q.split_once(':') {
                None => Ok((None, q.to_string())),
                Some((p, l)) if valid_name(p) && valid_name(l) => {
                    Ok((Some(p.to_string()), l.to_string()))
                }
                _ => this.err(&format!("invalid qualified name '{q}'")),
            }
        };
        let (prefix, local) = split(self, &raw)?;
        let namespace = resolve(prefix.as_deref());
        if prefix.is_some() && namespace.is_none() {
            return Err(Error::UnboundPrefix(prefix.unwrap_or_default()));
        }
        let mut attributes: Vec<Attribute> = Vec::new();
        let mut seen: HashSet<(String, Option<Arc<str>>)> = HashSet::new();
        for (n, value) in plain {
            let (ap, al) = split(self, n)?;
            let ans = match ap.as_deref() {
                None => None,
                Some(_) => {
                    let u = resolve(ap.as_deref());
                    if u.is_none() {
                        return Err(Error::UnboundPrefix(ap.unwrap_or_default()));
                    }
                    u
                }
            };
            if !seen.insert((al.clone(), ans.clone())) {
                return Err(Error::DuplicateAttribute(n.to_string()));
            }
            attributes.push(Attribute {
                name: QName {
                    prefix: ap,
                    local: al,
                    namespace: ans,
                },
                value,
            });
        }
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node::Element(ElementData {
            name: QName {
                prefix,
                local,
                namespace,
            },
            namespaces: frame,
            attributes,
            children: Vec::new(),
            parent,
        }));
        if let Some(p) = parent {
            if let Some(Node::Element(e)) = self.nodes.get_mut(p.0) {
                e.children.push(id);
            }
        }
        Ok((id, raw, empty, declared))
    }

    /// Unwinds the bindings an element declared when it closes.
    fn leave_scope(&mut self, declared: Vec<Option<String>>) {
        for p in declared.into_iter().rev() {
            if let Some(stack) = self.scope.get_mut(&p) {
                stack.pop();
                if stack.is_empty() {
                    self.scope.remove(&p);
                }
            }
        }
    }
}
