//! The entity-registry face — the Rust mirror of lutaml/moxml's
//! `Moxml::EntityRegistry` (#496): a knowledge base of XML entity
//! definitions (W3C XML Core WG Character Entities) with
//! bidirectional lookup and the restoration policy.
//!
//! The face is built from provider data — `(name, lexical)` pairs in
//! provider order — so lexical parsing runs exactly as the Ruby
//! `:custom` provider path does. Committed semantics: a
//! multi-character lexical (an entity naming a sequence) maps to its
//! FIRST character's codepoint; an empty lexical is dropped; the
//! reverse map keeps provider order with the lowercase name preferred
//! as primary; the five predefined entities restore regardless of
//! config; strict restoration answers as lenient until DTD parsing
//! exists.
//!
//! The `"\\uXXXX"` escape body parses as a leading hex run (Ruby's
//! `String#to_i(16)` shape for the spellings the dataset pins; sign
//! and underscore variants are outside the translated subset). The
//! Ruby face NFC-normalizes the escape spelling first — a no-op on
//! its ASCII characters, so the Rust mirror skips it.

/// The five XML predefined entities' codepoints (XML spec §4.6).
pub const STANDARD_CODEPOINTS: [u32; 5] = [0x26, 0x3C, 0x3E, 0x22, 0x27];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityRegistry {
    by_name: Vec<(String, u32)>,
    by_codepoint: Vec<(u32, Vec<String>)>,
}

impl EntityRegistry {
    /// Builds the registry from `(name, lexical)` pairs in provider
    /// order. Entries whose lexical does not resolve to a codepoint
    /// (empty lexicals) are dropped — the registry under-reports,
    /// never false-reports.
    pub fn from_provider<'a, I>(seed: I) -> EntityRegistry
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut registry = EntityRegistry::default();
        for (name, lexical) in seed {
            let Some(codepoint) = parse_codepoint(lexical) else {
                continue;
            };
            if registry.by_name.iter().any(|(n, _)| n == name) {
                continue;
            }
            registry.by_name.push((name.to_string(), codepoint));
            match registry
                .by_codepoint
                .iter_mut()
                .find(|(cp, _)| *cp == codepoint)
            {
                Some((_, names)) => names.push(name.to_string()),
                None => registry
                    .by_codepoint
                    .push((codepoint, vec![name.to_string()])),
            }
        }
        registry
    }

    /// Whether an entity name is declared.
    pub fn declared(&self, name: &str) -> bool {
        self.codepoint_for_name(name).is_some()
    }

    /// The Unicode codepoint for an entity name.
    pub fn codepoint_for_name(&self, name: &str) -> Option<u32> {
        self.by_name
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, cp)| *cp)
    }

    /// All entity names for a codepoint, in provider order.
    pub fn names_for_codepoint(&self, codepoint: u32) -> Vec<&str> {
        self.by_codepoint
            .iter()
            .find(|(cp, _)| *cp == codepoint)
            .map(|(_, names)| names.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// The primary (preferred) entity name for a codepoint:
    /// the lowercase name when one exists, else the first name.
    pub fn primary_name_for_codepoint(&self, codepoint: u32) -> Option<&str> {
        let names = self.names_for_codepoint(codepoint);
        let first = names.first()?;
        names
            .iter()
            .find(|n| **n == n.to_lowercase())
            .copied()
            .or(Some(*first))
    }

    /// Whether a codepoint is one of the 5 XML predefined entities.
    pub fn standard_entity(codepoint: u32) -> bool {
        STANDARD_CODEPOINTS.contains(&codepoint)
    }

    /// Whether an entity reference should be restored for a
    /// codepoint. Standard XML entities always restore (required by
    /// the XML spec); other known codepoints restore only when
    /// `restore_entities` is on — the `strict` mode answers as
    /// `lenient` until DTD parsing exists.
    pub fn should_restore(
        &self,
        codepoint: u32,
        restore_entities: bool,
        restoration_mode: &str,
    ) -> bool {
        let Some(_name) = self.primary_name_for_codepoint(codepoint) else {
            return false;
        };
        if Self::standard_entity(codepoint) {
            return true;
        }
        if !restore_entities {
            return false;
        }
        matches!(restoration_mode, "lenient" | "strict")
    }
}

fn parse_codepoint(lexical: &str) -> Option<u32> {
    if let Some(body) = lexical.strip_prefix("\\u") {
        let hex = body
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect::<String>();
        Some(u32::from_str_radix(&hex, 16).unwrap_or(0))
    } else {
        lexical.chars().next().map(|c| c as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EntityRegistry {
        EntityRegistry::from_provider([
            ("NonBreakingSpace", "\u{00A0}"),
            ("nbsp", "\u{00A0}"),
            ("fjlig", "fj"),
            ("empty", ""),
        ])
    }

    #[test]
    fn forward_and_reverse_lookup_keeps_provider_order() {
        let r = sample();
        assert!(r.declared("nbsp"));
        assert_eq!(r.codepoint_for_name("nbsp"), Some(0xA0));
        assert_eq!(
            r.names_for_codepoint(0xA0),
            vec!["NonBreakingSpace", "nbsp"]
        );
        assert_eq!(r.primary_name_for_codepoint(0xA0), Some("nbsp"));
    }

    #[test]
    fn multi_character_lexical_maps_to_first_char() {
        let r = sample();
        assert_eq!(r.codepoint_for_name("fjlig"), Some(102));
        assert!(!r.declared("empty"));
    }

    #[test]
    fn escape_lexical_parses_hex_prefix() {
        let r = EntityRegistry::from_provider([("esc", "\\u00A0")]);
        assert_eq!(r.codepoint_for_name("esc"), Some(0xA0));
    }

    #[test]
    fn standard_entities_restore_regardless_of_config() {
        let r = EntityRegistry::from_provider([("amp", "&"), ("nbsp", "\u{00A0}")]);
        assert!(EntityRegistry::standard_entity(0x26));
        assert!(r.should_restore(0x26, false, "lenient"));
        assert!(!r.should_restore(0xA0, false, "lenient"));
        assert!(r.should_restore(0xA0, true, "lenient"));
        assert!(r.should_restore(0xA0, true, "strict"));
    }

    #[test]
    fn unknown_codepoint_never_restores() {
        let r = sample();
        assert!(!r.should_restore(9999, true, "lenient"));
    }
}
