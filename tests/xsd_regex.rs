// The regex-translator conformance runner (#496): every row of the
// `xsd_regex_translation` family must answer exactly as the Ruby pins
// expect — translatable-or-not, and what the translated regex does to
// the sample value.

use moxml::xsd_regex;

const DATASET: &str = include_str!("data/moxml-conformance.json");

#[test]
fn every_row_answers_as_the_ruby_pins_expect() {
    let data: serde_json::Value = serde_json::from_str(DATASET).expect("dataset parses");
    let rows = data["cases"]["xsd_regex_translation"]
        .as_array()
        .expect("xsd_regex_translation cases");
    assert!(rows.len() >= 20, "expected the full regex family");
    let mut failures = Vec::new();
    for row in rows {
        let pattern = row["pattern"].as_str().expect("pattern");
        let value = row["value"].as_str().expect("value");
        let matches = row["matches"].as_bool().expect("matches");
        let translatable = row["translatable"].as_bool().expect("translatable");
        match xsd_regex::translate(pattern) {
            Some(regex) => {
                if !translatable {
                    failures.push(format!("{pattern:?}: dataset says NOT translatable"));
                } else {
                    let answer = regex.is_match(value);
                    if answer != matches {
                        failures.push(format!(
                            "{pattern:?} on {value:?}: got {answer}, dataset expects {matches}"
                        ));
                    }
                }
            }
            None => {
                if translatable {
                    failures.push(format!("{pattern:?}: dataset says translatable, got None"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "conformance divergences:\n{}",
        failures.join("\n")
    );
}
