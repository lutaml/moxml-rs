//! The moxml XML processing model — Rust implementation.
//!
//! moxml is a *correct/harmonized* XML model engine: engines parse and
//! evaluate; moxml owns the single committed answer to every semantic
//! question. This crate carries the pure-logic model faces (the
//! harmonization machinery) so every language binding answers
//! identically; the conformance dataset in `tests/data/` pins the
//! faces against the Ruby implementation.
//!
//! Phase 1 face: the XSD built-in lexical table
//! ([`lexical::valid`]) — one committed answer for the Datatypes
//! Part 2 lexical spaces, mirrored from lutaml/moxml's
//! `Moxml::XSD::BuiltinLexical` (#496/#498).

pub mod entity_registry;
pub mod lexical;
pub mod wildcard;
pub mod xsd_regex;

#[cfg(target_arch = "wasm32")]
pub mod wasmface;
