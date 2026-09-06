import { closeSync, fstatSync, openSync, readSync, statSync } from 'fs';

const readBudget = 1024 * 1024;
// Production static messages only. Do not return their surrounding log lines.
const failureSignatures = [
  'ffmpeg channel stream exited unsuccessfully',
  'retained MPEG-TS reserve ended',
  'retained MPEG-TS reserve refused',
  'retained interval startup was refused',
  'retained interval input preparation failed',
  'retained interval watermark preparation failed',
  'ffmpeg hls segmenter exited unsuccessfully',
  'failed to wait for ffmpeg hls segmenter',
  'retained HLS input preparation failed',
  'retained HLS watermark preparation failed',
  'retained HLS graphics preparation failed',
  'retained HLS failed after media publication',
];

export type HostLogEvidence = { bytesRead: number; failures: string[]; error?: string };

export function hostLogFailures(text: string, intentionalCutoffWallMilliseconds?: number): string[] {
  const lines = text.split('\n');
  return failureSignatures.filter(signature => lines.some(line => {
    if (!line.includes(signature)) return false;
    // ReserveError::Cancelled is the expected client-owned capture cutoff.
    // Exempt only a timestamped outcome at/after the intentional client cutoff.
    // Unknown time/cutoff is not evidence of a normal disconnect.
    const timestamp = line.match(/^\s*(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2}))\s/);
    const wallMilliseconds = timestamp ? Date.parse(timestamp[1]) : NaN;
    return signature !== 'retained MPEG-TS reserve ended' ||
      !/\berror=retained MPEG-TS response was cancelled\s*$/.test(line) ||
      !Number.isFinite(intentionalCutoffWallMilliseconds) || !Number.isFinite(wallMilliseconds) ||
      wallMilliseconds < intentionalCutoffWallMilliseconds!;
  }));
}

export function beginHostLogWindow(path: string): (intentionalCutoffWallMilliseconds?: number) => HostLogEvidence {
  let fd: number | undefined;
  try {
    fd = openSync(path, 'r');
    const initial = fstatSync(fd);
    if (!initial.isFile() || !Number.isSafeInteger(initial.size)) throw new Error();
    const file = fd;
    let consumed = false;
    return (intentionalCutoffWallMilliseconds?: number) => {
      if (consumed) return { bytesRead: 0, failures: [], error: 'Host log window already consumed' };
      consumed = true;
      try {
        const current = fstatSync(file);
        const named = statSync(path);
        if (current.dev !== initial.dev || current.ino !== initial.ino ||
            named.dev !== initial.dev || named.ino !== initial.ino ||
            current.birthtimeMs !== initial.birthtimeMs || current.size < initial.size) {
          return { bytesRead: 0, failures: [], error: 'Host log rotated or truncated during capture' };
        }
        const length = current.size - initial.size;
        if (!Number.isSafeInteger(length) || length > readBudget) {
          return { bytesRead: 0, failures: [], error: 'Host log growth exceeded 1 MiB read budget' };
        }
        const bytes = Buffer.alloc(length);
        if (readSync(file, bytes, 0, length, initial.size) !== length) {
          return { bytesRead: 0, failures: [], error: 'Host log changed during bounded read' };
        }
        const text = bytes.toString('utf8');
        return { bytesRead: length, failures: hostLogFailures(text, intentionalCutoffWallMilliseconds) };
      } catch {
        return { bytesRead: 0, failures: [], error: 'Host log observation unavailable' };
      } finally {
        // Cleanup must not replace the capture's original sanitized failure.
        try { closeSync(file); } catch { /* Descriptor cleanup only. */ }
      }
    };
  } catch {
    if (fd !== undefined) { try { closeSync(fd); } catch { /* Preserve the static error below. */ } }
    throw new Error('Host log must be an accessible regular file');
  }
}
