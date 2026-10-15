# Design: outbound proxy mode

Status: implemented, unreleased

## Problem

Echoproof 0.2 answers third-party calls from the recording only when they go through Laravel's `Http` client, via echoproof/laravel. Payment and messaging SDKs often bring their own HTTP client (raw curl, their own Guzzle instance), and services in other languages have no package at all. Those calls still reach the real third party during replay: unsafe, unstable, and invisible to the diff.

## Goal

Record and answer outbound HTTP and HTTPS calls from any language and any client that honors the standard proxy settings, and compare what each build sent, with the same report as today (`outbound[POST host/path] $.amount`).

## Design

### One proxy, two moments

Echoproof runs a forward proxy on localhost. The service under test is started with its outbound traffic pointed at it.

- **Recording** (`echoproof record --outbound-listen 127.0.0.1:8899`): the proxy forwards each call to the real destination and stores request and response on the inbound exchange that is in flight. A call that starts while several inbound requests are in flight cannot be attributed; it is stored as unattributed and counted in a warning. Recording is therefore meant for staging or scripted traffic, where requests come one at a time.
- **Replay** (`echoproof check --outbound-proxy 127.0.0.1:8899`): replay runs sequentially, so every outbound call belongs to the exchange being replayed. The proxy answers from that exchange's recorded calls (same matching as echoproof/laravel: method, host and path, in order), answers anything else with `599` marked unrecorded, and never contacts the real destination. What the build sent becomes the exchange's `outbound` list, so the existing diff, report, intent judge and MCP output work unchanged.

### HTTPS

To see inside HTTPS calls, the proxy terminates TLS with certificates issued by its own certificate authority.

- `echoproof ca init` creates `.echoproof/ca.pem` and `.echoproof/ca-key.pem` (mode `0600`) in the project, valid for 90 days, and prints the settings for each runtime.
- Leaf certificates are issued per host in memory and never written to disk.
- During recording, the proxy connects to the real destination with normal certificate verification (system roots). The service only ever talks TLS to the proxy, so it only needs to trust the Echoproof CA.
- The CA must only be trusted by test builds, through process settings, never installed system-wide. The docs and `ca init` output say so, and `.echoproof/` is added to `.gitignore` by `ca init`.

### Pointing a service at the proxy

| Runtime | Proxy | Trust the CA |
|---|---|---|
| PHP (curl, Guzzle) | `HTTPS_PROXY`; `HTTP_PROXY` only works on the CLI SAPI, because PHP-FPM ignores it to block httpoxy. PHP-FPM pools need `env[HTTPS_PROXY]` because `clear_env` drops it | `-d curl.cainfo=... -d openssl.cafile=...` or the same in a test `php.ini` |
| Node | `HTTPS_PROXY` plus `NODE_USE_ENV_PROXY=1` (Node 22.21 or 24) | `NODE_EXTRA_CA_CERTS` |
| Python (requests, httpx) | `HTTPS_PROXY` | `REQUESTS_CA_BUNDLE` or `SSL_CERT_FILE` |
| Go, Rust, curl | `HTTPS_PROXY` | `SSL_CERT_FILE` |
| JVM | `-Dhttps.proxyHost` and `-Dhttps.proxyPort` | a test truststore |

`echoproof ca env --runtime php|node|python|go` prints these lines for the current project.

### Calls the proxy cannot see

A client that sets its own proxy, or ignores the environment, sends calls directly. The proxy cannot know about them. Two safeguards keep this from passing silently:

- `check` reports how many outbound calls the proxy saw for each exchange. When the recording has calls for an exchange and the replay shows none through the proxy, that exchange gets a `call removed` change for each recorded call, so a client that bypasses the proxy is visible, not a pass.
- The docs recommend running test builds without a network route to the outside (a container network with only the proxy reachable) where possible, so a bypassing call fails loudly instead of reaching production.

### Relation to echoproof/laravel

Laravel apps can keep using the package, which needs no CA and attributes calls exactly even under concurrent recording. The proxy is for everything else. A snapshot can contain calls from either source; replay uses whichever mode is enabled.

## Out of scope

- Non-HTTP dependencies (SMTP, AMQP, Kafka, gRPC over raw sockets, databases of other services).
- HTTP/2-only and WebSocket upstreams in the first version; the proxy speaks HTTP/1.1 to the service and to the destination.
- Calls made after the response, from queues or background jobs, which cannot be attributed to a request.
- Concurrent replay with the proxy.

## Security

- The proxy listens on localhost only and refuses to start on another address.
- The CA key stays in the project directory with mode `0600`; Echoproof refuses to read a key file that is group- or world-readable.
- In replay mode the proxy has no upstream connection code path at all; it cannot leak a call to the real destination even if matching fails.
- Recorded outbound calls use the same redaction rules as everything else, including query parameters such as `api_key`.

## Validation plan

1. A PHP script that calls an HTTPS API through raw curl (not Laravel's `Http`), and a small Node service using `fetch`, both recorded through the proxy against a public test API.
2. Replay with the base build: zero changes, zero connections from the proxy to the outside.
3. A PR that changes a field sent to the API: reported as an outbound change while the service's own response is unchanged.
4. A build whose client bypasses the proxy: reported as `call removed`, not as a pass.

## Rollout

echoproof 0.4.0: `ca init`, `ca env`, `record --outbound-listen`, `check --outbound-proxy`, docs for each runtime in both READMEs.
