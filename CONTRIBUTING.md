# Contributing to Echoproof

Thanks for your interest in Echoproof. Bug reports, real-world feedback and pull requests are all welcome.

## Ways to help

- **Try it on a real service** and open an issue describing what worked, what was noisy, and what was missing. Field reports are the most valuable contribution right now.
- **Report bugs** with the bug report template.
- **Propose features** with the feature request template before starting large changes.
- **Add eval cases** in `eval/judge/cases.json`, especially regressions taken from real systems (anonymized).

## Development setup

Requirements: Rust 1.85 or later, `curl`, and Node.js 18 or later for the eval.

```bash
git clone https://github.com/est57/echoproof.git
cd echoproof
cargo build
./scripts/demo.sh
```

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
./scripts/demo.sh
```

If you change the semantic diff, the intent judge prompt or the report format, also run the free eval checks:

```bash
eval/judge/prepare.sh
MODE=oracle python3 eval/judge/mock-api.py 39200 &
EVAL_MOCK_URL=http://127.0.0.1:39200 node eval/judge/selftest.mjs
```

## Code style

- Keep files small: split a module once it grows past about 250 lines.
- Comment only what the code cannot say itself.
- Plain ASCII in code and program output (no emoji).
- Treat recorded traffic, intent text and model output as untrusted input.
- Add a test for every bug fix and every new behavior.

## Commit messages

Use the imperative mood and explain why the change is needed, for example `Match array items by id so reordered lists do not look changed`.

## License

By contributing, you agree that your contributions are dual licensed under the MIT and Apache-2.0 licenses, as described in the README.
