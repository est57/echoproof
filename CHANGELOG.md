# Changelog

## Unreleased

- Outbound proxy for any language: `echoproof ca init` creates a per-project CA, `record --outbound-listen` records each outbound HTTP(S) call with the request in flight, and `check`/`replay --outbound-proxy` answer every call from the recording through a loopback proxy that has no path to the real destination. `echoproof ca env --runtime php|node|python|go` prints the settings for the service. Validated with raw curl in PHP and `fetch` in Node 25 against a real HTTPS API.
- Secret values are now redacted everywhere in an exchange once they are found in a redacted place, so a key that a third party echoes back in its response no longer leaks into the snapshot.
- Echoproof's own HTTP clients ignore proxy environment variables, and the recording proxy refuses requests that already passed through it, so exporting the build's proxy settings in the same shell cannot create a loop.
- Fixtures for echoproof/laravel are only posted to targets that answer its probe, so a catch-all route on another service never runs on the fixture path.

## 0.3.0 - 2026-10-15

- IDs created during a sequential replay are carried into later requests for the same entity: order 9812 in the recording, created as order 57 in the test database, so `GET /orders/9812` and `{"order_id": 9812}` are sent as 57. Entities come from field names, enclosing keys and the request path, so an order ID never rewrites a table number. Turn it off with `--no-id-mapping` or `[replay] map_ids = false`.
- Requests that succeeded in the recording but fail on the base build are reported as *not reproduced* and are no longer counted as unchanged. The agent report states them as unverified, with examples such as `GET /orders/9812: recorded 200, base build 404`.
- MCP: when `code_diff_file` is omitted, the intent judge gets the project's uncommitted changes from `git diff HEAD` (external diff drivers, textconv filters and fsmonitor hooks disabled, capped at 100,000 characters). Agents no longer need to write a diff file for the judge to see the code.

## 0.2.1 - 2026-10-15

- MCP: a pass that skipped write requests now reads as partial in the agent report ("PASS for the N replayed requests only; M write requests were NOT verified"), and projects with `[replay] reset` are told to allow every write. In a real agent test this turned an overstated "all requests pass" into a full re-check.
- Claude Code plugin: install with `/plugin marketplace add est57/echoproof` and `/plugin install echoproof@echoproof`. It bundles the MCP server and a `verify-behavior` skill.
- README: install instructions for Cursor, VS Code, Claude Desktop, Codex CLI and Gemini CLI.

## 0.2.0 - 2026-10-15

- Third-party calls: builds running echoproof/laravel 0.2.0 in replay mode answer outbound HTTP calls from the recording and block all others. Reports show fields a build sent differently (`outbound[POST host/path] $.amount`), added or removed calls, and calls that were never recorded (breaking).
- Captured outbound calls that cannot be read fail the request instead of passing unverified.
- The intent judge and the MCP `echoproof_exchange` tool see outbound calls.
- Snapshots gain an optional `outbound` field; existing snapshots stay valid.

## 0.1.2 - 2026-10-14

- Record traffic from Laravel without a proxy with `composer require echoproof/laravel`, documented in both READMEs.
- Releases publish to crates.io through trusted publishing and to the MCP Registry through GitHub OIDC, with no stored tokens. The Homebrew tap updates itself.

## 0.1.1 - 2026-10-14

- MCP: the agent report now also travels in `structuredContent`, because some clients show only the structured result to the model.
- Install with Homebrew: `brew install est57/tap/echoproof`.
- GitHub Action: `uses: est57/echoproof-action@v1`.
- Listed in the official MCP Registry as `io.github.est57/echoproof`.

## 0.1.0 - 2026-10-14

- First release: record, replay, semantic diff, learned noise, safe mode, intent judge, MCP server.
