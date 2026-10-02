import { start, destroy } from "./channel-preview.js";

const attached = new WeakSet();
const sessions = new Map();
const terminal = state => ["completed", "failed", "cancelled"].includes(state);

function message(form, state) {
  const root = form.querySelector("[data-channel-preview]");
  return root.dataset[`message${state[0].toUpperCase()}${state.slice(1)}`];
}
function status(form, state) {
  const root = form.querySelector("[data-channel-preview]");
  root.dataset.previewState = state;
  root.querySelector("[data-preview-status]").textContent = message(form, state);
}
function failure(form, code) {
  status(form, ["busy", "cleanup", "configuration", "capacity", "authorization"].includes(code)
    ? code : "startup");
  form.querySelector("textarea").value =
    form.querySelector("[data-preview-status]").textContent;
}
function endpoint(id) {
  return `/api/troubleshoot/playback/previews/${encodeURIComponent(id)}`;
}
function retire(form, session) {
  session.controller.abort();
  destroy(form.querySelector("video"));
  if (session.id && !session.done) {
    // Navigation cancels only this owned diagnostic job; the server still reaps it.
    void fetch(`${endpoint(session.id)}/stop`, { method: "POST", keepalive: true }).catch(() => {});
  }
  sessions.delete(form);
}
async function render(form, session, job) {
  session.id = job.id;
  // A short process can finish just before its readiness result is published.
  // Keep observing until output readiness or a definitive failure arrives.
  session.done = terminal(job.state) && !(job.state === "completed" && !job.ready);
  form.dataset.previewId = job.id;
  const root = form.querySelector("[data-channel-preview]");
  root.dataset.operationState = job.state;
  root.querySelector("[data-preview-operation-status]").textContent = message(form, job.state);
  const play = form.querySelector("[data-preview-play]");
  const stop = form.querySelector("[data-preview-stop]");
  play.disabled = !session.done || !job.cleanup_confirmed;
  stop.disabled = session.done || job.state === "cancelling";
  stop.formAction = `/admin-api/troubleshoot/playback/${job.id}/stop`;
  const refresh = root.querySelector("[data-preview-refresh]");
  refresh.href = job.return_url;
  refresh.hidden = false;
  const timestamp = root.querySelector("[data-preview-timestamp]");
  timestamp.hidden = false;
  timestamp.querySelector("time").textContent = job.selected_at ?? job.created_at;
  timestamp.querySelector("time").dateTime = job.selected_at ?? job.created_at;
  if (job.log) {
    form.querySelector("textarea").value = job.log.text;
    const speed = form.querySelector("[data-preview-speed]");
    speed.hidden = !Number.isFinite(job.log.speed);
    speed.querySelector("output").textContent = speed.hidden ? "" : job.log.speed.toFixed(2);
    speed.dataset.speedState = job.log.speed < 0.9 ? "slow" : job.log.speed > 1.1 ? "fast" : "normal";
  }
  if (job.state === "failed") {
    destroy(form.querySelector("video"));
    status(form, ["cleanup", "configuration", "capacity"].includes(job.error_code)
      ? job.error_code : "startup");
    if (!job.log) failure(form, job.error_code);
  } else if (job.state === "cancelled" || job.state === "cancelling") {
    destroy(form.querySelector("video"));
    status(form, "stopped");
  } else if (job.ready && !session.playing) {
    session.playing = true;
    await start(form.querySelector("video"), job.playlist_url);
  }
}
async function observe(form, session) {
  const signal = session.controller.signal;
  try {
    while (!signal.aborted && form.isConnected) {
      const response = await fetch(endpoint(session.id), {
        signal, cache: "no-store",
      });
      if (!response.ok) throw new Error("Preview status unavailable");
      const job = await response.json();
      if (signal.aborted || !form.isConnected) return;
      await render(form, session, job);
      if (session.done) return;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
  } catch {
    if (!signal.aborted && form.isConnected) {
      status(form, "network");
      // Keep ownership and Stop available when a status request fails. Refresh
      // renders the authoritative state; a network error cannot prove cleanup.
    }
  }
}
function sessionFor(form, id = "") {
  const session = { id, controller: new AbortController(), done: false, playing: false };
  sessions.set(form, session);
  return session;
}
function attach() {
  for (const form of document.querySelectorAll("form[data-playback-troubleshooting]")) {
    if (attached.has(form)) continue;
    attached.add(form);
    if (form.dataset.previewId) void observe(form, sessionFor(form, form.dataset.previewId));
  }
  for (const [form, session] of sessions) {
    if (!form.isConnected) retire(form, session);
  }
}

// Delegation and observation cover SSR and client-side navigation alike.
document.addEventListener("submit", async event => {
  const form = event.target;
  if (!form.matches("form[data-playback-troubleshooting]")) return;
  event.preventDefault();
  const current = sessions.get(form);
  if (event.submitter?.matches("[data-preview-stop]")) {
    if (!current?.id || current.done) return;
    form.querySelector("[data-preview-stop]").disabled = true;
    try {
      const response = await fetch(`${endpoint(current.id)}/stop`, { method: "POST" });
      if (!response.ok) throw new Error("Preview cancellation unavailable");
      if (!form.isConnected || sessions.get(form) !== current) return;
      await render(form, current, await response.json());
      // Restart observation if an earlier transient network error ended polling.
      current.controller.abort();
      current.controller = new AbortController();
      void observe(form, current);
    } catch {
      if (form.isConnected) {
        status(form, "network");
        form.querySelector("[data-preview-stop]").disabled = false;
      }
    }
    return;
  }
  if (current && !current.done) return;
  if (current) retire(form, current);
  const session = sessionFor(form);
  const url = new URL("/api/troubleshoot/playback/previews", location.origin);
  url.search = new URLSearchParams(new FormData(form)).toString();
  form.querySelector("[data-preview-play]").disabled = true;
  form.querySelector("textarea").value = "";
  form.querySelector("[data-preview-speed]").hidden = true;
  destroy(form.querySelector("video"));
  status(form, "connecting");
  try {
    // Admission returns an identity immediately; playlist reloads only read
    // that job's output and cannot launch or attach to a replacement process.
    const response = await fetch(url, { method: "POST", signal: AbortSignal.timeout(10000) });
    if (!response.ok) {
      const error = await response.json().catch(() => ({}));
      session.done = true;
      if (form.isConnected) {
        failure(form, response.status === 401 || response.status === 403 ? "authorization" : error.code);
        form.querySelector("[data-preview-play]").disabled = false;
      }
      return;
    }
    const job = await response.json();
    session.id = job.id;
    if (!form.isConnected) { retire(form, session); return; }
    await render(form, session, job);
    void observe(form, session);
  } catch {
    // Admission may have reached the server. Preserve its existing deadline;
    // never invent an ID or cancel another preview after a lost response.
    if (form.isConnected) {
      status(form, "network");
      if (!session.id) {
        session.done = true;
        form.querySelector("[data-preview-play]").disabled = false;
      }
    }
  }
});
new MutationObserver(attach).observe(document.body, { childList: true, subtree: true });
attach();
window.addEventListener("pagehide", () => {
  for (const [form, session] of sessions) retire(form, session);
});
