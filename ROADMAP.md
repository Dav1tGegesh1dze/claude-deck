# Roadmap

Each phase should end in something runnable/demoable, not just code. See
[SPEC.md](SPEC.md) for the "why" behind each item.

## Phase 0 — De-risk before building (spikes, not product code)

Goal: prove the two riskiest assumptions before investing in the app shell.

- [x] Install Rust toolchain + `cargo tauri` CLI.
- [ ] Hardware spike: standalone Rust binary using `mirajazz` that connects
      to the AKP03E, pushes a static test PNG to one button, and logs button
      press events to stdout. No UI, no polling — just "can we talk to the
      device at all." **Blocked on hardware access, not scope** — no AKP03E
      has been connected to the dev machine yet. Not treated as a hard
      blocker for Phase 1+: the `device` module will be written against
      `mirajazz`'s API and compiled, but left unverified against real
      hardware until the device is available. Revisit before Phase 5
      release.
- [x] Data spike: confirmed working against a real account. See
      [docs/phase-0-findings.md](docs/phase-0-findings.md) for the payload
      shape and the corrected (platform-dependent) credential-storage
      details — this superseded the `~/.claude/.credentials.json`-only
      assumption below.
- [x] Write down actual findings from both spikes (payload shape, any auth
      quirks, device response latency) — these will likely correct details
      in SPEC.md §4/§5.

**Exit criteria**: a button on the physical device shows a hardcoded image,
and a terminal prints real session/weekly percentages for your account.

## Phase 1 — App skeleton

- [ ] `cargo tauri init` project structure; minimal window that opens.
- [ ] Port the Phase 0 device spike into a `device` module + Tauri command.
- [ ] Port the Phase 0 data spike into a `usage` module with interval
      polling (default 5 min) and a Tauri event emitting fresh data to the
      frontend.
- [ ] Local config file read/write (device selection, refresh interval).

**Exit criteria**: launching the app auto-connects to the AKP03E and starts
polling usage in the background, visible in a bare-bones dev-tools log —
nothing on the device screen yet.

## Phase 2 — Live usage display (the core feature)

- [ ] `render` module: percentage + gauge → PNG sized for the button.
- [ ] Wire session % to button 1, weekly % to button 2, refreshed on each
      poll tick.
- [ ] Color thresholds (green/yellow/red), user-configurable in settings UI.
- [ ] Press-to-reveal reset time on a usage button.
- [ ] Host OS tray icon mirroring the same two numbers.
- [ ] Graceful degradation: "stale since HH:MM" state if polling fails or
      credentials are missing, instead of crashing or showing 0%.

**Exit criteria**: this is a usable v1 — physical device shows live,
accurate session/weekly usage with no manual steps beyond `claude login`.

## Phase 3 — Personalization

- [ ] Built-in icon set + custom image upload per button.
- [ ] Button↔metric reassignment UI (any button can show session, weekly,
      or the Phase 3.5 self-imposed budget).
- [ ] Self-imposed daily/monthly soft-budget feature (§6.3 of SPEC.md):
      config UI, local calculation, distinct visual treatment from the two
      official numbers, optional OS notification on crossing the cap.

**Exit criteria**: two users with the same device can configure visibly
different setups (different icons, different button assignments, one using
the soft-budget feature and one not).

## Phase 4 — Multi-device support & robustness

- [ ] Generalize `device` module using `mirajazz`'s protocol-version
      abstraction; test against at least one non-AKP03E device if
      available, otherwise document as best-effort/untested.
- [ ] Reconnect handling (device unplugged/replugged mid-session).
- [ ] Basic structured logging for troubleshooting reports from other users.

## Phase 5 — Public distribution

- [ ] Pick and add LICENSE (MIT recommended for max adoption).
- [ ] GitHub Actions workflow: build macOS (.dmg) and Windows (.msi/.exe) on
      tag push, attach to a GitHub Release.
- [ ] README: install instructions, supported devices, `claude login`
      prerequisite, unsigned-binary security-prompt note, troubleshooting.
- [ ] Public v1.0.0 release.

## Phase 6 — Stretch (post-v1, not committed)

- [ ] Linux packaging.
- [ ] Optional Anthropic Console API (org billing/spend) as an alternate,
      opt-in data source for API-key users.
- [ ] Multi-account switching.
- [ ] Tauri auto-updater.
