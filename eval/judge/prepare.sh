#!/usr/bin/env bash
# Builds the eval fixtures: one baseline recorded from production demo-shop and
# one candidate snapshot per case (demo-shop with that case's mutations).
# Free: no API calls. Deterministic traffic, so diffs are stable across runs.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
BIN=$ROOT/target/release
FIX=fixtures
PROD=39300 PR=39301 PROXY=39380

cargo build --release -q --manifest-path $ROOT/Cargo.toml -p parity-cli -p demo-shop
rm -rf "$FIX" && mkdir -p "$FIX"
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT
wait_for() { for _ in $(seq 50); do curl -sf "$1" >/dev/null && return; sleep 0.1; done; echo "timeout: $1"; exit 1; }

PORT=$PROD $BIN/demo-shop 2>/dev/null &
wait_for "http://127.0.0.1:$PROD/health"
$BIN/parity --config /dev/null record --upstream "http://127.0.0.1:$PROD" --listen "127.0.0.1:$PROXY" --out "$FIX/baseline.jsonl" 2>/dev/null &
RECORDER=$!
wait_for "http://127.0.0.1:$PROXY/health"
$BIN/demo-traffic --target "http://127.0.0.1:$PROXY" -n 2000 2>/dev/null
kill -TERM $RECORDER && wait $RECORDER

for row in $(node -e 'for (const c of require("./cases.json")) console.log(c.id + ":" + c.mutations.join(","))'); do
  id=${row%%:*} muts=${row#*:}
  DEMO_MUTATIONS=$muts PORT=$PR $BIN/demo-shop 2>/dev/null &
  SHOP=$!
  wait_for "http://127.0.0.1:$PR/health"
  $BIN/parity --config /dev/null replay -s "$FIX/baseline.jsonl" -t "http://127.0.0.1:$PR" -o "$FIX/$id.jsonl" --allow-writes "*" 2>/dev/null
  kill $SHOP && wait $SHOP 2>/dev/null || true
done
echo "fixtures: $(ls $FIX | wc -l | tr -d ' ') files in eval/judge/$FIX"
node check-labels.mjs
