# agentBridge (asset generation)

Copy this section into a project's `AGENTS.md` or `CLAUDE.md` so coding agents know the tool exists.

---

## Generating assets with `agb`

This machine has `agb` (agentBridge), a CLI that generates images, sound effects, speech and text through
the user's own AI provider keys. Use it whenever the task needs an asset you cannot write as code
(icons, sprites, illustrations, UI sounds, voice lines) instead of leaving placeholders.

Always pass `--json` and read the result from stdout.

```bash
agb image "pixel art sword icon, flat colors" --out assets/sword.png --size 64x64 --transparent --json
agb audio "short metallic click" --out assets/click.wav --duration 0.5 --json
agb speech "Welcome back!" --out assets/welcome.mp3 --json
agb ask "Summarize this log" --file build.log --json
```

Rules:
- Name the capability, not the provider; the user's routing decides. Use `--provider` only if the user asked.
- Use `--dry-run --json` first when unsure what a call would cost.
- Check `"ok"`. On failure branch on `error.code`:
  - `budget_exceeded` (exit 3): stop and tell the user; do not retry or switch providers.
  - `no_provider` (exit 4): no usable API key; ask the user to add one with `agb keys set <provider>`.
  - `provider_error` (exit 5): the message says why; adjust the prompt once at most, then report.
  - `output_error` (exit 6): the file exists; pass `--overwrite` only if replacing it is intended.
- Read `warnings`: sizes are not resized yet and some providers cannot make transparent backgrounds.
- Never print, log or ask for API key values.
