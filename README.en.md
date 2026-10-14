# Echoproof

[![CI](https://github.com/est57/echoproof/actions/workflows/ci.yml/badge.svg)](https://github.com/est57/echoproof/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/echoproof.svg)](https://crates.io/crates/echoproof)
[![Release](https://img.shields.io/github/v/release/est57/echoproof?sort=semver)](https://github.com/est57/echoproof/releases)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

[Bahasa Indonesia](README.md) | **English**

**Behavioral verification for every code change, based on real traffic.**

Echoproof records real requests and responses from your service, replays them against the build that contains a code change, and reports **what changed, on which endpoint and field, for what share of requests, and whether that change was intended**. Use the result as a merge gate in CI, or let a coding agent (Claude Code, Cursor and similar tools) check its own work directly.

<p align="center"><img src="https://raw.githubusercontent.com/est57/echoproof/main/docs/demo.svg" alt="Echoproof catching a pricing regression that the change intent does not explain" width="900"></p>

```
record  ->  replay against old and new build  ->  semantic diff  ->  judge against intent  ->  gate
```

## Contents

- [Why Echoproof](#why-echoproof)
- [Key features](#key-features)
- [Installation](#installation)
- [Quick start](#quick-start)
- [Demo](#demo)
- [Concepts](#concepts)
- [Command reference](#command-reference)
- [Configuration](#configuration)
- [Reading the report](#reading-the-report)
- [CI integration](#ci-integration)
- [Coding agent integration (MCP)](#coding-agent-integration-mcp)
- [Security and privacy](#security-and-privacy)
- [Limitations](#limitations)
- [Development](#development)
- [License](#license)

## Why Echoproof

When much of the code is written by AI agents, the number of changes grows far faster than people can review them. Passing tests and a tidy-looking diff do not guarantee that the system still behaves correctly: tests are written from assumptions, while bugs often come from real combinations of data that nobody thought of.

Echoproof answers a question that neither unit tests nor code review answer:

> Does this change alter how the system behaves on real traffic, and is that the change that was meant?

An example of what Echoproof catches: a pull request meant to "add a `currency` field to the catalog" accidentally passes one argument in the wrong position. As a result, the price of 3 out of 8 menu items changes while the other 5 stay correct. Unit tests pass and the code diff looks reasonable. Echoproof reports `products[*].price: 18000 -> 22000` on every catalog request and flags it as a regression.

## Key features

- **Records real traffic.** A reverse proxy records every request and response, with sensitive data redacted before anything is written to disk.
- **Semantic diff.** JSON is compared field by field, not line by line. Changes are grouped per endpoint and field and classified as `breaking`, `behavior` or `additive`.
- **Resistant to noise.** UUIDs, timestamps and random values are recognized automatically. Fields that change on their own between runs are learned by replaying the old build twice.
- **Safe with state.** Write requests are not replayed unless allowed per endpoint. The old and new builds are compared on the same data, with a reset command between every pass.
- **Aware of intent.** Each group of changes can be judged by Claude against the pull request description or the agent's task. Changes the intent does not explain are flagged as regressions.
- **Built for CI and agents.** Exit codes for merge gates, Markdown reports for pull request comments, JSON for automation, and an MCP server for coding agents.

## Installation

### Homebrew (macOS and Linux)

```bash
brew install est57/tap/echoproof
```

### Prebuilt binary (recommended)

Download the archive for your platform from the [releases page](https://github.com/est57/echoproof/releases), or from the terminal:

```bash
VERSION=v0.1.0
TARGET=aarch64-apple-darwin   # or x86_64-apple-darwin, x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu
curl -sLO https://github.com/est57/echoproof/releases/download/$VERSION/echoproof-$VERSION-$TARGET.tar.gz
curl -sLO https://github.com/est57/echoproof/releases/download/$VERSION/echoproof-$VERSION-$TARGET.tar.gz.sha256
shasum -a 256 -c echoproof-$VERSION-$TARGET.tar.gz.sha256
tar -xzf echoproof-$VERSION-$TARGET.tar.gz
sudo mv echoproof-$VERSION-$TARGET/echoproof /usr/local/bin/
```

```bash
echoproof --version
```

### With Cargo

If you have Rust 1.85 or later:

```bash
cargo install echoproof
```

### From source

**Requirements:** Rust 1.85 or later. `curl` is needed to run the demo.

```bash
git clone https://github.com/est57/echoproof.git
cd echoproof
cargo install --path crates/echoproof-cli
```

The `echoproof` binary is installed in `~/.cargo/bin`. Make sure that directory is on your `PATH`:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc
```

```bash
echoproof --version
```

## Quick start

These steps check a code change on a service running in your test environment.

**1. Create a configuration file** (optional; every setting has a default):

```bash
echoproof init
```

**2. Record traffic.** Run the proxy in front of the service, then point your app, Postman or automated tests at the proxy address:

```bash
echoproof record --upstream http://127.0.0.1:8000 --listen 127.0.0.1:8080 --out traffic.jsonl
```

Stop it with `Ctrl-C` once representative traffic has been collected.

**Laravel apps can record without a proxy** using [echoproof/laravel](https://github.com/est57/echoproof-laravel): run `composer require echoproof/laravel`, set `ECHOPROOF_RECORD=true` in `.env`, and traffic is written to `storage/echoproof/traffic.jsonl` with the same redaction rules.

**3. Run two builds**, the version before the change (base) and after it (PR), using the same database.

**4. Compare:**

```bash
echoproof check -s traffic.jsonl --base http://127.0.0.1:8001 -t http://127.0.0.1:8002
```

**5. Read the result.** Exit code `0` means no unexpected changes, `1` means a regression was found, and `2` means an error occurred. See [Reading the report](#reading-the-report).

To judge the changes against their intent, add the pull request description and the code diff:

```bash
git diff main...HEAD > change.diff
echoproof check -s traffic.jsonl --base http://127.0.0.1:8001 -t http://127.0.0.1:8002 \
  --intent-file pr-description.md --code-diff change.diff --output echoproof-report.md
```

## Demo

The full demo takes about 10 seconds and does not need a service of your own:

```bash
./scripts/demo.sh
```

The demo uses a small POS backend (`examples/demo-shop`) that has state: product stock goes down with every order. A "pull request from an agent" is meant to add a `currency` field to products, but it also changes how promo and voucher discounts are combined.

| Step | What it shows |
|---|---|
| 1. Record 2,000 requests from the production version | Recording |
| 2. Replay against the same version, compared with the recording | False alarms caused by state: stock was consumed during recording |
| 3. Compare the base and PR builds with `--base` and `--reset`, safe mode | False alarms disappear. `currency` is detected. Write requests are reported as unverified |
| 4. Same as step 3, with `--allow-writes "POST /orders"` | The discount bug is caught, only on orders that use a promo and a voucher together |
| 5. Intent judging by Claude (when `ANTHROPIC_API_KEY` is set) | `currency` is judged intended, the discount change is judged a regression |

## Concepts

### Snapshot

A snapshot is a JSONL file with one request and response pair per line. Headers and sensitive fields are redacted before they are written. The same snapshot can be reused to check many pull requests.

### Two comparison modes

| Mode | Command | Suited for |
|---|---|---|
| Against the recording | `echoproof check -s traffic.jsonl -t URL_PR` | Stateless services, or environments whose data matches the time of recording |
| Base vs PR | `echoproof check -s traffic.jsonl --base URL_BASE -t URL_PR` | Services backed by a database (recommended) |

In **base vs PR** mode, the recorded responses are not used as the reference. Echoproof replays the same traffic against both builds on the same data:

```
reset  ->  replay against base        ->  reference
reset  ->  replay against base again  ->  fields that differ are recorded as noise
reset  ->  replay against PR          ->  candidate
diff reference and candidate, ignoring noise
```

Each build's own address (for example `http://127.0.0.1:8001` inside image URLs or pagination links) is normalized automatically, so two builds running on different ports remain comparable.

### Safe mode

By default only `GET`, `HEAD` and `OPTIONS` requests are replayed. Write requests are skipped and **never counted as passing**: the report states how many there were and warns that their behavior is unverified.

Allow write requests per endpoint, and only against an environment that may be modified or can be reset:

```bash
echoproof check ... --allow-writes "POST /orders" --allow-writes "PUT /cart/*"
```

Use `--reset` (or `replay.reset`) to restore state before every pass, for example by restoring a database dump or calling a reset endpoint in the test environment. Echoproof refuses to replay write requests without a reset while noise is being learned, because state changes between runs would be learned as noise and hide regressions.

### Semantic diff

- JSON is compared field by field. `5` and `5.0` are equal, and key order is ignored.
- Every change is classified:

  | Classification | Examples |
  |---|---|
  | `breaking` | Field removed, type changed (including to `null`), 2xx status turned into an error |
  | `behavior` | Value changed, array length changed, status changed |
  | `additive` | New field |

- Paths are normalized: `/products/42` becomes `GET /products/{id}`, and `items[3].price` becomes `$.items[*].price`. Each change is counted once per request.
- UUIDs, ULIDs and date-times of the same shape are treated as noise. Bare dates (such as `2026-10-14`) are still compared.

### Change intent

There are two ways to tell Echoproof which changes are expected.

| Option | Granularity | When to use it |
|---|---|---|
| `--expect "GET /products/*"` | Endpoint | Fast, free, no external service |
| `--intent "..."` | Endpoint, field and value | When the intended change and a bug can occur on the same endpoint |

`--expect` lets every change on the listed endpoints through. If a pull request is meant to change the catalog but also breaks prices in the catalog by accident, `--expect "GET /catalog"` will not catch it.

With `--intent`, each group of changes is judged by Claude and given a verdict of `intended`, `regression` or `unclear`, with a reason that cites example values. The judge sees the stated intent, the code diff (optional), the share of affected requests, examples of changed requests with their before and after values, and examples of unchanged requests on the same endpoint for contrast. `unclear` verdicts and groups the judge did not rule on are treated as regressions (fail closed).

Intent judging requires an Anthropic API key. Each run makes one API call, and only when something changed. A typical run costs about USD 0.01 to 0.02 with the default model.

## Command reference

| Command | Purpose |
|---|---|
| `echoproof init` | Writes `echoproof.toml` with every setting explained |
| `echoproof record --upstream URL --out FILE` | Runs a reverse proxy that records traffic |
| `echoproof replay -s FILE -t URL -o FILE` | Replays a snapshot against a target and saves its responses |
| `echoproof diff BASELINE CANDIDATE` | Compares two snapshots |
| `echoproof check -s FILE -t URL` | Replays and diffs in one step, against the recording |
| `echoproof check -s FILE --base URL -t URL` | Replays and diffs, base vs PR on the same state |
| `echoproof accept CANDIDATE --baseline FILE` | Makes reviewed responses the new baseline |
| `echoproof mcp` | Runs the MCP server for coding agents |

Common options:

| Option | Applies to | Description |
|---|---|---|
| `--config FILE` | all | Configuration file, default `echoproof.toml` |
| `--expect PATTERN` | `diff`, `check` | Endpoint expected to change. Repeatable |
| `--intent TEXT`, `--intent-file FILE` | `diff`, `check` | Change intent to be judged by Claude |
| `--code-diff FILE` | `diff`, `check` | Code diff given to the intent judge |
| `--format text\|markdown\|json` | `diff`, `check` | Terminal output format, default `text` |
| `--output FILE` | `diff`, `check` | Saves the report. The format follows the extension (`.md`, `.json`) |
| `--max-examples N` | `diff`, `check` | Examples per group of changes, default 2 |
| `--base URL` | `check` | Reference build for base vs PR mode |
| `--reset CMD` | `check` | Shell command that restores state before every pass |
| `--noise-runs N` | `check` | Extra base replays used to learn noise, default 1 |
| `--allow-writes PATTERN` | `replay`, `check` | Endpoint whose write requests may be replayed. Repeatable |
| `--concurrency N` | `replay`, `check` | Parallel requests, default 1 |
| `--save FILE` | `check` | Saves the candidate snapshot |

Endpoint patterns: `POST /orders`, `GET /products/*` (`*` matches one segment), `/admin/**` (`**` matches any number of segments), or `*` for everything.

**Exit codes:** `0` passed or only expected changes, `1` regression found, `2` error (configuration, network, failed reset, or failed intent judging).

## Configuration

Every setting is optional. Run `echoproof init` to create a file with all settings and their explanations.

```toml
[record]
upstream = "http://127.0.0.1:8000"
listen = "127.0.0.1:8080"

[sanitize]
# Applied when recording and when replaying, so redacted values stay comparable.
redact_headers = ["authorization", "cookie", "set-cookie", "x-api-key"]
redact_fields = ["password", "token", "pin", "card_number", "phone", "email"]

[replay]
concurrency = 1                                          # raise only for stateless endpoints
timeout_ms = 10000
headers = { authorization = "Bearer ${STAGING_TOKEN}" }  # replaces redacted headers
safe_methods = ["GET", "HEAD", "OPTIONS"]
allow_writes = ["POST /orders"]                          # empty by default (safe mode)
reset = "./scripts/restore-test-db.sh"
noise_runs = 1

[diff]
ignore = ["updated_at", "$.meta.trace_id", "items[*].etag"]
expect = ["GET /products/*"]
auto_noise = true
compare_headers = ["content-type"]

[judge]
model = "claude-opus-5-5"
effort = "high"                                          # low, medium, high, xhigh, max
fallbacks = true

[mcp]
allowed_hosts = []                                       # hosts besides localhost that agents may use
```

`ignore` patterns: a bare name (`updated_at`) matches at any depth. Patterns that start with `$` or contain a dot (`$.meta.trace_id`, `items[*].etag`) are anchored at the body root.

**Environment variables:**

| Variable | Description |
|---|---|
| `ANTHROPIC_API_KEY` | API key for intent judging (alternative: `ANTHROPIC_AUTH_TOKEN`) |
| `ANTHROPIC_WORKSPACE_ID` | Required when the API key is not scoped to a single workspace |
| `ANTHROPIC_BASE_URL` | Alternative API endpoint. Must use `https`, except for `localhost` |
| `ECHOPROOF_JUDGE_TRACE` | Saves the judge's request and response to a file (mode `0600`) for debugging |

## Reading the report

Example terminal report:

```
echoproof: REGRESSION - 83 exchanges, 5 changed, 78 unchanged
  ignored 1 noisy fields that differ between runs of the base build: GET /up body
  intent judged by claude-opus-5-5: The currency field is added as intended. ...

  x GET /catalog                5/5 changed (100.0%)  unexpected
      behavior  value changed   $.result.products[*].price  (5 requests)
          [regression] The diff swaps the third argument of products() ...
          GET /catalog  [00e36fd43ade]
            $.result.products[1].price: 18000 -> 22000
      additive  added           $.result.currency  (5 requests)
          [intended] Adding 'currency' => 'IDR' matches the intent.

  - GET /branches               0/5 changed (0.0%)  unchanged

result: FAIL
```

| Marker | Meaning |
|---|---|
| `x` | The endpoint changed in an unexpected way (regression) |
| `~` | The endpoint changed as expected or as intended |
| `-` | The endpoint did not change, or its requests were not replayed |
| `(N requests)` | Number of requests with that change |
| `[intended]`, `[regression]`, `[unclear]` | Intent verdict and its reason |

Other formats:

- `--format markdown` produces a report ready to post as a pull request comment.
- `--format json` produces structured data, including `findings`: one sentence per problem, ready to hand back to a coding agent.

Once changes have been reviewed and approved, make the result the new baseline:

```bash
echoproof replay -s traffic.jsonl -t URL_PR -o candidate.jsonl
echoproof accept candidate.jsonl --baseline traffic.jsonl
```

## CI integration

Use the [Echoproof GitHub Action](https://github.com/est57/echoproof-action). It installs the release binary, runs `echoproof check`, writes the report to the job summary and exposes the verdict as an output:

```yaml
- name: Start base and PR builds
  run: ./scripts/start-builds.sh   # base on :8001, PR on :8002, same test database

- run: git diff origin/${{ github.base_ref }}...HEAD > change.diff

- name: Behavior check
  id: echoproof
  uses: est57/echoproof-action@v1
  with:
    snapshot: traffic/baseline.jsonl
    base: http://127.0.0.1:8001
    target: http://127.0.0.1:8002
    reset: ./scripts/reset-db.sh
    intent: ${{ github.event.pull_request.title }}
    code-diff: change.diff
    anthropic-api-key: ${{ secrets.ANTHROPIC_API_KEY }}

- name: Comment on PR
  if: always()
  run: gh pr comment ${{ github.event.number }} --body-file "${{ steps.echoproof.outputs.report }}"
  env:
    GH_TOKEN: ${{ github.token }}
```

A regression fails the step and blocks the merge. Set `fail-on-regression: false` to only report. On other CI systems, install the binary and run `echoproof check` directly; exit code `1` means a regression.

## Coding agent integration (MCP)

<p align="center"><img src="https://raw.githubusercontent.com/est57/echoproof/main/docs/agent-demo.svg" alt="A coding agent finds and fixes a discount bug it did not write, verified by Echoproof" width="900"></p>

In this recorded run, Claude Code (Sonnet) was asked to trim the cashier name in the order service. A discount bug had been planted in the same file beforehand. The check failed, the agent traced the regression to the bug it had not written, fixed it, and only reported the task done after the check passed.

`echoproof mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server over stdio, so a coding agent can check the changes it made before declaring the work done.

**Claude Code** (run from the project root that holds the snapshot):

```bash
claude mcp add echoproof -- echoproof mcp
```

**Other clients** (`.mcp.json`):

```json
{ "mcpServers": { "echoproof": { "command": "echoproof", "args": ["mcp"] } } }
```

Echoproof is listed in the [official MCP Registry](https://registry.modelcontextprotocol.io) under this name:

mcp-name: io.github.est57/echoproof

| Tool | Main inputs | Purpose |
|---|---|---|
| `echoproof_check` | `snapshot`, `target`, `base`, `intent`, `expect`, `allow_writes` | Replays and diffs. Returns a report for the agent, structured JSON and a `run_id` |
| `echoproof_diff` | `baseline`, `candidate`, `intent`, `expect` | Compares two existing snapshots |
| `echoproof_exchange` | `run_id`, `id` | Full request, reference and candidate responses, and field-level changes |

Agent workflow: change the code, start the build, call `echoproof_check` with the task as `intent`, and on a regression, open examples with `echoproof_exchange`, fix the code and check again. The server deliberately offers no `accept` tool, because approving a new baseline remains a human decision.

## Security and privacy

**Recorded data**

- Headers and fields listed in `[sanitize]` are replaced with `[REDACTED]` before they are written to disk. Add the fields that hold personal data in your domain, such as phone numbers, email addresses or identity numbers.
- Snapshots contain business data. Keep them out of public repositories or make sure they are in `.gitignore`.

**Data sent to Anthropic when using `--intent`**

The intent text, the code diff (when given), endpoint and field names, the number of affected requests, and example request bodies with before and after values, after redaction. Example bodies over 4,000 characters are truncated. Nothing is sent when `--intent` is not used.

**Built-in safeguards**

- Write requests are never replayed without explicit per-endpoint permission.
- Paths from a snapshot are validated so that requests, and any injected credentials, cannot be redirected to another host.
- Responses are capped at 32 MB.
- The API key is only sent over `https`, except to `localhost`.
- Text from outside (intent, diff, traffic) cannot change the structure of the judge's prompt or break tables in Markdown reports.

**Safeguards for coding agents (MCP)**

- `target` and `base` must be on `localhost` unless the host is listed in `[mcp] allowed_hosts`. Replays carry the headers from `replay.headers`, so an agent must not be able to send them to another host.
- File paths (`snapshot`, `save`, `code_diff_file`) must be inside the project directory. Symlinks that lead outside it are rejected.
- The reset command is only read from `echoproof.toml`, never from the agent.

## Limitations

- **Write requests** need a test environment that can be reset. Without one, use safe mode, which only verifies read requests.
- **External dependencies** such as payment gateways, SMS or third-party APIs are not mocked yet. Both the base and PR builds call them during replay.
- **IDs across requests** are not mapped yet. A recorded request such as `GET /orders/123` may refer to data that does not exist in the test environment.
- **Request order** is preserved by replaying sequentially (`concurrency = 1`). Large traffic can be slow, and parallel replay is only safe for stateless endpoints.
- **Changes that should have happened but did not** are hard to detect, because Echoproof reports requests that changed.
- **Rare noise** can slip past a single learning run. Raise `--noise-runs` or add an `ignore` pattern when needed.

## Development

**Repository layout**

```
crates/echoproof-core     snapshot format, sanitization, diff, reports, intent judging
crates/echoproof-cli      `echoproof` binary: recorder, replay, CLI, MCP server
examples/demo-shop     POS backend for the demo and the eval
eval/judge             intent judge eval: cases, runner and grader
scripts/demo.sh        end-to-end demo
```

**Running the tests**

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

**Intent judge eval**

`eval/judge` contains 26 labelled pull request scenarios (54 groups of changes) that measure how accurately the judge separates intended changes from regressions. With the default configuration the judge scored 100% correct gate decisions across 78 runs, with no false alarms. The current scenarios are no longer hard enough to tell better configurations apart, so cases from real systems are being added. The case list is in `eval/judge/CASES.md`.

```bash
eval/judge/prepare.sh                                                         # builds fixtures, no API cost
node eval/judge/run-eval.mjs --flow eval/judge/runs/intent-judge --variant baseline --reps 3
node eval/judge/summarize.mjs eval/judge/runs/intent-judge/baseline
```

The runner refuses to start until the harness has been approved once with `--approve-harness`, and asks for approval again whenever the runner, the cases or the judge change.

## License

Echoproof is licensed under either of the following, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

Unless you explicitly state otherwise, any contribution you submit for inclusion in this project is dual licensed as above, without any additional terms or conditions.
