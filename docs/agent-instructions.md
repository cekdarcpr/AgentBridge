# agentBridge agent instructions

This file teaches a coding agent to use `agb`. Copy everything below the line into the instruction file
your agent reads in your project root: `CLAUDE.md` for Claude Code, `AGENTS.md` for Codex, Cursor and
most other agents. Keep the text as it is; it is written for the agent, not for people.

---

## Creating assets with `agb`

This machine has `agb` (agentBridge), a command-line tool that creates finished assets (images, icon
sets, sound effects, voice lines and text) through the user's own AI provider accounts, and saves them
at the exact size and format requested. Use it whenever a task needs an asset that cannot be written as
code, instead of leaving a placeholder or asking the user to supply the file.

### Commands

Always add `--json` and read the result from stdout.

```bash
agb image  "<prompt>" --out <file.png|.jpg|.webp> [--size WxH] [--transparent] [--trim] [--fit cover|contain|stretch] --json
agb icon-set "<prompt>" --out-dir <dir> [--preset web|app] --json
agb icon-set --from <image> --out-dir <dir> [--preset web|app] --json
agb process <input> --out <file> [--size WxH] [--transparent] [--trim] [--fit …] --json
agb audio  "<prompt>" --out <file.mp3|.wav> [--duration SECONDS] --json
agb speech "<text>"   --out <file.mp3|.wav> [--voice VOICE] --json
agb ask    "<prompt>" [--file PATH]... [--system TEXT] [--out FILE] --json
```

Pick the command by what the project needs:

| Need | Command |
|---|---|
| A sprite, item or UI icon | `agb image … --size 64x64 --transparent --trim` |
| A banner, background or illustration | `agb image … --size 1600x900` (fills the frame, crops overflow) |
| Website favicons | `agb icon-set "…" --out-dir public --preset web`, then add the returned `html` to `<head>` |
| A desktop app icon | `agb icon-set "…" --out-dir <icons dir> --preset app` |
| A different size, format or background for an existing image | `agb process` (local, free; prefer it over generating again) |
| A sound effect | `agb audio … --duration 0.5` |
| A voice line | `agb speech` |

Examples:

```bash
agb image "pixel art iron sword, 16-bit style, flat colors" --out assets/items/sword.png --size 64x64 --transparent --trim --json
agb icon-set "a friendly rocket, flat style, bold shapes" --out-dir public --preset web --json
agb audio "short metallic UI click" --out assets/sfx/click.wav --duration 0.5 --json
```

Useful options:

- `--size`: always pass the size the code will display. The output has exactly this size.
- `--transparent`: works with every provider. Do not describe a background in the prompt.
- `--dry-run`: shows the provider, model and estimated cost without spending anything.
- `--overwrite`: replaces an existing file. Without it, existing files are never touched.

### Reading the result

The output is a single JSON object.

- `"ok": true`: the file is at `output` (for `icon-set`, all files are in `files`). Use those paths in
  the code. Images also report their final `width` and `height`. For `ask`, the answer is in `text`.
- For `icon-set --preset web`, add the tags from `html` to the page's `<head>`.
- Read `warnings`, for example an upscaled image or a background that could not be removed, and
  adjust or tell the user.
- `"ok": false`: branch on `error.code`, never on the message text:

| `error.code` | Exit | What to do |
|---|:---:|---|
| `budget_exceeded` | 3 | Stop. Tell the user which limit was reached. Do not retry and do not try another provider. |
| `no_provider` | 4 | No API key is set up for this capability. Ask the user to add one with `agb keys set <provider>`. |
| `provider_error` | 5 | Read `error.message`. Rephrase the prompt at most once if it was rejected, then report to the user. |
| `output_error` | 6 | The file exists or cannot be written. Choose another path, or add `--overwrite` only if replacing it is intended. |
| `usage` | 2 | The command was invalid, for example `--transparent` with a `.jpg` file. Fix the command. |
| anything else | 1 | A local problem. Report the message to the user. |

### Rules

- Ask for a capability, not a vendor. Do not pass `--provider` or `--model` unless the user asks for a
  specific provider or model; the user's routing chooses it.
- Save assets into the project's existing asset folders, with descriptive file names.
- Write specific prompts: subject, style, colors and intended use. Keep the style words identical
  across assets that belong together. Avoid text in icons.
- Generate only what the task needs. Every generation costs the user money; `agb process` and
  `agb icon-set --from` are free.
- Never print, log, store or ask for API key values. Keys are managed by the user with `agb keys`.
