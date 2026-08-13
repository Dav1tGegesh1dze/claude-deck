# Claude Deck

A companion app for small USB "stream controller" devices (e.g. AJAZZ AKP03E
and similar Ajazz/Mirabox-family hardware) that keeps your Claude Code
session (5-hour) and weekly (7-day) usage visible on the physical device at
all times — no login, no API key. It reads the OAuth credentials Claude Code
already caches locally after `claude login`.

## Download

**[Get the latest release →](https://github.com/Dav1tGegesh1dze/claude-deck/releases/latest)**
(macOS and Windows installers)

Status: tested through several rounds of real hardware fixes on an AJAZZ
AKP03E (macOS). Windows builds are produced by CI but haven't been tested on
actual Windows hardware. See [ROADMAP.md](ROADMAP.md) for what's still open
and [SPEC.md](SPEC.md) for the full design.

## Why

Claude Code enforces a rolling 5-hour session budget and a 7-day weekly
budget. This surfaces both on hardware you're already looking at, instead of
finding out mid-task via a CLI error.

## Supported hardware

- AJAZZ AKP03E (primary/reference device — this is what it's been tested on)
- Other devices supported by the [`mirajazz`](https://github.com/4ndv/mirajazz)
  protocol layer (AKP03/AKP03R + rev. 2 variants, Mirabox N3 6602/6603) —
  best-effort, untested

## Features

- Live session (5h) / weekly (7d) usage on physical device buttons, each
  labeled ("5H"/"7D"/"BUD") and colored by Anthropic's own severity
  (normal/warning/critical)
- Menu bar / tray icon mirroring the same numbers
- Per-button metric assignment (session / weekly / a self-imposed daily
  budget / none), laid out in Settings to match the device's physical
  grid, applied immediately when changed
- **No buttons assigned on first install** — every button starts at
  `none`; you opt in per button. Once you assign one, that choice is saved
  and reloaded on every future launch (it does not reset itself)
- Zero-setup auth — reuses the OAuth token `claude login` already cached
  locally (macOS Keychain; `~/.claude/.credentials.json` elsewhere,
  unverified on Windows/Linux)

## Requirements

- A supported device connected via USB
- Claude Code installed and logged in (`claude login`) on the same machine
- macOS or Windows

## Setup: configure your device's own software first

If your device came with its own control app (e.g. AJAZZ's "Stream Dock"),
**you need to use it before using Claude Deck** — not instead of it:

1. Open your device's own app (Stream Dock, etc.) and decide which buttons
   you want to dedicate to Claude Deck's usage display.
2. For each of those buttons, set them to **blank / no action** in that
   app. Don't leave a hotkey or app-launch bound to them.
3. *Then* open Claude Deck and assign `session`/`weekly`/`budget` to those
   same button positions in Settings.

Why this order matters, confirmed against real hardware: Claude Deck is
display-only — it paints button images but never intercepts button
presses. If the vendor app still considers itself in charge of a button,
it actively **repaints its own icon back** over Claude Deck's the moment
you interact with that button. There is no partial-override — a button is
either the vendor app's or Claude Deck's, not both. If you skip step 2,
Claude Deck's display will keep getting overwritten and pressing the
button will trigger whatever the vendor app had bound there.

The AKP03E has 9 physical buttons but only **6 have a screen** (2 rows of
3) — the other 3 are plain push-buttons and aren't shown in Claude Deck's
settings, since there's nowhere to display anything on them.

These button displays are also **write-only**: there is no API to read
back what's currently on a button, so Claude Deck has no way to detect
"has this button already been configured by something else." If you
assign a button in Claude Deck and change your mind, **Settings → Reset
all to none** (or switch that one button back to `none`) actively blanks
its screen — it does not and cannot restore whatever the vendor app had
on it before; that app has to repaint its own icon itself (switch
pages/profiles in it, or unplug/replug the device).

## Development

```sh
cd app
npm install
npm run tauri dev
```

Requires Rust (`rustup`) and Node 20+. See [ROADMAP.md](ROADMAP.md) for the
phased build plan and current status of each piece.
