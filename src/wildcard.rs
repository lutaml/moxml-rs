//! The SchemaRules wildcard face — the Rust mirror of lutaml/moxml's
//! `Moxml::XSD::SchemaRules::Wildcard` (#496): a wildcard's namespace
//! constraint and disallowed names, per XML Schema Structures
//! §3.10.1–3.10.2.
//!
//! [`Wildcard::covers`] answers whether the wildcard admits a
//! namespace; [`Wildcard::negative_exclusion`] answers why it refuses
//! an item outright (§3.10.4.1 — a refusal is a match-level failure,
//! so it applies at every processContents). `notQName` prefixed forms
//! resolve against schema bindings on the Ruby/index side; this face
//! serves the unprefixed, `##defined` and `##definedSibling` forms
//! the dataset pins.

/// The namespace spellings the face answers on: the special
/// `##local`/`##targetNamespace` roles plus real URIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Namespace {
    Local,
    Tns,
    Uri(String),
}

impl Namespace {
    /// Parses the dataset's item-namespace spelling (`"local"`,
    /// `"tns"`, or a URI).
    pub fn parse(spelling: &str) -> Namespace {
        match spelling {
            "local" => Namespace::Local,
            "tns" => Namespace::Tns,
            other => Namespace::Uri(other.to_string()),
        }
    }
}

/// Why the wildcard refuses an item outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    NotQName,
    NotNamespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Any,
    Other,
    List,
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NsEntry {
    Tns,
    Local,
    Uri(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Disallowed {
    Defined,
    Sibling,
    QName(String),
}

/// A wildcard's namespace constraint and disallowed names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wildcard {
    #[allow(dead_code)]
    process: String,
    mode: Mode,
    namespaces: Vec<NsEntry>,
    disallowed: Vec<Disallowed>,
}

impl Wildcard {
    /// Builds the face from the constructor spellings the dataset
    /// pins: `constraint` in its attribute form (`None` = `##any`),
    /// `not_namespace` / `not_qname` as the pre-resolution token
    /// lists. Prefixed `notQName` tokens have no in-scope bindings on
    /// this face and are dropped — the index under-reports, never
    /// false-reports.
    pub fn new(
        constraint: Option<&str>,
        process: &str,
        not_namespace: Option<&str>,
        not_qname: &[&str],
    ) -> Wildcard {
        let (mode, namespaces) = match not_namespace {
            Some(tokens) if !tokens.trim().is_empty() => (Mode::Not, namespace_entries(tokens)),
            _ => match constraint.map(str::trim) {
                None | Some("") | Some("##any") => (Mode::Any, Vec::new()),
                Some("##other") => (Mode::Other, Vec::new()),
                Some(tokens) => (Mode::List, namespace_entries(tokens)),
            },
        };
        let disallowed = not_qname
            .iter()
            .filter(|token| !token.trim().is_empty())
            .filter_map(|token| match *token {
                "##defined" => Some(Disallowed::Defined),
                "##definedSibling" => Some(Disallowed::Sibling),
                _ => {
                    if token.contains(':') {
                        None
                    } else {
                        Some(Disallowed::QName(format!("{{}}{token}")))
                    }
                }
            })
            .collect();
        Wildcard {
            process: process.to_string(),
            mode,
            namespaces,
            disallowed,
        }
    }

    /// Whether the wildcard admits `namespace` (§3.10.4.2).
    pub fn covers(&self, namespace: &Namespace) -> bool {
        match self.mode {
            Mode::Any => true,
            Mode::Other => *namespace != Namespace::Local && *namespace != Namespace::Tns,
            Mode::Not => self.namespaces.iter().all(|e| !entry_covers(e, namespace)),
            Mode::List => self.namespaces.iter().any(|e| entry_covers(e, namespace)),
        }
    }

    /// Why the wildcard refuses an item outright, or `None` when the
    /// item's name is not excluded (§3.10.4.1 Item Valid).
    pub fn negative_exclusion(
        &self,
        qname: &str,
        namespace: &Namespace,
        defined: bool,
        sibling_names: &[String],
    ) -> Option<Exclusion> {
        for entry in &self.disallowed {
            match entry {
                Disallowed::Defined => {
                    if defined {
                        return Some(Exclusion::NotQName);
                    }
                }
                Disallowed::Sibling => {
                    if sibling_names.iter().any(|name| name == qname) {
                        return Some(Exclusion::NotQName);
                    }
                }
                Disallowed::QName(name) => {
                    if name == qname {
                        return Some(Exclusion::NotQName);
                    }
                }
            }
        }
        if self.mode == Mode::Not && self.namespaces.iter().any(|e| entry_covers(e, namespace)) {
            return Some(Exclusion::NotNamespace);
        }
        None
    }
}

fn entry_covers(entry: &NsEntry, namespace: &Namespace) -> bool {
    match (entry, namespace) {
        (NsEntry::Tns, Namespace::Tns) => true,
        (NsEntry::Local, Namespace::Local) => true,
        (NsEntry::Uri(a), Namespace::Uri(b)) => a == b,
        _ => false,
    }
}

fn namespace_entries(tokens: &str) -> Vec<NsEntry> {
    tokens
        .split_whitespace()
        .map(|entry| match entry {
            "##targetNamespace" => NsEntry::Tns,
            "##local" => NsEntry::Local,
            other => NsEntry::Uri(other.to_string()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_covers_everything() {
        let w = Wildcard::new(Some("##any"), "strict", None, &[]);
        assert!(w.covers(&Namespace::Local));
        assert!(w.covers(&Namespace::Tns));
        assert!(w.covers(&Namespace::Uri("urn:a".into())));
    }

    #[test]
    fn other_excludes_local_and_tns() {
        let w = Wildcard::new(Some("##other"), "strict", None, &[]);
        assert!(!w.covers(&Namespace::Local));
        assert!(!w.covers(&Namespace::Tns));
        assert!(w.covers(&Namespace::Uri("urn:other".into())));
    }

    #[test]
    fn list_and_not_entries_share_one_convention() {
        let w = Wildcard::new(Some("urn:a urn:b"), "strict", None, &[]);
        assert!(w.covers(&Namespace::Uri("urn:a".into())));
        assert!(!w.covers(&Namespace::Uri("urn:c".into())));

        let w = Wildcard::new(Some("urn:a"), "strict", Some("urn:z"), &[]);
        assert!(w.covers(&Namespace::Uri("urn:a".into())));
        assert!(!w.covers(&Namespace::Uri("urn:z".into())));
    }

    #[test]
    fn exact_not_qname_excludes_at_every_process() {
        for process in ["strict", "lax", "skip"] {
            let w = Wildcard::new(Some("##any"), process, None, &["x"]);
            assert_eq!(
                w.negative_exclusion("{}x", &Namespace::Local, false, &[]),
                Some(Exclusion::NotQName),
                "exclusion is match-level, {process} included"
            );
        }
    }

    #[test]
    fn defined_and_sibling_forms() {
        let w = Wildcard::new(Some("##any"), "strict", None, &["##defined"]);
        assert!(w
            .negative_exclusion("{}x", &Namespace::Local, false, &[])
            .is_none());
        assert_eq!(
            w.negative_exclusion("{}x", &Namespace::Local, true, &[]),
            Some(Exclusion::NotQName)
        );

        let w = Wildcard::new(Some("##any"), "strict", None, &["##definedSibling"]);
        assert_eq!(
            w.negative_exclusion("{}x", &Namespace::Local, false, &["{}x".to_string()]),
            Some(Exclusion::NotQName)
        );
    }
}
