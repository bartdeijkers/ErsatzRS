import { expect, type Page } from '@playwright/test';
import { writeFileSync } from 'fs';

const startupLimit = 20_000;

type BrowserHlsProof = {
  reason: string; startupMilliseconds: number | null; firstMediaTime: number | null;
  lastMediaTime: number | null; playlistOrigin: number | null; initialSequence: number | null;
  requestWallMilliseconds: number | null; seedToRequestSeconds: number | null;
  cutEarliest: number | null; cutLatest: number | null; coverageProven: boolean;
  maximumFrameGapMilliseconds: number; frames: number;
  events: { kind: string; wallMilliseconds: number; mediaTime: number; afterStart: boolean }[];
  samples: { wallMilliseconds: number; mediaTime: number; bufferedAhead: number; readyState: number }[];
};

export async function captureHls(page: Page, base: string, number: string, output: string,
  cutOffsetSeconds: number, scheduledStartMilliseconds: number) {
  const chunks = new Map<number, Buffer>();
  const pending = new Set<Promise<void>>();
  const arrivals: { sequence: number; bytes: number }[] = [];
  let bytes = 0;
  let captureFailure: string | undefined;
  let abortedResponses = 0;
  let player: BrowserHlsProof | undefined;
  const stop = async () => {
    await page.evaluate(() => {
      (window as Window & { stopSponsorBlockProof?: () => void }).stopSponsorBlockProof?.();
    }).catch(() => {});
  };
  const onResponse = (response: import('@playwright/test').Response) => {
    const path = new URL(response.url()).pathname;
    if (!path.startsWith(`/iptv/session/${number}/`) || !path.endsWith('.ts')) return;
    const sequence = Number(path.match(/(\d+)\.ts$/)?.[1]);
    if (!Number.isSafeInteger(sequence)) { captureFailure = 'Invalid segment sequence'; void stop(); return; }
    if (!response.ok()) { captureFailure = 'Production segment request failed'; void stop(); return; }
    if (Number(response.headers()['content-length'] ?? 0) > 64 * 1024 * 1024) {
      captureFailure = 'Segment exceeds capture budget'; void stop(); return;
    }
    const work = (async () => {
      const body = await response.body();
      if (chunks.has(sequence)) return;
      if (!body.length || body.length % 188 !== 0 || body[0] !== 0x47) {
        captureFailure = 'Invalid MPEG-TS segment'; await stop(); return;
      }
      if (bytes + body.length > 64 * 1024 * 1024) {
        captureFailure = 'HLS capture exceeded 64 MiB'; await stop(); return;
      }
      bytes += body.length;
      chunks.set(sequence, body);
      arrivals.push({ sequence, bytes: body.length });
    })().catch(() => { abortedResponses++; });
    pending.add(work);
    void work.finally(() => pending.delete(work));
  };
  page.on('response', onResponse);
  try {
    // Same-origin test video, not the admin preview endpoint or a second worker.
    await page.goto(new URL('/healthz', base).toString());
    player = await page.evaluate(async ({ master, cutOffsetSeconds, scheduledStartMilliseconds, startupLimit }) => {
      const modulePath = '/public/vendor/hls/hls.mjs';
      let importTimer: ReturnType<typeof setTimeout> | undefined;
      const { default: Hls } = await Promise.race([
        import(modulePath),
        new Promise<never>((_resolve, reject) => {
          importTimer = setTimeout(() => reject(new Error('Vendored player load exceeded 5 seconds')), 5_000);
        }),
      ]).finally(() => clearTimeout(importTimer));
      if (!Hls.isSupported()) throw new Error('Vendored HLS player unsupported');
      const video = document.createElement('video');
      video.muted = true;
      video.defaultMuted = true;
      video.playsInline = true;
      document.body.replaceChildren(video);
      if (!video.requestVideoFrameCallback) throw new Error('Presented-frame observation unavailable');
      performance.clearResourceTimings();
      performance.setResourceTimingBufferSize(512);
      const started = performance.now();
      // Same defaults as channel-preview.js; do not force a VOD start or seek.
      const hls = new Hls({ enableWorker: true, lowLatencyMode: true, backBufferLength: 30,
        fragLoadingTimeOut: 30000, manifestLoadingTimeOut: 30000 });
      const proof: BrowserHlsProof = { reason: '', startupMilliseconds: null,
        firstMediaTime: null, lastMediaTime: null, playlistOrigin: null, initialSequence: null,
        requestWallMilliseconds: null, seedToRequestSeconds: null,
        cutEarliest: null, cutLatest: null, coverageProven: false,
        maximumFrameGapMilliseconds: 0, frames: 0, events: [], samples: [] };
      return await new Promise<BrowserHlsProof>(resolveProof => {
        let stopped = false;
        let lastFrameWall: number | null = null;
        let callback = 0;
        let startupTimer: ReturnType<typeof setTimeout> | undefined;
        const elapsed = () => performance.now() - started;
        const finish = (reason: string) => {
          if (stopped) return;
          stopped = true;
          proof.reason = reason;
          proof.coverageProven = proof.firstMediaTime !== null && proof.lastMediaTime !== null &&
            proof.initialSequence === 0 && proof.cutEarliest !== null && proof.cutLatest !== null &&
            proof.firstMediaTime < proof.cutEarliest && proof.lastMediaTime >= proof.cutLatest + 6;
          clearInterval(sampling);
          clearTimeout(cutoff);
          clearTimeout(startupTimer);
          video.cancelVideoFrameCallback(callback);
          hls.stopLoad();
          hls.destroy();
          video.pause();
          video.removeAttribute('src');
          video.load();
          resolveProof(proof);
        };
        (window as Window & { stopSponsorBlockProof?: () => void }).stopSponsorBlockProof =
          () => finish('Capture stopped');
        const cutoff = setTimeout(() => finish('60-second playback cutoff'), 60_000);
        const sampling = setInterval(() => {
          if (stopped || proof.firstMediaTime === null) return;
          const now = performance.now();
          const gap = now - (lastFrameWall ?? now);
          proof.maximumFrameGapMilliseconds = Math.max(proof.maximumFrameGapMilliseconds, gap);
          let bufferedAhead = 0;
          for (let index = 0; index < video.buffered.length; index++) {
            if (video.buffered.start(index) <= video.currentTime && video.currentTime <= video.buffered.end(index)) {
              bufferedAhead = video.buffered.end(index) - video.currentTime;
              break;
            }
          }
          proof.samples.push({ wallMilliseconds: elapsed(), mediaTime: video.currentTime,
            bufferedAhead, readyState: video.readyState });
          if (gap > 250) finish('Presented video froze for more than 250 ms');
        }, 100);
        let startupSeekInFlight = false;
        for (const kind of ['playing', 'waiting', 'stalled', 'seeking', 'seeked', 'error', 'ended']) {
          video.addEventListener(kind, () => {
            if (stopped) return;
            const afterStart = proof.firstMediaTime !== null;
            proof.events.push({ kind, wallMilliseconds: elapsed(), mediaTime: video.currentTime, afterStart });
            // A startup seek can present its first frame before dispatching
            // seeked. Consume only that pending completion; a new post-start
            // seeking event clears the allowance and still fails immediately.
            const completesStartupSeek = kind === 'seeked' && startupSeekInFlight;
            if (kind === 'seeking') startupSeekInFlight = !afterStart;
            if (kind === 'seeked') startupSeekInFlight = false;
            if (kind === 'error' || (afterStart &&
                (['waiting', 'stalled', 'seeking', 'ended'].includes(kind) ||
                  (kind === 'seeked' && !completesStartupSeek)))) {
              finish(`Player event: ${kind}`);
            }
          });
        }
        const frame = (now: number, metadata: VideoFrameCallbackMetadata) => {
          if (stopped) return;
          const timing = performance.getEntriesByName(master, 'resource')[0];
          if (proof.firstMediaTime === null) {
            proof.firstMediaTime = metadata.mediaTime;
            if (timing) {
              proof.startupMilliseconds = now - timing.startTime;
              proof.requestWallMilliseconds = performance.timeOrigin + timing.startTime;
              const seedToRequest = (proof.requestWallMilliseconds - scheduledStartMilliseconds) / 1000;
              proof.seedToRequestSeconds = seedToRequest;
              // Source selection happens after the request and before its startup deadline.
              // Bracket that uncertainty rather than assuming playback began at the seeded join.
              if (proof.playlistOrigin !== null && seedToRequest >= 0) {
                proof.cutLatest = proof.playlistOrigin + cutOffsetSeconds - seedToRequest;
                proof.cutEarliest = proof.cutLatest - startupLimit / 1000;
              }
            }
            clearTimeout(startupTimer);
            if (proof.startupMilliseconds === null || proof.startupMilliseconds > startupLimit) {
              finish('First presented frame exceeded or lacked the startup measurement'); return;
            }
          } else {
            const wallGap = now - (lastFrameWall ?? now);
            proof.maximumFrameGapMilliseconds = Math.max(proof.maximumFrameGapMilliseconds, wallGap);
            const mediaGap = metadata.mediaTime - (proof.lastMediaTime ?? metadata.mediaTime);
            if (wallGap > 250 || mediaGap < 0 || mediaGap > wallGap / 1000 + 0.08) {
              finish('Presented-frame discontinuity'); return;
            }
          }
          proof.frames++;
          proof.lastMediaTime = metadata.mediaTime;
          lastFrameWall = now;
          if (proof.cutLatest !== null && metadata.mediaTime >= proof.cutLatest + 6) {
            finish('Observed through cut window'); return;
          }
          callback = video.requestVideoFrameCallback(frame);
        };
        callback = video.requestVideoFrameCallback(frame);
        hls.on(Hls.Events.MANIFEST_LOADING, () => {
          startupTimer ??= setTimeout(() => finish('No presented frame within startup deadline'), startupLimit);
        });
        hls.on(Hls.Events.LEVEL_LOADED, (_event: string, data: { details: { fragments: { start: number; sn: number }[] } }) => {
          const first = data.details.fragments[0];
          if (first && proof.playlistOrigin === null) {
            proof.playlistOrigin = first.start;
            proof.initialSequence = first.sn;
          }
        });
        hls.on(Hls.Events.ERROR, (_event: string, data: { fatal: boolean; type: string; details: string }) => {
          // Record only Hls.js error enums, never its raw network diagnostics.
          if (data.fatal || proof.firstMediaTime !== null) finish(`HLS playback error: ${data.type}/${data.details}`);
        });
        hls.on(Hls.Events.MEDIA_ATTACHED, () => hls.loadSource(master));
        hls.on(Hls.Events.MANIFEST_PARSED, () => {
          video.play().catch(() => finish('Browser rejected playback'));
        });
        hls.attachMedia(video);
      });
    }, { master: new URL(`/iptv/channel/${number}.m3u8?mode=segmenter`, base).toString(),
      cutOffsetSeconds, scheduledStartMilliseconds, startupLimit });
  } finally {
    await stop();
    page.off('response', onResponse);
    await Promise.all([...pending]);
    const sequences = [...chunks.keys()].sort((a, b) => a - b);
    if (sequences.some((sequence, index) => index > 0 && sequence !== sequences[index - 1] + 1)) {
      captureFailure = 'Captured browser segment sequence has gaps';
    }
    writeFileSync(output, Buffer.concat(sequences.map(sequence => chunks.get(sequence)!)));
    writeFileSync(`${output}.capture.json`, JSON.stringify({ player, bytes, arrivals, sequences,
      abortedResponses, captureFailure, contentCutCorrectness: 'Requires decoded source-reference comparison',
      audibleContinuity: 'Not established by muted browser observation',
      frameCadenceThresholdMilliseconds: 250,
      visualContentFreezes: 'Repeated decoded image content requires separate source-reference validation' }, null, 2));
  }
  expect(captureFailure, 'Production segment capture must be complete and bounded').toBeUndefined();
  expect(player?.reason, 'Browser stall/startup failure; inspect saved player measurements').toBe('Observed through cut window');
  expect(player?.coverageProven, 'Cut coverage not proven: default live-edge start or a sliding initial playlist may skip the conservative cut window').toBe(true);
  expect(chunks.size).toBeGreaterThan(0);
  return { startupMilliseconds: player!.startupMilliseconds!, bytes, arrivals, player };
}
