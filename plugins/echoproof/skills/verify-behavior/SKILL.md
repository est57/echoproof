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
- **Writes**: whether state is restored between passes. If `echoproof.toml` has `[replay] reset`, or the project docs say the test database is disposable and reset between passes, every write may be replayed: use `allow_writes: ["*"]`. Otherwise leave writes out; never replay writes against a database that is not disposable.

If the `echoproof_check` tool is not available, the `echoproof` binary is probably not installed. Tell the user to run `brew install est57/tap/echoproof` or `cargo install echoproof`, then restart Claude Code.

## Run the check

1. Call `echoproof_check` with:
   - `snapshot`, `target`, and `base` when available
   - `intent`: what the user asked you to change, in your own words, including fields or endpoints you expect to change
   - `code_diff_file`: leave it out in a git repository; the judge then reads your uncommitted changes (`git diff HEAD`). Pass a diff file only when the change is already committed or lives outside git
   - `allow_writes`: `["*"]` when state is restored between passes (see above). Do not list single endpoints when everything may be replayed: a misspelled path silently skips requests
2. Read the verdict:
   - **PASS** or **INTENDED** on every request: report the result, including any changes the judge marked intended.
   - **PASS for the replayed requests only** (the report says write requests were NOT verified): if writes may be replayed, check again with `allow_writes: ["*"]`. If they may not, your final answer must say how many requests were not verified.
   - **REGRESSION**: open the listed exchanges with `echoproof_exchange` (pass the `run_id` and exchange `id` from the report), find the cause in the code, fix it, and check again.

A finding may come from code you did not write, such as uncommitted work already in the file. Trace it anyway and tell the user what you found; fix it only if it is clearly a bug and within reach of the task, and say so in your report.

Fields shown as `outbound[METHOD host/path] $.field` are calls the service sent to another service (a payment gateway, a messaging API). They matter even when the service's own response did not change.

## Rules

- Do not report the task as done while the check reports REGRESSION.
- Never edit snapshot files, and never widen the intent beyond the task you were given, to make a check pass.
- Report coverage exactly as Echoproof states it. If any requests were not replayed, say how many, and never write that all requests passed.
- Replay targets must be local test builds. Never point a check at production.
