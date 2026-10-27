# Guide

Practical recipes for the assets projects need most. Each recipe works the same whether you type the
command yourself or your coding agent runs it.

- [Game sprites and items](#game-sprites-and-items)
- [UI icons](#ui-icons)
- [Website favicons](#website-favicons)
- [Desktop app icons](#desktop-app-icons)
- [Banners, backgrounds and illustrations](#banners-backgrounds-and-illustrations)
- [Working with images you already have](#working-with-images-you-already-have)
- [Sound effects](#sound-effects)
- [Voice lines](#voice-lines)
- [Asking a text model](#asking-a-text-model)
- [Choosing a provider or model](#choosing-a-provider-or-model)
- [Writing good prompts](#writing-good-prompts)

## Game sprites and items

```bash
agb image "pixel art iron sword, 16-bit style, flat colors" \
  --out assets/items/sword.png --size 64x64 --transparent --trim
```

- `--transparent` removes the background, whichever provider made the image.
- `--trim` crops the empty margin around the subject, so the sword fills the 64×64 frame.
- Without `--fit`, transparent images are fitted with `contain`: the whole subject is kept and the rest
  is padded with transparency.

For a set of items in the same style, keep the style words identical in every prompt:

```bash
agb image "pixel art health potion, 16-bit style, flat colors" --out assets/items/potion.png --size 64x64 --transparent --trim
agb image "pixel art wooden shield, 16-bit style, flat colors" --out assets/items/shield.png --size 64x64 --transparent --trim
```

## UI icons

```bash
agb image "minimal outline settings gear icon, dark gray, rounded strokes" \
  --out src/assets/icons/settings.png --size 48x48 --transparent --trim
```

For sharp results at small sizes, ask for simple shapes and few colors. Generate at the size you
display, or at 2× for high-density screens (`--size 96x96`).

## Website favicons

One command creates every file browsers and phones look for:

```bash
agb icon-set "a friendly rocket, flat style, bold shapes" --out-dir public --preset web
```

| File | Used by |
|---|---|
| `favicon.ico` (16, 32 and 48 px inside) | Browser tabs, bookmarks, older browsers |
| `favicon-16x16.png`, `favicon-32x32.png` | Modern browser tabs |
| `apple-touch-icon.png` (180 px, opaque) | iPhone and iPad home screens |
| `android-chrome-192x192.png`, `android-chrome-512x512.png` | Android home screens, installable web apps |
| `site.webmanifest` | Web app manifest that lists the Android icons |

The command also prints the tags to paste into your page's `<head>`:

```html
<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png">
<link rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png">
<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
<link rel="manifest" href="/site.webmanifest">
```

The generated master image is kept as `icon-source.png` in the same folder. Already have a logo?
Build the set from it, for free:

```bash
agb icon-set --from brand/logo.png --out-dir public --preset web
```

> [!NOTE]
> iOS shows transparent areas of home-screen icons as black, so `apple-touch-icon.png` is always placed
> on a white background. The other icons keep their transparency.

## Desktop app icons

```bash
agb icon-set "a friendly rocket, flat style" --out-dir build/icons --preset app
```

This writes `icon-32.png` through `icon-1024.png` and a multi-size Windows `icon.ico` (16 to 256 px),
the formats desktop frameworks such as Tauri, Electron and .NET ask for on Windows and Linux. A macOS
`.icns` file is not produced yet.

## Banners, backgrounds and illustrations

```bash
agb image "wide illustration of a cozy coding desk at night, warm light" \
  --out public/hero.webp --size 1600x900
```

Opaque images default to `--fit cover`: the image fills the whole frame and any overflow is cropped
from the center. Use `--fit contain` to keep the full image and pad it instead, or `--fit stretch` to
ignore the aspect ratio. For photos and illustrations, `.webp` or `.jpg` files are much smaller than
`.png`.

## Working with images you already have

`agb process` uses the same finishing steps on existing files. It runs on your machine, sends nothing
to any provider and costs nothing:

```bash
# Resize and convert
agb process screenshot.png --out docs/screenshot.webp --size 1280x720

# Remove a flat background and crop the margins
agb process product-on-white.jpg --out assets/product.png --transparent --trim

# Make a square thumbnail
agb process photo.jpg --out thumbs/photo.jpg --size 300x300 --fit cover
```

Background removal works when the background is one flat color, such as a studio white or a solid
color. For busy photographic backgrounds, agb leaves the image unchanged and reports a warning.

## Sound effects

```bash
agb audio "short metallic UI click" --out assets/sfx/click.wav --duration 0.5
agb audio "magical sparkle, rising, bright" --out assets/sfx/powerup.mp3 --duration 1.5
agb audio "gentle rain loop, no thunder" --out assets/sfx/rain.mp3 --duration 20
```

Sound effects are made by ElevenLabs. `--duration` accepts 0.5 to 30 seconds; without it the provider
picks a length. `.wav` files are uncompressed 16-bit audio, which game engines load without decoding;
`.mp3` files are smaller.

## Voice lines

```bash
agb speech "Welcome back, commander." --out assets/voice/welcome.mp3
agb speech "Mission complete." --out assets/voice/complete.wav --voice nova --provider openai
```

Speech is made by ElevenLabs or OpenAI, depending on your routing. `--voice` takes the provider's
voice name or id; the defaults are set in [Configuration](configuration.md).

## Asking a text model

```bash
agb ask "Summarize what went wrong in this build log" --file build.log
agb ask "Write 20 fantasy tavern names, one per line" --out data/tavern-names.txt
```

`ask` is useful for a second opinion from another model, for summarizing large files or for generating
placeholder content. The answer is printed, or saved with `--out`. Each `--file` may be up to 2 MB.

## Choosing a provider or model

By default, the provider is chosen by your [routing](configuration.md#routing): the first provider in
the list that has a key, with fallback to the next one if it fails. To choose for a single call:

```bash
# Use one provider, with no fallback
agb image "…" --out a.png --provider gemini

# Use a specific model; the provider is recognized from the name
agb image "…" --out a.png --model gpt-image-1
agb ask "…" --model gemini-2.5-pro

# Any OpenRouter model, written as vendor/model
agb image "…" --out a.png --model google/gemini-2.5-flash-image
agb ask "…" --model anthropic/claude-sonnet-4
```

Images through OpenRouter use its [Image API](https://openrouter.ai/collections/image-models), so any
image model works, including image-only models such as `qwen/qwen-image-2.1-turbo` or
`black-forest-labs/flux.2-pro`. OpenRouter requires a balance of at least $1.00 in the account for
image requests; below that, calls fail with `HTTP 402`.

## Writing good prompts

- **Describe the subject, style and use:** *"pixel art iron sword, 16-bit style, flat colors, game
  inventory icon"* works better than *"sword"*.
- **Keep style words identical** across a set of assets so they look like they belong together.
- **Ask for simple shapes** for anything shown small, such as icons and favicons, and avoid text;
  lettering becomes unreadable at small sizes.
- **Do not describe the background** when using `--transparent`. agentBridge adds the right
  instructions for the provider and removes the background afterwards.
- **Preview with `--dry-run`** whenever you are unsure which provider will be used or what it costs.
