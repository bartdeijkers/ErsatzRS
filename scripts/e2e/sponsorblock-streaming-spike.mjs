#!/usr/bin/env node
// Feasibility only: source-range concat -> one encoder -> HLS arrival reserve.
// BROWSER_PROOF=1 adds real Chromium video/buffer evidence, NOT audible continuity,
// decoded-content correctness, actual-host or production acceptance.
// FFMPEG_PATH=/existing/ffmpeg FFPROBE_PATH=/existing/ffprobe node scripts/e2e/sponsorblock-streaming-spike.mjs
// Optional experimental settings, NOT production defaults: OPEN_DELAY_MS=250,
// OPEN_JITTER_MS=150, BODY_BYTES_PER_SECOND=131072, STARTUP_RESERVE_SECONDS=8,
// MAX_STARTUP_SECONDS=15 (target10), INITIAL_BURST_SECONDS=8. Artifacts are retained.
// Earlier 5-second-cap measurements remain historical; no result is rewritten.
import { spawn, execFileSync } from 'node:child_process';
import { createReadStream, existsSync, mkdtempSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { setTimeout as sleep } from 'node:timers/promises';

function number(name, fallback, minimum, maximum) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isFinite(value) || value < minimum || value > maximum) throw new Error(`Invalid ${name}`);
  return value;
}
const config = {
  openDelayMs: number('OPEN_DELAY_MS', 250, 0, 5000),
  openJitterMs: number('OPEN_JITTER_MS', 150, 0, 2000),
  bodyBytesPerSecond: number('BODY_BYTES_PER_SECOND', 131072, 16384, 8388608),
  startupReserve: number('STARTUP_RESERVE_SECONDS', 8, 4, 16),
  startupLimit: number('MAX_STARTUP_SECONDS', 15, 0.1, 15),
  startupTarget: 10,
  initialBurst: number('INITIAL_BURST_SECONDS', 8, 0, 16),
};
if (process.env.BROWSER_PROOF && !['0', '1'].includes(process.env.BROWSER_PROOF)) throw new Error('BROWSER_PROOF must be 0 or 1');
const browserProof = process.env.BROWSER_PROOF === '1';
const ffmpeg = process.env.FFMPEG_PATH;
const ffprobe = process.env.FFPROBE_PATH;
for (const [tool, path] of [['ffmpeg', ffmpeg], ['ffprobe', ffprobe]]) {
  if (!path || !existsSync(path)) throw new Error(`Explicit existing ${tool.toUpperCase()}_PATH required`);
  const version = execFileSync(path, ['-version'], { encoding: 'utf8', timeout: 10000 });
  if (!new RegExp(`^${tool} version (?:n)?9\\.`).test(version)) throw new Error(`${tool} 9.x required`);
}
const root = mkdtempSync(join(tmpdir(), 'ersatzrs-streaming-spike-'));
console.log(`Retained artifacts: ${root}`);
const source = join(root, 'source.mp4');
// Offline fixture generation, completed before any streaming scenario starts.
execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-y',
  '-f', 'lavfi', '-i', 'testsrc2=size=320x180:rate=25',
  '-f', 'lavfi', '-i', 'sine=frequency=997:sample_rate=48000', '-t', '40',
  '-filter_threads', '1', '-filter_complex_threads', '1', '-c:v', 'libx264',
  '-threads:v', '1', '-preset', 'veryfast', '-g', '25', '-bf', '0',
  '-c:a', 'aac', '-threads:a', '1', '-b:a', '96k', '-movflags', '+faststart', source],
{ timeout: 45000, stdio: 'inherit' });
const sourceProbe = JSON.parse(execFileSync(ffprobe, ['-v', 'error', '-show_format', '-show_streams', '-of', 'json', source],
  { encoding: 'utf8', timeout: 10000 }));
writeFileSync(join(root, 'source-probe.json'), JSON.stringify(sourceProbe, null, 2));
const size = statSync(source).size;
if (size > 8 * 1024 * 1024) throw new Error('Fixture exceeds the fixed 8 MiB budget');
const browser = browserProof ? await (await import('playwright')).chromium.launch() : null;
const vendor = browserProof ? readFileSync(new URL('../../crates/ersatzrs-web/public/vendor/hls/hls.mjs', import.meta.url)) : null;

function playerHtml(epoch) {
  return `<!doctype html><meta charset="utf-8"><title>Isolated concat playback diagnostic</title>
<video muted playsinline width="320" height="180"></video><script type="module">
import Hls from '/vendor/hls.mjs';
const video = document.querySelector('video');
const elapsed = () => (Date.now() - ${epoch}) / 1000;
const stats = window.proof = { hlsVersion:Hls.version, firstPlaying:null, ended:false, frames:0,
  events:[], errors:[], freezes:[], samples:[], firstMediaTime:null, finalMediaTime:0,
  lastPresentedMediaTime:null, frameFreezeThresholdMs:250, maximumFrameGapMs:0, endFrameToleranceSeconds:0.041 };
let firstPlayingWall = null, lastFrame = null, lastMediaTime = null, previousTime = 0, lastProgress = null;
function frameAge(now) {
  const reference = lastFrame ?? firstPlayingWall;
  if (reference === null) return;
  const gap = Math.max(0, now-reference);
  stats.maximumFrameGapMs = Math.max(stats.maximumFrameGapMs, gap);
  if (gap > stats.frameFreezeThresholdMs) stats.freezes.push({kind:'frame-age', seconds:elapsed(), gapMs:gap});
}
for (const kind of ['playing','waiting','stalled','seeking','seeked','error','ended']) video.addEventListener(kind, () => {
  const afterStart = stats.firstPlaying !== null;
  stats.events.push({kind, seconds:elapsed(), mediaTime:video.currentTime, afterStart});
  if (kind === 'playing' && !afterStart) { stats.firstPlaying = elapsed(); firstPlayingWall = performance.now(); lastProgress = firstPlayingWall; }
  if (kind === 'error') stats.errors.push({kind, code:video.error?.code, message:video.error?.message});
  if (kind === 'ended') { frameAge(performance.now()); stats.ended = true; stats.finalMediaTime = video.currentTime; }
});
function frame(now, metadata) {
  if (stats.firstPlaying !== null && !video.ended) {
    if (stats.firstMediaTime === null) stats.firstMediaTime = metadata.mediaTime;
    frameAge(now);
    if (lastMediaTime !== null && metadata.mediaTime < lastMediaTime) stats.errors.push({kind:'nonmonotonic-frame-time'});
    if (lastFrame !== null && metadata.mediaTime-lastMediaTime > (now-lastFrame)/1000+0.08) stats.errors.push({kind:'forward-frame-time-jump'});
    lastFrame=now; lastMediaTime=metadata.mediaTime; stats.lastPresentedMediaTime=metadata.mediaTime; stats.frames++;
  }
  video.requestVideoFrameCallback(frame);
}
if (video.requestVideoFrameCallback) video.requestVideoFrameCallback(frame);
else stats.errors.push({kind:'requestVideoFrameCallback-unavailable'});
setInterval(() => {
  if (stats.firstPlaying === null || video.ended) return;
  const now=performance.now();
  frameAge(now);
  if (video.currentTime > previousTime) lastProgress=now;
  else if (lastProgress !== null && now-lastProgress > stats.frameFreezeThresholdMs) stats.freezes.push({kind:'currentTime-stall', seconds:elapsed(), gapMs:now-lastProgress});
  if (video.currentTime < previousTime) stats.errors.push({kind:'nonmonotonic-currentTime'});
  previousTime=video.currentTime;
  stats.samples.push({seconds:elapsed(), mediaTime:video.currentTime, readyState:video.readyState,
    reserveSeconds:video.buffered.length ? video.buffered.end(video.buffered.length-1)-video.currentTime : 0});
},100);
window.startPlayback = () => {
  const hls = new Hls({startPosition:0, autoStartLoad:false, enableWorker:false});
  hls.on(Hls.Events.ERROR, (_, data) => stats.errors.push({kind:'hls', seconds:elapsed(),
    details:data.details, fatal:data.fatal, afterStart:stats.firstPlaying !== null}));
  hls.on(Hls.Events.MEDIA_ATTACHED, () => hls.loadSource('/index.m3u8'));
  hls.on(Hls.Events.MANIFEST_PARSED, () => { hls.startLoad(0); video.play().catch(error => stats.errors.push({kind:'play', message:error.message})); });
  hls.attachMedia(video);
};
</script>`;
}

// Replay the nominal, never-paused consumer clock against measured publication events.
// A deficit is reported, not hidden by simulated rebuffering. EOS is not an underrun.
function consumption(arrivals, expectedDuration) {
  const initial = arrivals.find(event => event.availableSeconds >= config.startupReserve);
  if (!initial) return { started: false, startupWithinLimit: false, underruns: [], minimumReserveSeconds: null };
  let available = initial.availableSeconds;
  let previousTime = initial.seconds;
  let minimum = available;
  const underruns = [];
  for (const event of arrivals.filter(event => event.seconds > initial.seconds)) {
    const before = available - (event.seconds - initial.seconds);
    minimum = Math.min(minimum, before);
    if (before < 0 && available < expectedDuration) {
      const from = Math.max(previousTime, initial.seconds + available);
      if (event.seconds > from) underruns.push({ fromSeconds: from, toSeconds: event.seconds, seconds: event.seconds - from });
    }
    available = event.availableSeconds;
    previousTime = event.seconds;
  }
  return { started: true, startupSeconds: initial.seconds,
    startupWithinLimit: initial.seconds <= config.startupLimit,
    minimumReserveSeconds: minimum, underruns,
    missingOutputSeconds: Math.max(0, expectedDuration - available) };
}

async function scenario(name, ranges) {
  const directory = join(root, name);
  mkdirSync(directory);
  const events = [];
  const arrivals = [];
  let origin = performance.now();
  let encoderEpoch = Date.now();
  const seconds = () => (performance.now() - origin) / 1000;
  const server = createServer(async (request, response) => {
    if (browserProof && request.method === 'GET') {
      const path = request.url;
      if (path === '/player') { response.writeHead(200, {'Content-Type':'text/html','Cache-Control':'no-store'}).end(playerHtml(encoderEpoch)); return; }
      if (path === '/vendor/hls.mjs') { response.writeHead(200, {'Content-Type':'text/javascript'}).end(vendor); return; }
      if (path === '/index.m3u8' || /^\/segment-\d{3}\.ts$/.test(path ?? '')) {
        const file = join(directory, path.slice(1));
        if (!existsSync(file)) { response.writeHead(404).end(); return; }
        response.writeHead(200, {'Content-Type':path.endsWith('.ts')?'video/mp2t':'application/vnd.apple.mpegurl','Cache-Control':'no-store'});
        createReadStream(file).on('error', () => response.destroy()).pipe(response); return;
      }
    }
    if (!/^\/source\.mp4\?range=\d+$/.test(request.url ?? '')) { response.writeHead(404).end(); return; }
    if (!['GET','HEAD'].includes(request.method)) { response.writeHead(405).end(); return; }
    const event = { openedSeconds: seconds(), method: request.method, url: request.url,
      range: request.headers.range ?? null, bytes: 0, chunks: [] };
    events.push(event);
    response.on('close', () => { event.closedSeconds = seconds(); });
    const match = request.headers.range?.match(/^bytes=(\d+)-(\d*)$/);
    if (request.headers.range && !match) { response.writeHead(416).end(); return; }
    const start = match ? Number(match[1]) : 0;
    const end = match?.[2] ? Math.min(Number(match[2]), size - 1) : size - 1;
    if (start > end || start >= size) { response.writeHead(416, { 'Content-Range': `bytes */${size}` }).end(); return; }
    event.delayMs = config.openDelayMs + ((events.length - 1) % 3) * config.openJitterMs;
    await sleep(event.delayMs);
    if (response.destroyed) return;
    event.headersSeconds = seconds();
    response.writeHead(match ? 206 : 200, { 'Content-Type': 'video/mp4',
      'Content-Length': end - start + 1, 'Accept-Ranges': 'bytes',
      ...(match ? { 'Content-Range': `bytes ${start}-${end}/${size}` } : {}) });
    if (request.method === 'HEAD') { response.end(); return; }
    const input = createReadStream(source, { start, end, highWaterMark: 16384 });
    response.on('close', () => input.destroy());
    try {
      for await (const chunk of input) {
        await sleep(chunk.length / config.bodyBytesPerSecond * 1000);
        if (response.destroyed) break;
        event.chunks.push({ seconds: seconds(), start: start + event.bytes, end: start + event.bytes + chunk.length });
        event.bytes += chunk.length;
        if (!response.write(chunk)) {
          await new Promise(resolve => {
            const done = () => { response.off('drain', done); response.off('close', done); resolve(); };
            response.once('drain', done); response.once('close', done);
          });
        }
      }
      if (!response.destroyed) response.end();
    } catch { response.destroy(); }
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  const concat = join(directory, 'retained.ffconcat');
  writeFileSync(concat, 'ffconcat version 1.0\n' + ranges.map(([start, end], index) =>
    `file 'http://127.0.0.1:${port}/source.mp4?range=${index}'\ninpoint ${start}\noutpoint ${end}\nduration ${end - start}\n`).join(''));
  const args = ['-hide_banner', '-loglevel', 'warning', '-nostdin', '-y',
    '-filter_threads', '1', '-filter_complex_threads', '1', '-threads', '1',
    '-readrate', '1.05', '-readrate_initial_burst', String(config.initialBurst), '-copyts',
    '-f', 'concat', '-safe', '0', '-protocol_whitelist', 'file,http,tcp', '-segment_time_metadata', '1', '-i', concat,
    // Preserve concat's common translated A/V timeline; do not rebuild each clock independently.
    '-vf', 'select=concatdec_select',
    '-af', 'aselect=concatdec_select', '-fps_mode', 'passthrough',
    '-c:v', 'libx264', '-threads:v', '1', '-preset', 'veryfast', '-g', '100', '-keyint_min', '100', '-sc_threshold', '0', '-bf', '0',
    '-c:a', 'aac', '-threads:a', '1', '-b:a', '96k', '-f', 'hls', '-hls_time', '4', '-hls_list_size', '0',
    '-hls_flags', 'temp_file+independent_segments', '-hls_segment_filename', join(directory, 'segment-%03d.ts'), join(directory, 'index.m3u8')];
  writeFileSync(join(directory, 'argv.json'), JSON.stringify(args, null, 2));
  origin = performance.now();
  encoderEpoch = Date.now();
  const child = spawn(ffmpeg, args, { stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true });
  let stderr = '';
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-100000); });
  let finished = false;
  let timedOut = false;
  const completion = new Promise(resolve => {
    child.once('error', error => { finished = true; resolve({ error: error.message }); });
    child.once('close', (code, signal) => { finished = true; resolve({ code, signal }); });
  });
  const timeout = setTimeout(() => { timedOut = true; child.kill('SIGKILL'); }, 65000);
  const seen = new Set();
  let availableSeconds = 0;
  let page = null;
  const browserErrors = [];
  let playbackRequested = false;
  let player = null;
  function observe() {
    const playlist = join(directory, 'index.m3u8');
    if (!existsSync(playlist)) return;
    const text = readFileSync(playlist, 'utf8');
    for (const match of text.matchAll(/#EXTINF:([\d.]+),[^\n]*\n([^#\n][^\n]*)/g)) {
      const file = match[2].trim();
      if (seen.has(file) || !existsSync(join(directory, file))) continue;
      seen.add(file);
      availableSeconds += Number(match[1]);
      arrivals.push({ file, durationSeconds: Number(match[1]), seconds: seconds(), availableSeconds });
    }
  }
  try {
    if (browser) {
      page = await browser.newPage();
      page.on('pageerror', error => browserErrors.push(error.message));
      await page.goto(`http://127.0.0.1:${port}/player`, { waitUntil:'load', timeout:10000 });
    }
    while (seconds() < 75) {
      observe();
      if (page && !playbackRequested && availableSeconds >= config.startupReserve) {
        playbackRequested = true;
        if (!await page.evaluate(() => { if (!window.startPlayback) return false; window.startPlayback(); return true; })) browserErrors.push('Player did not initialize');
      }
      if (page) player = await page.evaluate(() => window.proof);
      if (finished && (!page || player?.ended || !playbackRequested)) break;
      await sleep(100);
    }
    if (!finished) { timedOut = true; child.kill('SIGKILL'); }
    observe();
    const processResult = await completion;
    const expectedDuration = ranges.reduce((sum, [start, end]) => sum + end - start, 0);
    const buffer = consumption(arrivals, expectedDuration);
    const accountingStartupSeconds = browserProof ? player?.firstPlaying : buffer.startupSeconds;
    const beforeStartup = events.flatMap(event => event.chunks)
      .filter(chunk => accountingStartupSeconds != null && chunk.seconds <= accountingStartupSeconds)
      .sort((a, b) => a.start - b.start);
    let coveredEnd = 0;
    let uniqueSourceBytesBeforeStartup = 0;
    for (const chunk of beforeStartup) {
      uniqueSourceBytesBeforeStartup += Math.max(0, chunk.end - Math.max(chunk.start, coveredEnd));
      coveredEnd = Math.max(coveredEnd, chunk.end);
    }
    const wholeSourceOfferedBeforeStartup = accountingStartupSeconds != null && uniqueSourceBytesBeforeStartup >= size;
    const openedBeforeStartup = new Set(events.filter(event => accountingStartupSeconds != null && event.openedSeconds <= accountingStartupSeconds)
      .map(event => event.url));
    const futureRangeOpenedAfterStartup = accountingStartupSeconds != null && events.some(event =>
      event.openedSeconds > accountingStartupSeconds && !openedBeforeStartup.has(event.url));
    const result = { name, config, ranges, expectedDuration, processResult, timedOut,
      outputSeconds: availableSeconds, sourceBytes: size,
      httpBytesOffered: events.reduce((sum, event) => sum + event.bytes, 0),
      httpBytesOfferedBeforeStartup: beforeStartup.reduce((sum, chunk) => sum + chunk.end - chunk.start, 0),
      accountingStartupSeconds, uniqueSourceBytesBeforeStartup, wholeSourceOfferedBeforeStartup, futureRangeOpenedAfterStartup,
      note: 'Bytes are offered to HTTP, not confirmed decoder consumption. Bodies paced per request. 100ms arrival sampling; simulated consumer has no transport/decode overhead. aselect trims audio frames, not sample-exact boundaries. Removed pixels/audio, A/V continuity and actual host playback are NOT verified. Optional real-player evidence is separate.',
      buffer, arrivals, http: events, player, browserErrors };
    result.reserveFeasible = processResult.code === 0 && !timedOut && buffer.started && buffer.startupWithinLimit
      && buffer.minimumReserveSeconds >= 0 && buffer.underruns.length === 0
      && !wholeSourceOfferedBeforeStartup && futureRangeOpenedAfterStartup
      && Math.abs(availableSeconds - expectedDuration) < 0.25;
    if (browserProof) {
      result.playerFeasible = result.reserveFeasible && player?.hlsVersion === '1.6.16'
        && player.firstPlaying !== null && player.firstPlaying <= config.startupLimit
        && player.events.find(event => event.kind === 'playing')?.mediaTime < 0.5
        && browserErrors.length === 0 && player.ended && player.frames > 0 && player.firstMediaTime < 0.5
        && Math.abs(player.finalMediaTime - expectedDuration) < 0.25
        && player.lastPresentedMediaTime !== null
        && Math.abs(player.lastPresentedMediaTime - (expectedDuration - 1/25)) <= player.endFrameToleranceSeconds
        && player.freezes.length === 0
        && !player.events.some(event => event.afterStart && ['waiting','stalled','seeking','seeked','error'].includes(event.kind))
        && !player.errors.some(error => error.kind !== 'hls' || error.fatal || error.afterStart);
      result.playerStartupTargetMet = player?.firstPlaying != null && player.firstPlaying <= config.startupTarget;
      result.playerLimitation = 'Muted video/buffer proof only. 250ms ongoing frame/currentTime-gap detection; smaller glitches may escape. Final presented frame must be within one25fps frame plus1ms of expected last frame; forward media jumps allow two-frame scheduling jitter (80ms). Audible continuity, retained/removed content and A/V sync NOT verified.';
    }
    writeFileSync(join(directory, 'result.json'), JSON.stringify(result, null, 2));
    writeFileSync(join(directory, 'ffmpeg.log'), stderr);
    return result;
  } finally {
    clearTimeout(timeout);
    if (!finished) { child.kill('SIGKILL'); await completion; }
    if (page) await page.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
}

const results = [];
try {
// Same cut plan, with the late join beginning midway through a retained range.
for (const [name, ranges] of [
  ['early-close-cuts', [[0, 1], [3, 4], [7, 9], [12, 16], [19, 27], [31, 39]]],
  ['late-join', [[8, 9], [12, 16], [19, 27], [31, 39]]],
]) {
  const result = await scenario(name, ranges);
  results.push(result);
  console.log(JSON.stringify({ name, reserveFeasible: result.reserveFeasible, playerFeasible: result.playerFeasible,
    firstPlayingSeconds: result.player?.firstPlaying, buffer: result.buffer,
    outputSeconds: result.outputSeconds, httpBytesOffered: result.httpBytesOffered,
    uniqueSourceBytesBeforeStartup: result.uniqueSourceBytesBeforeStartup,
    wholeSourceOfferedBeforeStartup: result.wholeSourceOfferedBeforeStartup, processResult: result.processResult }));
}
} finally { if (browser) await browser.close(); }
writeFileSync(join(root, 'summary.json'), JSON.stringify({ limitation: 'Feasibility, not uninterrupted playback proof', results }, null, 2));
if (results.some(result => browserProof ? !result.playerFeasible : !result.reserveFeasible)) process.exitCode = 1;
