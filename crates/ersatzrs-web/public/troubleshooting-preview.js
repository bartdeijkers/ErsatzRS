import { start, destroy } from "./channel-preview.js";

const pending = new WeakSet();
const originalDisabled = new WeakMap();
let releasePreview = () => {};

function updateEngine(form) {
  const next = form.elements.streamingEngine.value === "Next";
  for (const field of form.querySelectorAll("[data-legacy-preview-field]")) {
    field.hidden = next;
    for (const control of field.querySelectorAll("select, input")) {
      if (!originalDisabled.has(control)) originalDisabled.set(control, control.disabled);
      control.disabled = next || originalDisabled.get(control);
    }
  }
  const watermark = form.elements.watermark;
  if (watermark) {
    // Preserve the first selected real watermark when switching from multiple
    // selections. An empty option keeps single selection clearable.
    const selected = Array.from(watermark.selectedOptions).find(option => option.value);
    watermark.multiple = !next;
    if (next) watermark.value = selected?.value ?? "";
    const label = form.querySelector("[data-watermark-label]");
    label.textContent = next ? label.dataset.single : label.dataset.multiple;
  }
}

function status(form, state) {
  const root = form.querySelector("[data-channel-preview]");
  root.dataset.previewState = state;
  root.querySelector("[data-preview-status]").textContent =
    root.dataset[`message${state[0].toUpperCase()}${state.slice(1)}`];
}

async function loadLogs(form, signal) {
  const output = form.querySelector("textarea");
  try {
    while (!signal.aborted && form.isConnected) {
      const response = await fetch("/api/troubleshoot/playback/log", { signal, cache: "no-store" });
      if (!response.ok) throw new Error("Preview log unavailable");
      const log = await response.json();
      if (signal.aborted || !form.isConnected) return;
      if (log !== null) {
        output.value = log;
        return;
      }
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  } catch {
    if (!signal.aborted && form.isConnected) {
      output.value = form.querySelector("[data-channel-preview]").dataset.messageUnavailable;
    }
  }
}

// Delegation also covers SSR pages reached through client-side navigation.
document.addEventListener("change", event => {
  const form = event.target.closest("form[data-playback-troubleshooting]");
  if (form && event.target.name === "streamingEngine") updateEngine(form);
});

document.addEventListener("submit", async event => {
  const form = event.target;
  if (!form.matches("form[data-playback-troubleshooting]")) return;
  event.preventDefault();
  if (pending.has(form)) return;
  pending.add(form);
  updateEngine(form);
  const video = form.querySelector("video");
  const button = form.querySelector('button[type="submit"]');
  const url = new URL(form.action);
  url.search = new URLSearchParams(new FormData(form)).toString();
  releasePreview();
  const controller = new AbortController();
  const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(120000)]);
  const removed = new MutationObserver(() => {
    if (!form.isConnected) releasePreview();
  });
  removed.observe(document.body, { childList: true, subtree: true });
  releasePreview = () => {
    removed.disconnect();
    controller.abort();
    destroy(video);
    releasePreview = () => {};
  };
  button.disabled = true;
  form.querySelector("textarea").value = "";
  destroy(video);
  status(form, "connecting");
  try {
    // Start the transcode once. Only the final static playlist URL is handed
    // to Hls.js, so its reloads cannot launch another troubleshooting process.
    const response = await fetch(url, { signal });
    if (!form.isConnected) return;
    if (!response.ok) {
      status(form, response.status === 401 || response.status === 403
        ? "authorization" : "unavailable");
      return;
    }
    await response.body?.cancel();
    await start(video, response.url);
    await loadLogs(form, signal);
  } catch {
    if (form.isConnected) status(form, "network");
  } finally {
    button.disabled = false;
    pending.delete(form);
  }
});

window.addEventListener("pagehide", () => releasePreview());
window.addEventListener("pageshow", () => {
  document.querySelectorAll("form[data-playback-troubleshooting]").forEach(updateEngine);
});
