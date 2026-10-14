#!/usr/bin/env node
// Group-level confusion matrix for one variant, from results.jsonl.
//   node eval/judge/summarize.mjs eval/judge/runs/intent-judge/baseline
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
if (!dir) { console.error('usage: summarize.mjs <variant dir>'); process.exit(2); }
const rows = readFileSync(join(dir, 'results.jsonl'), 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l));
const errors = existsSync(join(dir, 'errors.jsonl'))
  ? readFileSync(join(dir, 'errors.jsonl'), 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l)) : [];

// Wilson 95% interval for k successes out of n.
function wilson(k, n) {
  if (!n) return [NaN, NaN];
  const z = 1.96, p = k / n, d = 1 + z * z / n;
  const c = (p + z * z / (2 * n)) / d, h = z * Math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d;
  return [Math.max(0, c - h), Math.min(1, c + h)];
}
const pct = x => `${(100 * x).toFixed(1)}%`;
const fmt = (k, n) => n ? `${pct(k / n)} (${k}/${n}, 95% CI ${wilson(k, n).map(pct).join('-')})` : 'n/a';

let regTotal = 0, regCaught = 0, intTotal = 0, intKept = 0, gate = 0, unclear = 0, groups = 0;
let inTok = 0, outTok = 0;
for (const r of rows) {
  const c = r.meta.counts;
  regTotal += c.reg_total; regCaught += c.reg_caught; intTotal += c.int_total; intKept += c.int_kept;
  gate += r.grade.gate_correct;
  for (const g of r.meta.groups) { groups++; if (g.verdict === 'unclear') unclear++; }
  inTok += r.usage?.input_tokens ?? 0; outTok += r.usage?.output_tokens ?? 0;
}
// Consistency: does every rep of a case reach the same gate decision?
const byCase = new Map();
for (const r of rows) byCase.set(r.prompt_id, [...(byCase.get(r.prompt_id) ?? []), r.grade.gate_correct]);
const multi = [...byCase.values()].filter(v => v.length > 1);
const stable = multi.filter(v => v.every(x => x === v[0])).length;

console.log(`rows: ${rows.length} (${byCase.size} cases), failed attempts: ${errors.length}`);
console.log(`regressions caught (recall):        ${fmt(regCaught, regTotal)}`);
console.log(`intended kept (specificity):        ${fmt(intKept, intTotal)}`);
console.log(`false alarms on intended groups:    ${fmt(intTotal - intKept, intTotal)}`);
console.log(`gate correct per case run:          ${fmt(gate, rows.length)}`);
console.log(`groups judged unclear:              ${fmt(unclear, groups)}`);
if (multi.length) console.log(`cases with the same gate every rep: ${fmt(stable, multi.length)}`);
if (rows.length) console.log(`tokens per run: ${Math.round(inTok / rows.length)} in / ${Math.round(outTok / rows.length)} out (mean)`);
const wrong = rows.filter(r => !r.grade.gate_correct).map(r => `${r.prompt_id}#${r.rep}`);
if (wrong.length) console.log(`wrong gate: ${wrong.join(', ')}`);
