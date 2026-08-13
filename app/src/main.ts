import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface LimitEntry {
  kind: string;
  percent: number;
  severity: string;
  resets_at: string | null;
  is_active: boolean;
}

interface UsageSnapshot {
  limits: LimitEntry[];
  fetched_at: string;
}

interface DiscoveredDevice {
  name: string;
  vid: number;
  pid: number;
}

type ButtonEvent = { Down: number } | { Up: number };

type Metric = "session" | "weekly" | "budget" | "none";

interface ButtonAssignment {
  metric: Metric;
  icon_path: string | null;
}

interface BudgetConfig {
  enabled: boolean;
  daily_cap_percent: number;
}

interface AppConfig {
  refresh_interval_secs: number;
  buttons: ButtonAssignment[];
  budget: BudgetConfig;
}

// Matches device::SCREEN_KEY_COUNT in the Rust backend. The AKP03-family
// device has 9 physical buttons total, but only the first 6 have a screen
// (the other 3 are plain push-buttons) - those 6 are all that's
// configurable here.
const SCREEN_KEY_COUNT = 6;

let lastFetchedAt: string | null = null;

function renderSnapshot(snapshot: UsageSnapshot) {
  renderLimit("session", snapshot.limits.find((l) => l.kind === "session"));
  renderLimit("weekly", snapshot.limits.find((l) => l.kind === "weekly_all"));

  lastFetchedAt = snapshot.fetched_at;
  setStatus(`Last updated ${new Date(snapshot.fetched_at).toLocaleTimeString()}`);
}

function renderLimit(prefix: "session" | "weekly", entry: LimitEntry | undefined) {
  const percentEl = document.querySelector<HTMLElement>(`#${prefix}-percent`);
  const severityEl = document.querySelector<HTMLElement>(`#${prefix}-severity`);
  const resetsEl = document.querySelector<HTMLElement>(`#${prefix}-resets`);

  if (!entry) {
    if (percentEl) percentEl.textContent = "—";
    if (severityEl) severityEl.textContent = "no data";
    if (resetsEl) resetsEl.textContent = "";
    return;
  }

  if (percentEl) percentEl.textContent = `${Math.round(entry.percent)}%`;
  if (severityEl) {
    severityEl.textContent = entry.severity;
    severityEl.className = `severity severity-${entry.severity}`;
  }
  if (resetsEl && entry.resets_at) {
    resetsEl.textContent = `resets ${new Date(entry.resets_at).toLocaleString()}`;
  }
}

function setStatus(text: string) {
  const status = document.querySelector<HTMLElement>("#status-line");
  if (status) status.textContent = text;
}

function setStale(err: unknown) {
  if (lastFetchedAt) {
    setStatus(
      `Stale since ${new Date(lastFetchedAt).toLocaleTimeString()} — ${err}`,
    );
  } else {
    setStatus(`No data yet — ${err}`);
  }
}

async function refreshNow() {
  setStatus("Refreshing…");
  try {
    const snapshot = await invoke<UsageSnapshot>("refresh_usage_now");
    renderSnapshot(snapshot);
  } catch (err) {
    setStale(err);
  }
}

async function loadCached() {
  const snapshot = await invoke<UsageSnapshot | null>("get_usage_snapshot");
  if (snapshot) {
    renderSnapshot(snapshot);
  } else {
    setStatus("No data yet — waiting for first poll…");
  }
}

function deviceOutput(text: string) {
  const out = document.querySelector<HTMLElement>("#device-output");
  if (out) out.textContent = text;
}

async function listDevices() {
  deviceOutput("Listing…");
  try {
    const devices = await invoke<DiscoveredDevice[]>("list_devices");
    deviceOutput(
      devices.length === 0
        ? "No supported devices found."
        : devices.map((d) => `${d.name} (${d.vid.toString(16)}:${d.pid.toString(16)})`).join("\n"),
    );
  } catch (err) {
    deviceOutput(`Error: ${err}`);
  }
}

async function pushTestImage() {
  deviceOutput("Pushing test image…");
  try {
    const result = await invoke<string>("push_test_pattern");
    deviceOutput(result);
  } catch (err) {
    deviceOutput(`Error: ${err}`);
  }
}

async function readButtonEvents() {
  deviceOutput("Waiting up to 5s for a button press…");
  try {
    const events = await invoke<ButtonEvent[]>("read_button_events", { timeoutSecs: 5 });
    deviceOutput(events.length === 0 ? "No button events received." : JSON.stringify(events));
  } catch (err) {
    deviceOutput(`Error: ${err}`);
  }
}

function iconLabel(path: string | null): string {
  if (!path) return "none";
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

async function loadSettings() {
  const cfg = await invoke<AppConfig>("get_config");

  const intervalInput = document.querySelector<HTMLInputElement>("#refresh-interval-input");
  if (intervalInput) intervalInput.value = String(cfg.refresh_interval_secs);

  const budgetEnabled = document.querySelector<HTMLInputElement>("#budget-enabled-input");
  const budgetCap = document.querySelector<HTMLInputElement>("#budget-cap-input");
  if (budgetEnabled) budgetEnabled.checked = cfg.budget.enabled;
  if (budgetCap) budgetCap.value = String(cfg.budget.daily_cap_percent);

  renderButtonsTable(cfg.buttons);
}

function flashSaved(el: HTMLElement) {
  el.textContent = "Saved ✓";
  el.classList.add("saved-flash");
  window.setTimeout(() => {
    el.textContent = "";
    el.classList.remove("saved-flash");
  }, 1500);
}

function renderButtonsTable(buttons: ButtonAssignment[]) {
  const tbody = document.querySelector<HTMLElement>("#buttons-tbody");
  if (!tbody) return;

  tbody.innerHTML = "";

  for (let i = 0; i < SCREEN_KEY_COUNT; i++) {
    const assignment: ButtonAssignment = buttons[i] ?? { metric: "none", icon_path: null };

    const row = document.createElement("tr");

    const indexCell = document.createElement("td");
    indexCell.textContent = String(i);
    row.appendChild(indexCell);

    const statusSpan = document.createElement("span");
    statusSpan.className = "status-flash";

    const metricCell = document.createElement("td");
    const select = document.createElement("select");
    for (const option of ["session", "weekly", "budget", "none"] as Metric[]) {
      const opt = document.createElement("option");
      opt.value = option;
      opt.textContent = option;
      opt.selected = assignment.metric === option;
      select.appendChild(opt);
    }
    select.addEventListener("change", async () => {
      await invoke("set_button_metric", {
        buttonIndex: i,
        metric: select.value as Metric,
      });
      flashSaved(statusSpan);
    });
    metricCell.appendChild(select);
    row.appendChild(metricCell);

    const iconCell = document.createElement("td");
    const iconLabelSpan = document.createElement("span");
    iconLabelSpan.textContent = iconLabel(assignment.icon_path);
    iconLabelSpan.className = "icon-label";
    const iconBtn = document.createElement("button");
    iconBtn.textContent = "Choose icon…";
    iconBtn.addEventListener("click", async () => {
      const path = await invoke<string | null>("pick_icon_for_button", { buttonIndex: i });
      if (path) {
        iconLabelSpan.textContent = iconLabel(path);
        flashSaved(statusSpan);
      }
    });
    iconCell.appendChild(iconBtn);
    iconCell.appendChild(iconLabelSpan);
    row.appendChild(iconCell);

    const statusCell = document.createElement("td");
    statusCell.appendChild(statusSpan);
    row.appendChild(statusCell);

    tbody.appendChild(row);
  }
}

async function saveRefreshInterval() {
  const input = document.querySelector<HTMLInputElement>("#refresh-interval-input");
  if (!input) return;
  await invoke("set_refresh_interval", { seconds: Number(input.value) });
}

async function saveBudget() {
  const enabled = document.querySelector<HTMLInputElement>("#budget-enabled-input")?.checked ?? false;
  const cap = Number(document.querySelector<HTMLInputElement>("#budget-cap-input")?.value ?? "20");
  await invoke("set_budget_config", { enabled, dailyCapPercent: cap });
}

async function resetButtons() {
  const confirmed = confirm(
    "Reset all buttons to session (0) + weekly (1) only? This won't restore " +
      "whatever another app had on the other buttons - it just stops Claude " +
      "Deck from repainting them going forward.",
  );
  if (!confirmed) return;
  await invoke("reset_button_assignments");
  await loadSettings();
}

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#refresh-btn")?.addEventListener("click", refreshNow);
  document.querySelector("#list-devices-btn")?.addEventListener("click", listDevices);
  document.querySelector("#push-test-btn")?.addEventListener("click", pushTestImage);
  document.querySelector("#read-events-btn")?.addEventListener("click", readButtonEvents);
  document
    .querySelector("#save-refresh-interval-btn")
    ?.addEventListener("click", saveRefreshInterval);
  document.querySelector("#save-budget-btn")?.addEventListener("click", saveBudget);
  document.querySelector("#reset-buttons-btn")?.addEventListener("click", resetButtons);

  loadCached();
  loadSettings();

  listen<UsageSnapshot>("usage://updated", (event) => renderSnapshot(event.payload));
  listen<string>("usage://error", (event) => setStale(event.payload));
});
