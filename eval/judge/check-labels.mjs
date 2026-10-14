#!/usr/bin/env node
// Fails unless every case's labels match exactly the change groups of its fixture.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { groupsOf, labelFor } from './grading.mjs';

const cases = JSON.parse(readFileSync(new URL('./cases.json', import.meta.url)));
const bin = new URL('../../target/release/parity', import.meta.url).pathname;
const fix = new URL('./fixtures/', import.meta.url).pathname;
let problems = 0;
for (const c of cases) {
  let out;
  try {
    out = execFileSync(bin, ['--config', '/dev/null', 'diff', `${fix}baseline.jsonl`, `${fix}${c.id}.jsonl`, '--format', 'json'], { encoding: 'utf8' });
  } catch (e) { out = e.stdout; } // exit 1 = regression verdict, still a report
  const groups = groupsOf(JSON.parse(out));
  const unlabeled = groups.filter(g => !labelFor(c, g));
  const unused = c.labels.filter(l => !groups.some(g => labelFor(c, g) === l));
  const line = groups.map(g => `${g.endpoint} ${g.pattern} ${g.kind} x${g.count}`).join(' | ');
  if (unlabeled.length || unused.length) {
    problems++;
    console.log(`FAIL ${c.id}: ${line}`);
    for (const g of unlabeled) console.log(`    unlabeled group: ${g.endpoint} ${g.pattern} (${g.kind})`);
    for (const l of unused) console.log(`    label matches no group: ${l.endpoint} ${l.field}`);
  } else {
    console.log(`ok   ${c.id}: ${line}`);
  }
}
if (problems) { console.log(`${problems} case(s) need label fixes`); process.exit(1); }
console.log(`all ${cases.length} cases fully labelled`);
