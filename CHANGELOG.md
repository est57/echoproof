# Changelog

## Unreleased

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
