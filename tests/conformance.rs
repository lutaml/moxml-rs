//! The cross-language conformance runner (#496): every row of the
//! vendored dataset (exported by lutaml/moxml's
//! `rake conformance:export`) must answer through this crate exactly
//! as the Ruby pins expect. A missing case is a failure — coverage is
//! part of conformance.

use moxml::lexical;

const DATASET: &str = include_str!("data/moxml-conformance.json");

#[test]
fn dataset_parses_and_carries_rows() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    assert_eq!(data["schema"], 2, "dataset schema version");
    let rows = data["cases"]["builtin_lexical"]
        .as_array()
        .expect("builtin_lexical cases");
    assert!(
        rows.len() >= 80,
        "expected the full dataset, got {}",
        rows.len()
    );
}

#[test]
fn every_row_answers_as_the_ruby_pins_expect() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["builtin_lexical"].as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let builtin = row["type"].as_str().expect("type");
        let lexical = row["lexical"].as_str().expect("lexical");
        let expected = row["valid"].as_bool().expect("valid");
        match lexical::valid(builtin, lexical) {
            Ok(answer) => {
                if answer != expected {
                    failures.push(format!(
                        "{builtin} {lexical:?}: got {answer}, dataset expects {expected}"
                    ));
                }
            }
            Err(e) => failures.push(format!("{builtin} {lexical:?}: errored {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "conformance divergences:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_dataset_type_is_in_the_table() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["builtin_lexical"].as_array().unwrap();
    for row in rows {
        let builtin = row["type"].as_str().expect("type");
        assert!(
            lexical::names().contains(&builtin),
            "{builtin} is in the dataset but missing from the table"
        );
    }
}

#[test]
fn unknown_names_error() {
    assert!(lexical::valid("xs:nope", "3").is_err());
}
