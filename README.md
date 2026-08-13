# Claude Deck

A companion app for small USB "stream controller" devices (e.g. AJAZZ AKP03E
and similar Ajazz/Mirabox-family hardware) that keeps your Claude Code
session (5-hour) and weekly (7-day) usage visible on the physical device at
all times — no login, no API key. It reads the OAuth credentials Claude Code
already caches locally after `claude login`.

Status: first real-hardware test passed (session/weekly render correctly on
an AKP03E). Still pre-release — see [ROADMAP.md](ROADMAP.md) for what's left.
See [SPEC.md](SPEC.md) for the full design.

## Why

Claude Code enforces a rolling 5-hour session budget and a 7-day weekly
budget. This surfaces both on hardware you're already looking at, instead of
finding out mid-task via a CLI error.

## Supported hardware

- AJAZZ AKP03E (primary/reference device, confirmed working)
- Other devices supported by the [`mirajazz`](https://github.com/4ndv/mirajazz)
  protocol layer (AKP03/AKP03R + rev. 2 variants, Mirabox N3 6602/6603) —
  best-effort, untested

## Features

- Live session (5h) / weekly (7d) usage on physical device buttons, each
  labeled ("5H"/"7D"/"BUD") and colored by Anthropic's own severity
  (normal/warning/critical)
- Menu bar / tray icon mirroring the same numbers
- Per-button metric assignment (session / weekly / a self-imposed daily
  budget) and custom icon images, applied immediately when changed in
  Settings
- Zero-setup auth — reuses the OAuth token `claude login` already cached
  locally (macOS Keychain; `~/.claude/.credentials.json` elsewhere,
  unverified)

## Requirements

- A supported device connected via USB
- Claude Code installed and logged in (`claude login`) on the same machine
- macOS or Windows

### Important: quit the vendor's control software first

If your device came with its own app (e.g. AJAZZ's "Stream Dock"), **quit it
before using Claude Deck.** Claude Deck is display-only in v1 — it paints
button images but doesn't intercept button presses — so if the vendor app is
still running, pressing a button will still trigger whatever action *it* has
bound there, regardless of what image Claude Deck is showing. Two apps can't
both own the same physical device's button actions at once.

### Only assign the buttons you want to give up

In Settings → Button assignments, leave every button you want a *different*
app (vendor software, or nothing) to control set to `none` — Claude Deck
skips those entirely. Only assign `session`/`weekly`/`budget` to buttons you
want dedicated to it.

These button displays are **write-only**: there is no API to read back
what's currently on a button, so Claude Deck has no way to detect "has this
button already been configured by something else," and no way to restore a
button's previous image once it's painted over it. If you accidentally let
Claude Deck take over a button another app was using, use **Settings →
Reset to defaults** to stop Claude Deck from repainting it going forward,
then get the other app to redraw its own image (switch pages/profiles in
it, or unplug/replug the device — most vendor software repaints its whole
grid on reconnect).

## Development

```sh
cd app
npm install
npm run tauri dev
```

Requires Rust (`rustup`) and Node 20+. See [ROADMAP.md](ROADMAP.md) for the
phased build plan and current status of each piece.
