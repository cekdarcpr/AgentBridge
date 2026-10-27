# Configuration

agentBridge works without any configuration. Create a config file when you want to change which
providers are used, the default models, the spending limits or the prices.

- [The config file](#the-config-file)
- [Routing](#routing)
- [Default models and voices](#default-models-and-voices)
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
speech = ["elevenlabs", "openai"]
text = ["gemini", "openai", "openrouter"]

[limits]
daily_usd = 5.0
project_daily_usd = 2.0
max_call_usd = 1.0

[models]
openai_image = "gpt-image-1"
openai_text = "gpt-4.1-mini"
openai_speech = "gpt-4o-mini-tts"
openai_voice = "alloy"
gemini_image = "gemini-2.5-flash-image"
gemini_text = "gemini-2.5-flash"
elevenlabs_speech = "eleven_multilingual_v2"
elevenlabs_voice = "21m00Tcm4TlvDq8ikWAM"
elevenlabs_wav_sample_rate = 44100
openrouter_image = "google/gemini-2.5-flash-image"
openrouter_text = "google/gemini-2.5-flash"

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
| `gemini` | ✓ | | | ✓ |
| `elevenlabs` | | ✓ | ✓ | |
| `openrouter` | ✓ | | | ✓ |

An empty list disables the capability: agents get a `no_provider` error.

## Default models and voices

`[models]` sets the model used when the agent does not pass `--model`.

| Setting | Default | Used for |
|---|---|---|
| `openai_image` | `gpt-image-1` | Images through OpenAI |
| `openai_text` | `gpt-4.1-mini` | `agb ask` through OpenAI |
| `openai_speech` | `gpt-4o-mini-tts` | Speech through OpenAI |
| `openai_voice` | `alloy` | Default OpenAI voice |
| `gemini_image` | `gemini-2.5-flash-image` | Images through Gemini |
| `gemini_text` | `gemini-2.5-flash` | `agb ask` through Gemini |
| `elevenlabs_speech` | `eleven_multilingual_v2` | Speech through ElevenLabs |
| `elevenlabs_voice` | `21m00Tcm4TlvDq8ikWAM` | Default ElevenLabs voice id |
| `elevenlabs_wav_sample_rate` | `44100` | Sample rate of `.wav` sound effects; higher rates may need a paid ElevenLabs plan |
| `openrouter_image` | `google/gemini-2.5-flash-image` | Images through OpenRouter's Image API; any image model, for example `qwen/qwen-image-2.1-turbo` |
| `openrouter_text` | `google/gemini-2.5-flash` | `agb ask` through OpenRouter |

`http_timeout_secs` (default 180) is how long a single provider request may take. Image models can
need a minute or more.

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
[pricing."openai/gpt-image-1"]
per_call = 0.042

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
