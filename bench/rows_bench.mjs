// The boundary budget (moxml-rs#2, docs/data-plane.md): rows/µs
// across the wasm boundary for three canonical shapes — wide-flat,
// deep-narrow, attribute-heavy — the Ruby scoreboard rows as
// template. The batched read must hold more rows/µs than the
// per-node baseline or the face is not batched enough.
import { readFile } from "node:fs/promises";
import { initSync, DocumentFace } from "../pkg/moxml.js";

const pkgDir = new URL("../pkg/", import.meta.url);
initSync({ module: await readFile(new URL("moxml_bg.wasm", pkgDir)) });

function wideFlat(n) {
  const kids = Array.from({ length: n }, (_, i) => `<item i="${i}">v${i}</item>`);
  return `<r>${kids.join("")}</r>`;
}

function deepNarrow(d) {
  let inner = "leaf";
  for (let i = 0; i < d; i++) inner = `<l${i}>${inner}</l${i}>`;
  return `<r>${inner}</r>`;
}

function attrHeavy(n, attrsPerEl) {
  const kids = Array.from({ length: n }, (_, i) => {
    const attrs = Array.from(
      { length: attrsPerEl },
      (_, j) => `a${j}="v${i}-${j}"`,
    ).join(" ");
    return `<e ${attrs}></e>`;
  });
  return `<r>${kids.join("")}</r>`;
}

const SHAPES = [
  ["wide-flat (2000 elements, 1 attr)", wideFlat(2000)],
  ["deep-narrow (depth 240)", deepNarrow(240)],
  ["attribute-heavy (500 elements x 8 attrs)", attrHeavy(500, 8)],
];

const WARMUP = 5;
const RUNS = 30;

function measure(fn) {
  const start = process.hrtime.bigint();
  fn();
  return Number(process.hrtime.bigint() - start) / 1000; // µs
}

function bestOf(fn) {
  for (let i = 0; i < WARMUP; i++) fn();
  let best = Infinity;
  for (let i = 0; i < RUNS; i++) best = Math.min(best, measure(fn));
  return best;
}

let headroom = Infinity;
let fail = 0;

for (const [label, xml] of SHAPES) {
  const bytes = new TextEncoder().encode(xml);
  const doc = new DocumentFace(bytes);
  const n = doc.element_rows().length;

  const batchUs = bestOf(() => doc.element_rows());
  const batchRops = n / batchUs;

  // The per-node baseline: N crossings of the single-row face.
  const perNodeUs = bestOf(() => {
    for (let i = 0; i < n; i++) doc.element_row(i);
  });
  const perNodeRops = n / perNodeUs;

  const ratio = batchRops / perNodeRops;
  headroom = Math.min(headroom, ratio);

  console.log(
    `${label}: ${n} rows | batch ${batchRops.toFixed(1)} rows/µs` +
    ` | per-node ${perNodeRops.toFixed(1)} rows/µs` +
    ` | batch ${ratio.toFixed(1)}x the baseline`,
  );
  if (ratio <= 1) {
    console.log(`  FAIL: the batched read does not beat the per-node baseline`);
    fail++;
  }
}

console.log(
  fail === 0
    ? `boundary budget: held (worst batch advantage ${headroom.toFixed(1)}x)`
    : `boundary budget: VIOLATED on ${fail} shape(s)`,
);
process.exit(fail === 0 ? 0 : 1);
