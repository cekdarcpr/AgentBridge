# Contributing

Thank you for helping improve agentBridge. Bug reports, documentation fixes and pull requests are all
welcome. For a larger change, such as a new provider or command, please open an issue first so the
approach can be agreed on before you spend time on it.

## Development setup

You need Rust 1.88 or newer. On Linux, also install the D-Bus development files for the keychain
(`libdbus-1-dev` and `pkg-config` on Debian and Ubuntu).

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

CI runs the same checks on Windows, macOS and Linux for every pull request, plus a `cargo audit` scan.

## Guidelines

- **Tests never call a provider.** They must pass without API keys or network access. Logic that
  depends on a response, such as parsing or retries, is tested with recorded JSON or a local server.
- **Never log or print a key.** Provider errors go through `providers::redact`, keys are sent only as
  headers, and test keys are obviously fake values.
- **Keep the JSON output stable.** Agents depend on its fields and error codes. Adding a field is fine;
  renaming or removing one is a breaking change.
- **Ask before adding a dependency.** The binary is kept small; a few lines of code are often better
  than a new crate.
- **Update the documentation** in `docs/` and add an entry under `[Unreleased]` in `CHANGELOG.md` for
  anything a user or agent will notice.

## Adding a provider

1. Add a module in `crates/agentbridge-core/src/providers/` that implements the `Provider` trait, and
   send its requests through `http::send` so retries and error handling stay consistent.
2. Add it to `ProviderId` in `capability.rs`, with its environment variable.
3. Add its default models and routing in `config.rs`, and its prices in `pricing.rs`.
4. Document it in the README provider table and in `docs/configuration.md`.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`,
`refactor:`, `test:` or `chore:`, followed by a short summary in English.

## License

By contributing, you agree that your contributions are dual licensed under the MIT and Apache-2.0
licenses, as described in the README.
