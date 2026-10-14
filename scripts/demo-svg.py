#!/usr/bin/env python3
"""Renders the animated terminal SVGs in docs/ from real Echoproof runs (paths and names anonymized)."""
from html import escape

COLORS = {"text": "#c9d1d9", "dim": "#8b949e", "bad": "#ff7b72", "good": "#7ee787",
          "hi": "#e3b341", "cmd": "#79c0ff", "agent": "#d2a8ff"}
WIDTH, LINE, TOP, LEFT, CHAR = 900, 19, 52, 18, 7.9
TYPE_PER_CHAR, STEP, HOLD = 0.03, 0.22, 5.0

# Each line: (kind, text, pause before it in seconds). "cmd" lines are typed out.
CHECK = [
    ("cmd", '$ echoproof check -s traffic.jsonl --base http://127.0.0.1:8001 -t http://127.0.0.1:8002 \\', 0),
    ("cmd", '    --intent "Add a currency field (IDR) to the catalog response." --code-diff change.diff', 0),
    ("dim", "echoproof replay: 83 requests to http://127.0.0.1:8001 in 1.4s", 0.4),
    ("dim", "echoproof replay: 83 requests to http://127.0.0.1:8002 in 1.3s", 0),
    ("dim", "echoproof: judging 2 change groups against the intent with claude-opus-5-5", 0),
    ("bad", "echoproof: REGRESSION - 83 exchanges, 5 changed, 78 unchanged", 0.6),
    ("text", "  intent judged by claude-opus-5-5: The currency field is added as intended. The same", 0),
    ("text", "  diff also changes the template argument passed to products(), which alters prices.", 0),
    ("text", "", 0),
    ("bad", "  x GET /catalog                5/5 changed (100.0%)  unexpected", 0),
    ("text", "      behavior  value changed   $.result.products[*].price  (5 requests)", 0),
    ("bad", "          [regression] The diff swaps the third argument of products() from", 0),
    ("bad", "          $branchTemplateId to $branchId, which the intent never asks for.", 0),
    ("hi", "            $.result.products[1].price: 18000 -> 22000", 0),
    ("text", "      additive  added           $.result.currency  (5 requests)", 0),
    ("good", "          [intended] Adding 'currency' => 'IDR' matches the intent.", 0),
    ("text", "", 0),
    ("dim", "  - GET /branches               0/5 changed (0.0%)  unchanged", 0),
    ("dim", "  - GET /payment-methods        0/3 changed (0.0%)  unchanged", 0),
    ("text", "", 0),
    ("bad", "result: FAIL", 0),
]

AGENT = [
    ("cmd", "> When an order is saved, store created_by without surrounding whitespace:", 0),
    ("cmd", "  some terminals send the cashier name with trailing spaces.", 0),
    ("dim", "  Read  app/Modules/PosSync/Services/PosOrderService.php", 0.5),
    ("text", "  Edit  PosOrderService.php: wrap created_by and edited_by in trim()", 0.3),
    ("dim", "  Bash  git diff > change.diff", 0.2),
    ("cmd", '  echoproof_check  intent: "Trim surrounding whitespace from created_by ..."', 0.3),
    ("bad", "    REGRESSION - 48 exchanges, 6 changed, 42 unchanged", 1.2),
    ("text", "    judge: the trim matches the intent but produces no visible diffs; all 7 groups", 0),
    ("text", "    come from the unmentioned discount_val change (discount_amount -> discount)", 0),
    ("hi", "      $.result.sales_recap.discount_total: 30418 -> 0", 0),
    ("hi", "      $.result.sales_recap.net_sales: 492398 -> 522816", 0),
    ("agent", "  The echoproof found a pre-existing bug in the modified file: discount_val reads", 0.8),
    ("agent", "  from $validated['discount'] but the validated key is discount_amount. Let me fix that too.", 0),
    ("text", "  Edit  PosOrderService.php: $validated['discount'] -> $validated['discount_amount']", 0.5),
    ("dim", "  Bash  git diff > change.diff", 0.2),
    ("cmd", '  echoproof_check  intent: "Trim surrounding whitespace ... Also fix discount_val ..."', 0.3),
    ("good", "    PASS - 48 exchanges, 0 changed, 48 unchanged", 1.2),
    ("text", "", 0),
    ("agent", "  Done. Trimmed created_by/edited_by. Pre-existing bug fixed: discounts were silently", 0.5),
    ("agent", "  dropped (stored as 0), inflating gross/net sales in shift recaps.", 0),
    ("dim", "  8 turns, 52 s, about USD 0.10", 0.3),
]


def render(path: str, title: str, label: str, lines: list) -> None:
    starts, t = [], 0.0
    for kind, text, pause in lines:
        t += pause
        starts.append(t)
        t += len(text) * TYPE_PER_CHAR + 0.3 if kind == "cmd" else STEP
    total = t + HOLD
    height = TOP + LINE * len(lines) + 18
    pct = lambda s: f"{100 * s / total:.2f}%"
    css = ["text{font:13px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;white-space:pre}"]
    body = []
    for i, ((kind, text, _), start) in enumerate(zip(lines, starts)):
        y = TOP + LINE * i
        if kind == "cmd":
            end = start + len(text) * TYPE_PER_CHAR
            css.append(f"@keyframes w{i}{{0%,{pct(start)}{{width:0}}{pct(end)},97%{{width:{9 + len(text) * CHAR:.0f}px}}"
                       f"100%{{width:0}}}}.w{i}{{animation:w{i} {total:.1f}s steps({max(len(text), 1)}) infinite}}")
            body.append(f'<clipPath id="c{i}"><rect class="w{i}" x="{LEFT}" y="{y - 14}" height="{LINE}" width="0"/></clipPath>'
                        f'<text x="{LEFT}" y="{y}" fill="{COLORS[kind]}" clip-path="url(#c{i})">{escape(text)}</text>')
        else:
            css.append(f"@keyframes l{i}{{0%,{pct(start)}{{opacity:0}}{pct(start + 0.05)},97%{{opacity:1}}100%{{opacity:0}}}}"
                       f".l{i}{{animation:l{i} {total:.1f}s linear infinite}}")
            body.append(f'<text class="l{i}" x="{LEFT}" y="{y}" fill="{COLORS[kind]}">{escape(text)}</text>')
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-label="{escape(label)}">
<style>{"".join(css)}</style>
<rect width="{WIDTH}" height="{height}" rx="10" fill="#0d1117"/>
<rect width="{WIDTH}" height="30" rx="10" fill="#161b22"/><rect y="20" width="{WIDTH}" height="10" fill="#161b22"/>
<circle cx="20" cy="15" r="6" fill="#ff5f56"/><circle cx="40" cy="15" r="6" fill="#ffbd2e"/><circle cx="60" cy="15" r="6" fill="#27c93f"/>
<text x="{WIDTH / 2:.0f}" y="19" fill="#8b949e" text-anchor="middle">{escape(title)}</text>
{chr(10).join(body)}
</svg>
'''
    with open(path, "w") as out:
        out.write(svg)
    widest = max(len(text) for _, text, _ in lines) * CHAR + LEFT
    print(f"{path}: {len(lines)} lines, {total:.1f}s loop, {len(svg)} bytes, widest line {widest:.0f}px of {WIDTH}")


if __name__ == "__main__":
    render("docs/demo.svg", "echoproof: base vs PR on the same database",
           "Echoproof catching a pricing regression that the intent does not explain", CHECK)
    render("docs/agent-demo.svg", "Claude Code (Sonnet) with the echoproof MCP server",
           "A coding agent finds and fixes a discount bug it did not write, verified by Echoproof", AGENT)
