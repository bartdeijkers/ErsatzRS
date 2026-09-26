import { start, destroy } from "./channel-preview.js";

const pending = new WeakSet();
let releasePreview = () => {};

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
        output.value = log.text;
        const speed = form.querySelector("[data-preview-speed]");
        speed.hidden = !Number.isFinite(log.speed);
        speed.querySelector("output").textContent = speed.hidden ? "" : log.speed.toFixed(2);
        speed.dataset.speedState = log.speed < 0.9 ? "slow" : log.speed > 1.1 ? "fast" : "normal";
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
document.addEventListener("submit", async event => {
  const form = event.target;
  if (!form.matches("form[data-playback-troubleshooting]")) return;
  event.preventDefault();
  if (pending.has(form)) return;
  pending.add(form);
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
  form.querySelector("[data-preview-speed]").hidden = true;
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
