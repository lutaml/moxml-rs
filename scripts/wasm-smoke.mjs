// The wasm-face conformance smoke (moxml-rs): drives every row of the
// vendored dataset through the wasm-bindgen face — the same runner
// contract as tests/, pointed at the bundle wasm-pack emits. Missing
// row coverage or a wrong answer fails the build.
import { readFile } from "node:fs/promises";
import {
  initSync,
  valid_lexical,
  WildcardFace,
  RegistryFace,
} from "../pkg/moxml.js";

const pkgDir = new URL("../pkg/", import.meta.url);
initSync({ module: await readFile(new URL("moxml_bg.wasm", pkgDir)) });
const data = JSON.parse(
  await readFile(new URL("../tests/data/moxml-conformance.json", pkgDir), "utf8"),
);
let fail = 0;
let total = 0;

for (const row of data.cases.builtin_lexical) {
  total++;
  const got = valid_lexical(row.type, row.lexical);
  if (got !== row.valid) {
    fail++;
    console.log("lexical", row.type, JSON.stringify(row.lexical), "got", got, "want", row.valid);
  }
}

for (const row of data.cases.schema_wildcards) {
  total++;
  const w = new WildcardFace(row.constraint, row.process, row.not_namespace, row.not_qname);
  if (w.covers(row.item_namespace) !== row.covers) {
    fail++;
    console.log("covers", row.constraint, row.item_namespace);
  }
  const ex = w.negative_exclusion(row.item_qname, row.item_namespace, row.defined, row.sibling_names);
  if ((ex ?? null) !== row.exclusion) {
    fail++;
    console.log("exclusion", row.constraint, row.item_qname, "got", ex, "want", row.exclusion);
  }
  w.free();
}

for (const row of data.cases.entity_registry) {
  total++;
  const names = Object.keys(row.seed);
  const lexicals = names.map((n) => row.seed[n]);
  const r = new RegistryFace(names, lexicals);
  const answers = [
    r.declared(row.qname) === row.declared,
    (r.codepoint_for_name(row.qname) ?? null) === row.codepoint,
    JSON.stringify(r.names_for_codepoint(row.qcp)) === JSON.stringify(row.names),
    (r.primary_name_for_codepoint(row.qcp) ?? null) === row.primary,
    RegistryFace.standard_entity(row.qcp) === row.standard,
    r.should_restore(row.qcp, row.restore_entities, row.mode) === row.restore,
  ];
  if (answers.some((a) => !a)) {
    fail++;
    console.log("entity", JSON.stringify(row.seed), row.qname, row.qcp, answers);
  }
  r.free();
}

console.log(`wasm smoke: ${total} rows, ${fail} divergences`);
process.exit(fail === 0 ? 0 : 1);
