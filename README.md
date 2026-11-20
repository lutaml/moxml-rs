# moxml-rs

The moxml XML processing model — Rust implementation.

moxml is a *correct/harmonized* XML model engine: it does not parse XML itself. Engines parse and evaluate (leptris, yeptris, …); moxml owns the single committed answer to every semantic question — node identity and document order, namespaces, XPath 1.0 result semantics, serialization, canonicalization, XSD validation semantics, error shapes — so that *which engine is running* is a deployment decision, never an API decision.

## The contract

The normative contract is the moxml processing model: [`docs/_references/processing-model.adoc`](https://github.com/lutaml/moxml/blob/main/docs/_references/processing-model.adoc) in lutaml/moxml, pinned executable expectations in `spec/consistency/`. It is living and versioned — not a frozen interface snapshot. Every face implemented here carries mirrored conformance pins so cross-language agreement is tested, never assumed.

## Scope of this crate

- **Model + harmonization layer in Rust**: SchemaRules (strict/negative wildcards, xsi:nil, pattern-facet dismissal with the XSD-regex dialect translator, substitution-group analysis, builtin lexical table, structured error rows, schema-set compilation semantics), document order, error taxonomy.
- **Engine binding**: leptris-rs as the primary backend.

Non-goals (v1): quick-xml or DOM/xmldom-style alternate backends (may join later, only behind the same cross-backend conformance gates), JSON/YAML/toml data formats, HTML parsing.

## Embedding in a WASM library

The default `wasm-bindings` feature exports the standalone JavaScript model
faces. Existing `wasm-pack build` commands keep those exports. A Rust consumer
that only uses the model/document APIs inside its own WASM library can set
`default-features = false` on its Moxml dependency to omit the JavaScript face
and its `wasm-bindgen`/`js-sys` dependencies. Native model APIs are unchanged.

Cargo combines features for the same package revision: another dependency
enabling `wasm-bindings` on that revision enables its exports for the binary.
When multiple Moxml revisions are linked, enable the face on at most one to
avoid duplicate export names.

## Roadmap

See [docs/architecture.md](docs/architecture.md) for the layered architecture and phased plan (shared across moxml, moxml-rs, moxml-ts).

## Team

Permissions mirror [lutaml/moxml](https://github.com/lutaml/moxml) (staff admin; developers/ci/editors push). All changes via PRs.
