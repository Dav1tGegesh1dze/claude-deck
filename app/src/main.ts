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

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#refresh-btn")?.addEventListener("click", refreshNow);
  document.querySelector("#list-devices-btn")?.addEventListener("click", listDevices);
  document.querySelector("#push-test-btn")?.addEventListener("click", pushTestImage);
  document.querySelector("#read-events-btn")?.addEventListener("click", readButtonEvents);

  loadCached();

  listen<UsageSnapshot>("usage://updated", (event) => renderSnapshot(event.payload));
  listen<string>("usage://error", (event) => setStale(event.payload));
});
