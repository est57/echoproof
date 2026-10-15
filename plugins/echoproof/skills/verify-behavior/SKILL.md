---
name: verify-behavior
description: Verify that a change to an HTTP service's code does not alter its behavior on recorded real traffic, using the echoproof MCP tools. Use after editing service code (API handlers, services, models, queries) in a project that has an Echoproof snapshot (a .jsonl traffic recording) or an echoproof.toml, before reporting the task as done, or when the user asks to check a change for regressions.
---

# Verify behavior with Echoproof

Echoproof replays recorded HTTP traffic against the build before your change (base) and the build with your change (target), then reports which responses changed and whether your stated intent explains each change.

## Before the first check

Find these in the project, and ask the user for any you cannot find. Do not guess URLs.

- **Snapshot**: a `.jsonl` recording, often named `traffic.jsonl` or kept under `storage/echoproof/`. Check `CLAUDE.md`, `echoproof.toml` and the README for its location.
- **Target**: the URL where your changed code is running, usually a local dev server.
- **Base**: the URL of a running build of the code before your change. Needed when the service reads a database; without it, Echoproof compares against the recorded responses.
- **Writes**: whether the project has a disposable test database that is reset between passes (`[replay] reset` in `echoproof.toml`). Only then pass `allow_writes`.

If the `echoproof_check` tool is not available, the `echoproof` binary is probably not installed. Tell the user to run `brew install est57/tap/echoproof` or `cargo install echoproof`, then restart Claude Code.

## Run the check

1. Save your change as a diff: `git diff > change.diff`.
2. Call `echoproof_check` with:
   - `snapshot`, `target`, and `base` when available
   - `intent`: what the user asked you to change, in your own words, including fields or endpoints you expect to change
   - `code_diff_file`: `change.diff`
   - `allow_writes`: `["*"]` only when the test database is reset between passes
3. Read the verdict:
   - **PASS** or **INTENDED**: report the result, including any changes the judge marked intended.
   - **REGRESSION**: open the listed exchanges with `echoproof_exchange` (pass the `run_id` and exchange `id` from the report), find the cause in the code, fix it, write a new diff, and check again.

A finding may come from code you did not write, such as uncommitted work already in the file. Trace it anyway and tell the user what you found; fix it only if it is clearly a bug and within reach of the task, and say so in your report.

Fields shown as `outbound[METHOD host/path] $.field` are calls the service sent to another service (a payment gateway, a messaging API). They matter even when the service's own response did not change.

## Rules

- Do not report the task as done while the check reports REGRESSION.
- Never edit snapshot files, and never widen the intent beyond the task you were given, to make a check pass.
- If writes were skipped (safe mode), say that write endpoints were not verified.
- Replay targets must be local test builds. Never point a check at production.
