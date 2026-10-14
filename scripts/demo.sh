#!/usr/bin/env bash
# End-to-end demo: record production traffic, then check an agent's PR against
# it, first naively, then the state-safe way.
set -euo pipefail
cd "$(dirname "$0")/.."

N="${N:-2000}"
PROD=39100 PR=39101 BASE=39102 PROXY=39180
WORK=.echoproof-demo
BIN=target/release

cargo build --release -q -p echoproof -p demo-shop
rm -rf "$WORK" && mkdir -p "$WORK"
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT

wait_for() { for _ in $(seq 50); do curl -sf "$1" >/dev/null && return; sleep 0.1; done; echo "timeout: $1"; exit 1; }
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
echoproof() { set +e; $BIN/echoproof --config /dev/null "$@"; echo "exit code: $?"; set -e; }

step "1. Production (v1) on :$PROD, recording $N requests through the echoproof proxy"
PORT=$PROD $BIN/demo-shop 2>/dev/null &
wait_for "http://127.0.0.1:$PROD/health"
$BIN/echoproof --config /dev/null record --upstream "http://127.0.0.1:$PROD" --listen "127.0.0.1:$PROXY" --out "$WORK/baseline.jsonl" 2>/dev/null &
RECORDER=$!
wait_for "http://127.0.0.1:$PROXY/health"
$BIN/demo-traffic --target "http://127.0.0.1:$PROXY" -n "$N"
kill -TERM "$RECORDER" && wait "$RECORDER"

step "2. Naive check: replay against production v1 itself"
echo "Orders consumed stock while recording, so even unchanged code looks different:"
echoproof check -s "$WORK/baseline.jsonl" -t "http://127.0.0.1:$PROD" --max-examples 1 2>/dev/null

step "3. Agent PR (v2) vs base (v1) on the same state; safe mode (writes not replayed)"
echo "Intent of the PR: \"add currency to GET /products/{id}\""
PORT=$BASE $BIN/demo-shop 2>/dev/null &
DEMO_VERSION=2 PORT=$PR $BIN/demo-shop 2>/dev/null &
wait_for "http://127.0.0.1:$BASE/health"; wait_for "http://127.0.0.1:$PR/health"
RESET="curl -sf -X POST http://127.0.0.1:$BASE/__reset && curl -sf -X POST http://127.0.0.1:$PR/__reset"
echoproof check -s "$WORK/baseline.jsonl" --base "http://127.0.0.1:$BASE" -t "http://127.0.0.1:$PR" \
  --reset "$RESET" --expect "GET /products/*" --max-examples 1 2>/dev/null

step "4. Same, with order writes replayed (disposable instances, reset before every pass)"
echoproof check -s "$WORK/baseline.jsonl" --base "http://127.0.0.1:$BASE" -t "http://127.0.0.1:$PR" \
  --reset "$RESET" --allow-writes "POST /orders" --expect "GET /products/*" --max-examples 1 \
  --save "$WORK/candidate.jsonl" --output "$WORK/report.md" 2>/dev/null
echo "PR comment: $WORK/report.md"

if [ -n "${ANTHROPIC_API_KEY:-}${ANTHROPIC_AUTH_TOKEN:-}" ]; then
  step "5. Same PR, judged against its stated intent by Claude (no --expect needed)"
  set +e
  $BIN/echoproof --config /dev/null check -s "$WORK/baseline.jsonl" --base "http://127.0.0.1:$BASE" -t "http://127.0.0.1:$PR" \
    --reset "$RESET" --allow-writes "POST /orders" --max-examples 1 \
    --intent "Add a currency field (IDR) to the GET /products/{id} response." --output "$WORK/report-intent.md"
  echo "exit code: $?"
  set -e
else
  echo; echo "(set ANTHROPIC_API_KEY to also run step 5: judging the PR against its stated intent)"
fi
