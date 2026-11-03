# Native document API (`moxml::document`)

Needed by lutaml/lutaml-model#1033 (Rust XML codecs). Follows the
direction of moxml-rs#2 and [data-plane.md](data-plane.md).

## Shape

| face | surface |
|---|---|
| parse + policy | `Document::parse`, `Document::parse_with(&ParserPolicy)` |
| batch read (primary) | `Document::element_rows() -> Vec<ElementRow>`: one row per element, document order; name (prefix/local/URI), namespace declarations, attributes (with namespace), and the ordered `content` run (`Text`, `CData`, `Comment`, `ProcessingInstruction`, `Element(ordinal)`) |
| batch write (primary) | `ElementSpec` (fluent, owned) -> `Document::build` |
| serialize | `Document::to_xml`, `to_xml_with(&SerializeOptions)` |
| handle layer (convenience) | `NodeId`: `root`, `name`, `attributes`, `children`, `text`, `parent`, `append_element/text/cdata`, `set_attribute`, `declare_namespace` |
| backend seam | `trait Engine { parse, serialize }`; `ReferenceEngine` is the only backend |

Row contract: rows are in pre-order; `ordinal` is the row index (root 0);
`ElementRow.content` keeps child order, with text and CDATA distinct;
`xmlns` declarations are listed in `namespaces`, never in `attributes`;
unprefixed attributes are in no namespace.

## Parser policy

Default: **DOCTYPE is rejected** (`Error::DoctypeForbidden`), so no XXE and
no entity expansion can start. `allow_doctype: true` accepts an internal
subset with general internal entities only, bounded by
`max_entity_expansion` (bytes produced) and a nesting cap. External
entities, external subsets and parameter entities are always rejected
(`Error::ExternalEntity`). `max_depth` bounds element nesting; the parser
is iterative.

## Deterministic serialization

Declarations first (document order), then attributes in order; fixed
escaping; `<a/>` for empty elements; missing namespace declarations for
built names are added on the element that needs them; optional
two-space-style indent only for element-only content.

## Deviations from the data-plane proposal, and why

- `plan`, `visit` and `xpath_full` are not here: they are engine faces
  (leptris descriptor ABI / XPath) and need the leptris backend.
- Writes are one batch call (`ElementSpec`), not the leptris-ruby#204
  bulk-build crossing, which does not exist yet.
- No benchmark gate: there is no language boundary on the native path.

## Why the backend is not leptris yet

The `leptris` crate (1.6.5) exposes a safe `Document`/`Element`/`attributes`
read surface only. Namespace URI/prefix, text and CDATA nodes, element
construction and `add_child`/`set_attribute` exist solely in its private
`ffi` module and its wasm32 `wasmface`. It also links a system
`libleptris`, which `cargo test` here cannot assume. Once leptris-rs
publishes those as safe native APIs, a `LeptrisEngine: Engine` slots in
behind the same `Document` and the same tests. Parsing policy for that
backend also needs an engine-side entity-expansion option; until then the
default policy ("reject DOCTYPE") is the guard.
