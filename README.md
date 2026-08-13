# Claude Deck

A companion app for small USB "stream controller" devices (e.g. AJAZZ AKP03E
and similar Ajazz/Mirabox-family hardware) that keeps your Claude Code
session (5-hour) and weekly (7-day) usage visible on the physical device at
all times — no login, no API key. It reads the OAuth credentials Claude Code
already caches locally after `claude login`.

Status: pre-alpha, in planning. See [SPEC.md](SPEC.md) for what this is and
[ROADMAP.md](ROADMAP.md) for the build plan and current phase.

## Why

Claude Code enforces a rolling 5-hour session budget and a 7-day weekly
budget. This surfaces both on hardware you're already looking at, instead of
finding out mid-task via a CLI error.

## Supported hardware (planned)

- AJAZZ AKP03E (primary/reference device)
- Other devices supported by the [`mirajazz`](https://github.com/4ndv/mirajazz)
  protocol layer (other Ajazz models, Mirabox 293S/N3/N4) — best-effort

## Status

Nothing installable yet — see ROADMAP.md Phase 0/1.
