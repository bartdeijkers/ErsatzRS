const players = new WeakMap();

function rootFor(video) {
  return video.closest("[data-channel-preview]");
}

function messageFor(root, state, httpStatus) {
  if (!root) return "";
  if (httpStatus === 401) return root.dataset.messageAuthorization;
  if (httpStatus === 404) return root.dataset.messageUnavailable;
  if (httpStatus === 503) return root.dataset.messageCapacity;
  const messages = {
    idle: root.dataset.messageIdle,
    connecting: root.dataset.messageConnecting,
    live: root.dataset.messageLive,
    paused: root.dataset.messagePaused,
    recovering: root.dataset.messageRecovering,
    stopped: root.dataset.messageStopped,
    unsupported: root.dataset.messageUnsupported,
    network: root.dataset.messageNetwork,
    media: root.dataset.messageMedia,
    error: root.dataset.messageUnavailable,
  };
  return messages[state] || "";
}

function setState(video, state, httpStatus, details) {
  const root = rootFor(video);
  if (!root) return;
  root.dataset.previewState = state;
  const status = root.querySelector("[data-preview-status]");
  if (status) status.textContent = messageFor(root, state, httpStatus);
  root.dispatchEvent(new CustomEvent("ersatzrs-preview-state", {
    detail: { state, httpStatus, details },
  }));
}

function removeNativeListeners(video, entry) {
  if (!entry?.nativeListeners) return;
  for (const [event, listener] of entry.nativeListeners) {
    video.removeEventListener(event, listener);
  }
}

function attachCaptions(video, entry) {
  const button = rootFor(video)?.querySelector("[data-preview-captions]");
  if (!button) return;
  const tracks = () => Array.from(video.textTracks)
    .filter((track) => track.kind === "subtitles" || track.kind === "captions");
  const sync = () => {
    const hls = entry.hls;
    const available = hls ? hls.subtitleTracks.length > 0 : tracks().length > 0;
    const showing = hls
      ? hls.subtitleDisplay && hls.subtitleTrack >= 0
      : tracks().some((track) => track.mode === "showing");
    button.disabled = !available;
    button.setAttribute("aria-pressed", String(available && showing));
  };
  const toggle = () => {
    const show = button.getAttribute("aria-pressed") !== "true";
    if (entry.hls) {
      if (show && entry.hls.subtitleTrack < 0) entry.hls.subtitleTrack = 0;
      entry.hls.subtitleDisplay = show;
    } else {
      const available = tracks();
      const selected = available.find((track) => track.mode === "showing") || available[0];
      for (const track of available) {
        track.mode = show && track === selected ? "showing" : "disabled";
      }
    }
    sync();
  };
  button.addEventListener("click", toggle);
  for (const event of ["addtrack", "removetrack", "change"]) {
    video.textTracks.addEventListener(event, sync);
  }
  entry.syncCaptions = sync;
  entry.removeCaptions = () => {
    button.removeEventListener("click", toggle);
    for (const event of ["addtrack", "removetrack", "change"]) {
      video.textTracks.removeEventListener(event, sync);
    }
    button.disabled = true;
    button.setAttribute("aria-pressed", "false");
  };
  sync();
}

function destroy(video) {
  const entry = players.get(video);
  entry?.removeCaptions?.();
  if (entry?.hls) entry.hls.destroy();
  removeNativeListeners(video, entry);
  players.delete(video);
  video.pause();
  video.removeAttribute("src");
  video.load();
  const root = rootFor(video);
  if (root) delete root.dataset.previewEngine;
  setState(video, "stopped");
}

function attachVideoState(video, entry) {
  const listeners = [
    ["playing", () => setState(video, "live")],
    ["pause", () => {
      if (!entry.failureState && !video.ended && video.currentTime > 0) {
        setState(video, "paused");
      }
    }],
    ["error", () => {
      if (!entry.failureState) setState(video, "media");
    }],
  ];
  for (const [event, listener] of listeners) {
    video.addEventListener(event, listener);
  }
  entry.nativeListeners = listeners;
}

async function start(video, url) {
  destroy(video);
  // Preview authorization and the Hls.js import are asynchronous. Keep both
  // the default and current mute state set so autoplay remains browser-safe;
  // native controls let the administrator unmute after playback begins.
  video.defaultMuted = true;
  video.muted = true;
  setState(video, "connecting");
  const entry = {
    hls: null,
    nativeListeners: [],
    mediaRecoveryAttempted: false,
    failureState: false,
  };
  players.set(video, entry);
  attachVideoState(video, entry);
  attachCaptions(video, entry);

  try {
    const module = await import("/public/vendor/hls/hls.mjs");
    if (players.get(video) !== entry) return;
    const Hls = module.default;
    if (Hls?.isSupported()) {
      const root = rootFor(video);
      if (root) root.dataset.previewEngine = "hlsjs";
      const hls = new Hls({
        enableWorker: true,
        lowLatencyMode: true,
        backBufferLength: 30,
        fragLoadingTimeOut: 30000,
        manifestLoadingTimeOut: 30000,
      });
      entry.hls = hls;
      hls.on(Hls.Events.SUBTITLE_TRACKS_UPDATED, entry.syncCaptions);
      hls.on(Hls.Events.SUBTITLE_TRACK_SWITCH, entry.syncCaptions);
      entry.syncCaptions?.();
      hls.attachMedia(video);
      hls.on(Hls.Events.MEDIA_ATTACHED, () => hls.loadSource(url));
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        video.play().catch(() => setState(video, "paused"));
      });
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (!data.fatal) return;
        const httpStatus = data.response?.code;
        if (data.type === Hls.ErrorTypes.MEDIA_ERROR && !entry.mediaRecoveryAttempted) {
          entry.mediaRecoveryAttempted = true;
          setState(video, "recovering", httpStatus, data.details);
          hls.recoverMediaError();
          return;
        }
        if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
          entry.failureState = true;
          setState(video, "network", httpStatus, data.details);
        } else {
          entry.failureState = true;
          setState(video, "media", httpStatus, data.details);
        }
        entry.removeCaptions?.();
        hls.destroy();
        entry.hls = null;
      });
      return;
    }
  } catch {
    // Fall through to native HLS only when Hls.js is unavailable.
  }

  if (players.get(video) !== entry) return;
  if (video.canPlayType("application/vnd.apple.mpegurl")) {
    const root = rootFor(video);
    if (root) root.dataset.previewEngine = "native";
    video.src = url;
    try {
      await video.play();
    } catch (error) {
      if (error?.name === "NotAllowedError") {
        setState(video, "paused");
      } else {
        entry.failureState = true;
        setState(video, "network", undefined, error?.name);
      }
    }
    return;
  }

  const root = rootFor(video);
  if (root) root.dataset.previewEngine = "unsupported";
  setState(video, "unsupported");
}

async function reload(video, url) {
  await start(video, url);
}

window.ErsatzRSChannelPreview = { start, reload, destroy };

window.addEventListener("pagehide", () => {
  document.querySelectorAll("[data-channel-preview] video").forEach(destroy);
});

export { start, reload, destroy };
