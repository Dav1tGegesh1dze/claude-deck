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
- [x] ~~Custom image upload per button~~ — **built, then removed** after
      real hardware testing (2026-08-14). The `pick_icon_for_button`
      command worked, but it turned out to solve the wrong problem:
      hardware testing showed the actual blocker was button *ownership*
      (vendor software like AJAZZ's Stream Dock repainting over Claude
      Deck on interaction), not decoration. Removed
      `tauri-plugin-dialog`, `render_percent_on_background`, and
      `icon_path` from config to keep scope matched to what actually
      matters. No built-in icon set either — same reasoning.
- [x] Button↔metric reassignment UI: settings grid laid out to match the
      device's physical 2-row-of-3 screen layout (corrected from an
      earlier flat list of all `device::KEY_COUNT` (9) buttons once
      hardware testing showed only `SCREEN_KEY_COUNT` (6) have a screen).
      **No button assigned by default** — user opts in per button, since
      Claude Deck can't know what else is using a given button.
- [x] Self-imposed daily soft-budget feature (`budget.rs`, SPEC.md §6.3):
      enable/cap % configurable in settings, computed locally from the
      weekly-percent delta since local midnight, rendered with its own
      `"budget"` kind (visually identical gauge style, but conceptually
      distinct — never mixed into the real session/weekly numbers). Known
      limitation documented in the module: under-counts for the rest of a
      day if the weekly window resets mid-day. **No OS notification on
      crossing the cap yet** — small follow-up, not done.

**Exit criteria mostly met**: two users can configure different button
layouts and budget settings. OS notifications and press-to-reveal still
open. "Icons" dropped from the exit criteria per the removal above.

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

## Phase 5 — Public distribution (in progress, 2026-08-13)

- [x] LICENSE: MIT (user's choice).
- [x] GitHub Actions: `.github/workflows/release.yml` builds macOS (.dmg)
      and Windows (.msi/.exe) via `tauri-apps/tauri-action` on `v*` tag push,
      creates a **draft** GitHub Release (not auto-published, so a tag push
      can't accidentally ship something unverified). `.github/workflows/ci.yml`
      runs build+test on every push/PR to main/develop.
- [x] README: install requirements, features, supported devices, `claude
      login` prerequisite, dev setup. Unsigned-binary security-prompt note
      still to add once a release is actually cut (wording depends on
      what Gatekeeper/SmartScreen actually show, easier to write accurately
      after seeing it once).
- [ ] Public v1.0.0 release — **blocked**: nothing has been run against
      real hardware yet (see Phase 0). Shipping installers to other AJAZZ
      owners before that would be irresponsible even though the code
      compiles clean and is unit-tested where it can be.

## Known issues — found 2026-08-14, fix before wider sharing

Found during real-world use of v0.1.x, deliberately **not fixed yet** —
documented for the next session rather than changed mid-release-cycle.

- [ ] **429 rate limiting on the usage endpoint at low poll intervals.**
      User hit `HTTP 429 Too Many Requests` with `refresh_interval_secs`
      manually set to 50. Researched rather than guessed: our request
      headers (`Authorization: Bearer`, `anthropic-beta: oauth-2025-04-20`)
      already match a known-working reference implementation
      ([claude-code-statusline](https://github.com/ohugonnot/claude-code-statusline)),
      which explicitly defaults to `REFRESH_INTERVAL=300` and warns *"do
      not set to 0 (causes rate limiting)"*. So this isn't a missing-header
      bug — the endpoint's undocumented rate limit genuinely does not
      tolerate aggressive polling. Two related upstream reports confirm
      this is a known rough edge in the endpoint itself, not something we
      can fully engineer around:
      [anthropics/claude-code#31021](https://github.com/anthropics/claude-code/issues/31021),
      [anthropics/claude-code#31637](https://github.com/anthropics/claude-code/issues/31637)
      (the latter also notes 429s can persist with no `Retry-After` header
      even after exponential backoff).
      **Proposed fix**: enforce a sane minimum in the refresh-interval
      setting UI (user suggested 100s as a floor; given the reference
      tool's proven-safe default is 300s, consider defaulting new installs
      to 300s and floor-clamping manual input well above 50 — exact
      numbers to decide next session) plus handling a `429`/`Retry-After`
      response by backing off rather than just marking the poll failed.
- [ ] **Tray/menu-bar icon renders, but as an unrecognizable blank
      marker** — confirmed 2026-08-14 via a real screenshot from the user
      (corrects the earlier guess that nothing renders at all: something
      does show up and is clickable — right menu items appear ("Show
      Claude Deck" / "Quit") — it's just not a real icon, so it's
      practically invisible among other apps' icons in the menu bar).
      Root cause still applies: checked `TrayIconBuilder` in `lib.rs`,
      `.icon(...)` is never called, and Tauri's own source
      (`crates/tauri/src/tray/mod.rs`) confirms no automatic fallback
      exists — whatever's rendering is some minimal OS/framework
      placeholder, not our app icon.
      **Proposed fix**: add a proper small monochrome "template" icon
      (macOS menu-bar convention — single-color silhouette + alpha, like
      Scroll Reverser or AJAZZ's own tray icon use) and pass it to
      `.icon(...)`. See SPEC.md §6.4 for the requirement this adds.
- [ ] **Quitting via the Dock (right-click/two-finger-click → Quit) or
      Cmd+Q bypasses the hide-on-close fix entirely and fully exits the
      app**, stopping background updates the same way the pre-v0.1.1 bug
      did. Root cause: v0.1.1 only intercepts `WindowEvent::CloseRequested`
      (the window's own close button). Standard macOS quit gestures don't
      go through that event at all — they go through the app-level exit
      flow directly.
      **Investigated the technical fix**: Tauri exposes
      `RunEvent::ExitRequested { code, api }` in the `.run()` event loop.
      `api.prevent_exit()` can block it, and `code` distinguishes *why* it
      fired: `None` = user-initiated (Cmd+Q, Dock quit), `Some(_)` = our
      own programmatic call (e.g. the tray's own "Quit" item, which calls
      `app.exit(0)`). So it's technically straightforward to block only
      the ambient OS-level quit gestures while still letting our own
      "Quit" menu item work — this is not a hard problem, it's a **design
      decision, not investigated further than confirming the mechanism**:
      - Do nothing — Cmd+Q/Dock-Quit actually quitting is standard macOS
        convention; apps that silently ignore it are usually considered
        broken (or malware-like). Once the tray icon fix above makes the
        app's presence obvious, that may be enough of a signal.
      - Intercept and always block ambient quit requests — matches "never
        stops monitoring," but fights the platform's own conventions and
        could feel like the app can't be quit, which is a bad look.
      - Intercept and show a confirmation dialog ("Quitting stops Claude
        Deck from updating your device — quit anyway?") — middle ground,
        catches accidental muscle-memory quits without fully overriding
        user intent.
      - Independent of the above: on any deliberate quit, push a
        visibly "paused/stopped" image to the assigned buttons before
        actually exiting, so the physical device honestly shows it
        stopped instead of silently freezing on stale numbers. Worth
        doing regardless of which quit-policy is chosen.
      - Add a "Launch at Login" setting so even an accidental quit
        self-heals on next login — softer mitigation, doesn't require
        fighting the OS at all.
      **Decide next session** which combination to implement — this is a
      product-philosophy call as much as a technical one.

## Phase 6 — Stretch (post-v1, not committed)

- [ ] Linux packaging.
- [ ] Optional Anthropic Console API (org billing/spend) as an alternate,
      opt-in data source for API-key users.
- [ ] Multi-account switching.
- [ ] Tauri auto-updater.
