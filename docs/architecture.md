# moxml cross-language architecture

Status: adopted direction (2026-10-29). Parent program: lutaml/moxml. Languages: Ruby (today), Rust (moxml-rs), TypeScript (moxml-ts).

## Vision

moxml is a **correct/harmonized XML model engine that does not parse**. Engines parse and evaluate; moxml owns the single committed answer to every semantic question. Three layers, three homes:

| tier | owns | today | target |
|---|---|---|---|
| **Engine** — parse, XPath 1.0, c14n, XSD/RNG evaluation | libleptris (pure C99), yeptris, nokogiri, ox, … | per-language engines; Opal gets an oga fork + rexml | **one engine everywhere**: libleptris compiled to wasm32 for JS, native elsewhere |
| **Model + harmonization** — SchemaRules, builtin lexical table, XSD-regex dialect translation, dismissal/vouch logic, document order, error taxonomy, entity registry, orchestration | moxml (Ruby) | one copy, transpiled a second time under Opal | **moxml-rs**: one implementation in Rust, bound per language |
| **Binding** — per-language facade | moxml (Ruby), none for JS | Opal-compiled Ruby | moxml-rs (Rust native) + **moxml-ts** (wasm-bindgen over moxml-rs + leptris-wasm) |

The deep win: Opal multiplies the model tier (Ruby semantics + JS semantics that must be kept in agreement by hand — the same "two implementations must agree" hazard we fight with engines, self-inflicted). Under this architecture the model has **one** implementation; languages are bindings.

## The contract

The normative cross-language contract is the moxml **processing model** (`docs/_references/processing-model.adoc` in lutaml/moxml), pinned by `spec/consistency/`. Notes that settle past debates:

- It is **living and versioned**, not a frozen interface snapshot. What is frozen is the model document's authority, not its contents.
- It is **not a small interface** (~ten methods / "XPath-lite" understates it by two orders of magnitude): eleven node kinds with identity and document-order semantics (incl. the parent-of-attribute contract), full XPath 1.0, serialization with EOL/indent/entity-escaping contracts, c14n + digest, XSD validation with SchemaRules (strict + negative wildcards, pattern dismissal via the XSD-regex translator, substitution analysis, ~53-type builtin lexical table, structured error rows, version face, schema-set compilation), RELAX NG, XSLT, SAX, builder DSL, entity registry, loud-degradation error hierarchy.
- Per-language interfaces **derive** from the model's face list; they do not replace it.
- **XPath 1.0 is full, and free**: leptris implements it natively; the contract pins results, the engine does the work (same as in Ruby).

## The load-bearing deliverable: cross-language conformance

The consistency suite becomes multi-target: identical fixtures and expected answers run against the Ruby, Rust, and TS implementations. Without it, three implementations = three divergent models within a year. The seed is mechanical: every pure-logic pin already in `spec/consistency` (lexical table rows, XSD-regex translations, SchemaRules vouch/dismiss decisions) is mirrored into the Rust crate's tests as it is implemented.

## Backend multiplexing policy

- **v1: leptris only** (native via leptris-rs; wasm via leptris-wasm). One engine everywhere is what makes "deployment decision, not API decision" hold across languages.
- DOM / xmldom / quick-xml as *alternate backends* may join later — but only behind the same cross-backend conformance gates the Ruby adapter matrix enforces today. Admitting browser DOM quirks ungoverned would restart the entire harmonization war in JS.

## Phased plan

1. **moxml-rs skeleton** (pure addition, nothing breaks): crate + the pure-logic model faces — builtin lexical table, XSD-regex translation, SchemaRules index + dismissals, entity registry — each with mirrored pins.
2. **Engine to wasm**: libleptris → wasm32 build + wasm-bindgen bindings through leptris-rs (engine-side work, tracked on leptris/leptris).
3. **moxml-ts package**: npm artifact = moxml-rs + leptris-wasm. Acceptance baseline = the faces the Opal surface pinned (parse, XPath, serialize, builder, entities — the Opal suite's expectations transfer verbatim).
4. **Delete Opal** from lutaml/moxml: vendored opal-oga/opal-ruby-ll forks, the 2,138-line generated entity table (`entity_registry_opal_data.rb`), `lib/compat/opal/` tree, config branches across ~10 lib files, `.github/workflows/opal.yml`, Gemfile path gems + opal dev deps. Removal happens only here — the Opal surface keeps working until the TS package covers its pinned faces.
