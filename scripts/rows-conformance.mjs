// The data-plane row-contract conformance (moxml-rs#2): the
// element_rows tuples as JS arrays, pinned row by row against a
// canonical document — shape (columns), order (document order), and
// encoding (qualified names, null-namespace as null, flat attr-pair
// runs, character-data text). A wrong tuple here breaks the
// cross-language contract, not just this bundle.
import { readFile } from "node:fs/promises";
import { initSync, DocumentFace } from "../pkg/moxml.js";

const pkgDir = new URL("../pkg/", import.meta.url);
initSync({ module: await readFile(new URL("moxml_bg.wasm", pkgDir)) });

const DOC = [
  '<r xmlns="urn:t" xmlns:p="urn:p" k="1">',
  '  <a x="1">one</a>',
  '  <p:b><![CDATA[raw <data>]]></p:b>',
  '  <c><!-- note -->tail</c>',
  '  <d/>',
  '</r>',
].join("\n");

// [ordinal, parent, depth, name, ns, attrs, text]. Unprefixed
// children inherit the default namespace; the text column is the
// concatenation of ALL character-data rows — whitespace runs
// between elements included (the committed reference answer, not
// a filtered convenience view).
const WS = "\n  \n  \n  \n  \n";
const EXPECTED = [
  [0, null, 0, "r", "urn:t", ["k", "1"], WS],
  [1, 0, 1, "a", "urn:t", ["x", "1"], "one"],
  [2, 0, 1, "p:b", "urn:p", [], "raw <data>"],
  [3, 0, 1, "c", "urn:t", [], "tail"],
  [4, 0, 1, "d", "urn:t", [], ""],
];

const doc = new DocumentFace(new TextEncoder().encode(DOC));
const rows = doc.element_rows();

let fail = 0;
if (rows.length !== EXPECTED.length) {
  console.log(`rows: got ${rows.length}, want ${EXPECTED.length}`);
  fail++;
}
for (let i = 0; i < Math.min(rows.length, EXPECTED.length); i++) {
  const got = rows[i];
  const want = EXPECTED[i];
  const same =
    got.length === 7 &&
    got[0] === want[0] && got[1] === want[1] && got[2] === want[2] &&
    got[3] === want[3] && got[4] === want[4] &&
    got[5].length === want[5].length &&
    want[5].every((v, j) => got[5][j] === v) &&
    got[6] === want[6];
  if (!same) {
    console.log(`row ${i}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
    fail++;
  }
}

// The per-node baseline answers the same tuple.
const single = doc.element_row(2);
if (JSON.stringify(single) !== JSON.stringify(rows[2])) {
  console.log(`row_at(2): got ${JSON.stringify(single)}, batch ${JSON.stringify(rows[2])}`);
  fail++;
}

// The serialization contract rides the same handle.
const xml = doc.to_xml();
if (!xml.includes('xmlns:p="urn:p"') || !xml.includes("<a x=\"one\"".replace("one", "1"))) {
  console.log(`to_xml: unexpected shape: ${xml.slice(0, 120)}`);
  fail++;
}

// Malformed input fails loudly, never a silent face.
try {
  new DocumentFace(new TextEncoder().encode("<r><a></r>"));
  console.log("parse: malformed input did not raise");
  fail++;
} catch (e) {
  // expected
}

console.log(`rows conformance: ${EXPECTED.length} rows, ${fail} failures`);
process.exit(fail === 0 ? 0 : 1);
