# Claude Deck — Product & Technical Specification

## 1. Problem

Claude Code / Claude.ai Pro & Max plans enforce two usage limits — a rolling
**5-hour session** budget and a **7-day (weekly)** budget — and hitting either
one blind, mid-task, is the thing this project exists to prevent. There is no
official monthly limit; only these two exist.

The product is a small companion app that talks to a USB "stream controller"
style device (small per-key LCD buttons, e.g. AJAZZ AKP03E and similar
Ajazz/Mirabox-family hardware) and keeps live session/weekly usage visible on
the physical device at all times, independent of whether Claude Code is
actively running.

## 2. Goals

- Show **live, accurate** session (5h) and weekly (7d) usage % + reset
  countdown on physical device buttons, refreshed in the background.
- **Zero-setup auth**: reuse the OAuth credentials Claude Code already stores
  locally after `claude login` — no separate login, no pasted API key.
- Let the user **assign which metric (or none) each screen button shows**,
  with no buttons assigned by default — the user always opts in explicitly,
  since Claude Deck can't know which physical buttons other software (e.g.
  vendor control apps) is already using.
- Support an optional **user-defined ("self-imposed") daily/monthly soft
  budget** on top of the real limits, since Anthropic doesn't expose those.
- Ship as a **single downloadable installer** for macOS and Windows via
  GitHub Releases — no separate third-party app required.
- Architect the device layer so it isn't hard-locked to one SKU — the AKP03E
  is the primary target and test device, but the same app should work with
  other devices the `mirajazz` protocol layer already understands (other
  Ajazz models, Mirabox 293S/N3/N4).

## 3. Non-goals (v1)

- Linux support (revisit once macOS/Windows are solid — Tauri/mirajazz both
  support Linux, so this is a later config/packaging task, not a rewrite).
- Anthropic Console API billing/spend tracking (organization admin usage —
  different auth model, different audience; explicitly deferred).
- Multi-account switching, team/org dashboards.
- Full Stream Deck plugin-SDK compatibility (we are not building a general
  automation platform, just a usage display — no arbitrary button actions in
  v1 beyond "show a metric" / "open a link").

## 4. Target hardware

- **Primary / test device**: AJAZZ AKP03E Desktop Console (International
  Version).
- **Device layer**: built on the `mirajazz` Rust crate (crates.io), which
  talks to Ajazz AKP03/AKP153 and Mirabox 293S/N3/N4 devices via a
  protocol-version abstraction (v0–v3) rather than per-model hardcoding —
  this is what makes multi-device support realistic without us reverse
  engineering USB traffic ourselves.
- Capabilities we rely on: per-button image push (PNG), button press events,
  screen clear, brightness control. GIF support (protocol v3+) is a later
  nice-to-have, not required for v1.
- No vendor SDK/documentation exists for these devices; `mirajazz` is itself
  built on community reverse engineering. **Risk**: protocol quirks per
  firmware revision are possible — Phase 0 includes a hardware spike to
  confirm basic image-push and button-press work on the actual AKP03E unit
  before committing further engineering time.

## 5. Data source & auth

- **Primary source**: the same undocumented endpoint Claude Code's own
  statusline integration and community tools (`claude-code-statusline`,
  `claude-usage-widget`) already use — `https://api.anthropic.com/api/oauth/usage`,
  called with `Authorization: Bearer <accessToken>` and
  `anthropic-beta: oauth-2025-04-20` (confirmed working in Phase 0). The
  response includes a `limits[]` array (`kind`, `percent`, `severity`,
  `resets_at`, `is_active`) — parse this rather than the flatter
  `five_hour`/`seven_day` fields, and use Anthropic's own `severity` as the
  default color state rather than inventing our own thresholds.
- **Credentials**: read the OAuth token Claude Code already caches locally
  after `claude login`. This is **platform-dependent** (confirmed in Phase 0
  — see [docs/phase-0-findings.md](docs/phase-0-findings.md)): on macOS it's
  the Keychain (service `Claude Code-credentials`, account = OS username),
  not a file; `~/.claude/.credentials.json` is presumed for Linux/Windows
  but not yet verified. If no credential is found, the app shows a clear
  "run `claude login` first" state — it never prompts for a separate login
  or API key.
- **Polling**: background poll on an interval (default 5 min, user
  configurable, minimum floor to avoid hammering an undocumented endpoint —
  treat this as a scarce/fragile resource, not a normal API).
- **Fragility is a first-class risk**: this endpoint is undocumented and
  unsupported. The app must degrade gracefully (show "stale data since
  HH:MM" rather than crash or silently show wrong numbers) if the endpoint
  changes shape or starts rejecting requests, and should be built with a
  thin adapter around this call so swapping the source later (should
  Anthropic ship an official API, e.g. the statusline `rate_limits` stdin
  field for in-session cases) is a small, contained change.
- **No monthly limit**: confirmed there isn't one. We show session (5h) and
  weekly (7d) only for the "real" numbers (see §6.3 for the separate,
  clearly-labeled self-imposed budget feature).

## 6. Features (v1)

### 6.1 Live usage display
- Each of the AKP03E's 6 screen buttons can be independently assigned
  session / weekly / self-imposed budget / none, laid out in the settings
  UI to match the device's real 2-row-of-3 physical grid. **No buttons are
  assigned by default** — see §6.2.
- Each assigned button renders a generated PNG: a fill-bar gauge + a short
  label ("5H"/"7D"/"BUD") + the percentage as text, colored by Anthropic's
  own `severity` field (normal/warning/critical) rather than user-defined
  thresholds — added after hardware testing showed identical-looking
  buttons were impossible to tell apart.
- Pressing a usage button showing the exact reset time (e.g. a brief
  overlay) is **not implemented** — deferred, see ROADMAP.md Phase 3.

### 6.2 Button ownership, not customization
- **Removed from scope** (was: built-in icon set + custom image upload per
  button). Real hardware testing showed the harder problem wasn't
  decoration, it was **ownership**: these HID button displays are
  write-only (no readback), and if another app (e.g. AJAZZ's Stream Dock)
  still manages a button, it actively repaints its own icon back over
  Claude Deck's on interaction — there is no partial-override possible.
  Given that, per-button custom icons added complexity without addressing
  the actual problem users hit.
- Because of this, **no button is assigned by default** — assigning one
  is an explicit, deliberate choice, and the settings UI carries a
  standing warning to first set that same button to blank/no-action in
  any other control software before assigning it here.
- A "reset all to none" action exists for quickly backing out of an
  assignment (it cannot restore what another app had on the button —
  nothing can, given the write-only display).

### 6.3 Self-imposed daily/monthly budget ("one-day limiter")
- Since Anthropic doesn't expose real daily/monthly limits, this is an
  explicitly user-defined, locally-computed soft cap: "warn me if I use more
  than X% of my weekly budget before Y time today/this month."
- Computed locally from the same session/weekly polling data (e.g. rate of
  change since the last weekly reset), not a separate data source.
- Clearly labeled in the UI as a **personal budget**, visually distinct from
  the two official limits, so it's never confused with an Anthropic-enforced
  number.
- Optional OS-level notification when the soft cap is crossed.

### 6.4 Host-side presence
- Menu bar / system tray icon on the host OS mirroring the same two
  official numbers, for when the user isn't looking at the physical device.
- Settings window: button↔metric mapping (grid matching the physical
  device), refresh interval, soft-budget configuration.

## 7. Architecture

- **Framework**: Tauri (Rust backend + web frontend). Chosen because the
  maintained device libraries (`mirajazz`, `ajazz-sdk`, `elgato-streamdeck`)
  are Rust-native — Tauri lets the backend call them directly with no FFI/IPC
  bridge to a separate helper process, and its bundler produces native
  installers (.dmg / .msi) for both target platforms.
- **Backend (Rust)**:
  - `device` module: thin wrapper over `mirajazz`, device discovery/connect,
    image push, button-press event stream.
  - `usage` module: OAuth-credential reader, poller, adapter around the
    `/api/oauth/usage` call, in-memory cache with staleness tracking.
  - `render` module: turns a usage percentage + label into a PNG sized
    for the target button.
  - `budget` module: local soft-cap calculation for §6.3.
  - Tauri commands/events bridge state to the frontend.
- **Frontend (web view)**: settings UI only (button/metric configuration)
  — the physical device is the primary "display," the desktop window is
  just for configuration and the tray fallback view.
- **Storage**: local config file (JSON) under the OS app-data dir —
  button↔metric mappings, refresh interval, budget settings. No cloud
  sync, no telemetry.

## 8. Distribution

- Public GitHub repository, MIT-licensed (confirm license choice before
  first release).
- GitHub Actions CI builds and attaches signed-where-possible installers
  (.dmg for macOS, .msi/.exe for Windows) to GitHub Releases on tag push.
  Unsigned builds will trigger Gatekeeper/SmartScreen warnings on first run
  — acceptable for v1, call out in the README, revisit signing certificates
  later if adoption warrants the cost.
- README covers: supported devices, install steps, `claude login`
  prerequisite, troubleshooting for the "no credentials found" state.

## 9. Key risks (ranked)

1. **Undocumented usage endpoint** could change or disappear without notice
   — mitigated by the adapter boundary in §5, not by hoping it's stable.
2. **Undocumented device protocol** could behave differently on other
   Ajazz/Mirabox SKUs than on the AKP03E test unit — mitigated by scoping
   "multi-device support" as best-effort, tested primarily against AKP03E.
3. **Unsigned installers** produce OS security warnings that hurt adoption
   for a general GitHub download audience — acceptable tradeoff for v1,
   documented, revisit if the project gains traction.
