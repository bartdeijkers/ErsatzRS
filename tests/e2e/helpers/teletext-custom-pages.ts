import { expect, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';

// Setup uses the actual upload/link handlers on the owned throwaway host.
// It grants no publication authority and never edits a checked document.
export async function uploadCustomPage(page: Page, channel: number, number: number, rows: string[]): Promise<number> {
  const bytes = Buffer.from(`DE,Synthetic fields\r\nPN,${number}00\r\nSC,0000\r\nPS,8000\r\n`
    + rows.map((row, index) => `OL,${index + 1},${row}\r\n`).join(''), 'ascii');
  const uploaded = await page.request.post('/admin-api/teletext-elements/upload', {
    multipart: { file: { name: `synthetic-${number}.tti`, mimeType: 'text/plain', buffer: bytes } }, maxRedirects: 0,
  });
  expect(uploaded.status()).toBe(303);
  const location = uploaded.headers().location;
  expect(location).toMatch(/^\/teletext-elements\/\d+\?saved=uploaded/);
  const id = Number(location.match(/^\/teletext-elements\/(\d+)/)![1]);
  const linked = await page.request.post(`/admin-api/channels/${channel}/teletext-elements/save`, {
    form: { element_id: String(id), page_number: String(number), enabled: '1' }, maxRedirects: 0,
  });
  expect(linked.status()).toBe(303);
  expect(linked.headers().location).toContain('saved=custom-page');
  return id;
}

// Independent libzvbi decoding of the exact served bytes. Retain raw decoder
// output, including stderr at the finite capture boundary, without rewriting TS.
export function decodeCustomPage(ffmpeg: string, capture: string, number: number): string {
  const decoded = spawnSync(ffmpeg, ['-v', 'error', '-nostdin', '-fix_teletext_pts', '0',
    '-txt_page', String(number), '-txt_format', 'text', '-txt_duration', '1000', '-i', capture,
    '-map', '0:i:512', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 });
  writeFileSync(`${capture}.page-${number}.srt`, decoded.stdout ?? '');
  writeFileSync(`${capture}.page-${number}.stderr`, decoded.stderr ?? '');
  expect(decoded.error).toBeUndefined();
  expect(decoded.status).toBe(0);
  return decoded.stdout.toString();
}

// Decode a complete interior of the five-second finite prefix, keeping the
// capture's deliberately partial final PES outside this AV assertion.
export function decodeCustomPageAv(ffmpeg: string, capture: string): void {
  const decoded = spawnSync(ffmpeg, ['-v', 'error', '-nostdin', '-i', capture, '-t', '3',
    '-map', '0:v:0', '-map', '0:a:0', '-f', 'framemd5', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 });
  writeFileSync(`${capture}.av.framemd5`, decoded.stdout ?? '');
  writeFileSync(`${capture}.av.stderr`, decoded.stderr ?? '');
  expect(decoded.error).toBeUndefined(); expect(decoded.status).toBe(0);
  expect(decoded.stderr.length).toBe(0);
  const lines = decoded.stdout.toString().split('\n');
  expect(lines.filter(line => line.startsWith('0,')).length).toBeGreaterThan(50);
  expect(lines.filter(line => line.startsWith('1,')).length).toBeGreaterThan(100);
}
