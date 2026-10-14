// Runs Echoproof's intent judge through its real entry point (`echoproof diff --intent`)
// and grades each judged change group against its label.

import { execFile } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { score } from './grading.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ECHOPROOF = join(HERE, '../../target/release/echoproof');
const FIXTURES = join(HERE, 'fixtures');
const DEFAULT_MODEL = 'claude-opus-5-5';
const EFFORT = process.env.JUDGE_EFFORT || 'high';

/** EVAL_CASES=c01,c02 limits a run to those cases (pilots). */
export async function loadCases() {
  if (!process.env.EVAL_MOCK_URL && !process.env.ANTHROPIC_API_KEY && !process.env.ANTHROPIC_AUTH_TOKEN)
    throw new Error('ANTHROPIC_API_KEY is not set (and ANTHROPIC_WORKSPACE_ID if the key is not scoped to a workspace)');
  let cases = JSON.parse(readFileSync(join(HERE, 'cases.json'), 'utf8'));
  if (process.env.EVAL_CASES) {
    const wanted = new Set(process.env.EVAL_CASES.split(',').map(s => s.trim()));
    const unknown = [...wanted].filter(id => !cases.some(c => c.id === id));
    if (unknown.length) throw new Error(`unknown case ids in EVAL_CASES: ${unknown.join(', ')}`);
    cases = cases.filter(c => wanted.has(c.id));
  }
  for (const c of cases)
    if (!existsSync(join(FIXTURES, `${c.id}.jsonl`)))
      throw new Error(`missing fixture for ${c.id}; run eval/judge/prepare.sh first`);
  return cases.map(c => ({
    ...c,
    prompt: `${c.intent}\n\n[code changes: ${c.mutations.join(', ')}]`,
  }));
}

function runEchoproof(args, env) {
  return new Promise((resolvePromise) => {
    execFile(ECHOPROOF, args, { env, maxBuffer: 64 * 1024 * 1024 }, (error, stdout, stderr) =>
      resolvePromise({ code: error ? (error.code ?? 1) : 0, stdout, stderr }));
  });
}

/** EVAL_MOCK_URL points the judge at mock-api.py; the case id in the path selects its labels. */
function judgeEnv(input, tracePath) {
  const env = { ...process.env, ECHOPROOF_JUDGE_TRACE: tracePath };
  if (process.env.EVAL_MOCK_URL) {
    env.ANTHROPIC_BASE_URL = `${process.env.EVAL_MOCK_URL}/${input.id}`;
    env.ANTHROPIC_API_KEY = 'mock';
  }
  return env;
}

export async function runCase(input, ctx) {
  const model = ctx.model ?? DEFAULT_MODEL;
  const dir = mkdtempSync(join(tmpdir(), 'echoproof-eval-'));
  try {
    const config = join(dir, 'echoproof.toml');
    const tracePath = join(dir, 'trace.json');
    writeFileSync(config, `[judge]\nmodel = ${JSON.stringify(model)}\neffort = ${JSON.stringify(EFFORT)}\nfallbacks = true\n`);
    const { code, stdout, stderr } = await runEchoproof(
      ['--config', config, 'diff', join(FIXTURES, 'baseline.jsonl'), join(FIXTURES, `${input.id}.jsonl`),
       '--intent', input.intent, '--format', 'json', '--max-examples', '3'],
      judgeEnv(input, tracePath));
    if (code !== 0 && code !== 1) {
      const message = stderr.trim().split('\n').pop() || `echoproof exited ${code}`;
      const e = new Error(message);
      if (/declined the request/.test(message)) e.failure_class = 'refusal';
      else if (/cut off at max_tokens/.test(message)) e.failure_class = 'truncated';
      else if (/\b(429|529|5\d\d)\b|overloaded/i.test(message)) e.status = 429; // let the harness back off
      else e.failure_class = 'harness_error';
      throw e;
    }
    const report = JSON.parse(stdout);
    if (!report.judge) throw Object.assign(new Error('report was not judged'), { failure_class: 'harness_error' });
    const trace = JSON.parse(readFileSync(tracePath, 'utf8'));
    const response = trace.response;
    const answer = (response.content ?? []).filter(b => b.type === 'text').map(b => b.text).join('');
    return {
      output: report,
      transcript: [
        { role: 'system', content: trace.request.system },
        { role: 'user', content: trace.request.messages[0].content },
        { role: 'assistant', content: answer },
      ],
      model: response.model,
      usage: response.usage,
      stop_reason: response.stop_reason,
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

export async function gradeCase(input, run) {
  const s = score(input, run.output);
  const lines = s.groups.map(g =>
    `${g.verdict === g.expected ? 'ok' : 'MISMATCH'} ${g.endpoint} ${g.pattern}: judged ${g.verdict}, expected ${g.expected}. ${g.reason ?? ''}`);
  return {
    grade: s.grade,
    explanation: { gate_correct: `verdict ${run.output.verdict}\n${lines.join('\n')}` },
    meta: { counts: s.counts, groups: s.groups.map(({ endpoint, pattern, verdict, expected }) => ({ endpoint, pattern, verdict, expected })) },
  };
}

export function perfFrom(run) {
  return { in_tokens: run.usage?.input_tokens, out_tokens: run.usage?.output_tokens };
}

