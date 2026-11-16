// The data-plane rows exporter (moxml-rs#2): the reference
// implementation of the element-rows contract emits the canonical
// dataset the testsuite vendors (tests/data-plane-rows.json). Each
// case carries the XML and the exact row tuples per the model's
// contract (docs: § The data plane (row contracts)).
//
// Usage: cargo run --bin export_rows -- <out-path>
use moxml::document::{Document, QName};

fn q(prefix: Option<&str>, local: &str) -> String {
    match prefix {
        Some(p) => format!("{}:{}", p, local),
        None => local.to_string(),
    }
}

fn xml_to_tuple(name: &QName, ns: Option<&str>) -> (String, Option<String>) {
    (
        q(name.prefix.as_deref(), &name.local),
        ns.map(|s| s.to_string()),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = std::env::args()
        .nth(1)
        .expect("usage: export_rows <out-path>");

    let cases: Vec<(&str, &str)> = vec![
        ("simple", "<r><a>one</a><b x=\"1\">two</b></r>"),
        (
            "namespaced",
            "<r xmlns=\"urn:t\" xmlns:p=\"urn:p\"><a/><p:b p:k=\"v\"/></r>",
        ),
        ("mixed-content", "<r>a<b/>c</r>"),
        (
            "cdata-comment-pi",
            "<r><![CDATA[raw]]><!--c--><?pi data?>t</r>",
        ),
        ("whitespace", "<r>\n  <a/>\n</r>"),
        ("empty", "<r/>"),
        ("deep", "<l0><l1><l2>leaf</l2></l1></l0>"),
        (
            "attrs-many",
            "<r k1=\"v1\" k2=\"v2\" k3=\"v3\"><a z=\"9\"/></r>",
        ),
    ];

    let mut out_cases = serde_json::Map::new();
    for (id, xml) in cases {
        let doc = Document::parse(xml.as_bytes())?;
        let rows: Vec<serde_json::Value> = doc
            .element_rows()
            .into_iter()
            .map(|row| {
                let name = row.name.clone();
                let (qualified, ns) = xml_to_tuple(&name, row.name.namespace.as_deref());
                let pairs: Vec<serde_json::Value> = row
                    .attributes
                    .iter()
                    .flat_map(|a| {
                        let an = a.name.clone();
                        vec![
                            serde_json::Value::String(q(an.prefix.as_deref(), &an.local)),
                            serde_json::Value::String(a.value.clone()),
                        ]
                    })
                    .collect();
                let mut text = String::new();
                for content in &row.content {
                    match content {
                        moxml::document::ContentRow::Text(t) => text.push_str(t),
                        moxml::document::ContentRow::CData(t) => text.push_str(t),
                        _ => {}
                    }
                }
                serde_json::json!([
                    row.ordinal,
                    row.parent,
                    row.depth,
                    qualified,
                    ns,
                    pairs,
                    text,
                ])
            })
            .collect();
        out_cases.insert(
            id.to_string(),
            serde_json::json!({ "xml": xml, "rows": rows }),
        );
    }

    let payload = serde_json::json!({
        "schema": 1,
        "generator": format!("moxml-rs {}", env!("CARGO_PKG_VERSION")),
        "contract": "processing-model § The data plane (row contracts)",
        "cases": out_cases,
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&payload)? + "\n")?;
    println!(
        "export_rows: wrote {} cases to {}",
        out_cases.len(),
        out_path
    );
    Ok(())
}
