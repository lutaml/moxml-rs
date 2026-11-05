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
    assert_eq!(data["schema"], 5, "dataset schema version");
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

#[test]
fn every_regex_translation_row_answers() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["xsd_regex_translation"].as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let pattern = row["pattern"].as_str().expect("pattern");
        let value = row["value"].as_str().expect("value");
        let expected = row["matches"].as_bool().expect("matches");
        let translatable = row["translatable"].as_bool().expect("translatable");
        let compiled = moxml::xsd_regex::translate(pattern);
        match (translatable, compiled) {
            (false, None) => {}
            (true, Some(regex)) => {
                let answer = regex.is_match(value);
                if answer != expected {
                    failures.push(format!(
                        "{pattern:?} {value:?}: got {answer}, dataset expects {expected}"
                    ));
                }
            }
            (false, Some(_)) => failures.push(format!(
                "{pattern:?}: dataset marks untranslatable, the face compiled it"
            )),
            (true, None) => failures.push(format!(
                "{pattern:?}: dataset marks translatable, the face answered None"
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "regex conformance divergences:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_wildcard_row_answers() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["schema_wildcards"].as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let constraint = row["constraint"].as_str();
        let process = row["process"].as_str().expect("process");
        let not_namespace = row["not_namespace"].as_str();
        let not_qname: Vec<&str> = row["not_qname"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let wildcard =
            moxml::wildcard::Wildcard::new(constraint, process, not_namespace, &not_qname);
        let item_namespace = moxml::wildcard::Namespace::parse(
            row["item_namespace"].as_str().expect("item_namespace"),
        );
        let item_qname = row["item_qname"].as_str().expect("item_qname");
        let defined = row["defined"].as_bool().expect("defined");
        let sibling_names: Vec<String> = row["sibling_names"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let covers = wildcard.covers(&item_namespace);
        if covers != row["covers"].as_bool().expect("covers") {
            failures.push(format!(
                "{constraint:?} item {item_qname:?}: covers got {covers}, dataset expects {}",
                row["covers"]
            ));
        }
        let exclusion =
            wildcard.negative_exclusion(item_qname, &item_namespace, defined, &sibling_names);
        let got = exclusion.map(|e| match e {
            moxml::wildcard::Exclusion::NotQName => "not_qname",
            moxml::wildcard::Exclusion::NotNamespace => "not_namespace",
        });
        let expected_str = row["exclusion"].as_str();
        if got != expected_str {
            failures.push(format!(
                "{constraint:?} item {item_qname:?}: exclusion got {got:?}, dataset expects {expected_str:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "wildcard conformance divergences:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_entity_registry_row_answers() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["entity_registry"].as_array().unwrap();
    let mut failures = Vec::new();
    for row in rows {
        let seed_json = row["seed"].as_object().expect("seed object");
        let seed: Vec<(&str, &str)> = seed_json
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().expect("seed lexical")))
            .collect();
        let registry = moxml::entity_registry::EntityRegistry::from_provider(seed);
        let qname = row["qname"].as_str().expect("qname");
        let qcp = row["qcp"].as_u64().expect("qcp") as u32;
        let restore_entities = row["restore_entities"].as_bool().expect("restore_entities");
        let mode = row["mode"].as_str().expect("mode");

        let declared = registry.declared(qname);
        if declared != row["declared"].as_bool().expect("declared") {
            failures.push(format!("{qname}: declared got {declared}"));
        }
        let codepoint = registry.codepoint_for_name(qname);
        let expected_codepoint = row["codepoint"].as_u64().map(|v| v as u32);
        if codepoint != expected_codepoint {
            failures.push(format!(
                "{qname}: codepoint got {codepoint:?}, want {expected_codepoint:?}"
            ));
        }
        let names = registry.names_for_codepoint(qcp);
        let expected_names: Vec<&str> = row["names"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        if names != expected_names {
            failures.push(format!(
                "{qcp}: names got {names:?}, want {expected_names:?}"
            ));
        }
        let primary = registry.primary_name_for_codepoint(qcp);
        let expected_primary = row["primary"].as_str();
        if primary != expected_primary {
            failures.push(format!(
                "{qcp}: primary got {primary:?}, want {expected_primary:?}"
            ));
        }
        let standard = moxml::entity_registry::EntityRegistry::standard_entity(qcp);
        if standard != row["standard"].as_bool().expect("standard") {
            failures.push(format!("{qcp}: standard got {standard}"));
        }
        let restore = registry.should_restore(qcp, restore_entities, mode);
        if restore != row["restore"].as_bool().expect("restore") {
            failures.push(format!("{qname}: restore got {restore}"));
        }
    }
    assert!(
        failures.is_empty(),
        "entity-registry conformance divergences:\n{}",
        failures.join("\n")
    );
}
