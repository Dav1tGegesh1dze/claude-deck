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

## Phase 1 — App skeleton (done, 2026-08-13)

- [x] `npm create tauri-app` project structure (vanilla-ts + Rust), lives in
      `app/`; minimal window opens, smoke-tested (launched, stayed alive,
      no crash, clean build with zero warnings).
- [x] `device` module (`app/src-tauri/src/device.rs`): VID/PID table for
      the AKP03 family, `connect`, `push_test_pattern`, `read_events_once`,
      all wired as Tauri commands. **Written against mirajazz 0.16.2's real
      API (verified via clean compile), not yet run against physical
      hardware** — still blocked on the AKP03E being connected.
- [x] `usage` module (`app/src-tauri/src/usage.rs`): reads the Keychain
      token, polls the usage endpoint every `refresh_interval_secs`
      (default 300s), emits `usage://updated` / `usage://error` events.
      Runs an immediate poll on startup too.
- [x] Local config file read/write (`app/src-tauri/src/config.rs`) — JSON
      under the OS app-config dir. `refresh_interval_secs` only for now;
      device selection deferred to Phase 3 (multiple devices aren't
      supported yet anyway).
- [x] Went slightly further than "bare-bones log": the frontend
      (`app/index.html`, `app/src/main.ts`) already renders session/weekly
      % + severity + reset time as real UI, plus manual spike-test buttons
      (list devices / push test image / read button events) — this
      overlaps a bit with Phase 2 but cost little extra once the Tauri
      event wiring existed.

**Exit criteria met**: launching the app polls usage in the background and
shows it in a real window. Device-side exit criteria ("button shows a
hardcoded image") still pending real hardware.

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
