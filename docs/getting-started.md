# Getting started

This walkthrough takes about five minutes. At the end, `agb` is installed, connected to at least one
AI provider, and your coding agent knows how to use it.

1. [Install agb](#1-install-agb)
2. [Add an API key](#2-add-an-api-key)
3. [Make your first asset](#3-make-your-first-asset)
4. [Connect your coding agent](#4-connect-your-coding-agent)
5. [Keep an eye on spending](#5-keep-an-eye-on-spending)

## 1. Install agb

agentBridge is built with Rust. If you do not have it yet, install it from [rustup.rs](https://rustup.rs);
version 1.82 or newer is required. Then install `agb` directly from GitHub:

```bash
cargo install --git https://github.com/cekdarcpr/AgentBridge agentbridge-cli
```

The command is placed in Cargo's bin folder (`~/.cargo/bin`, or `%USERPROFILE%\.cargo\bin` on
Windows), which is already on your `PATH` after installing Rust. Open a new terminal and check:

```bash
agb --version
```

To update later, run the same `cargo install` command again with `--force`.

## 2. Add an API key

agentBridge uses your own accounts with AI providers. You pay the provider directly, at their prices;
agentBridge itself is free and has no server. One provider is enough to start:

| If you want… | Get a key from | Command |
|---|---|---|
| Images and text, the simplest start | [OpenAI](https://platform.openai.com/api-keys) | `agb keys set openai` |
| Images and text from Google | [Google AI Studio](https://aistudio.google.com/apikey) | `agb keys set gemini` |
| Sound effects and natural voices | [ElevenLabs](https://elevenlabs.io/app/settings/api-keys) | `agb keys set elevenlabs` |
| Images, voices and text from many vendors with one key | [OpenRouter](https://openrouter.ai/settings/keys) | `agb keys set openrouter` |

Run the command, paste the key when asked and press Enter:

```text
$ agb keys set openai
Paste the OpenAI API key and press Enter (the input is visible, so check nobody is watching):
OpenAI key stored in the OS keychain
```

The key goes into your operating system's keychain (Windows Credential Manager, macOS Keychain or the
Secret Service on Linux). It is never shown again and never written to a file.

Check that it works. This sends a free, read-only request:

```bash
agb keys test openai
```

> [!TIP]
> Already have keys in environment variables such as `OPENAI_API_KEY`? agentBridge picks those up
> automatically. `agb keys list` shows where each key comes from.

## 3. Make your first asset

Preview what a call would do and cost, without spending anything:

```bash
agb image "pixel art sword icon, flat colors" --out assets/sword.png --size 64x64 --transparent --dry-run
```

```text
dry run: would use openai/gpt-image-2.5-flare, estimated ~$0.040
```

Now make it for real:

```bash
agb image "pixel art sword icon, flat colors" --out assets/sword.png --size 64x64 --transparent
```

```text
C:\projects\my-game\assets\sword.png
openai/gpt-image-2.5-flare · ~$0.040 · 64x64
```

You now have a 64×64 PNG with a transparent background in `assets/`. The model drew a large image;
agentBridge cut out the background and scaled it to exactly the size you asked for.

A few more things to try:

```bash
# A complete favicon set for a website, plus the HTML to paste into <head>
agb icon-set "a friendly rocket, flat style" --out-dir public --preset web

# A short sound effect
agb audio "soft UI click" --out assets/click.wav --duration 0.5

# Resize and convert an image you already have; local and free
agb process logo.png --out public/logo-256.webp --size 256x256
```

To give every image in the project the same look, create a project style and edit it:

```bash
agb style init
```

It writes `agentbridge.toml` with a style description, a palette and things to avoid; you can also point
it to a reference image. See [One style for the whole project](guide.md#one-style-for-the-whole-project).

The [Guide](guide.md) has recipes for common assets.

## 4. Connect your coding agent

Your agent needs to know that `agb` exists and how to use it. Open
[`agent-instructions.md`](agent-instructions.md) and copy everything below the line into the
instruction file your agent reads, at the root of your project:

| Agent | File |
|---|---|
| Claude Code | `CLAUDE.md` |
| Codex, Cursor and most other agents | `AGENTS.md` |

The instructions tell the agent when to reach for `agb`, to always ask for `--json` output, how to
choose sizes and formats, and what to do when something fails, for example to stop and ask you when the
budget is used up. Then just describe what you need:

> Add a pixel-art sword icon and a short click sound to the inventory screen.

The agent runs `agb`, gets the files and wires them into the code.

## 5. Keep an eye on spending

By default an agent can spend at most **$5 per day**, **$2 per project per day** and **$1 per call**.
Calls that would go over are refused before they are sent. To see where you stand:

```bash
agb usage     # today's spending and the last 7 days, against your limits
agb log       # the most recent calls with provider, prompt and cost
```

To change the limits, create a config file and edit it; see [Configuration](configuration.md):

```bash
agb config init
agb config path
```

## Next steps

- [Guide](guide.md): recipes for sprites, icons, favicons, app icons, banners, sounds and voices
- [How it works](concepts.md): routing, the asset pipeline, transparency and budgets in detail
- [Commands](commands.md): the complete reference
