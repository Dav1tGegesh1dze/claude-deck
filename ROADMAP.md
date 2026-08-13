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

## Phase 2 — Live usage display (the core feature) (done, 2026-08-13)

- [x] `render` module (`app/src-tauri/src/render.rs`): percentage → fill-bar
      gauge + centered percentage text, PNG sized for the button. Uses a
      bundled Roboto variable font (OFL license, see
      `app/src-tauri/assets/Roboto-OFL.txt`). Unit-tested (size, fill
      behavior, out-of-range clamping) — no hardware needed to verify this
      part.
- [x] Wire session % to button 0, weekly % to button 1, refreshed on each
      poll tick and on manual refresh. Device connects lazily/best-effort:
      if no supported device is plugged in, the app just skips the device
      push and keeps working via its own window + tray — this is the
      graceful-degradation behavior for "no device" as well as "poll
      failed". **Still hardware-unverified** (see Phase 0/1 notes).
- [x] Color: uses Anthropic's own `severity` field directly (normal/
      warning/critical → green/amber/red) per SPEC.md §5, rather than
      user thresholds. User-configurable overrides are still a Phase 3 item.
- [x] Host OS tray icon (`tauri::tray`) mirroring session/weekly %, updated
      on every poll; has a Quit menu item.
- [x] Graceful degradation: frontend now shows "Stale since HH:MM — <error>"
      on a failed poll instead of blanking the numbers (`app/src/main.ts`).
- **Deferred to Phase 3**: press-to-reveal reset time on a button press.
  Reason: this needs a continuous background button-reader holding the
  device's one-reader-at-a-time slot, which would conflict with the
  Phase 0/1 manual spike commands (`push_test_pattern`,
  `read_button_events`) — better to design that interaction once real
  hardware is available to actually observe the conflict, instead of
  guessing at concurrency behavior blind.

**Exit criteria**: this is a usable v1 — physical device shows live,
accurate session/weekly usage with no manual steps beyond `claude login`.

## Phase 3 — Personalization (mostly done, 2026-08-13)

- [ ] Press-to-reveal reset time on a usage button (still deferred — needs
      real hardware to design the reader/reconnect interaction safely).
- [x] Custom image upload per button (`pick_icon_for_button` command: native
      file picker → copies into the app config dir → assigned as that
      button's background). **No curated built-in icon set** — scope-cut
      as a content/design task, not an architectural one; custom upload
      covers the "choose the icon" requirement from the original ask.
      Revisit if a built-in set turns out to matter to users.
- [x] Button↔metric reassignment UI: settings table for all
      `device::KEY_COUNT` (9) buttons, each independently assigned
      session/weekly/budget/none via `set_button_metric`.
- [x] Self-imposed daily soft-budget feature (`budget.rs`, SPEC.md §6.3):
      enable/cap % configurable in settings, computed locally from the
      weekly-percent delta since local midnight, rendered with its own
      `"budget"` kind (visually identical gauge style, but conceptually
      distinct — never mixed into the real session/weekly numbers). Known
      limitation documented in the module: under-counts for the rest of a
      day if the weekly window resets mid-day. **No OS notification on
      crossing the cap yet** — small follow-up, not done.

**Exit criteria mostly met**: two users can configure different button
layouts, icons, and budget settings. OS notifications and press-to-reveal
still open.

## Phase 4 — Multi-device support & robustness (2026-08-13)

- [x] `device` module already generalized via `mirajazz`'s VID/PID +
      protocol-version table since Phase 1 (covers AKP03/AKP03E/AKP03R +
      rev.2 variants + Mirabox N3 6602/6603) — nothing new needed here.
      Still **untested against any non-AKP03E device**, and untested
      against AKP03E itself (no hardware connected during development).
- [x] Reconnect handling already exists since Phase 2: `apply_snapshot`
      clears the held device connection on any push failure, so the next
      poll tick attempts `connect_first()` again. Lightweight (poll-interval
      granularity, not event-driven), but functional. A faster
      `DeviceWatcher`-based reconnect is a possible future improvement, not
      needed for v1.
- [x] Structured logging: `tauri-plugin-log`, all `println!`/`eprintln!`
      replaced with `log::info!`/`log::error!`. Logs to stdout in dev and
      the OS log dir in production, so a user reporting an issue has a real
      file to share.

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
