import { spawn } from 'child_process';
import { writeFileSync } from 'fs';
import { get as httpGet, type IncomingMessage } from 'http';

const startupLimit = 20_000;
const captureLimit = 64 * 1024 * 1024;

// Diagnostic decoder only: the production response is requested exactly once.
// Arrival at framecrc is decoded delivery, not presentation or audible playback.
export async function captureTs(url: URL, output: string) {
  const executable = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH;
  if (!executable) throw new Error('Configure the verified FFmpeg 9 executable for decoded TS capture');
  const args = ['-hide_banner', '-nostdin', '-loglevel', 'error',
    '-threads', '1', '-filter_threads', '1', '-i', 'pipe:0',
    '-map', '0:v:0', '-map', '0:a:0', '-c:v', 'rawvideo', '-c:a', 'pcm_s16le',
    '-threads', '1', '-f', 'framecrc', '-flush_packets', '1', 'pipe:1'];
  const started = performance.now();
  const decoder = spawn(executable, args, { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  const chunks: Buffer[] = [];
  const arrivals: { wallMilliseconds: number; bytes: number }[] = [];
  const decoded: { stream: number; wallMilliseconds: number; ptsSeconds: number; durationSeconds: number }[] = [];
  const timeBases = new Map<number, number>();
  const firstDecodedMilliseconds: (number | null)[] = [null, null];
  const lastDecodedMilliseconds: (number | null)[] = [null, null];
  const maximumDecodedDeliveryGapMilliseconds = [0, 0];
  let startupMilliseconds: number | undefined;
  let intentionalCutoffWallMilliseconds: number | undefined;
  let bytes = 0;
  let stderrBytes = 0;
  let decoderOutputBytes = 0;
  let lineBuffer = '';
  let failure: string | undefined;
  let stopping = false;
  let response: IncomingMessage | undefined;
  let captureTimer: ReturnType<typeof setTimeout> | undefined;
  let decoderExit: { code: number | null; signal: string | null } | undefined;
  const decoderClosed = new Promise<void>(resolveClose => {
    decoder.once('close', (code, signal) => {
      decoderExit = { code, signal };
      resolveClose();
    });
  });
  let complete!: () => void;
  const completed = new Promise<void>(resolveComplete => { complete = resolveComplete; });
  const finish = (reason?: string) => {
    if (stopping) return;
    stopping = true;
    failure = reason;
    clearTimeout(startupTimer);
    clearTimeout(captureTimer);
    request.destroy();
    response?.destroy();
    if (reason) {
      decoder.stdin.destroy();
      decoder.kill('SIGKILL');
    } else {
      // Flush only the bytes already accepted by the bounded stdin queue.
      decoder.stdin.end();
    }
    complete();
  };
  const failDecoder = (reason: string) => {
    if (!stopping) { finish(reason); return; }
    // Decoder errors during the normal EOF drain still invalidate the proof.
    failure ??= reason;
    decoder.stdin.destroy();
    decoder.kill('SIGKILL');
  };
  const startupTimer = setTimeout(() => finish('Decoded audio/video startup exceeded 20 seconds'), startupLimit);
  const request = httpGet(url, incoming => {
    response = incoming;
    if (stopping) { incoming.destroy(); return; }
    if (incoming.statusCode !== 200) { finish('TS HTTP response was not successful'); return; }
    incoming.on('data', (chunk: Buffer) => {
      if (stopping) return;
      if (bytes + chunk.length > captureLimit || arrivals.length >= 40_000) {
        finish('TS capture exceeded bounded size or observation count'); return;
      }
      const wallMilliseconds = performance.now() - started;
      if (startupMilliseconds === undefined) {
        startupMilliseconds = wallMilliseconds;
        captureTimer = setTimeout(() => {
          if (stopping) return;
          intentionalCutoffWallMilliseconds = Date.now();
          finish();
        }, 35_000);
      }
      bytes += chunk.length;
      chunks.push(chunk);
      arrivals.push({ wallMilliseconds, bytes: chunk.length });
      // At most one response chunk beyond Node's stdin high-water mark.
      if (!decoder.stdin.write(chunk)) incoming.pause();
    });
    incoming.on('end', () => finish('TS response ended before the 35-second capture window'));
    incoming.on('error', () => finish('TS body failed'));
  });
  request.on('error', () => finish('TS request failed'));
  decoder.stdin.on('drain', () => { if (!stopping) response?.resume(); });
  decoder.stdin.on('error', () => finish('Decoder input failed'));
  decoder.on('error', () => finish('Decoder could not run'));
  decoder.on('close', () => finish('Decoder exited before capture completed'));
  decoder.stdout.on('error', () => failDecoder('Decoder output failed'));
  decoder.stderr.on('error', () => failDecoder('Decoder diagnostics pipe failed'));
  decoder.stderr.on('data', (chunk: Buffer) => {
    stderrBytes += chunk.length;
    // Never save raw diagnostics: they can contain source-controlled metadata.
    if (stderrBytes > 64 * 1024) failDecoder('Decoder diagnostics exceeded 64 KiB');
  });
  decoder.stdout.setEncoding('utf8');
  decoder.stdout.on('data', (chunk: string) => {
    decoderOutputBytes += Buffer.byteLength(chunk);
    if (decoderOutputBytes > 8 * 1024 * 1024) { failDecoder('Decoder output exceeded 8 MiB'); return; }
    lineBuffer += chunk;
    const lines = lineBuffer.split('\n');
    lineBuffer = lines.pop()!;
    if (lineBuffer.length > 4096) { failDecoder('Decoder output line exceeded bound'); return; }
    for (const line of lines) {
      if (line.length > 4096) { failDecoder('Decoder output line exceeded bound'); return; }
      const timeBase = line.match(/^#tb\s+(\d+):\s+(\d+)\/(\d+)$/);
      if (timeBase) {
        const value = Number(timeBase[2]) / Number(timeBase[3]);
        if (!Number.isFinite(value) || value <= 0) { failDecoder('Invalid decoder time base'); return; }
        timeBases.set(Number(timeBase[1]), value);
        continue;
      }
      if (!line.trim() || line.startsWith('#')) continue;
      const fields = line.split(',').map(field => field.trim());
      const stream = Number(fields[0]);
      const pts = Number(fields[2]);
      const duration = Number(fields[3]);
      const size = Number(fields[4]);
      const timeBaseSeconds = timeBases.get(stream);
      if (fields.length < 6 || (stream !== 0 && stream !== 1) || !timeBaseSeconds ||
          !Number.isFinite(pts) || !Number.isFinite(duration) || duration <= 0 ||
          !Number.isFinite(size) || size <= 0 || decoded.length >= 40_000) {
        failDecoder('Invalid or excessive decoded frame observations'); return;
      }
      const wallMilliseconds = performance.now() - started;
      if (firstDecodedMilliseconds[stream] === null) firstDecodedMilliseconds[stream] = wallMilliseconds;
      const last = lastDecodedMilliseconds[stream];
      if (last !== null) maximumDecodedDeliveryGapMilliseconds[stream] = Math.max(
        maximumDecodedDeliveryGapMilliseconds[stream], wallMilliseconds - last);
      lastDecodedMilliseconds[stream] = wallMilliseconds;
      decoded.push({ stream, wallMilliseconds, ptsSeconds: pts * timeBaseSeconds,
        durationSeconds: duration * timeBaseSeconds });
      if (firstDecodedMilliseconds.every(value => value !== null)) {
        clearTimeout(startupTimer);
        if (firstDecodedMilliseconds.some(value => value! > startupLimit)) {
          finish('Decoded audio/video startup exceeded 20 seconds'); return;
        }
      }
    }
  });
  await completed;
  // Drain a normal cutoff; failures kill immediately. Always await close, with
  // a bounded second deadline reporting unconfirmed termination honestly.
  let killTimer: ReturnType<typeof setTimeout> | undefined;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  try {
    killTimer = setTimeout(() => {
      failure ??= 'Decoder did not finish within the five-second drain budget';
      decoder.stdin.destroy();
      decoder.kill('SIGKILL');
    }, 5_000);
    await Promise.race([decoderClosed, new Promise<void>(resolveTimeout => {
      closeTimer = setTimeout(() => {
        failure = 'Decoder termination was not confirmed';
        decoder.stdin.destroy(); decoder.stdout.destroy(); decoder.stderr.destroy();
        decoder.unref();
        resolveTimeout();
      }, 7_000);
    })]);
  } finally {
    clearTimeout(killTimer);
    clearTimeout(closeTimer);
  }
  if (decoderExit?.code !== 0) failure ??= 'Decoder exited unsuccessfully';
  if (firstDecodedMilliseconds.some(value => value === null)) failure ??= 'Decoded audio or video missing';
  const decodedSpans = [0, 1].map(stream => {
    const frames = decoded.filter(frame => frame.stream === stream);
    return frames.length ? Math.max(...frames.map(frame => frame.ptsSeconds + frame.durationSeconds)) -
      Math.min(...frames.map(frame => frame.ptsSeconds)) : 0;
  });
  const evidence = { startupMilliseconds, intentionalCutoffWallMilliseconds, bytes, decoderArgs: args,
    firstDecodedVideoMilliseconds: firstDecodedMilliseconds[0],
    firstDecodedAudioMilliseconds: firstDecodedMilliseconds[1],
    decodedSpans, maximumDecodedDeliveryGapMilliseconds, arrivals, decoded, stderrBytes, decoderOutputBytes,
    decoderExit, complete: failure === undefined, error: failure,
    scope: 'Same-response decoded delivery only; not presented-player gaps or audible/content correctness' };
  writeFileSync(output, Buffer.concat(chunks));
  writeFileSync(`${output}.capture.json`, JSON.stringify(evidence, null, 2));
  if (failure) throw new Error(failure);
  return { ...evidence, startupMilliseconds: startupMilliseconds! };
}
