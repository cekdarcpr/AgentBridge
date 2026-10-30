# Changelog

All notable changes to agentBridge are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.4.0] - 2026-10-30

Agents can be set up and routing changed from the command line, busy providers are retried, and
releases ship ready-to-run binaries.

### Added

- **`agb agent init`** adds the agent instructions to the project's `CLAUDE.md` and `AGENTS.md`, or
  creates `AGENTS.md`, between markers so running it again updates them in place.
  `agb agent instructions` prints them.
- **`agb config set <key> <value>`** changes one setting, for example
  `agb config set routing.image openrouter,gemini`, validates it and leaves the rest of the file as is.
- **Release builds.** Pushing a version tag publishes ready-to-run `agb` archives for Windows, Linux
  and macOS (Intel and Apple silicon) with SHA-256 checksums.
- **Free OpenRouter models** (ids ending in `:free`) are estimated and recorded at no cost.
- `SECURITY.md` and `CONTRIBUTING.md`.
- **Retries for busy providers.** Rate limits (HTTP 429) and temporary server errors (500, 502, 503,
  504) are retried up to twice with exponential backoff, honoring `Retry-After` up to 20 seconds,
  before the next provider is tried. Timeouts are not retried, so a slow request is never billed twice.
- **Continuous integration.** Formatting, clippy and tests run on Windows, macOS and Linux for every
  push and pull request, along with a minimum Rust version check and a `cargo audit` scan.

### Changed

- ElevenLabs speech is estimated from the length of the text ($0.08 per 1,000 characters, new
  `per_1k_chars` price field) instead of a flat $0.03, and sound effects at $0.06, the price of the
  30 second maximum. Both were underestimated for longer requests.

### Security

- API keys can no longer follow a redirect to another host. reqwest drops `Authorization` on a
  cross-origin redirect but not custom headers, which would have exposed Gemini and ElevenLabs keys.
  Redirects are now only followed within the same origin.

### Fixed

- The minimum Rust version is 1.88, not 1.82: current dependencies do not build with older
  compilers. It is now declared as `rust-version` so Cargo reports it clearly, and the install
  command uses `--locked` to build with the tested dependency versions.

## [0.3.0] - 2026-10-27

Every provider can be used for everything it offers, and the assets of a project share one look.

### Added

- **Project style profiles.** An `agentbridge.toml` at the project root holds a `[style]` with a
  description, a color palette, things to avoid and an optional reference image. `agb image` and
  `agb icon-set` apply it to every prompt and send the reference image to the provider, so assets made
  by different agents on different days match. `--no-style` skips it for one call.
- **`agb style show`** prints the style that applies, and **`agb style init`** writes a starting file.
- Image results report the style file that was applied in `style`.
- **Speech through Google Gemini** (`gemini-3.8-flash-tts`, `.wav` output).
- **Speech through OpenRouter** with any of its text-to-speech models, recorded at the billed amount
  when OpenRouter reports it.
- New settings `gemini_speech`, `gemini_voice`, `openrouter_speech` and `openrouter_voice`.

### Changed

- The default OpenAI image model is `gpt-image-2.5-flare`. OpenAI shut down `gpt-image-1` on
  October 23, 2026; configurations that still name it are switched automatically.
- The default speech routing is ElevenLabs, OpenAI, Gemini, then OpenRouter.

### Security

- API keys are now also removed from the error messages of `agb keys test`.
- Files attached with `agb ask --file` are no longer copied into the call log; only the prompt is.

## [0.2.0] - 2026-10-27

agentBridge becomes an asset tool: generated images are finished into the file the project needs.

### Added

- **Exact sizes.** `agb image --size WxH` produces exactly that size, with `--fit cover`, `contain` or
  `stretch`.
- **Format conversion.** The output extension (`.png`, `.jpg`, `.webp`) decides the format, whatever the
  provider returned.
- **Transparency with every provider.** Providers without native transparency draw on a flat backdrop
  that is removed locally, with anti-aliased edges un-blended so no colored halo remains.
- **`--trim`** crops transparent margins so the subject fills the frame.
- **`agb icon-set`** writes complete favicon (`web`) or desktop app (`app`) icon sets, including
  multi-size `.ico` files, a web manifest and the HTML tags, from a prompt or an existing image.
- **`agb process`** resizes, converts, trims or removes the background of existing images, locally and
  for free.
- **OpenRouter provider** for images and text with any `vendor/model` id. Images use OpenRouter's
  Image API, so image-only models such as Qwen Image and FLUX work too. Calls are recorded at the
  amount OpenRouter actually billed.
- Image results report their final `width` and `height`.

### Changed

- Output paths in results use the platform's separators consistently.
- `--transparent` no longer reorders providers; your routing order always decides.
- `--transparent` with a `.jpg` output, and `--trim` without `--transparent`, are rejected before any
  request is sent.

## [0.1.0] - 2026-10-26

First release.

### Added

- `agb image`, `agb audio`, `agb speech` and `agb ask` with OpenAI, Google Gemini and ElevenLabs.
- Routing per capability with automatic fallback.
- Daily, per-project and per-call spending limits, enforced before sending and safe with parallel agents.
- Local call log with costs; `agb usage` and `agb log`.
- API keys in the operating system keychain; `agb keys set`, `test`, `list` and `remove`.
- JSON output, stable error codes and exit codes for agents.

[Unreleased]: https://github.com/cekdarcpr/AgentBridge/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/cekdarcpr/AgentBridge/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/cekdarcpr/AgentBridge/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/cekdarcpr/AgentBridge/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cekdarcpr/AgentBridge/releases/tag/v0.1.0
