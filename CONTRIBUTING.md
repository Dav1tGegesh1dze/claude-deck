# Contributing to Claude Deck

Thanks for considering a contribution. This project is developed almost
entirely with Claude Code, tested on real hardware where possible, and
kept deliberately small — please read the notes below before opening a
PR, they'll save you a review round-trip.

## Before you start

- Check [ROADMAP.md](ROADMAP.md) (build history + known issues) and
  [SPEC.md](SPEC.md) (design decisions) first. Some things that might
  look like bugs or missing features are intentional — e.g. no buttons
  are assigned by default, and the per-button custom-icon-upload feature
  was deliberately removed. If you're not sure whether something is a
  bug or a decision, open an issue and ask before writing code.
- For anything touching device/hardware behavior (`app/src-tauri/src/device.rs`,
  button image rendering/pushing, connect/reconnect logic): **only the
  AJAZZ AKP03E has been tested against real hardware.** Other
  Ajazz/Mirabox variants listed in `device::Kind` are best-effort,
  untested. If you have one of those devices, real-hardware testing is
  the single most valuable thing you can contribute — say in your PR
  description exactly which device you tested on and what you observed.

## Workflow

1. **Branch off `develop`, not `main`.** `main` is release-only and
   protected — direct pushes aren't possible.
2. **Name your branch by type**: `feature/short-description`,
   `fix/short-description`, or `chore/short-description`.
3. **Open your PR against `develop`**, not `main`.
4. In the PR description, include:
   - **What** changed and **why** (the why matters more — link an issue
     if there is one).
   - **Test plan**: what you actually ran (`cargo check`, `cargo test`,
     `npx tsc --noEmit`), and — critically for anything hardware-related
     — whether you tested on real hardware and which device. "Compiles
     but untested on hardware" is a fine thing to say explicitly; it's
     much better than leaving it unstated.
5. A maintainer reviews and merges into `develop`. Releases to `main` are
   cut periodically from accumulated `develop` changes, not per-PR.

## Code style

- No comments explaining *what* code does — names should already make
  that clear. Comments are for *why*, when it's genuinely non-obvious
  (a hidden constraint, a workaround, a hardware quirk found the hard
  way).
- Match the existing scope of a change to the problem: a bug fix
  shouldn't carry unrelated refactors or new abstractions along with it.
- Rust: `cargo check` and `cargo test` (in `app/src-tauri/`) must pass
  clean. TypeScript: `npx tsc --noEmit` (in `app/`) must pass clean.

## Reporting bugs

Open a GitHub issue. Include your OS, the exact device model (VID/PID if
you know it — `Settings → List devices` in the app will show it), and
what you observed vs. expected. For anything intermittent (reconnect
timing, sleep/wake behavior), the app's own log file is the most useful
thing to attach:

- macOS: `~/Library/Logs/com.davitgegeshidze.claude-deck/Claude Deck.log`
- Windows: `%APPDATA%\com.davitgegeshidze.claude-deck\logs\Claude Deck.log`
  (Tauri's default log-plugin location — not independently verified on a
  Windows machine in this project yet; if it's wrong, that's itself a
  useful bug report)
