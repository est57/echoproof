# Design: outbound dependency mocking

Status: implemented in echoproof 0.2.0 and echoproof/laravel 0.2.0

## Problem

Most real services call other services while handling a request: payment gateways, messaging, tax APIs. During a replay those calls are sent for real. That is unsafe (a replayed order can charge a card or send a WhatsApp message twice) and unstable (third-party responses differ between the base pass and the PR pass, so they show up as false alarms). A change that alters what a service sends to a third party is also invisible today, because only the service's own response is compared.

## Goals

1. Replays never reach a real third party.
2. Each replayed request receives the same third-party responses that were recorded, in both the base and the PR build.
3. The report shows when the PR build sends something different to a third party (for example a different amount to the payment gateway), even when its own response is unchanged.

## Non-goals for the first version

- Languages other than PHP/Laravel, and HTTP clients other than Laravel's `Http` facade (raw Guzzle, curl, SDKs with their own client).
- Non-HTTP dependencies (SMTP, queues, gRPC, databases of other services).
- Recording outbound calls through the `echoproof record` proxy, which cannot see them.

## Design

### Recording (echoproof/laravel)

The middleware listens to Laravel's HTTP client events while it records a request and stores every outbound call on the exchange, in order:

```json
{"id": "...", "request": {...}, "response": {...},
 "outbound": [{"request": {"method": "POST", "url": "https://api.gateway.test/v2/charges", "headers": {...}, "body": {...}},
               "response": {"status": 200, "headers": {...}, "body": {...}}}]}
```

The same redaction rules apply to outbound headers and bodies. `outbound` is optional, so existing snapshots stay valid.

### Replay mode (echoproof/laravel)

The base and PR builds run with `ECHOPROOF_REPLAY=true`. The package refuses to enable it when `APP_ENV=production`. In replay mode:

- `Http::preventStrayRequests()` is on: no outbound HTTP call leaves the process, whether recorded or not.
- Two routes exist, only in replay mode: `POST /__echoproof/fixtures/{id}` receives the recorded outbound calls of one exchange, and `GET /__echoproof/outbound/{id}` returns the calls the build actually tried to make for that exchange, then forgets them.
- A request carrying `x-echoproof-exchange: {id}` is served with `Http::fake` answering from that exchange's fixtures. Calls are matched by method and URL without the query string, in recorded order. A call with no matching fixture gets a `599` response and is marked `unrecorded`.
- The response carries `x-echoproof-outbound: {count}` so the CLI knows the package is active and how many calls were captured.

The fixture and capture stores use the application cache with a short TTL, so they work with PHP-FPM, `php -S` and Octane alike.

### Replay (CLI)

For each exchange:

1. If it has recorded outbound calls, `POST` them to `/__echoproof/fixtures/{id}` on the target.
2. Send the request with `x-echoproof-exchange: {id}`.
3. If the response has `x-echoproof-outbound` greater than zero, `GET /__echoproof/outbound/{id}` and store the result as the replayed exchange's `outbound`.

The `x-echoproof-*` headers are removed from the stored response. Targets without the package behave exactly as today.

### Diff and report

Outbound calls of the base and PR replays are compared like responses:

- a call added, removed or sent to a different method or URL: `behavior`;
- a call answered with `unrecorded` in the PR build only: `breaking`, because the PR started talking to a dependency the recording never saw;
- a changed request body field: reported under a pattern such as `outbound[POST api.gateway.test/v2/charges] $.amount`.

Learned noise and `ignore` patterns apply to these fields as well, so a timestamp in an outbound body does not cause a false alarm. The intent judge sees outbound changes next to response changes.

## Safety

- Replay mode is opt-in by environment, refused in production, and blocks every real outbound call, recorded or not.
- The `__echoproof` routes exist only in replay mode. They accept fixtures for at most the size limit already used for bodies and keep them only for the cache TTL.
- Fixtures can contain third-party responses; they are redacted with the same rules as everything else in the snapshot.

## Validation plan

A Laravel 12 test app with a fake payment gateway route on a separate port:

1. Record checkout traffic that calls the gateway.
2. Base against base: zero changes, and the gateway receives no requests during replay.
3. A PR that sends `amount` in cents instead of rupiah: reported as an outbound change while the checkout response is unchanged.
4. A PR that adds a call to a new service: reported as `unrecorded` and breaking.

## Rollout

- echoproof/laravel 0.2.0: recording and replay mode.
- echoproof 0.2.0: snapshot field, replay steps, diff and report, MCP `echoproof_exchange` shows outbound calls.
- Both READMEs and the limits sections of the launch drafts are updated once the validation plan passes.
