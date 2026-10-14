#!/usr/bin/env node
// Free harness checks: runs every case against eval/judge/mock-api.py and
// prints the summary for that mock mode. No API spend.
//   MODE=oracle python3 eval/judge/mock-api.py 39200 &
//   EVAL_MOCK_URL=http://127.0.0.1:39200 node eval/judge/selftest.mjs
import { loadCases, runCase, gradeCase } from './judge-app.mjs';

if (!process.env.EVAL_MOCK_URL) { console.error('set EVAL_MOCK_URL to the mock API'); process.exit(2); }
let regTotal = 0, regCaught = 0, intTotal = 0, intKept = 0, gate = 0, n = 0;
const errors = [];
for (const c of await loadCases()) {
  try {
    const run = await runCase(c, { model: 'claude-opus-5-5' });
    const g = await gradeCase(c, run);
    const k = g.meta.counts;
    regTotal += k.reg_total; regCaught += k.reg_caught; intTotal += k.int_total; intKept += k.int_kept;
    gate += g.grade.gate_correct; n++;
  } catch (e) {
    errors.push(`${c.id}: [${e.failure_class ?? e.status ?? 'error'}] ${e.message}`);
  }
}
const pct = (k, t) => t ? `${(100 * k / t).toFixed(0)}% (${k}/${t})` : 'n/a';
console.log(`scored ${n}, errors ${errors.length} | recall ${pct(regCaught, regTotal)} | specificity ${pct(intKept, intTotal)} | gate ${pct(gate, n)}`);
for (const e of errors.slice(0, 3)) console.log('  ' + e);
