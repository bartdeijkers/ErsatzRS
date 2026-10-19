import { writeFileSync } from 'node:fs';
import { get as httpGet } from 'node:http';

export type TeletextTsCapture = { bytes: Buffer; ended: 'eof' | 'byte-limit' };

// Dedicated synthetic teletext capture: no provider cadence/idle heuristics.
// Timeouts are failures; a successful capture states whether EOF was observed.
export async function captureTeletextChannelTs(
  url: string,
  output: string,
  limits: { maximumBytes: number; maximumTimeMs: number },
): Promise<TeletextTsCapture> {
  if (!Number.isSafeInteger(limits.maximumBytes) || limits.maximumBytes < 188
      || limits.maximumBytes > 8 * 1024 * 1024 || limits.maximumBytes % 188 !== 0
      || !Number.isSafeInteger(limits.maximumTimeMs) || limits.maximumTimeMs < 1
      || limits.maximumTimeMs > 30_000) {
    throw new Error('Invalid finite synthetic capture limits.');
  }
  const address = new URL(url);
  if (!['http:', 'https:'].includes(address.protocol) || address.username || address.password) {
    throw new Error('Synthetic capture requires an HTTP endpoint without credentials.');
  }
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), limits.maximumTimeMs);
  let response: Response | undefined;
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  const chunks: Buffer[] = [];
  let count = 0;
  let ended: TeletextTsCapture['ended'] = 'byte-limit';
  let failure = 'request';
  try {
    response = await fetch(address, { signal: controller.signal, redirect: 'error' });
    failure = `HTTP ${response.status}`;
    if (response.status !== 200 || !response.body
        || !response.headers.get('content-type')?.toLowerCase().startsWith('video/mp2t')) {
      throw new Error('Synthetic stream response is unavailable or is not MPEG-TS.');
    }
    reader = response.body.getReader();
    failure = 'body read';
    while (count < limits.maximumBytes) {
      const chunk = await reader.read();
      if (chunk.done) { ended = 'eof'; break; }
      const accepted = chunk.value.subarray(0, limits.maximumBytes - count);
      chunks.push(Buffer.from(accepted));
      count += accepted.length;
    }
    failure = count < 188 ? 'empty body' : 'packet framing';
    if (count < 188 || count % 188 !== 0) throw new Error('Synthetic capture is empty or is not packet-aligned.');
    const bytes = Buffer.concat(chunks, count);
    writeFileSync(output, bytes);
    return { bytes, ended };
  } catch {
    throw new Error(controller.signal.aborted
      ? 'Synthetic stream capture exceeded its original time limit.'
      : `Synthetic stream capture failed (${failure}).`);
  } finally {
    controller.abort();
    clearTimeout(timer);
    if (reader) {
      await reader.cancel().catch(() => undefined);
      reader.releaseLock();
    } else if (response?.body) {
      await response.body.cancel().catch(() => undefined);
    }
  }
}

// Intentional host-stop observation, distinct from a complete finite capture.
// Keep only a bounded initial TS window while consuming the live response. The
// caller must independently prove host/worker cleanup and cannot infer it here.
export function observeTeletextUntilStop(url: string, isStopping: () => boolean): {
  ready: Promise<void>;
  closed: Promise<{ ended: string; bytes: number }>;
  prefix(): Buffer;
  close(): Promise<{ ended: string; bytes: number }>;
} {
  const address = new URL(url);
  if (address.protocol !== 'http:' || address.hostname !== '127.0.0.1' || address.username || address.password) {
    throw new Error('Shutdown observer requires its owned loopback HTTP host.');
  }
  const chunks: Buffer[] = [];
  const maximumPrefix = Math.floor(1024 * 1024 / 188) * 188;
  let stored = 0; let bytes = 0; let done = false; let callerClosed = false;
  let readyResolve!: () => void; let readyReject!: (error: Error) => void;
  const ready = new Promise<void>((resolve, reject) => { readyResolve = resolve; readyReject = reject; });
  let closedResolve!: (value: { ended: string; bytes: number }) => void;
  let closedReject!: (error: Error) => void;
  const closed = new Promise<{ ended: string; bytes: number }>((resolve, reject) => { closedResolve = resolve; closedReject = reject; });
  // Keep both failures observable by the owner without an unhandled-rejection
  // race while it is still waiting for the other boundary.
  void ready.catch(() => undefined); void closed.catch(() => undefined);
  const finish = (ended: string, failure?: Error): void => {
    if (done) return;
    done = true; clearTimeout(timer);
    const error = failure ?? (!callerClosed && !isStopping() ? new Error('Viewer ended before explicit host shutdown.') : undefined);
    if (error) { readyReject(error); closedReject(error); }
    else { readyResolve(); closedResolve({ ended, bytes }); }
  };
  const request = httpGet(address, { agent: false }, response => {
    if (response.statusCode !== 200 || !response.headers['content-type']?.toLowerCase().startsWith('video/mp2t')) {
      finish('invalid-response', new Error('Shutdown viewer did not receive MPEG-TS.'));
      response.destroy(); return;
    }
    response.on('data', (chunk: Buffer) => {
      bytes += chunk.length;
      if (stored < maximumPrefix) {
        const part = chunk.subarray(0, maximumPrefix - stored);
        chunks.push(Buffer.from(part)); stored += part.length;
        if (stored === maximumPrefix) readyResolve();
      }
    });
    response.once('end', () => finish('eof'));
    response.once('aborted', () => finish('aborted'));
    response.once('error', () => finish('read-error'));
  });
  const timer = setTimeout(() => {
    finish('timeout', new Error('Shutdown viewer exceeded its original 30-second time bound.'));
    request.destroy();
  }, 30_000);
  request.once('error', () => finish('request-error'));
  return {
    ready, closed,
    prefix: () => Buffer.concat(chunks, stored),
    close: async () => { callerClosed = true; request.destroy(); finish('client-close'); return closed; },
  };
}
