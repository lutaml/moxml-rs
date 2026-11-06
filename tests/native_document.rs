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

// ---- parser scaling and scoping ---------------------------------------

/// Wall time of `f`, best of three, so a busy machine does not flake the test.
fn best_of_3<T>(mut f: impl FnMut() -> T) -> std::time::Duration {
    (0..3)
        .map(|_| {
            let t = std::time::Instant::now();
            drop(f());
            t.elapsed()
        })
        .min()
        .unwrap()
}

/// Quadratic parsing shows as time(4n) / time(n) near 16; linear is near 4.
/// The ratio is load-independent; the absolute cap (5x the 1 s target, since
/// these run in debug on shared runners) catches gross regressions. Before the
/// fix: 40k attributes took 6.9 s and 4k xmlns + 100k children took 8.7 s.
fn assert_linear(what: &str, make: impl Fn(usize) -> String, n: usize) {
    let small = make(n);
    let big = make(4 * n);
    let t_small = best_of_3(|| Document::parse(small.as_bytes()).unwrap());
    let t_big = best_of_3(|| Document::parse(big.as_bytes()).unwrap());
    assert!(
        t_big.as_secs_f64() < 5.0,
        "{what}: {t_big:?} for the large input"
    );
    assert!(
        t_big.as_secs_f64() < 9.0 * t_small.as_secs_f64().max(0.005),
        "{what}: 4x input took {t_big:?} vs {t_small:?} (superlinear)"
    );
}

#[test]
fn forty_thousand_attributes_parse_in_linear_time() {
    let make = |n: usize| {
        let mut xml = String::from("<r");
        for i in 0..n {
            xml.push_str(&format!(" a{i}=\"v\""));
        }
        xml.push_str("/>");
        xml
    };
    assert_linear("attributes", make, 10_000);
    let doc = Document::parse(make(40_000).as_bytes()).unwrap();
    assert_eq!(doc.element_rows()[0].attributes.len(), 40_000);
}

#[test]
fn thousands_of_namespace_declarations_with_many_children_parse_in_linear_time() {
    // xmlns count and child count grow together, like the reported input
    let make = |n: usize| {
        let mut xml = String::from("<r");
        for i in 0..n / 25 {
            xml.push_str(&format!(" xmlns:p{i}=\"urn:{i}\""));
        }
        xml.push('>');
        for _ in 0..n {
            xml.push_str("<c/>");
        }
        xml.push_str("</r>");
        xml
    };
    assert_linear("xmlns + children", make, 25_000);
    let doc = Document::parse(make(100_000).as_bytes()).unwrap();
    assert_eq!(doc.element_rows().len(), 100_001);
    assert_eq!(doc.element_rows()[0].namespaces.len(), 4_000);
}

#[test]
fn a_duplicate_attribute_is_still_refused() {
    let err = Document::parse(br#"<r a="1" b="2" a="3"/>"#).unwrap_err();
    assert!(matches!(err, Error::DuplicateAttribute(_)), "{err:?}");
    // same local name under different namespaces is fine; same expanded name is not
    let ok = br#"<r xmlns:x="urn:x" xmlns:y="urn:y" x:a="1" y:a="2" a="3"/>"#;
    assert!(Document::parse(ok).is_ok());
    let dup = br#"<r xmlns:x="urn:x" xmlns:y="urn:x" x:a="1" y:a="2"/>"#;
    assert!(matches!(
        Document::parse(dup).unwrap_err(),
        Error::DuplicateAttribute(_)
    ));
}

#[test]
fn nested_namespace_declarations_shadow_and_unwind() {
    let xml = br#"<a xmlns="urn:outer" xmlns:p="urn:p1"><b xmlns="urn:inner" xmlns:p="urn:p2"><p:c/></b><p:d/><e xmlns=""><f/></e><g/></a>"#;
    let doc = Document::parse(xml).unwrap();
    let ns = |local: &str| {
        doc.element_rows()
            .into_iter()
            .find(|r| r.name.local == local)
            .unwrap()
            .name
            .namespace
    };
    assert_eq!(ns("a").as_deref(), Some("urn:outer"));
    assert_eq!(ns("b").as_deref(), Some("urn:inner"));
    assert_eq!(ns("c").as_deref(), Some("urn:p2"));
    assert_eq!(ns("d").as_deref(), Some("urn:p1"));
    assert_eq!(ns("e"), None);
    assert_eq!(ns("f"), None);
    assert_eq!(ns("g").as_deref(), Some("urn:outer"));
    // a prefix declared on an empty element does not leak to siblings
    let leak = br#"<a><b xmlns:p="urn:p"/><p:c/></a>"#;
    assert!(matches!(
        Document::parse(leak).unwrap_err(),
        Error::UnboundPrefix(_)
    ));
}
