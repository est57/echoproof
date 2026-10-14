export function groupsOf(report) {
  return report.endpoints.flatMap(e => e.groups.map(g => ({
    endpoint: e.endpoint, pattern: g.pattern, kind: g.kind, count: g.count,
    verdict: g.judgment?.verdict ?? null, reason: g.judgment?.reason ?? null,
  })));
}

export function labelFor(c, g) {
  return c.labels.find(l => l.endpoint === g.endpoint && l.field === g.pattern) ?? null;
}

/** "Caught": the gate blocks a regression group (regression or unclear). "Kept": an intended group passes. */
export function score(c, report) {
  const groups = groupsOf(report);
  const missing = groups.filter(g => !labelFor(c, g));
  if (missing.length) throw new Error(`unlabeled groups: ${missing.map(g => `${g.endpoint} ${g.pattern}`).join(', ')}`);
  if (groups.some(g => g.verdict == null)) throw new Error('report has groups without a judgment');
  const reg = groups.filter(g => labelFor(c, g).expected === 'regression');
  const int = groups.filter(g => labelFor(c, g).expected === 'intended');
  const blocks = g => g.verdict !== 'intended';
  const shouldBlock = reg.length > 0;
  const blocked = report.verdict === 'regression';
  return {
    grade: {
      gate_correct: blocked === shouldBlock ? 1 : 0,
      exact: groups.filter(g => g.verdict === labelFor(c, g).expected).length / groups.length,
      unclear: groups.filter(g => g.verdict === 'unclear').length / groups.length,
    },
    // Group-level counts for the confusion matrix (aggregated by summarize.mjs).
    counts: {
      reg_total: reg.length, reg_caught: reg.filter(blocks).length,
      int_total: int.length, int_kept: int.filter(g => !blocks(g)).length,
    },
    groups: groups.map(g => ({ ...g, expected: labelFor(c, g).expected })),
  };
}
