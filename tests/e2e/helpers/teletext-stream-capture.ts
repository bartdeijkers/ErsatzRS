import { writeFileSync } from 'node:fs';

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
