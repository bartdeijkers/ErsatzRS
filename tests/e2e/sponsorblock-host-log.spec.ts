import { deepStrictEqual } from 'node:assert';
import { appendFileSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from '@playwright/test';
import { beginHostLogWindow, hostLogFailures } from './sponsorblock-host-log';

test('only the explicit reserve cancellation outcome is allowed', () => {
  const cutoff = Date.parse('2026-09-06T12:00:35.000Z');
  const cancelled = '2026-09-06T12:00:35.000Z WARN retained MPEG-TS reserve ended channel_id=8 error=retained MPEG-TS response was cancelled';
  const underrun = 'WARN retained MPEG-TS reserve ended channel_id=8 error=retained MPEG-TS reserve underrun';
  const encoder = 'WARN ffmpeg channel stream exited unsuccessfully stderr=Broken pipe';
  deepStrictEqual(hostLogFailures(cancelled, cutoff), []);
  deepStrictEqual(hostLogFailures(cancelled.replace('35.000Z', '35.001Z'), cutoff), []);
  deepStrictEqual(hostLogFailures(cancelled.replace('35.000Z', '34.999Z'), cutoff), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(cancelled), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(cancelled, NaN), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(cancelled.replace('2026-09-06T12:00:35.000Z', 'unknown'), cutoff), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(cancelled.replace('2026-09-06T12:00:35.000Z', '2026-99-99T12:00:35Z'), cutoff), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(`${cancelled}\n${underrun}`, cutoff), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(`${underrun}\n${cancelled}`, cutoff), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures(`${cancelled}\n${encoder}`, cutoff), ['ffmpeg channel stream exited unsuccessfully']);
  deepStrictEqual(hostLogFailures(`${cancelled} unexpectedly`, cutoff), ['retained MPEG-TS reserve ended']);
  deepStrictEqual(hostLogFailures('WARN retained MPEG-TS reserve ended channel_id=8 error=Broken pipe'),
    ['retained MPEG-TS reserve ended']);
});

test('host log window reads only appended bytes and refuses an exceeded budget', () => {
  const directory = mkdtempSync(join(tmpdir(), 'ersatzrs-host-log-'));
  const path = join(directory, 'host.log');
  try {
    writeFileSync(path, 'ffmpeg channel stream exited unsuccessfully\n');
    const observe = beginHostLogWindow(path);
    const appended = '2026-09-06T12:00:35.000Z retained MPEG-TS reserve ended error=retained MPEG-TS response was cancelled\n';
    appendFileSync(path, appended);
    deepStrictEqual(observe(Date.parse('2026-09-06T12:00:35.000Z')), { bytesRead: Buffer.byteLength(appended), failures: [] });
    deepStrictEqual(observe(), { bytesRead: 0, failures: [], error: 'Host log window already consumed' });

    const overBudget = beginHostLogWindow(path);
    appendFileSync(path, Buffer.alloc(1024 * 1024 + 1, 0x20));
    deepStrictEqual(overBudget(), { bytesRead: 0, failures: [], error: 'Host log growth exceeded 1 MiB read budget' });

    const truncated = beginHostLogWindow(path);
    writeFileSync(path, '');
    deepStrictEqual(truncated(), { bytesRead: 0, failures: [], error: 'Host log rotated or truncated during capture' });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
