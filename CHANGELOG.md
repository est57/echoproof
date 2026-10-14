# Changelog

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
