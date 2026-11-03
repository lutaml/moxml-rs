//! Faces of the native document API: parse, read, build, serialize, policy.

use moxml::document::*;

const NS: &str = r#"<r:root xmlns:r="urn:r" xmlns="urn:d" r:id="7" plain="p">
  <child>a &amp; b<![CDATA[<raw> & ]]>tail</child>
  <r:other/>
  <!--note-->
</r:root>"#;

fn q(prefix: Option<&str>, local: &str, uri: Option<&str>) -> QName {
    QName {
        prefix: prefix.map(String::from),
        local: local.into(),
        namespace: uri.map(String::from),
    }
}

// ---- parse + read ------------------------------------------------------

#[test]
fn rows_carry_names_namespaces_attributes_and_ordered_content() {
    let doc = Document::parse(NS.as_bytes()).unwrap();
    let rows = doc.element_rows();
    assert_eq!(rows.len(), 3);

    let root = &rows[0];
    assert_eq!((root.ordinal, root.parent, root.depth), (0, None, 0));
    assert_eq!(root.name, q(Some("r"), "root", Some("urn:r")));
    assert_eq!(
        root.namespaces,
        vec![
            NamespaceDecl {
                prefix: Some("r".into()),
                uri: "urn:r".into()
            },
            NamespaceDecl {
                prefix: None,
                uri: "urn:d".into()
            },
        ]
    );
    // xmlns declarations are not attributes; unprefixed attrs have no namespace.
    assert_eq!(
        root.attributes,
        vec![
            Attribute::new(q(Some("r"), "id", Some("urn:r")), "7"),
            Attribute::new(q(None, "plain", None), "p"),
        ]
    );
    assert_eq!(
        root.content,
        vec![
            ContentRow::Text("\n  ".into()),
            ContentRow::Element(1),
            ContentRow::Text("\n  ".into()),
            ContentRow::Element(2),
            ContentRow::Text("\n  ".into()),
            ContentRow::Comment("note".into()),
            ContentRow::Text("\n".into()),
        ]
    );

    // Default namespace applies to unprefixed elements, in document order.
    assert_eq!(rows[1].name, q(None, "child", Some("urn:d")));
    assert_eq!(rows[1].parent, Some(0));
    assert_eq!(rows[1].depth, 1);
    // Text and CDATA stay distinct and ordered; entities are decoded.
    assert_eq!(
        rows[1].content,
        vec![
            ContentRow::Text("a & b".into()),
            ContentRow::CData("<raw> & ".into()),
            ContentRow::Text("tail".into()),
        ]
    );
    assert_eq!(rows[2].name, q(Some("r"), "other", Some("urn:r")));
}

#[test]
fn handle_layer_reads_the_same_arena() {
    let doc = Document::parse(NS.as_bytes()).unwrap();
    let root = doc.root();
    assert_eq!(doc.name(root).unwrap().qualified(), "r:root");
    let kids: Vec<NodeId> = doc
        .children(root)
        .unwrap()
        .into_iter()
        .filter_map(|c| {
            if let Child::Element(id) = c {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(kids.len(), 2);
    assert_eq!(doc.text(kids[0]).unwrap(), "a & b<raw> & tail");
    assert_eq!(doc.parent(kids[0]).unwrap(), Some(root));
    assert_eq!(doc.attributes(root).unwrap().len(), 2);
}

#[test]
fn xml_prefix_and_undeclared_default() {
    let doc = Document::parse(br#"<a xmlns="urn:x" xml:lang="en"><b xmlns=""/></a>"#).unwrap();
    let rows = doc.element_rows();
    assert_eq!(
        rows[0].attributes[0].name.namespace.as_deref(),
        Some(XML_NS)
    );
    assert_eq!(rows[1].name.namespace, None);
}

#[test]
fn malformed_and_namespace_errors_are_typed() {
    assert!(matches!(
        Document::parse(b"<a><b></a>"),
        Err(Error::Malformed { .. })
    ));
    assert!(matches!(
        Document::parse(b"<a>"),
        Err(Error::Malformed { .. })
    ));
    assert!(matches!(
        Document::parse(b"<a/><b/>"),
        Err(Error::Malformed { .. })
    ));
    assert!(matches!(Document::parse(b"<p:a/>"), Err(Error::UnboundPrefix(p)) if p == "p"));
    assert!(matches!(
        Document::parse(br#"<a x="1" x="2"/>"#),
        Err(Error::DuplicateAttribute(_))
    ));
    assert!(matches!(
        Document::parse(br#"<a xmlns:p="u" xmlns:q="u" p:x="1" q:x="2"/>"#),
        Err(Error::DuplicateAttribute(_))
    ));
    assert_eq!(Document::parse(&[0xff, 0xfe]).unwrap_err(), Error::Encoding);
    assert!(matches!(
        Document::parse(b"<a>&nope;</a>"),
        Err(Error::UndefinedEntity(_))
    ));
}

#[test]
fn char_refs_and_attribute_normalization() {
    let doc = Document::parse(b"<a v=\"x\ty&#9;z\r\n\">&#65;&#x42;\r\n</a>").unwrap();
    let rows = doc.element_rows();
    assert_eq!(rows[0].attributes[0].value, "x y\tz ");
    assert_eq!(rows[0].content, vec![ContentRow::Text("AB\n".into())]);
}

// ---- parser policy -----------------------------------------------------

const XXE: &str = r#"<?xml version="1.0"?>
<!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/passwd">]>
<r>&x;</r>"#;

const LAUGHS: &str = r#"<!DOCTYPE l [
<!ENTITY a "aaaaaaaaaa">
<!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
<!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">
<!ENTITY d "&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;">
]><l>&d;&d;&d;&d;&d;&d;&d;&d;&d;&d;</l>"#;

#[test]
fn default_policy_rejects_any_doctype() {
    assert_eq!(
        Document::parse(XXE.as_bytes()).unwrap_err(),
        Error::DoctypeForbidden
    );
    assert_eq!(
        Document::parse(LAUGHS.as_bytes()).unwrap_err(),
        Error::DoctypeForbidden
    );
    assert_eq!(
        Document::parse(b"<!DOCTYPE a><a/>").unwrap_err(),
        Error::DoctypeForbidden
    );
    assert!(!ParserPolicy::default().allow_doctype);
}

#[test]
fn external_entities_are_never_resolved_even_when_doctype_allowed() {
    let p = ParserPolicy {
        allow_doctype: true,
        ..Default::default()
    };
    assert_eq!(
        Document::parse_with(XXE.as_bytes(), &p).unwrap_err(),
        Error::ExternalEntity
    );
    assert_eq!(
        Document::parse_with(b"<!DOCTYPE r SYSTEM \"x.dtd\"><r/>", &p).unwrap_err(),
        Error::ExternalEntity
    );
    assert_eq!(
        Document::parse_with(b"<!DOCTYPE r [<!ENTITY % pe \"x\">]><r/>", &p).unwrap_err(),
        Error::ExternalEntity
    );
}

#[test]
fn entity_expansion_is_bounded_and_internal_entities_work_within_the_limit() {
    let p = ParserPolicy {
        allow_doctype: true,
        max_entity_expansion: 10_000,
        ..Default::default()
    };
    assert_eq!(
        Document::parse_with(LAUGHS.as_bytes(), &p).unwrap_err(),
        Error::EntityLimit
    );

    let ok = Document::parse_with(
        br#"<!DOCTYPE r [<!ENTITY e "hi &amp; bye">]><r a="&e;">&e;</r>"#,
        &p,
    )
    .unwrap();
    let rows = ok.element_rows();
    assert_eq!(rows[0].attributes[0].value, "hi & bye");
    assert_eq!(rows[0].content, vec![ContentRow::Text("hi & bye".into())]);

    // Self-referential entity hits the nesting bound, not the stack.
    let rec = br#"<!DOCTYPE r [<!ENTITY a "&a;">]><r>&a;</r>"#;
    assert_eq!(
        Document::parse_with(rec, &p).unwrap_err(),
        Error::EntityLimit
    );
}

#[test]
fn depth_limit_is_enforced_without_recursion() {
    let n = 5000;
    let xml = format!("{}{}", "<a>".repeat(n), "</a>".repeat(n));
    assert_eq!(
        Document::parse(xml.as_bytes()).unwrap_err(),
        Error::DepthLimit
    );
    let p = ParserPolicy {
        max_depth: n,
        ..Default::default()
    };
    assert_eq!(
        Document::parse_with(xml.as_bytes(), &p)
            .unwrap()
            .element_rows()
            .len(),
        n
    );
}

// ---- build + serialize ---------------------------------------------------

#[test]
fn build_with_namespaces_serializes_deterministically() {
    let spec = ElementSpec::new(QName::ns(Some("m"), "math", "urn:m"))
        .declare(Some("m"), "urn:m")
        .attr(QName::ns(Some("m"), "id", "urn:m"), "a\"b<c&\n")
        .attr(QName::local("plain"), "v")
        .child(ElementSpec::new(QName::local("empty")))
        .child(
            ElementSpec::new(QName::ns(None, "dflt", "urn:d"))
                .text("1 < 2 & 3 > 2")
                .cdata("x]]>y")
                .comment("c"),
        );
    let doc = Document::build(spec.clone()).unwrap();
    let want = "<m:math xmlns:m=\"urn:m\" m:id=\"a&quot;b&lt;c&amp;&#10;\" plain=\"v\">\
<empty/><dflt xmlns=\"urn:d\">1 &lt; 2 &amp; 3 &gt; 2<![CDATA[x]]]]><![CDATA[>y]]><!--c--></dflt></m:math>";
    assert_eq!(doc.to_xml().unwrap(), want);
    // Same spec, same bytes.
    assert_eq!(Document::build(spec).unwrap().to_xml().unwrap(), want);
}

#[test]
fn undeclared_namespaces_are_declared_on_demand_and_round_trip() {
    let spec = ElementSpec::new(QName::ns(Some("a"), "x", "urn:a"))
        .attr(QName::ns(Some("b"), "y", "urn:b"), "1");
    let xml = Document::build(spec).unwrap().to_xml().unwrap();
    assert_eq!(xml, r#"<a:x xmlns:a="urn:a" xmlns:b="urn:b" b:y="1"/>"#);
    let rows = Document::parse(xml.as_bytes()).unwrap().element_rows();
    assert_eq!(rows[0].name.namespace.as_deref(), Some("urn:a"));
    assert_eq!(
        rows[0].attributes[0].name.namespace.as_deref(),
        Some("urn:b")
    );
}

#[test]
fn child_in_no_namespace_under_default_namespace_gets_undeclared() {
    let spec =
        ElementSpec::new(QName::ns(None, "p", "urn:d")).child(ElementSpec::new(QName::local("c")));
    let xml = Document::build(spec).unwrap().to_xml().unwrap();
    assert_eq!(xml, r#"<p xmlns="urn:d"><c xmlns=""/></p>"#);
}

#[test]
fn parse_serialize_round_trip_is_stable() {
    let doc = Document::parse(NS.as_bytes()).unwrap();
    let once = doc.to_xml().unwrap();
    let twice = Document::parse(once.as_bytes()).unwrap().to_xml().unwrap();
    assert_eq!(once, twice);
    assert_eq!(
        Document::parse(once.as_bytes()).unwrap().element_rows(),
        doc.element_rows()
    );
    assert!(once.contains("<![CDATA[<raw> & ]]>"));
}

#[test]
fn serialize_options_declaration_and_indent() {
    let doc = Document::parse(b"<a><b>t</b><c><d/></c></a>").unwrap();
    let out = doc
        .to_xml_with(&SerializeOptions {
            xml_declaration: true,
            indent: Some(2),
        })
        .unwrap();
    assert_eq!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a>\n  <b>t</b>\n  <c>\n    <d/>\n  </c>\n</a>"
    );
}

#[test]
fn handle_layer_builds_and_matches_batch_build() {
    let mut doc = Document::with_root(QName::ns(Some("r"), "root", "urn:r")).unwrap();
    let root = doc.root();
    doc.declare_namespace(root, Some("r"), "urn:r").unwrap();
    doc.set_attribute(root, QName::local("k"), "1").unwrap();
    doc.set_attribute(root, QName::local("k"), "2").unwrap();
    let c = doc.append_element(root, QName::local("c")).unwrap();
    doc.append_text(c, "t").unwrap();
    doc.append_cdata(c, "<>").unwrap();
    assert_eq!(
        doc.to_xml().unwrap(),
        r#"<r:root xmlns:r="urn:r" k="2"><c>t<![CDATA[<>]]></c></r:root>"#
    );
}

#[test]
fn invalid_builds_are_rejected() {
    assert!(matches!(
        Document::with_root(QName::local("1bad")),
        Err(Error::InvalidName(_))
    ));
    assert!(matches!(
        Document::with_root(q(Some("p"), "x", None)),
        Err(Error::InvalidName(_))
    ));
    let doc = Document::build(ElementSpec::new(QName::local("a")).text("bad\u{1}char")).unwrap();
    assert!(matches!(doc.to_xml(), Err(Error::InvalidContent(_))));
    let doc = Document::build(ElementSpec::new(QName::local("a")).comment("a--b")).unwrap();
    assert!(matches!(doc.to_xml(), Err(Error::InvalidContent(_))));
}

#[test]
fn engine_trait_is_the_seam() {
    let e: &dyn Engine = &ReferenceEngine;
    let d = e.parse(b"<a/>", &ParserPolicy::default()).unwrap();
    assert_eq!(
        e.serialize(&d, &SerializeOptions::default()).unwrap(),
        "<a/>"
    );
}
