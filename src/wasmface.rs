//! The wasm-bindgen face over the pure-logic model faces — the
//! architecture's "one implementation of the model, bound per
//! language": moxml-ts consumes THESE faces instead of re-porting
//! them. Only compiled for wasm32 (the dependency is target-gated in
//! Cargo.toml; native builds never see it).
//!
//! Thin delegation, one committed answer per face: lexical table
//! ([`valid_lexical`]), XSD-regex translation to the JS dialect
//! ([`translate_regex_js`]), wildcard admission/exclusion
//! ([`WildcardFace`]), entity registry ([`RegistryFace`]).

use wasm_bindgen::prelude::*;

use crate::entity_registry::EntityRegistry;
use crate::lexical;
use crate::wildcard::{Exclusion, Namespace, Wildcard};
use crate::xsd_regex;

/// Whether a lexical value is valid for a built-in type.
/// Err = the type is not in the table (loud, never a silent answer).
#[wasm_bindgen]
pub fn valid_lexical(builtin: &str, value: &str) -> Result<bool, JsError> {
    lexical::valid(builtin, value).map_err(|e| JsError::new(&e.to_string()))
}

/// Translates an XSD pattern to the **JavaScript** RegExp dialect —
/// compile the returned string with the `u` flag and test values
/// through the JS engine. `undefined` when the pattern uses a
/// construct outside the translated subset: the engine's answer
/// then stands.
#[wasm_bindgen]
pub fn translate_regex_js(pattern: &str) -> Option<String> {
    xsd_regex::translate_to_js(pattern)
}

/// Every built-in type name the table serves.
#[wasm_bindgen]
pub fn lexical_names() -> Vec<String> {
    lexical::names().into_iter().map(String::from).collect()
}

/// The wildcard face: namespace admission (§3.10.4.2) and the
/// match-level negative exclusion (§3.10.4.1).
#[wasm_bindgen]
pub struct WildcardFace {
    inner: Wildcard,
}

#[wasm_bindgen]
impl WildcardFace {
    /// Builds from the constructor spellings the dataset pins
    /// (constraint in attribute form, `notNamespace`/`notQName` as
    /// pre-resolution token lists).
    #[wasm_bindgen(constructor)]
    pub fn new(
        constraint: Option<String>,
        process: &str,
        not_namespace: Option<String>,
        not_qname: Vec<String>,
    ) -> WildcardFace {
        let tokens: Vec<&str> = not_qname.iter().map(String::as_str).collect();
        WildcardFace {
            inner: Wildcard::new(
                constraint.as_deref(),
                process,
                not_namespace.as_deref(),
                &tokens,
            ),
        }
    }

    /// Whether the wildcard admits the namespace (`"local"`,
    /// `"tns"`, or a URI).
    pub fn covers(&self, namespace: &str) -> bool {
        self.inner.covers(&Namespace::parse(namespace))
    }

    /// Why the wildcard refuses an item outright:
    /// `"not_qname"` / `"not_namespace"` (the dataset's committed
    /// spellings), or undefined.
    pub fn negative_exclusion(
        &self,
        qname: &str,
        namespace: &str,
        defined: bool,
        sibling_names: Vec<String>,
    ) -> Option<String> {
        match self.inner.negative_exclusion(
            qname,
            &Namespace::parse(namespace),
            defined,
            &sibling_names,
        ) {
            Some(Exclusion::NotQName) => Some("not_qname".to_string()),
            Some(Exclusion::NotNamespace) => Some("not_namespace".to_string()),
            None => None,
        }
    }
}

/// The entity-registry face: bidirectional lookup, primary-name
/// preference, and the restoration policy.
#[wasm_bindgen]
pub struct RegistryFace {
    inner: EntityRegistry,
}

#[wasm_bindgen]
impl RegistryFace {
    /// Builds from parallel `(names, lexicals)` arrays in provider
    /// order — the `:custom` provider path the dataset pins, so
    /// multi-character and `"\\uXXXX"` lexicals parse exactly as
    /// they do in Ruby.
    #[wasm_bindgen(constructor)]
    pub fn new(names: Vec<String>, lexicals: Vec<String>) -> Result<RegistryFace, JsError> {
        if names.len() != lexicals.len() {
            return Err(JsError::new("names and lexicals must have the same length"));
        }
        let pairs = names
            .iter()
            .map(String::as_str)
            .zip(lexicals.iter().map(String::as_str));
        Ok(RegistryFace {
            inner: EntityRegistry::from_provider(pairs),
        })
    }

    pub fn declared(&self, name: &str) -> bool {
        self.inner.declared(name)
    }

    /// The Unicode codepoint for a name, or undefined.
    pub fn codepoint_for_name(&self, name: &str) -> Option<u32> {
        self.inner.codepoint_for_name(name)
    }

    /// All names for a codepoint, in provider order.
    pub fn names_for_codepoint(&self, codepoint: u32) -> Vec<String> {
        self.inner
            .names_for_codepoint(codepoint)
            .into_iter()
            .map(String::from)
            .collect()
    }

    /// The primary name for a codepoint (lowercase preferred,
    /// first-name fallback), or undefined.
    pub fn primary_name_for_codepoint(&self, codepoint: u32) -> Option<String> {
        self.inner
            .primary_name_for_codepoint(codepoint)
            .map(String::from)
    }

    /// Whether a codepoint is one of the five XML predefined entities.
    pub fn standard_entity(codepoint: u32) -> bool {
        EntityRegistry::standard_entity(codepoint)
    }

    /// The restoration policy: standard entities restore regardless
    /// of config; other known codepoints restore only with
    /// `restore_entities` (strict answers as lenient).
    pub fn should_restore(
        &self,
        codepoint: u32,
        restore_entities: bool,
        restoration_mode: &str,
    ) -> bool {
        self.inner
            .should_restore(codepoint, restore_entities, restoration_mode)
    }
}
