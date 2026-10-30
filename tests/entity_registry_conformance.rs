// The entity-registry conformance runner (#496): every row of the
// `entity_registry` family must answer exactly as the Ruby pins
// expect — forward/reverse lookup, primary-name preference, the
// standard-5 set, and the restoration policy, with seeds driven
// through the provider path (lexical forms included).

use moxml::entity_registry::EntityRegistry;
use serde_json::Value;

const DATASET: &str = include_str!("data/moxml-conformance.json");

fn seed_registry(seed: &Value) -> EntityRegistry {
    let obj = seed.as_object().expect("seed object");
    let pairs: Vec<(&str, &str)> = obj
        .iter()
        .map(|(name, lexical)| (name.as_str(), lexical.as_str().expect("lexical")))
        .collect();
    EntityRegistry::from_provider(pairs)
}

#[test]
fn every_row_answers_as_the_ruby_pins_expect() {
    let data: Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["entity_registry"]
        .as_array()
        .expect("entity_registry cases");
    assert!(rows.len() >= 10, "expected the full entity family");
    let mut failures = Vec::new();
    for row in rows {
        let seed = &row["seed"];
        let qname = row["qname"].as_str().expect("qname");
        let qcp = row["qcp"].as_u64().expect("qcp") as u32;
        let restore_entities = row["restore_entities"].as_bool().expect("restore_entities");
        let mode = row["mode"].as_str().expect("mode");

        let expected_declared = row["declared"].as_bool().expect("declared");
        let expected_codepoint = row["codepoint"].as_u64().map(|v| v as u32);
        let expected_names: Vec<&str> = row["names"]
            .as_array()
            .expect("names")
            .iter()
            .map(|v| v.as_str().expect("name"))
            .collect();
        let expected_primary = row["primary"].as_str();
        let expected_standard = row["standard"].as_bool().expect("standard");
        let expected_restore = row["restore"].as_bool().expect("restore");

        let registry = seed_registry(seed);
        let label = format!("seed {seed}, qname {qname:?}, qcp {qcp}");

        let answers = [
            (
                format!("declared({qname:?})"),
                registry.declared(qname).to_string(),
            ),
            (
                format!("codepoint_for_name({qname:?})"),
                format!("{:?}", registry.codepoint_for_name(qname)),
            ),
            (
                format!("names_for_codepoint({qcp})"),
                format!("{:?}", registry.names_for_codepoint(qcp)),
            ),
            (
                format!("primary_name_for_codepoint({qcp})"),
                format!("{:?}", registry.primary_name_for_codepoint(qcp)),
            ),
            (
                format!("standard_entity({qcp})"),
                format!("{:?}", EntityRegistry::standard_entity(qcp)),
            ),
        ];
        let expected = [
            (
                format!("declared({qname:?})"),
                expected_declared.to_string(),
            ),
            (
                format!("codepoint_for_name({qname:?})"),
                format!("{expected_codepoint:?}"),
            ),
            (
                format!("names_for_codepoint({qcp})"),
                format!("{expected_names:?}"),
            ),
            (
                format!("primary_name_for_codepoint({qcp})"),
                format!("{expected_primary:?}"),
            ),
            (
                format!("standard_entity({qcp})"),
                expected_standard.to_string(),
            ),
        ];
        for ((op, got), (_, want)) in answers.iter().zip(expected.iter()) {
            if got != want {
                failures.push(format!("{op} [{label}]: got {got}, dataset expects {want}"));
            }
        }

        let restore = registry.should_restore(qcp, restore_entities, mode);
        if restore != expected_restore {
            failures.push(format!(
                "should_restore({qcp}) [{label}]: got {restore}, dataset expects {expected_restore}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "conformance divergences:\n{}",
        failures.join("\n")
    );
}
