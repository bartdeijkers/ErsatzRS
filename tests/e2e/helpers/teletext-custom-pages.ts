import { expect, type Page } from '@playwright/test';
import { execFileSync, spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';

// Setup uses the actual upload/link handlers on the owned throwaway host.
// It grants no publication authority and never edits a checked document.
export async function uploadCustomPage(page: Page, channel: number, number: number, rows: string[],
  timing?: { seconds: number; nextRows: string[] }): Promise<number> {
  const subpages = timing ? [rows, timing.nextRows] : [rows];
  const bytes = Buffer.from('DE,Synthetic fields\r\n' + subpages.map((cells, subpage) =>
    `PN,${number}${String(subpage).padStart(2, '0')}\r\nSC,${String(subpage).padStart(4, '0')}\r\nPS,8000\r\n`
    + (timing ? `CT,${timing.seconds},T\r\n` : '')
    + cells.map((row, index) => `OL,${index + 1},${row}\r\n`).join('')).join(''), 'ascii');
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

// The public FFprobe demuxer supplies actual PES PTS/data. Decode only the
// standardized header address/subcode here; libzvbi separately proves visible
// rows. No product carousel index, wall clock or rewritten TS is an oracle.
export function qualifyCustomPageCycles(ffprobe: string, capture: string,
  expected: { page: number; seconds: number }[]): void {
  const raw = execFileSync(ffprobe, ['-v', 'error', '-select_streams', 'i:512',
    '-show_packets', '-show_data', '-show_entries', 'packet=pts,data', '-of', 'json', capture],
    { timeout: 30_000, maxBuffer: 8 * 1024 * 1024 });
  writeFileSync(`${capture}.cycle-packets.json`, raw);
  const packets = JSON.parse(raw.toString()).packets as { pts: number; data: string }[];
  const hamming = [0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f,
    0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea];
  const nibble = (byte: number) => {
    const reversed = Number.parseInt(byte.toString(2).padStart(8, '0').split('').reverse().join(''), 2);
    const value = hamming.indexOf(reversed); expect(value).toBeGreaterThanOrEqual(0); return value;
  };
  const headers = new Map(expected.map(value => [value.page, [] as { pts: number; subcode: number }[]]));
  for (const packet of packets) {
    expect(Number.isSafeInteger(packet.pts)).toBe(true);
    const hex = packet.data.trim().split('\n').map(line => {
      const match = line.match(/^[0-9a-f]{8}: ([0-9a-f ]+?)  /i);
      expect(match).not.toBeNull(); return match![1].replaceAll(' ', '');
    }).join('');
    const data = Buffer.from(hex, 'hex'); expect(data[0]).toBe(0x10);
    expect((data.length - 1) % 46).toBe(0);
    for (let offset = 1; offset < data.length; offset += 46) {
      const unit = data.subarray(offset, offset + 46); expect(unit[1]).toBe(44);
      if (unit[0] === 0xff) continue;
      expect(unit[0]).toBe(2); expect(unit[3]).toBe(0xe4);
      const address = nibble(unit[4]) | (nibble(unit[5]) << 4);
      if (address >> 3 !== 0) continue;
      const number = nibble(unit[6]) | (nibble(unit[7]) << 4);
      if ((number & 15) > 9 || (number >> 4) > 9) continue;
      const page = ((address & 7) || 8) * 100 + (number >> 4) * 10 + (number & 15);
      const selected = headers.get(page); if (!selected) continue;
      const subcode = nibble(unit[8]) | ((nibble(unit[9]) & 7) << 4)
        | (nibble(unit[10]) << 8) | ((nibble(unit[11]) & 3) << 12);
      expect([1, 2]).toContain(subcode);
      if (!selected.length || selected.at(-1)!.subcode !== subcode) selected.push({ pts: packet.pts, subcode });
    }
  }
  const result = expected.map(({ page, seconds }) => {
    const changes = headers.get(page)!; expect(changes.length).toBeGreaterThanOrEqual(4);
    // The first observed subpage may already be part way through its dwell.
    // Every subsequent complete dwell must retain the saved duration; the
    // small fixture admits whole-page placement within ten20ms intervals.
    const durations = changes.slice(2).map((change, index) => (change.pts - changes[index + 1].pts) / 90_000);
    for (const duration of durations) {
      expect(duration).toBeGreaterThanOrEqual(seconds);
      expect(duration).toBeLessThanOrEqual(seconds + 0.2);
    }
    return { page, seconds, changes, complete_dwell_seconds: durations };
  });
  writeFileSync(`${capture}.cycles.json`, JSON.stringify(result, null, 2));
}
