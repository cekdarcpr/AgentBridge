# Commands

The complete reference for `agb`. Run `agb <command> --help` for the same information in the terminal.

- [Creating assets](#creating-assets): [`image`](#agb-image), [`icon-set`](#agb-icon-set),
  [`process`](#agb-process), [`audio`](#agb-audio), [`speech`](#agb-speech), [`ask`](#agb-ask)
- [Options shared by generating commands](#options-shared-by-generating-commands)
- [Managing](#managing): [`style`](#agb-style), [`keys`](#agb-keys), [`providers`](#agb-providers), [`usage`](#agb-usage),
  [`log`](#agb-log), [`config`](#agb-config)
- [JSON output](#json-output)
- [Error codes and exit codes](#error-codes-and-exit-codes)

## Creating assets

### `agb image`

Generate an image and finish it into the requested file.

```bash
agb image <PROMPT> --out <FILE> [options]
```

| Option | Description |
|---|---|
| `--out <FILE>` | Output file. The extension selects the format: `.png`, `.jpg` or `.webp`. Required. |
| `--size <WxH>` | Exact output size in pixels, for example `64x64` or `1600x900`. Up to 8192×8192. |
| `--fit <MODE>` | How the image fills `--size`: `cover` (crop), `contain` (pad) or `stretch`. Default: `contain` with `--transparent`, otherwise `cover`. |
| `--transparent` | Transparent background. Works with every provider. Not possible with `.jpg`. |
| `--trim` | Crop transparent margins so the subject fills the frame. Requires `--transparent`. |
| `--quality <low\|medium\|high>` | Generation quality where the provider supports it. OpenAI defaults to `medium`. |
| `--no-style` | Ignore the [project style](#agb-style) for this image. |

Plus the [shared options](#options-shared-by-generating-commands).

### `agb icon-set`

Write a complete icon set from a prompt or from an existing image.

```bash
agb icon-set <PROMPT> --out-dir <DIR> [--preset web|app] [--transparent] [options]
agb icon-set --from <IMAGE> --out-dir <DIR> [--preset web|app]
```

| Option | Description |
|---|---|
| `--out-dir <DIR>` | Folder the icons are written to. Required. |
| `--preset <web\|app>` | Which set to produce. Default: `web`. |
| `--from <IMAGE>` | Build the set from this image instead of generating one. Local and free. |
| `--transparent` | Transparent background around a generated icon. |
| `--no-style` | Ignore the [project style](#agb-style) for the generated icon. |

| Preset | Files |
|---|---|
| `web` | `favicon.ico` (16, 32, 48), `favicon-16x16.png`, `favicon-32x32.png`, `apple-touch-icon.png` (180, opaque), `android-chrome-192x192.png`, `android-chrome-512x512.png`, `site.webmanifest` |
| `app` | `icon-32.png`, `icon-64.png`, `icon-128.png`, `icon-256.png`, `icon-512.png`, `icon-1024.png`, `icon.ico` (16 to 256) |

When generating, a 1024×1024 master is made first and saved as `icon-source.png` in the output folder,
so the set can be rebuilt later for free with `--from`. Existing icon files are not replaced unless
`--overwrite` is given; this is checked before anything is generated. The `web` preset also returns the
HTML tags for the page `<head>`.

Generating accepts the [shared options](#options-shared-by-generating-commands); with `--from`, only
`--overwrite` and `--dry-run` apply.

### `agb process`

Finish an image you already have. Runs locally, sends nothing and costs nothing.

```bash
agb process <INPUT> --out <FILE> [options]
```

| Option | Description |
|---|---|
| `--out <FILE>` | Output file; `.png`, `.jpg` or `.webp`. Required. |
| `--size <WxH>` | Exact output size. |
| `--fit <MODE>` | `cover`, `contain` or `stretch`; same defaults as `agb image`. |
| `--transparent` | Remove a flat, single-color background. |
| `--trim` | Crop transparent margins. |
| `--overwrite` | Replace the output file if it exists. The input may be overwritten in place. |

### `agb audio`

Generate a sound effect.

```bash
agb audio <PROMPT> --out <FILE> [--duration <SECONDS>] [options]
```

| Option | Description |
|---|---|
| `--out <FILE>` | `.mp3` or `.wav`. Required. |
| `--duration <SECONDS>` | Length between 0.5 and 30 seconds. The provider decides when omitted. |

### `agb speech`

Turn text into spoken audio.

```bash
agb speech <TEXT> --out <FILE> [--voice <VOICE>] [options]
```

| Option | Description |
|---|---|
| `--out <FILE>` | `.mp3` or `.wav`. Required. |
| `--voice <VOICE>` | The provider's voice name or id. Defaults are set in the configuration. |

Speech is available through ElevenLabs, OpenAI, Google Gemini and OpenRouter. Gemini text-to-speech
writes `.wav` only.

### `agb ask`

Ask a text model. The answer is printed, or written to a file with `--out`.

```bash
agb ask <PROMPT> [--file <PATH>]... [--system <TEXT>] [--out <FILE>] [options]
```

| Option | Description |
|---|---|
| `--file <PATH>` | Add a text file to the prompt. Repeatable; at most 2 MB per file. |
| `--system <TEXT>` | System instruction for the model. |
| `--out <FILE>` | Save the answer to this file. |

### Options shared by generating commands

| Option | Description |
|---|---|
| `--json` | Print the result as one JSON object on stdout. Recommended for agents. |
| `--dry-run` | Show the provider, model and estimated cost without sending anything or writing files. |
| `--provider <NAME>` | Use only this provider: `openai`, `gemini`, `elevenlabs` or `openrouter`. Disables fallback. |
| `--model <NAME>` | Use a specific model. The provider is recognized from the name: `gpt-…` and `o1`/`o3`/`o4` → OpenAI, `gemini-…` → Gemini, `eleven_…` → ElevenLabs, `vendor/model` → OpenRouter. |
| `--no-fallback` | Stop after the first provider that has a key, even if it fails. |
| `--overwrite` | Replace the output file if it exists. Without it, existing files are never touched. |
| `--project <NAME>` | Project name for per-project limits. Default: the git repository root of the current folder. |

## Managing

### `agb style`

The project style applied to every generated image. It is stored in `agentbridge.toml`; see
[Project style](configuration.md#project-style).

| Command | Description |
|---|---|
| `agb style show` | Print the style that applies in the current folder, the file it comes from and the reference image. |
| `agb style init [--force]` | Write an `agentbridge.toml` with an example style at the project root. |

`agb image` and `agb icon-set` apply the style automatically; `--no-style` skips it for one call.

### `agb keys`

| Command | Description |
|---|---|
| `agb keys set <provider>` | Store a key in the OS keychain. The key is read from the keyboard or stdin, never from arguments. |
| `agb keys test <provider>` | Verify the key with a free, read-only request. |
| `agb keys list` | Show where each provider's key comes from: `keychain`, `env` or `missing`. |
| `agb keys remove <provider>` | Delete the stored key. |

### `agb providers`

List every provider with its capabilities, default models and key status.

### `agb usage`

Spending today and per day, compared with your limits.

| Option | Description |
|---|---|
| `--days <N>` | Number of days to show. Default: 7. |
| `--project <NAME>` | Show only this project. |

### `agb log`

The most recent calls, including failures, with provider, model, prompt, output path and cost.

| Option | Description |
|---|---|
| `--limit <N>` | Number of calls to show. Default: 20. |

### `agb config`

| Command | Description |
|---|---|
| `agb config path` | Print the locations of the config file and the call log database. |
| `agb config show` | Print the effective configuration. |
| `agb config init [--force]` | Write a config file with all defaults, ready to edit. |

## JSON output

With `--json`, every command prints exactly one JSON object on stdout, and every object has an `ok`
field.

### Generated asset

```json
{
  "ok": true,
  "capability": "image",
  "provider": "gemini",
  "model": "gemini-2.5-flash-image",
  "output": "C:\\projects\\my-game\\assets\\sword.png",
  "width": 64,
  "height": 64,
  "cost_usd": 0.0387,
  "cost_is_estimate": false,
  "fallback_used": true,
  "dry_run": false,
  "call_id": 42,
  "attempts": [
    { "provider": "openai", "model": null, "status": "skipped", "reason": "no API key; run `agb keys set openai` or set OPENAI_API_KEY" },
    { "provider": "gemini", "model": "gemini-2.5-flash-image", "status": "succeeded" }
  ],
  "warnings": [],
  "style": "C:\\projects\\my-game\\agentbridge.toml"
}
```

| Field | Meaning |
|---|---|
| `output` | Absolute path of the written file. `null` for `ask` without `--out`. |
| `width`, `height` | Final pixel size, for images. |
| `text` | The answer, for `ask` only. |
| `cost_usd` | Cost of the call in US dollars. |
| `cost_is_estimate` | `false` when the cost was billed or calculated from reported usage; `true` when it comes from the price table. |
| `fallback_used` | `true` when the first provider in the routing was skipped or failed. |
| `dry_run` | `true` when nothing was generated. |
| `call_id` | Id of the entry in the call log. |
| `attempts` | Every provider considered, with `status` `succeeded`, `failed`, `skipped` or `planned`, and a `reason`. |
| `warnings` | Things the agent should know, for example that an image was upscaled or a background could not be removed. |
| `style` | The `agentbridge.toml` whose style was applied, for images. Absent when no style was used. |

### Icon set

```json
{
  "ok": true,
  "preset": "web",
  "out_dir": "C:\\projects\\site\\public",
  "source": "C:\\projects\\site\\public\\icon-source.png",
  "files": [
    "C:\\projects\\site\\public\\favicon-16x16.png",
    "C:\\projects\\site\\public\\favicon-32x32.png",
    "C:\\projects\\site\\public\\apple-touch-icon.png",
    "C:\\projects\\site\\public\\android-chrome-192x192.png",
    "C:\\projects\\site\\public\\android-chrome-512x512.png",
    "C:\\projects\\site\\public\\favicon.ico",
    "C:\\projects\\site\\public\\site.webmanifest"
  ],
  "html": "<link rel=\"icon\" href=\"/favicon.ico\" sizes=\"48x48\">\n…",
  "warnings": [],
  "generation": { "provider": "gemini", "model": "gemini-2.5-flash-image", "cost_usd": 0.039, "…": "…" }
}
```

`generation` holds the full result of the master image call and is absent with `--from`.

### Processed image

```json
{
  "ok": true,
  "input": "C:\\projects\\site\\logo.png",
  "output": "C:\\projects\\site\\public\\logo-256.webp",
  "width": 256,
  "height": 256,
  "warnings": []
}
```

### Failure

```json
{
  "ok": false,
  "error": {
    "code": "no_provider",
    "message": "no usable provider for audio; see attempts for details"
  },
  "attempts": [
    { "provider": "elevenlabs", "model": null, "status": "skipped", "reason": "no API key; run `agb keys set elevenlabs` or set ELEVENLABS_API_KEY" }
  ]
}
```

## Error codes and exit codes

Branch on `error.code` or the exit code, never on the message text.

| Exit | `error.code` | Meaning | What to do |
|:---:|---|---|---|
| `0` | | Success | Use the file in `output`. |
| `2` | `usage`, `config_error` | Invalid arguments or configuration, for example `--transparent` with a `.jpg` | Fix the command; retrying unchanged will fail again. |
| `3` | `budget_exceeded` | A spending limit would be exceeded | Stop and tell the user. Do not retry. |
| `4` | `no_provider` | No provider with a key supports this capability | Add a key with `agb keys set <provider>`. |
| `5` | `provider_error` | The provider call failed, after any fallbacks | Read the message; rephrase the prompt at most once, then report. |
| `6` | `output_error` | The output file exists, cannot be written, or the image could not be processed | Use another path, or `--overwrite` if replacing is intended. |
| `1` | `keychain_error`, `database_error`, `io_error`, `internal_error` | Local environment problem | Report the message. |
