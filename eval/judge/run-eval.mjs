#!/usr/bin/env node
// Eval runner for Parity's intent judge, built from the claude-api skill's runner scaffold.
//   node eval/judge/run-eval.mjs --flow eval/judge/runs/intent-judge --variant baseline --reps 3
import { mkdirSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { gradeCase, loadCases, perfFrom, runCase } from './judge-app.mjs';
import { WIN, appendFileNoFollow, eprint, isSymlink, lexists, mkdirNoFollow, readFileNoFollow, readIfPresent, setFlowRoot, writeFileNoFollow } from './runner/fs.mjs';
import { REF_EXTS, checkHarness, parseArgs, pathSafeId, withBackoff, withTimeout } from './runner/harness.mjs';

let workersStarted = false;

function preflight(args) {
  args.flow = args.flow.replace(WIN ? /(.)[\\/]+$/ : /(.)\/+$/, '$1');
  const segments = args.flow.split(WIN ? /[\\/]/ : '/');
  if (segments.some(s => s === '.' || s === '..')) {
    eprint(`refusing to run: --flow must not contain '.' or '..' segments, got '${args.flow}'`);
    process.exit(2);
  }
  const vdir = join(args.flow, args.variant);
  for (const p of [args.flow, join(args.flow, 'baseline'), vdir, join(vdir, 'traces'), join(vdir, 'results.jsonl'),
                   join(vdir, 'errors.jsonl'), join(vdir, 'progress.txt'), join(args.flow, 'baseline', 'ref'),
                   join(args.flow, '_state.json')])
    if (isSymlink(p)) { eprint(`refusing to run: ${p} is a symlink`); process.exit(2); }
  if (!isAbsolute(args.flow)) {
    let walk = '';
    for (const part of segments.filter(Boolean).slice(0, -1)) {
      walk = walk ? join(walk, part) : part;
      if (isSymlink(walk)) { eprint(`refusing to run: ${walk} is a symlink (ancestor of --flow)`); process.exit(2); }
    }
  }
  mkdirSync(args.flow, { recursive: true });
  setFlowRoot(args.flow);
  mkdirNoFollow(join(vdir, 'traces'));
  return vdir;
}

function loadState(statePath) {
  const text = readIfPresent(statePath);
  if (text == null) return {};
  try { return JSON.parse(text) || {}; } catch {
    eprint(`${statePath} exists but is not valid JSON - fix it before spending a pass`);
    process.exit(2);
  }
}

/** Accepts an exact match or a documented alias -> dated snapshot resolution. */
function assertServedModel(requested, served) {
  if (!requested || !served || served === requested) return;
  const base = requested.replace(/-latest$|-0$/, '');
  const rest = String(served).startsWith(base) ? String(served).slice(base.length) : null;
  if (!(rest != null && /^[-@](\d{8}|\d{4}-\d{2}-\d{2})$/.test(rest))) {
    throw Object.assign(new Error(`served model ${served} != requested ${requested}`), { failure_class: 'serving_substitution' });
  }
}

function validateIds(cases, st) {
  const seen = new Map();
  for (const c of cases) {
    const k = pathSafeId(c.id).toLowerCase();
    if (seen.has(k)) { eprint(`duplicate case id after sanitization: '${c.id}' collides with '${seen.get(k)}'`); process.exit(2); }
    seen.set(k, c.id);
  }
  for (const sid of [...(st.train_ids ?? []), ...(st.val_ids ?? []), ...(st.test_ids ?? [])]) {
    if (String(sid) !== pathSafeId(sid)) { eprint(`_state.json split id '${sid}' is not a path-safe id`); process.exit(2); }
  }
}

async function runOne(c, rep, ctx, paths) {
  const { args } = ctx;
  const safeId = pathSafeId(c.id);
  const t0 = Date.now();
  const deadline = args.timeoutS > 0 ? t0 + args.timeoutS * 1000 : Infinity;
  const appRetry = { count: 0 }, judgeRetry = { count: 0 };
  let lastRun = null, rowWritten = false;
  try {
    const { run, g, latency_s } = await withTimeout((async () => {
      let tAttempt = t0;
      const run = await withBackoff(() => { tAttempt = Date.now(); return runCase(c, ctx); }, appRetry, deadline);
      lastRun = run;
      const latency_s = (Date.now() - tAttempt) / 1000;
      assertServedModel(args.model, run.model);
      let ref = null;
      if (args.variant !== 'baseline') {
        for (const ext of REF_EXTS) {
          try { ref = readFileNoFollow(join(paths.refDir, safeId) + ext); break; }
          catch (e) { if (e?.code !== 'ENOENT') throw e; }
        }
      }
      const g = await withBackoff(() => gradeCase(c, run, ref, ctx), judgeRetry, deadline);
      return { run, g, latency_s };
    })(), args.timeoutS, `${c.id} rep${rep}`);
    const retries = { ...(appRetry.count ? { retries: appRetry.count } : {}), ...(judgeRetry.count ? { judge_retries: judgeRetry.count } : {}) };
    const row = {
      prompt_id: safeId, rep, prompt: c.prompt ?? c.id, tags: c.tags,
      meta: { ...(c.meta ?? {}), ...(safeId !== String(c.id) ? { original_id: String(c.id) } : {}), ...retries, ...(g.meta ?? {}) },
      model: run.model, usage: run.usage, stop_reason: run.stop_reason,
      status: run.status ?? (run.stop_reason === 'max_tokens' ? 'truncated' : 'ok'),
      judge_model: g.judge_model, judge_usage: g.judge_usage,
      latency_s, ...perfFrom(run), grade: g.grade, explanation: g.explanation,
    };
    appendFileNoFollow(paths.results, JSON.stringify(row) + '\n');
    rowWritten = true;
    if (run.transcript)
      writeFileNoFollow(join(paths.vdir, 'traces', `${safeId}_rep${rep}.json`), JSON.stringify(run.transcript, null, 2));
    if (args.variant === 'baseline' && run.output != null && !REF_EXTS.some(ext => lexists(join(paths.refDir, safeId) + ext))) {
      mkdirNoFollow(paths.refDir);
      writeFileNoFollow(join(paths.refDir, safeId), typeof run.output === 'string' ? run.output : JSON.stringify(run.output));
    }
    return true;
  } catch (e) {
    if (rowWritten) { eprint(`  [${args.variant}] ${c.id} rep${rep} scored, but a post-row write failed: ${e?.message || e}`); return false; }
    // Failed attempts never occupy the (case, rep) slot in results.jsonl.
    appendFileNoFollow(paths.errors, JSON.stringify({
      prompt_id: safeId, rep, failure_class: e?.failure_class ?? 'error', error: String(e?.message || e),
      retries: appRetry.count, judge_retries: judgeRetry.count,
      model: lastRun?.model, usage: lastRun?.usage, latency_s: (Date.now() - t0) / 1000,
    }) + '\n');
    eprint(`  [${args.variant}] ${c.id} rep${rep} FAILED: ${e?.message || e}`);
    return false;
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const vdir = preflight(args);
  const statePath = join(args.flow, '_state.json');
  const st = loadState(statePath);
  checkHarness(fileURLToPath(import.meta.url), statePath, st, args.approveHarness);
  const ctx = { ...args, args, state: st };
  const paths = { vdir, results: join(vdir, 'results.jsonl'), errors: join(vdir, 'errors.jsonl'), refDir: join(args.flow, 'baseline', 'ref') };

  const done = new Set();
  for (const line of (readIfPresent(paths.results) ?? '').split('\n')) {
    try { const r = JSON.parse(line); done.add(`${r.prompt_id}\0${r.rep}`); } catch {}
  }
  const cases = await loadCases();
  validateIds(cases, st);
  const tasks = cases.flatMap(c => Array.from({ length: args.reps }, (_, rep) => ({ c, rep })))
    .filter(({ c, rep }) => !done.has(`${pathSafeId(c.id)}\0${rep}`));
  eprint(`[${args.variant}] ${tasks.length} of ${cases.length * args.reps} (id,rep) to run`);
  for (const p of [paths.results, paths.errors]) {
    const tail = readIfPresent(p);
    if (tail && !tail.endsWith('\n')) appendFileNoFollow(p, '\n');
  }

  let next = 0, ok = 0, fail = 0;
  const t0 = Date.now();
  const progress = () => {
    const finished = ok + fail, el = (Date.now() - t0) / 1000;
    const eta = finished ? Math.round((el / finished) * (tasks.length - finished)) : null;
    const line = `[${args.variant}] ${finished}/${tasks.length} done (${ok} ok, ${fail} failed), ${Math.round(el)}s elapsed`
      + (eta != null ? `, ~${eta}s left` : '');
    eprint(line);
    try { writeFileNoFollow(join(vdir, 'progress.txt'), line + '\n'); } catch {}
  };
  const worker = async () => {
    while (next < tasks.length) {
      const { c, rep } = tasks[next++];
      if (await runOne(c, rep, ctx, paths)) ok++; else fail++;
    }
  };
  const tick = setInterval(progress, 30_000);
  workersStarted = true;
  await Promise.all(Array.from({ length: args.concurrency }, worker));
  clearInterval(tick);
  progress();
  eprint(`[${args.variant}] done - ${ok} ok, ${fail} failed -> ${paths.results}`);
  process.exit(fail ? 1 : 0);
}

main().catch(e => {
  const m = String(e?.message || e);
  if (workersStarted) { eprint('stopped mid-run (rows already written are kept; re-run to resume): ' + m); process.exit(1); }
  eprint(m.startsWith('refusing to ') ? m : 'refusing to run: ' + m);
  process.exit(2);
});
