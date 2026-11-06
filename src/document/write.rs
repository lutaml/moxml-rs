//! Deterministic serialization. Same document + same options => same
//! bytes: declarations first (in document order), then attributes in
//! document order, fixed escaping, `<a/>` for empty elements.

use super::error::{Error, Result};
use super::{check_decl, is_xml_char, valid_name, Document, Node, NodeId, XML_NS};

/// Serialization options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SerializeOptions {
    /// Emit `<?xml version="1.0" encoding="UTF-8"?>` plus a newline.
    pub xml_declaration: bool,
    /// Indent element-only content by this many spaces; `None` writes
    /// the content exactly as stored.
    pub indent: Option<usize>,
}

type Scope = Vec<(Option<String>, String)>;

fn lookup<'a>(scope: &'a Scope, prefix: Option<&str>) -> Option<&'a str> {
    scope
        .iter()
        .rev()
        .find(|(p, _)| p.as_deref() == prefix)
        .map(|(_, u)| u.as_str())
}

pub(super) fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
}

fn check_chars(s: &str, what: &str) -> Result<()> {
    if s.chars().all(is_xml_char) {
        Ok(())
    } else {
        Err(Error::InvalidContent(format!(
            "{what} has a character illegal in XML 1.0"
        )))
    }
}

pub(super) fn serialize(doc: &Document, opts: &SerializeOptions) -> Result<String> {
    let mut out = String::new();
    if opts.xml_declaration {
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    }
    let mut scope: Scope = vec![(Some("xml".into()), XML_NS.into())];
    // Iterative (explicit stack): tree depth never costs call-stack.
    let mut tasks = vec![Task::Node {
        id: doc.root,
        depth: 0,
        pad: None,
    }];
    while let Some(task) = tasks.pop() {
        match task {
            Task::Node { id, depth, pad } => {
                if let Some(n) = pad {
                    out.push('\n');
                    out.extend(std::iter::repeat_n(' ', n));
                }
                match &doc.nodes[id.0] {
                    Node::Element(_) => {
                        write_open(doc, id, opts, depth, &mut scope, &mut out, &mut tasks)?
                    }
                    other => write_leaf(other, &mut out)?,
                }
            }
            Task::Close { qn, mark, pad } => {
                if let Some(n) = pad {
                    out.push('\n');
                    out.extend(std::iter::repeat_n(' ', n));
                }
                out.push_str("</");
                out.push_str(&qn);
                out.push('>');
                scope.truncate(mark);
            }
        }
    }
    Ok(out)
}

enum Task {
    Node {
        id: NodeId,
        depth: usize,
        /// Newline plus this many spaces before the node.
        pad: Option<usize>,
    },
    Close {
        qn: String,
        mark: usize,
        pad: Option<usize>,
    },
}

/// Writes an element's start tag; queues its children and end tag.
fn write_open(
    doc: &Document,
    id: NodeId,
    opts: &SerializeOptions,
    depth: usize,
    scope: &mut Scope,
    out: &mut String,
    tasks: &mut Vec<Task>,
) -> Result<()> {
    let Node::Element(e) = &doc.nodes[id.0] else {
        return Err(Error::InvalidNode);
    };
    let mark = scope.len();
    // Declarations to emit: explicit ones first, then whatever the
    // element and its attributes need that is not yet in scope.
    let mut decls: Vec<(Option<String>, String)> = e.namespaces.clone();
    for (p, u) in &decls {
        scope.push((p.clone(), u.clone()));
    }
    let mut need = |prefix: Option<&str>, uri: Option<&str>, scope: &mut Scope| -> Result<()> {
        let want = uri.unwrap_or("");
        let have = lookup(scope, prefix).unwrap_or("");
        if have != want && !(prefix == Some("xml") && want == XML_NS) {
            // Never emit a document the parser refuses: a second
            // binding of one prefix on one element is a conflict.
            if decls.iter().any(|(p, _)| p.as_deref() == prefix) {
                return Err(Error::InvalidContent(format!(
                    "prefix '{}' is bound to two different namespaces on one element",
                    prefix.unwrap_or("")
                )));
            }
            check_decl(prefix, want)?;
            decls.push((prefix.map(String::from), want.to_string()));
            scope.push((prefix.map(String::from), want.to_string()));
        }
        Ok(())
    };
    if let Some(p) = e.name.prefix.as_deref() {
        if e.name.namespace.is_none() {
            return Err(Error::InvalidName(e.name.qualified()));
        }
        need(Some(p), e.name.namespace.as_deref(), scope)?;
    } else {
        need(None, e.name.namespace.as_deref(), scope)?;
    }
    for a in &e.attributes {
        match (&a.name.prefix, &a.name.namespace) {
            (Some(p), Some(_)) => need(Some(p), a.name.namespace.as_deref(), scope)?,
            (None, None) => {}
            _ => return Err(Error::InvalidName(a.name.qualified())),
        }
    }
    for (p, u) in &decls {
        check_decl(p.as_deref(), u)?;
    }
    let qn = e.name.qualified();
    if !valid_name(&e.name.local) {
        return Err(Error::InvalidName(qn));
    }
    out.push('<');
    out.push_str(&qn);
    for (p, u) in &decls {
        match p {
            Some(p) => {
                if !valid_name(p) {
                    return Err(Error::InvalidName(p.clone()));
                }
                out.push_str(" xmlns:");
                out.push_str(p);
            }
            None => out.push_str(" xmlns"),
        }
        out.push_str("=\"");
        check_chars(u, "namespace URI")?;
        escape_attr(u, out);
        out.push('"');
    }
    for a in &e.attributes {
        if !valid_name(&a.name.local) {
            return Err(Error::InvalidName(a.name.qualified()));
        }
        check_chars(&a.value, "attribute value")?;
        out.push(' ');
        out.push_str(&a.name.qualified());
        out.push_str("=\"");
        escape_attr(&a.value, out);
        out.push('"');
    }
    if e.children.is_empty() {
        out.push_str("/>");
        scope.truncate(mark);
    } else {
        out.push('>');
        let indent = opts.indent.filter(|_| {
            e.children
                .iter()
                .all(|c| !matches!(doc.nodes[c.0], Node::Text(_) | Node::CData(_)))
        });
        tasks.push(Task::Close {
            qn,
            mark,
            pad: indent.map(|n| n * depth),
        });
        for c in e.children.iter().rev() {
            tasks.push(Task::Node {
                id: *c,
                depth: depth + 1,
                pad: indent.map(|n| n * (depth + 1)),
            });
        }
    }
    Ok(())
}

fn write_leaf(node: &Node, out: &mut String) -> Result<()> {
    match node {
        Node::Element(_) => return Err(Error::InvalidNode),
        Node::Text(t) => {
            check_chars(t, "text")?;
            escape_text(t, out);
        }
        Node::CData(t) => {
            check_chars(t, "CDATA")?;
            // `]]>` cannot appear inside one section: split it.
            out.push_str("<![CDATA[");
            out.push_str(&t.replace("]]>", "]]]]><![CDATA[>"));
            out.push_str("]]>");
        }
        Node::Comment(t) => {
            check_chars(t, "comment")?;
            if t.contains("--") || t.ends_with('-') {
                return Err(Error::InvalidContent(
                    "comment contains '--' or ends with '-'".into(),
                ));
            }
            out.push_str("<!--");
            out.push_str(t);
            out.push_str("-->");
        }
        Node::Pi { target, data } => {
            if !valid_name(target) || target.eq_ignore_ascii_case("xml") || data.contains("?>") {
                return Err(Error::InvalidContent(
                    "invalid processing instruction".into(),
                ));
            }
            check_chars(data, "processing instruction")?;
            out.push_str("<?");
            out.push_str(target);
            if !data.is_empty() {
                out.push(' ');
                out.push_str(data);
            }
            out.push_str("?>");
        }
    }
    Ok(())
}
