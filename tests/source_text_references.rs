//! Source-compatible text ampersands are opt-in; strict XML and safety remain.
use moxml::document::*;
fn source(xml: &str) -> Result<Document> {
    ReferenceEngine.parse_with_text_recovery(
        xml.as_bytes(),
        &ParserPolicy::default(),
        TextReferenceRecovery::OxLookahead31,
    )
}
#[test]
fn source_window_uses_the_remaining_document_in_utf8_bytes() {
    // Measured Moxml0.6.4/Ox2.14.29 and the source TS reader agree.
    for count in [25, 26, 27, 28, 30, 31, 32] {
        let xml = format!("<r>&{}</r>", "x".repeat(count));
        assert!(Document::parse(xml.as_bytes()).is_err());
        if count < 27 {
            assert!(source(&xml).is_err());
        } else {
            let document = source(&xml).unwrap();
            assert_eq!(
                document.text(document.root()).unwrap(),
                format!("&{}", "x".repeat(count))
            );
        }
    }
    for count in [12, 13, 14, 15, 16] {
        let xml = format!("<r>&{}</r>", "α".repeat(count));
        assert_eq!(source(&xml).is_ok(), count >= 14);
    }
    let xml = format!("<r>&</r>{}", " ".repeat(64));
    let document = source(&xml).unwrap();
    assert_eq!(document.text(document.root()).unwrap(), "&");
}
#[test]
fn decoded_and_literal_ampersands_keep_their_source_order() {
    for (raw, expected) in [
        ("&a b;", "&a b;"),
        ("&#xzz;", "&#xzz;"),
        ("&&amp;", "&&"),
        ("&amp;;", "&;"),
        ("&#X41;", "A"),
        ("&foo;", "&foo;"),
    ] {
        let xml = format!("<r>{raw}</r>");
        let document = source(&xml).unwrap();
        assert_eq!(document.text(document.root()).unwrap(), expected);
    }
    let xml = "<r>first &amp; second &xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx final &amp;</r>";
    let document = source(xml).unwrap();
    assert_eq!(
        document.text(document.root()).unwrap(),
        "first & second &xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx final &"
    );
}
#[test]
fn attributes_and_xml_scalar_validation_remain_strict() {
    for xml in [
        "<r v=\"&\"/>",
        "<r v=\"&a b;\"/>",
        "<r>&#;</r>",
        "<r>&#x;</r>",
        "<r>&#0;</r>",
        "<r>&#1;</r>",
        "<r>&#x110000;</r>",
        "<r>a\u{1}b</r>",
    ] {
        assert!(source(xml).is_err(), "{xml:?}");
    }
    assert!(Document::parse(b"<r>&#X41;</r>").is_err());
    assert!(source("<r v=\"&#X41;\"/>").is_err());
    let strict = ReferenceEngine.parse_with_text_recovery(
        b"<r>&a b;</r>",
        &ParserPolicy::default(),
        TextReferenceRecovery::Strict,
    );
    assert!(strict.is_err());
}
#[test]
fn doctype_external_entity_and_resource_errors_keep_their_identity() {
    let dtd = b"<!DOCTYPE r [<!ENTITY x 'expanded'>]><r>&x;</r>";
    assert!(matches!(
        source(std::str::from_utf8(dtd).unwrap()),
        Err(Error::DoctypeForbidden)
    ));
    let mut policy = ParserPolicy {
        allow_doctype: true,
        max_entity_expansion: 2,
        ..ParserPolicy::default()
    };
    assert!(matches!(
        ReferenceEngine.parse_with_text_recovery(
            dtd,
            &policy,
            TextReferenceRecovery::OxLookahead31
        ),
        Err(Error::EntityLimit)
    ));
    let external = b"<!DOCTYPE r SYSTEM 'file:///never-read'><r/>";
    assert!(matches!(
        ReferenceEngine.parse_with_text_recovery(
            external,
            &policy,
            TextReferenceRecovery::OxLookahead31
        ),
        Err(Error::ExternalEntity)
    ));
    policy.max_depth = 2;
    assert!(matches!(
        ReferenceEngine.parse_with_text_recovery(
            b"<r><a><b/></a></r>",
            &policy,
            TextReferenceRecovery::OxLookahead31
        ),
        Err(Error::DepthLimit)
    ));
    assert!(matches!(source("<p:r/>"), Err(Error::UnboundPrefix(_))));
    assert!(matches!(
        source("<r a='1' a='2'/>"),
        Err(Error::DuplicateAttribute(_))
    ));
}

#[test]
fn source_attribute_whitespace_is_an_independent_explicit_opt_in() {
    let xml = b"<r v=\"a\r\nb\tc\"/>";
    let strict = Document::parse(xml).unwrap();
    assert_eq!(strict.element_rows()[0].attributes[0].value, "a b c");
    let options = ReferenceDecodingOptions {
        text_references: TextReferenceRecovery::OxLookahead31,
        attribute_whitespace: AttributeWhitespace::Preserve,
    };
    let source = ReferenceEngine
        .parse_with_decoding_options(xml, &ParserPolicy::default(), options)
        .unwrap();
    assert_eq!(source.element_rows()[0].attributes[0].value, "a\r\nb\tc");
    assert!(ReferenceEngine
        .parse_with_decoding_options(b"<r v=\"a\x01b\"/>", &ParserPolicy::default(), options)
        .is_err());
}
