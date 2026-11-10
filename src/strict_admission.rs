//! The strict wildcard admission face (#17): the engine-tier port of
//! the Ruby facade's SchemaRules strict-wildcard re-check.
//!
//! The model owns the committed answer: an engine may accept, inside
//! its recognized scope, instances that violate a strict `xs:any`
//! wildcard with an undeclared element or a strict `xs:anyAttribute`
//! with an undeclared attribute. This face re-checks those shapes
//! from the schema SOURCE (its own XML machinery — no engine
//! involvement, mirroring the Ruby posture) so every tier answers
//! identically.
//!
//! Tier-1 posture preserved: the index covers the schema document at
//! one-level content depth, so a missing piece can only
//! under-report, never false-report. Findings are one-directional:
//! a row is reported only when the admitted item matches a strict
//! wildcard AND no declaration vouches for it.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

/// One strict-admission finding: the message shape mirrors the
/// Ruby facade's rows ("attribute 'x': not declared (strict)" /
/// "element 'x': not declared (strict)").
#[derive(Debug, PartialEq)]
pub struct Finding {
    pub kind: &'static str,
    pub name: String,
    pub namespace: String,
}

impl Finding {
    pub fn message(&self) -> String {
        format!(
            "{} '{}': not declared (strict)",
            self.kind, self.name
        )
    }
}

/// A wildcard: namespace constraint + processContents (default
/// strict, per Structures §3.10.1/3.10.2).
#[derive(Debug)]
struct Wildcard {
    /// AnyNamespace | Other(ns) | List(ns...)
    namespaces: Vec<String>,
    other_only: bool,
    strict: bool,
}

impl Wildcard {
    fn admits_namespace(&self, ns: &str) -> bool {
        if self.namespaces.iter().any(|n| n == ns) {
            return !self.other_only;
        }
        if self.other_only {
            return ns != self.namespaces.first().map(String::as_str).unwrap_or("");
        }
        // ##any (empty list + not other) admits everything.
        true
    }
}

/// The schema index: global declarations and the root types' strict
/// wildcards with their declared attribute/child names.
#[derive(Debug, Default)]
pub struct SchemaIndex {
    target_namespace: String,
    attribute_default_qualified: bool,
    /// "{ns}name" for every global xs:element / xs:attribute.
    globals: HashMap<String, ()>,
    /// Strict wildcards declared on the root element's type
    /// (anyAttribute), with the attribute names the type declares.
    strict_attribute_wildcards: Vec<Wildcard>,
    declared_attributes: Vec<String>,
    /// Strict xs:any wildcards on the root type, with declared
    /// child names.
    strict_element_wildcards: Vec<Wildcard>,
    declared_children: Vec<String>,
}

pub(crate) const XSD_LEXICAL: &str = "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" targetNamespace=\"urn:t\" xmlns=\"urn:t\"><xs:element name=\"doc\"><xs:complexType><xs:sequence><xs:element name=\"n\" type=\"xs:int\"/><xs:element name=\"flag\" type=\"xs:boolean\"/></xs:sequence><xs:attribute name=\"a\" type=\"xs:string\"/><xs:anyAttribute namespace=\"##any\" processContents=\"strict\"/></xs:complexType></xs:element><xs:attribute name=\"ext\" type=\"xs:string\"/></xs:schema>";
/// Parse a schema document into the index. Only the pieces the
/// strict-admission check consumes are kept.
pub fn index_schema(schema_xml: &str) -> SchemaIndex {
    let mut index = SchemaIndex::default();
    let mut reader = Reader::from_str(schema_xml);
    reader.config_mut().trim_text(false);

    // Namepace resolution needs prefix -> uri; quick-xml exposes
    // resolve on attributes.
    let mut stack: Vec<String> = Vec::new();
    let mut current_global: Option<String> = None;
    let mut type_depth = 0usize;
    let mut in_complex_type = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name =
                    String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                let attrs = attributes_with_ns(&e, &mut reader);
                match name.as_str() {
                    "schema" => {
                        index.target_namespace = attr_value(&attrs, "targetNamespace").unwrap_or_default();
                        index.attribute_default_qualified =
                            attr_value(&attrs, "attributeFormDefault").as_deref() == Some("qualified");
                        stack.push("schema".into());
                    }
                    "element" | "attribute" => {
                        let is_global = stack.len() == 1; // direct child of xs:schema
                        if is_global {
                            if let Some(n) = attr_value(&attrs, "name") {
                                let qname = format!(
                                    "{{{}}}{}",
                                    index.target_namespace, n
                                );
                                index.globals.insert(qname.clone(), ());
                                if current_global.is_none() {
                                    current_global = Some(qname);
                                }
                            }
                        }
                        if name == "element" {
                            stack.push("element".into());
                        } else {
                            stack.push("attribute".into());
                        }
                    }
                    "complexType" | "simpleType" => {
                        in_complex_type = name == "complexType";
                        type_depth += 1;
                        stack.push(name.to_string());
                    }
                    "anyAttribute" | "any" => {
                        let strict = attr_value(&attrs, "processContents")
                            .map(|v| v == "strict")
                            .unwrap_or(true);
                        if strict && in_complex_type && type_depth == 1 {
                            let wildcard = wildcard_from(&attrs, &index);
                            if name == "anyAttribute" {
                                index.strict_attribute_wildcards.push(wildcard);
                            } else {
                                index.strict_element_wildcards.push(wildcard);
                            }
                        }
                        stack.push(name.to_string());
                    }
                    "attributeGroup" | "group" => stack.push(name.to_string()),
                    "sequence" | "choice" | "all" => stack.push(name.to_string()),
                    "complexContent" | "simpleContent" | "extension"
                    | "restriction" => stack.push(name.to_string()),
                    _ => stack.push(name.to_string()),
                }
            }
            Ok(Event::Empty(e)) => {
                let name =
                    String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                let attrs = attributes_with_ns(&e, &mut reader);
                match name.as_str() {
                    "element" | "attribute" if stack.len() == 1 => {
                        if let Some(n) = attr_value(&attrs, "name") {
                            let qname =
                                format!("{{{}}}{}", index.target_namespace, n);
                            index.globals.insert(qname, ());
                        }
                    }
                    "attribute" if in_complex_type && type_depth == 1 => {
                        if let Some(n) = attr_value(&attrs, "name") {
                            let ns = form_namespace(
                                &attrs,
                                "attributeFormDefault",
                                &index,
                            );
                            index
                                .declared_attributes
                                .push(format!("{{{}}}{}", ns, n));
                        }
                    }
                    "anyAttribute" | "any" => {
                        let strict = attr_value(&attrs, "processContents")
                            .map(|v| v == "strict")
                            .unwrap_or(true);
                        if strict && in_complex_type && type_depth == 1 {
                            let wildcard = wildcard_from(&attrs, &index);
                            if name == "anyAttribute" {
                                index.strict_attribute_wildcards.push(wildcard);
                            } else {
                                index.strict_element_wildcards.push(wildcard);
                            }
                        }
                    }
                    "element" if in_complex_type && type_depth == 1 => {
                        if let Some(n) = attr_value(&attrs, "name") {
                            let ns =
                                form_namespace(&attrs, "elementFormDefault", &index);
                            index
                                .declared_children
                                .push(format!("{{{}}}{}", ns, n));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                let popped = stack.pop();
                if matches!(popped.as_deref(), Some("complexType")) {
                    in_complex_type = false;
                    type_depth = type_depth.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    index
}

/// The strict-admission check over one instance document against the
/// index. Returns the findings the raw engine tier misses.
pub fn check(index: &SchemaIndex, instance_xml: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let reader = Reader::from_str(instance_xml);
    let mut ns_reader = quick_xml::NsReader::from_reader(reader.into_inner());
    ns_reader.config_mut().trim_text(false);

    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut seen_root = false;

    loop {
        let (resolved_ns, event) = match ns_reader.read_resolved_event() {
            Ok(pair) => pair,
            Err(_) => break,
        };
        let event_ns = match resolved_ns {
            quick_xml::name::ResolveResult::Bound(v) => {
                String::from_utf8_lossy(v.as_ref()).to_string()
            }
            _ => String::new(),
        };
        match event {
            quick_xml::events::Event::Start(ref e)
            | quick_xml::events::Event::Empty(ref e) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref())
                    .to_string();
                let qname = format!("{{{}}}{}", event_ns, local);

                if !seen_root {
                    seen_root = true;
                    if let Some(wildcard) =
                        index.strict_element_wildcards.first()
                    {
                        let declared = index.globals.contains_key(&qname)
                            || index
                                .declared_children
                                .contains(&qname);
                        if wildcard.strict
                            && wildcard.admits_namespace(&event_ns)
                            && !declared
                        {
                            findings.push(Finding {
                                kind: "element",
                                name: local.clone(),
                                namespace: event_ns.clone(),
                            });
                        }
                    }
                }

                for attr in e.attributes().flatten() {
                    let (resolved, local_name) = ns_reader
                        .resolve_attribute(attr.key);
                    let bare = String::from_utf8_lossy(
                        local_name.as_ref(),
                    )
                    .to_string();
                    let ns_attr = match resolved {
                        quick_xml::name::ResolveResult::Bound(v) => {
                            String::from_utf8_lossy(v.as_ref()).to_string()
                        }
                        _ => String::new(),
                    };
                    // Namespace declarations are not attributes
                    // (XML Namespaces §3): never admission-checked.
                    if ns_attr == "http://www.w3.org/2000/xmlns/"
                        || bare == "xmlns"
                    {
                        continue;
                    }
                    let attr_qname = format!("{{{}}}{}", ns_attr, bare);
                    if let Some(wildcard) =
                        index.strict_attribute_wildcards.first()
                    {
                        let declared = index
                            .globals
                            .contains_key(&attr_qname)
                            || index
                                .declared_attributes
                                .contains(&attr_qname);
                        if wildcard.strict
                            && wildcard.admits_namespace(&ns_attr)
                            && !declared
                            && seen
                                .insert(
                                    format!("{}|{}", qname, attr_qname),
                                    (),
                                )
                                .is_none()
                        {
                            findings.push(Finding {
                                kind: "attribute",
                                name: bare,
                                namespace: ns_attr,
                            });
                        }
                    }
                }
                seen.insert(qname, ());
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    findings
}


fn attr_value(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(k, _)| k.rsplit(':').next() == Some(name))
        .map(|(_, v)| v.clone())
}

/// Attributes with values, namespace prefixes unresolved (the
/// wildcard namespace constraints on ##other carry explicit URIs, so
/// prefix resolution is only needed for form defaults — handled by
/// the schema-level defaults here).
fn attributes_with_ns(
    e: &quick_xml::events::BytesStart,
    _reader: &mut Reader<&[u8]>,
) -> Vec<(String, String)> {
    e.attributes()
        .flatten()
        .filter_map(|a| {
            Some((String::from_utf8_lossy(a.key.as_ref()).to_string(), String::from_utf8_lossy(&a.value).to_string()))
        })
        .collect()
}

fn wildcard_from(attrs: &[(String, String)], index: &SchemaIndex) -> Wildcard {
    let value = attr_value(attrs, "namespace").unwrap_or_else(|| "##any".into());
    let strict = attr_value(attrs, "processContents")
        .map(|v| v == "strict")
        .unwrap_or(true);
    let (namespaces, other_only) = if value == "##other" {
        (
            vec![index.target_namespace.clone()],
            true,
        )
    } else if value == "##any" || value == "##targetNamespace" {
        (
            if value == "##targetNamespace" {
                vec![index.target_namespace.clone()]
            } else {
                Vec::new()
            },
            false,
        )
    } else {
        (
            value
                .split_whitespace()
                .filter(|t| !t.starts_with("##"))
                .map(String::from)
                .collect(),
            false,
        )
    };
    Wildcard {
        namespaces,
        other_only,
        strict,
    }
}

fn form_namespace(
    attrs: &[(String, String)],
    default_attr: &str,
    index: &SchemaIndex,
) -> String {
    match attr_value(attrs, "form").as_deref() {
        Some("qualified") => index.target_namespace.clone(),
        Some("unqualified") => String::new(),
        _ => {
            if default_attr == "attributeFormDefault"
                && index.attribute_default_qualified
            {
                index.target_namespace.clone()
            } else {
                String::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn undeclared_attribute_under_strict_wildcard_is_reported() {
        let index = index_schema(XSD_LEXICAL);
        let findings = check(
            &index,
            r#"<d:doc xmlns:d="urn:t" ghost="z" a="x"><n>1</n><flag>false</flag></d:doc>"#,
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, "attribute");
        assert_eq!(findings[0].name, "ghost");
        assert_eq!(findings[0].namespace, "");
    }

    #[test]
    fn declared_global_attribute_under_strict_wildcard_is_accepted() {
        let index = index_schema(XSD_LEXICAL);
        // The global `ext` is {urn:t}ext; the instance must carry it
        // with an explicit prefix (attribute defaults never qualify).
        let findings = check(
            &index,
            r#"<d:doc xmlns:d="urn:t" xmlns:t="urn:t" t:ext="y" a="x"><n>1</n><flag>false</flag></d:doc>"#,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn locally_declared_attribute_is_accepted() {
        let index = index_schema(XSD_LEXICAL);
        let findings = check(
            &index,
            r#"<d:doc xmlns:d="urn:t" a="x"><n>1</n><flag>false</flag></d:doc>"#,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }
}
