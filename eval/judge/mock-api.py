#!/usr/bin/env python3
"""Stand-in for POST /v1/messages used to test the eval harness for free.

The runner points each case at http://127.0.0.1:PORT/<case id>, so the mock
knows which labels apply. MODE: oracle | all_intended | all_regression |
skip_all | malformed.
Usage: MODE=oracle python3 eval/judge/mock-api.py 39200
"""
import json, os, re, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

MODE = os.environ.get("MODE", "oracle")
CASES = {c["id"]: c for c in json.load(open(os.path.join(os.path.dirname(__file__), "cases.json")))}

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        case = CASES[self.path.strip("/").split("/")[0]]
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        judgments, endpoint = [], None
        for line in body["messages"][0]["content"].splitlines():
            m = re.match(r"Endpoint (\S+ \S+):", line)
            if m:
                endpoint = m.group(1)
            g = re.match(r"\s+\[(g\d+)\] (\S+)", line)
            if g:
                label = next(l for l in case["labels"] if l["endpoint"] == endpoint and l["field"] == g.group(2))
                verdict = {"oracle": label["expected"], "all_intended": "intended", "all_regression": "regression"}.get(MODE)
                if verdict:
                    judgments.append({"group_id": g.group(1), "verdict": verdict, "reason": f"mock {MODE}"})
        text = "{not json" if MODE == "malformed" else json.dumps({"judgments": judgments, "summary": f"mock {MODE}"})
        data = json.dumps({"model": body["model"], "stop_reason": "end_turn",
                           "usage": {"input_tokens": 1000, "output_tokens": 100},
                           "content": [{"type": "text", "text": text}]}).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
