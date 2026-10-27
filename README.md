<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/brand/agentbridge-logo-dark.svg">
  <img alt="agentBridge" src="assets/brand/agentbridge-logo.svg" height="72">
</picture>

`agb` is a command-line tool that lets coding agents (Claude Code, Codex, Cursor, ...) generate images,
sound effects, speech and text through the user's own API keys for several AI providers.

- **Capability first:** the agent asks for an image, not for a provider. Your routing decides who serves it.
- **Fallback:** a missing key or a failing provider moves on to the next one in your list.
- **Always a file:** output goes to `--out`; `--json` reports the path, provider, model and cost.
- **Budgets:** daily, per-project and per-call limits are enforced before any request is sent, safely across parallel agents.
- **Keys stay local:** stored in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service), never printed or logged.

## Providers

| Provider | image | audio (sfx) | speech | text |
|---|---|---|---|---|
| OpenAI | gpt-image-1 | | gpt-4o-mini-tts | gpt-4.1-mini |
| Google Gemini | gemini-2.5-flash-image | | | gemini-2.5-flash |
| ElevenLabs | | sound-generation | eleven_multilingual_v2 | |

Default models can be changed in the config.

## Build

```bash
cargo build --release
```

The binary is `target/release/agb`. To put it on your PATH:

```bash
cargo install --path crates/agentbridge-cli
```

## Usage

```bash
agb keys set openai          # reads the key from stdin
agb keys test openai         # free read-only check
agb providers
agb image "pixel art sword icon" --out assets/sword.png --transparent --json
agb audio "short metallic click" --out assets/click.wav --duration 0.5
agb speech "Welcome back!" --out assets/welcome.mp3
agb ask "Summarize this log" --file build.log
agb usage
agb log
```

Common flags: `--provider`, `--model`, `--no-fallback`, `--dry-run`, `--overwrite`, `--project`, `--json`.

Exit codes: `0` ok, `2` usage/config, `3` budget exceeded, `4` no usable provider, `5` provider error, `6` output error.

Keys are looked up in the OS keychain first, then in `OPENAI_API_KEY`, `GEMINI_API_KEY` and `ELEVENLABS_API_KEY`.

## Configuration

`agb config init` writes the defaults; `agb config path` shows where. Example:

```toml
http_timeout_secs = 180

[routing]
image = ["openai", "gemini"]
audio = ["elevenlabs"]
speech = ["elevenlabs", "openai"]
text = ["gemini", "openai"]

[limits]
daily_usd = 5.0
project_daily_usd = 2.0
max_call_usd = 1.0

[pricing."openai/gpt-image-1"]
per_call = 0.042
```

Built-in prices are approximate. Costs are flagged `cost_is_estimate` unless computed from provider-reported token usage.

`AGENTBRIDGE_HOME` relocates config and database, `AGENTBRIDGE_PROJECT` sets the project name,
and `AGENTBRIDGE_NO_KEYCHAIN=1` disables keychain access.

## Agent setup

Copy [docs/agent-instructions.md](docs/agent-instructions.md) into your project's `AGENTS.md` or `CLAUDE.md`.

## Layout

- `crates/agentbridge-core`: routing, providers, budgets, key storage, SQLite log.
- `crates/agentbridge-cli`: the `agb` binary.

## Not yet

Resizing and format conversion, background removal, MCP server mode, async jobs, prebuilt binaries.
