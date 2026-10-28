# Configuration

agentBridge works without any configuration. Create a config file when you want to change which
providers are used, the default models, the spending limits or the prices.

- [The config file](#the-config-file)
- [Routing](#routing)
- [Default models and voices](#default-models-and-voices)
- [Project style](#project-style)
- [Limits](#limits)
- [Prices](#prices)
- [Environment variables](#environment-variables)
- [Where data is stored](#where-data-is-stored)

## The config file

```bash
agb config init     # write a file with every default, ready to edit
agb config path     # show where it is
agb config show     # print the configuration agb is actually using
```

The file uses [TOML](https://toml.io). Settings you leave out keep their default, so a config file can
be as small as:

```toml
[limits]
daily_usd = 2.0
```

`agb` refuses to run with an invalid config, for example a provider listed for a capability it does not
support, and the error says what to fix.

<details>
<summary><strong>Complete default configuration</strong></summary>

```toml
http_timeout_secs = 180

[routing]
image = ["openai", "gemini", "openrouter"]
audio = ["elevenlabs"]
speech = ["elevenlabs", "openai", "gemini", "openrouter"]
text = ["gemini", "openai", "openrouter"]

[limits]
daily_usd = 5.0
project_daily_usd = 2.0
max_call_usd = 1.0

[models]
openai_image = "gpt-image-2.5-flare"
openai_text = "gpt-4.1-mini"
openai_speech = "gpt-4o-mini-tts"
openai_voice = "alloy"
gemini_image = "gemini-2.5-flash-image"
gemini_text = "gemini-2.5-flash"
elevenlabs_speech = "eleven_multilingual_v2"
elevenlabs_voice = "21m00Tcm4TlvDq8ikWAM"
elevenlabs_wav_sample_rate = 44100
gemini_speech = "gemini-3.8-flash-tts"
gemini_voice = "Kore"
openrouter_image = "google/gemini-2.5-flash-image"
openrouter_text = "google/gemini-2.5-flash"
openrouter_speech = "google/gemini-3.8-flash-tts"
openrouter_voice = "Kore"

[pricing]
```

</details>

## Routing

`[routing]` lists, per capability, which providers may be used and in which order. The first provider
with a key is called; if it fails, the next one is tried.

```toml
[routing]
image = ["gemini", "openrouter"]   # prefer Gemini, fall back to OpenRouter, never use OpenAI
audio = ["elevenlabs"]
speech = ["openai"]
text = ["openrouter"]
```

| Provider | `image` | `audio` | `speech` | `text` |
|---|:---:|:---:|:---:|:---:|
| `openai` | ✓ | | ✓ | ✓ |
| `gemini` | ✓ | | ✓ | ✓ |
| `elevenlabs` | | ✓ | ✓ | |
| `openrouter` | ✓ | | ✓ | ✓ |

An empty list disables the capability: agents get a `no_provider` error.

## Default models and voices

`[models]` sets the model used when the agent does not pass `--model`.

| Setting | Default | Used for |
|---|---|---|
| `openai_image` | `gpt-image-2.5-flare` | Images through OpenAI. `gpt-image-1` was shut down by OpenAI and is replaced automatically |
| `openai_text` | `gpt-4.1-mini` | `agb ask` through OpenAI |
| `openai_speech` | `gpt-4o-mini-tts` | Speech through OpenAI |
| `openai_voice` | `alloy` | Default OpenAI voice |
| `gemini_image` | `gemini-2.5-flash-image` | Images through Gemini |
| `gemini_text` | `gemini-2.5-flash` | `agb ask` through Gemini |
| `gemini_speech` | `gemini-3.8-flash-tts` | Speech through Gemini; `.wav` output only |
| `gemini_voice` | `Kore` | Default Gemini voice, for example `Puck`, `Zephyr` or `Charon` |
| `elevenlabs_speech` | `eleven_multilingual_v2` | Speech through ElevenLabs |
| `elevenlabs_voice` | `21m00Tcm4TlvDq8ikWAM` | Default ElevenLabs voice id |
| `elevenlabs_wav_sample_rate` | `44100` | Sample rate of `.wav` sound effects; higher rates may need a paid ElevenLabs plan |
| `openrouter_image` | `google/gemini-2.5-flash-image` | Images through OpenRouter's Image API; any image model, for example `qwen/qwen-image-2.1-turbo` |
| `openrouter_text` | `google/gemini-2.5-flash` | `agb ask` through OpenRouter |
| `openrouter_speech` | `google/gemini-3.8-flash-tts` | Speech through OpenRouter; any model from its [text-to-speech list](https://openrouter.ai/collections/text-to-speech-models) |
| `openrouter_voice` | `Kore` | Default voice for `openrouter_speech`. Voices belong to the model: `Kore` works for Gemini TTS, `alloy` for OpenAI models. When you change the model, change the voice too |

Speech models differ in the formats they return. Gemini TTS, directly or through OpenRouter, only
produces `.wav`; some OpenRouter models, such as MiniMax, only produce `.mp3`. When a model cannot
produce the requested format, the call fails and the next provider in the routing is tried.

`http_timeout_secs` (default 180) is how long a single provider request may take. Image models can
need a minute or more.

## Project style

The style that keeps a project's images consistent is not part of this file. It lives in
`agentbridge.toml` inside the project, so it is versioned with the code and shared by everyone who
works on it:

```toml
[style]
description = "flat vector illustration, bold rounded shapes, soft shading, clean outlines"
palette = ["#1E293B", "#6366F1", "#F59E0B", "#F8FAFC"]
avoid = "text, watermarks, photorealism"
reference = "assets/style-reference.png"
```

| Key | Meaning |
|---|---|
| `description` | How every image should look: medium, rendering, shapes, lighting. |
| `palette` | Colors to stay within, as hex codes. |
| `avoid` | What to keep out of every image. |
| `reference` | An existing image whose style new images should match, relative to `agentbridge.toml` and inside the project folder. It is sent to the provider with every image request. |

Every key is optional. `agb style init` writes a starting file at the project root and `agb style show`
prints the style that applies. agb looks for `agentbridge.toml` in the current folder and its parents,
up to the root of the git repository. See [Consistent style](concepts.md#consistent-style) for how it is
used.

## Limits

`[limits]` caps spending in US dollars. A call whose estimated cost would cross any limit is refused
before it is sent.

| Setting | Default | Applies to |
|---|---|---|
| `daily_usd` | `5.0` | All calls on one calendar day, across all projects |
| `project_daily_usd` | `2.0` | All calls from one project on one day |
| `max_call_usd` | `1.0` | Any single call |

A project is the git repository the command runs in. See [Budgets](concepts.md#budgets) for how
limits behave with parallel agents.

## Prices

agentBridge ships approximate list prices for its default models. Override them, or add prices for
other models, under `[pricing]`, keyed by `provider/model`:

```toml
# A flat price per call
[pricing."openai/gpt-image-2.5-flare"]
per_call = 0.04

# Token prices in US dollars per million tokens, used when the provider reports token usage
[pricing."gemini/gemini-2.5-pro"]
input_per_mtok = 1.25
output_per_mtok = 10.0

# OpenRouter models are keyed with their full vendor/model id
[pricing."openrouter/anthropic/claude-sonnet-4"]
input_per_mtok = 3.0
output_per_mtok = 15.0
```

With both `per_call` and token prices, `per_call` is used for the budget estimate before the call and
the token prices for the actual cost afterwards. OpenRouter reports the billed amount itself, so its
calls are always recorded at the real cost; its prices are only used for the estimate before the call.
A model with no price at all is estimated at $0.10 per call.

## Environment variables

| Variable | Purpose |
|---|---|
| `OPENAI_API_KEY`, `GEMINI_API_KEY`, `ELEVENLABS_API_KEY`, `OPENROUTER_API_KEY` | API keys, used when no key is stored in the keychain. |
| `AGENTBRIDGE_PROJECT` | Project name for per-project limits, instead of the git repository root. |
| `AGENTBRIDGE_HOME` | Keep the config file and call log in this folder instead of the default locations. |
| `AGENTBRIDGE_NO_KEYCHAIN` | Set to `1` to skip the keychain and read keys only from environment variables, for example in CI. |

## Where data is stored

| Data | Windows location | Other platforms |
|---|---|---|
| Config file | `%APPDATA%\agentbridge\config\config.toml` | shown by `agb config path` |
| Call log | `%LOCALAPPDATA%\agentbridge\data\agentbridge.db` | shown by `agb config path` |
| API keys | Windows Credential Manager, entries named `agentbridge` | macOS Keychain or the Secret Service |

The call log is a SQLite database that includes your prompts. Delete it to clear the history.
