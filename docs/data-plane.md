# The data plane — the batched-first public surface (moxml-rs#2)

Status: **proposal** for the owner to bless (moxml-rs#2 records the
requirement; this document proposes the concrete faces). The rule it
implements: **cross a language boundary once per batch, never per
node** — the Ruby ledger's evidence (Plan 1.0×, StructPlan 1.7–2.6×,
SAX drain 1-alloc/element, attribute_pairs ~2×) is the template, and
for moxml-ts the wasm boundary makes it existential.

## Faces

The primary surface is three row-shaped reads plus the write plane;
node objects (`WasmNode`) remain the convenience/debug layer over
them, never the only path.

| face | contract | engine tier |
|---|---|---|
| `plan(doc, plan) -> rows` | the Descriptor/plan ABI (leptris-rs `descriptor_abi`): compile a plan tree once, materialize a subtree in ONE native pass; rows carry (path, values, tags) in document order | exists natively — needs the wasm exposure |
| `element_rows(doc, selector) -> rows` | flat walk of the matched elements: `(ordinal, name, ns, attr-pair-run, text)` columns; attrs ride the SAME crossing (no per-node attribute hop) | new engine face (batched reader over the DOM walk) |
| `xpath_full(doc, expr)` | typed result (nodeset/boolean/number/string) — shipped leptris-wasm 1.6.0; nodes are handles but the RESULT crosses once | shipped |
| `visit(doc, selector, budget) -> events` | per-node traversal executed Rust-side under a callback budget — the ONLY sanctioned per-node shape across the boundary | new engine face (visitor) |
| writes | `create_element`/`create_text`/`add_child`/`set_attribute` shipped 1.6.0; bulk-build crossing is the leptris-ruby#204 follow-through | shipped (bulk-build pending) |
| `to_xml(doc) -> bytes` | serialization contract per the processing model | shipped |

## Row-contract naming

Every data-plane face names its row contract in the processing-model
section it implements (shape, order, encoding) — and the
cross-language conformance dataset pins the SHAPES, not only the
answers: a `data_plane_rows` family (schema 4) carries canonical
documents with the exact expected row tuples, so Ruby, Rust, and TS
runners assert byte-identical row output.

## Boundary budget (the measured rule)

`bench/rows_bench.mjs` reports rows/µs across the wasm boundary for
three canonical shapes (wide-flat, deep-narrow, attribute-heavy —
the Ruby scoreboard rows as template) and CI records the numbers on
the wasm release body next to the size budget. A face that cannot
hold its rows/µs under the per-node baseline is not batched enough.

## What this asks of the engine tier

Tracked on leptris/leptris-rs#15's follow-through: the batched
element reader and the budgeted visitor. Everything else is
exposure work over shipped ABI.
