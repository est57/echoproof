# Security policy

Echoproof handles recorded HTTP traffic, credentials for test environments and, optionally, an Anthropic API key. Security reports are taken seriously.

## Reporting a vulnerability

Please do **not** open a public issue for security problems. Report them privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**.

Include what you found, how to reproduce it, and the impact you expect. You will receive an acknowledgement within 7 days.

## Scope

Examples of issues that are in scope:

- Secrets or personal data that escape redaction and reach snapshots, reports or the intent judge
- Replayed requests or injected credentials that can be redirected to an unintended host
- MCP tools that let an agent read or write files outside the project or reach hosts that are not allowed
- Prompt or report injection through recorded traffic, intent text or model output

## Supported versions

Only the latest release receives security fixes.
