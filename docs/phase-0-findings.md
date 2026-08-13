# Phase 0 findings

## Data spike — confirmed working (2026-08-13)

**Credential storage is platform-dependent** — this corrects an assumption
in SPEC.md §5:

- **macOS**: Claude Code stores the OAuth token in the **Keychain**, not a
  file. Service name `Claude Code-credentials`, account = the OS username
  (`whoami`). Read via `security find-generic-password -a "$(whoami)" -s
  "Claude Code-credentials" -w`, which returns a JSON blob containing
  `claudeAiOauth.accessToken` (plus `refreshToken`, `expiresAt`,
  `subscriptionType`, `organizationUuid`, etc).
- **Linux/Windows**: presumed to still be `~/.claude/.credentials.json` as
  originally assumed — **not yet verified**, needs a spike on those
  platforms before the `usage` module ships cross-platform.

**Confirmed request recipe**:

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <accessToken>
anthropic-beta: oauth-2025-04-20
```

Returned HTTP 200 with real data on the first try.

**Response shape** — richer than expected. Two ways to read the same data:

1. Named fields: `five_hour.utilization` / `seven_day.utilization` (plus a
   long list of other keys that are `null` for this account — internal
   feature-flag names like `nimbus_quill`, `cinder_cove`, `tangelo` —
   presumably other limit types not yet relevant to us).
2. **`limits[]` array — use this as the primary parse target.** Each entry
   has `kind` (`"session"` / `"weekly_all"`), `percent`, `severity`
   (`"normal"` / `"critical"` / presumably others), `resets_at`, `is_active`.
   This is cleaner than picking thresholds ourselves — **Anthropic's own
   `severity` field should drive the default green/yellow/red state**, with
   user-configured thresholds in SPEC.md §6.1 as an override, not the only
   source of truth.

There's also a `spend`/`extra_usage` block related to pay-as-you-go overage
credits (org-level, disabled on this account) — out of scope for v1, not a
session/weekly limit.

Reusable probe script: [`spikes/usage_probe.sh`](../spikes/usage_probe.sh)
(macOS only, never prints the raw token).

## Hardware spike — not yet run

AKP03E was not connected to the dev machine during Phase 0 setup. Blocked
until the device is plugged in.
