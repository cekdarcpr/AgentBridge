# Changelog

All notable changes to agentBridge are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

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

[0.2.0]: https://github.com/cekdarcpr/AgentBridge/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cekdarcpr/AgentBridge/releases/tag/v0.1.0
