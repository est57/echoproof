#!/usr/bin/env python3
"""Renders docs/demo.svg: an animated terminal replaying a real Echoproof run."""
from html import escape

COMMAND = ('$ echoproof check -s traffic.jsonl --base http://127.0.0.1:8001 -t http://127.0.0.1:8002 \\',
           '    --intent "Add a currency field (IDR) to the catalog response." --code-diff change.diff')
OUTPUT = [
    ("dim", "echoproof replay: 83 requests to http://127.0.0.1:8001 in 1.4s"),
    ("dim", "echoproof replay: 83 requests to http://127.0.0.1:8002 in 1.3s"),
    ("dim", "echoproof: judging 2 change groups against the intent with claude-opus-5-5"),
    ("bad", "echoproof: REGRESSION - 83 exchanges, 5 changed, 78 unchanged"),
    ("text", "  intent judged by claude-opus-5-5: The currency field is added as intended. The same"),
    ("text", "  diff also changes the template argument passed to products(), which alters prices."),
    ("text", ""),
    ("bad", "  x GET /catalog                5/5 changed (100.0%)  unexpected"),
    ("text", "      behavior  value changed   $.result.products[*].price  (5 requests)"),
    ("bad", "          [regression] The diff swaps the third argument of products() from"),
    ("bad", "          $branchTemplateId to $branchId, which the intent never asks for."),
    ("hi", "            $.result.products[1].price: 18000 -> 22000"),
    ("text", "      additive  added           $.result.currency  (5 requests)"),
    ("good", "          [intended] Adding 'currency' => 'IDR' matches the intent."),
    ("text", ""),
    ("dim", "  - GET /branches               0/5 changed (0.0%)  unchanged"),
    ("dim", "  - GET /payment-methods        0/3 changed (0.0%)  unchanged"),
    ("text", ""),
    ("bad", "result: FAIL"),
]
COLORS = {"text": "#c9d1d9", "dim": "#8b949e", "bad": "#ff7b72", "good": "#7ee787", "hi": "#e3b341", "cmd": "#79c0ff"}
WIDTH, LINE, TOP, LEFT = 900, 19, 52, 18
TYPE_SECONDS, STEP, HOLD = 2.4, 0.22, 5.0

lines = [("cmd", c) for c in COMMAND] + OUTPUT
starts = [0.0, TYPE_SECONDS * 0.6] + [TYPE_SECONDS + 0.6 + i * STEP for i in range(len(OUTPUT))]
total = starts[-1] + HOLD
height = TOP + LINE * len(lines) + 18
pct = lambda t: f"{100 * t / total:.2f}%"

css = [f"text{{font:13px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;white-space:pre}}"]
body = []
for i, ((kind, content), start) in enumerate(zip(lines, starts)):
    y = TOP + LINE * i
    css.append(f"@keyframes l{i}{{0%,{pct(start)}{{opacity:0}}{pct(start + 0.05)},97%{{opacity:1}}100%{{opacity:0}}}}"
               f".l{i}{{animation:l{i} {total:.1f}s linear infinite}}")
    if kind == "cmd":
        width = 9 + len(content) * 7.9
        css.append(f"@keyframes w{i}{{0%,{pct(start)}{{width:0}}{pct(start + TYPE_SECONDS * 0.55)},97%{{width:{width:.0f}px}}100%{{width:0}}}}"
                   f".w{i}{{animation:w{i} {total:.1f}s steps({len(content)}) infinite}}")
        body.append(f'<clipPath id="c{i}"><rect class="w{i}" x="{LEFT}" y="{y - 14}" height="{LINE}" width="0"/></clipPath>'
                    f'<text x="{LEFT}" y="{y}" fill="{COLORS[kind]}" clip-path="url(#c{i})">{escape(content)}</text>')
    else:
        body.append(f'<text class="l{i}" x="{LEFT}" y="{y}" fill="{COLORS[kind]}">{escape(content)}</text>')

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-label="Echoproof catching a pricing regression that the intent does not explain">
<style>{"".join(css)}</style>
<rect width="{WIDTH}" height="{height}" rx="10" fill="#0d1117"/>
<rect width="{WIDTH}" height="30" rx="10" fill="#161b22"/><rect y="20" width="{WIDTH}" height="10" fill="#161b22"/>
<circle cx="20" cy="15" r="6" fill="#ff5f56"/><circle cx="40" cy="15" r="6" fill="#ffbd2e"/><circle cx="60" cy="15" r="6" fill="#27c93f"/>
<text x="{WIDTH / 2:.0f}" y="19" fill="#8b949e" text-anchor="middle">echoproof: base vs PR on the same database</text>
{chr(10).join(body)}
</svg>
'''
open("docs/demo.svg", "w").write(svg)
print(f"docs/demo.svg: {len(lines)} lines, {total:.1f}s loop, {len(svg)} bytes")
