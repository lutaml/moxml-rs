// The wildcard-face conformance runner (#496): every row of the
// `schema_wildcards` family must answer exactly as the Ruby pins
// expect — admission (covers) and negative exclusion.

use moxml::wildcard::{Exclusion, Namespace, Wildcard};

const DATASET: &str = include_str!("data/moxml-conformance.json");

#[test]
fn every_row_answers_as_the_ruby_pins_expect() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["schema_wildcards"]
        .as_array()
        .expect("schema_wildcards cases");
    assert!(rows.len() >= 20, "expected the full wildcard family");
    let mut failures = Vec::new();
    for row in rows {
        let constraint = row["constraint"].as_str();
        let process = row["process"].as_str().expect("process");
        let not_namespace = row["not_namespace"].as_str();
        let not_qname: Vec<&str> = row["not_qname"]
            .as_array()
            .expect("not_qname list")
            .iter()
            .map(|v| v.as_str().expect("not_qname token"))
            .collect();
        let item_ns = Namespace::parse(row["item_namespace"].as_str().expect("item_namespace"));
        let item_qname = row["item_qname"].as_str().expect("item_qname");
        let defined = row["defined"].as_bool().expect("defined");
        let sibling_names: Vec<String> = row["sibling_names"]
            .as_array()
            .expect("sibling_names list")
            .iter()
            .map(|v| v.as_str().expect("sibling name").to_string())
            .collect();
        let covers_expected = row["covers"].as_bool().expect("covers");
        let exclusion_expected = row["exclusion"].as_str();

        let wildcard = Wildcard::new(constraint, process, not_namespace, &not_qname);

        let covers_answer = wildcard.covers(&item_ns);
        if covers_answer != covers_expected {
            failures.push(format!(
                "covers({item_ns:?}) under {constraint:?}: got {covers_answer}, dataset expects {covers_expected}"
            ));
        }

        let exclusion_answer =
            wildcard.negative_exclusion(item_qname, &item_ns, defined, &sibling_names);
        let exclusion_expected = match exclusion_expected {
            Some("not_qname") => Some(Exclusion::NotQName),
            Some("not_namespace") => Some(Exclusion::NotNamespace),
            Some(other) => panic!("unknown exclusion spelling {other:?}"),
            None => None,
        };
        if exclusion_answer != exclusion_expected {
            failures.push(format!(
                "negative_exclusion({item_qname:?}, {item_ns:?}) under {constraint:?}: got {exclusion_answer:?}, dataset expects {exclusion_expected:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "conformance divergences:\n{}",
        failures.join("\n")
    );
}
