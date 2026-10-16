// Shared native SSR editor transfer mechanics; fixed vector expectations remain
// in each owning spec. No hydration or runtime-client claim is supplied here.
import { expect, Locator, Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { lstatSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const bound = 64 * 1024;
export const cultures = [['en-us', 'en_us'], ['nl', 'nl'], ['de-de', 'de_de'],
  ['fr-fr', 'fr_fr'], ['es-es', 'es_es'], ['es-419', 'es_419'], ['it-it', 'it_it'],
  ['pt-br', 'pt_br'], ['pl', 'pl']] as const;
export type Channel = {
  channel_id: number; theme: 'day' | 'night'; addon_id: string;
  design_id: number; revision: number; digest: string; original_digest: string;
  profile_id: number; profile_revision: number; generation_id: number;
  original_preview_url: string; admin_url: string;
};
export type Upload = { name: string; mimeType: string; buffer: Buffer };
export type Raster = { width: number; height: number; pixels: number[] };
export function hash(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
export function bounded(path: string, maximum = bound): Buffer {
  const stat = lstatSync(path);
  expect(stat.isFile() && !stat.isSymbolicLink()).toBe(true);
  expect(stat.size).toBeGreaterThan(0); expect(stat.size).toBeLessThanOrEqual(maximum);
  const bytes = readFileSync(path);
  expect(bytes.length).toBeLessThanOrEqual(maximum);
  return bytes;
}
export function marker(directory: string, name: string, value: unknown): void {
  const bytes = Buffer.from(JSON.stringify(value, null, 2));
  expect(bytes.length).toBeLessThanOrEqual(bound);
  writeFileSync(join(directory, name), bytes, { flag: 'wx' });
}
export function translation(module: string, key: string): string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, pathToFileURL(__filename))), 'utf8');
  const found = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
  if (!found) throw new Error(`Missing teletext translation ${module}/${key}.`);
  return JSON.parse(found[1]) as string;
}
export function preview(channel: Channel, revision = channel.revision, page = 124): string {
  return `/channels/${channel.channel_id}/teletext?design_id=${channel.design_id}&revision=${revision}&page=${page}&subcode=0020`;
}
export async function visit(page: Page, url: string): Promise<void> {
  expect((await page.goto(url))?.status()).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await expect(page.getByTestId('teletext-preview-model')).toBeVisible();
}
export async function submit(page: Page, form: Locator): Promise<void> {
  const [response] = await Promise.all([
    page.waitForResponse(value => value.request().method() === 'POST'
      && new URL(value.url()).pathname.startsWith('/admin-api/teletext/design/')),
    page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() }),
    form.locator('button[type="submit"]').click(),
  ] as const);
  expect(response.status()).toBe(303);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
}
export async function field(form: Locator, name: string): Promise<string> {
  return form.locator(`[name="${name}"]`).inputValue();
}
export async function publication(page: Page): Promise<{ generation_id: number; request_id: number; digest: string }> {
  const form = page.getByTestId('teletext-publication-publish-form');
  return {
    generation_id: Number(await field(form, 'expected_generation_id')),
    request_id: Number(await field(form, 'expected_request_id')),
    digest: await field(form, 'digest'),
  };
}
export async function raster(page: Page, number: number): Promise<Raster> {
  const model = page.getByTestId('teletext-preview-model');
  await expect(model).toHaveAttribute('data-page', String(number));
  await expect(model).toHaveAttribute('data-subcode', '0020');
  const image = model.locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete
    && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  return image.evaluate(element => {
    const image = element as HTMLImageElement;
    const svg = new DOMParser().parseFromString(atob(image.src.split(',')[1]), 'image/svg+xml');
    const geometry = svg.documentElement.getAttribute('viewBox')!.split(' ').map(Number);
    const [width, height] = geometry.slice(2);
    if (geometry[0] !== 0 || geometry[1] !== 0 || width !== 672 || height !== 250) {
      throw new Error('Native bank preview geometry changed.');
    }
    const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height;
    const context = canvas.getContext('2d')!; context.imageSmoothingEnabled = false;
    context.drawImage(image, 0, 0, width, height);
    return { width, height, pixels: Array.from(context.getImageData(0, 0, width, height).data) };
  });
}
export function rasterHash(value: Raster): string { return hash(Buffer.from(value.pixels)); }
export function archiveFiles(path: string): Upload[] {
  // Reuse the existing editor-flow's standard-library ZIP extraction boundary.
  const script = String.raw`import base64,json,re,sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    entries=archive.infolist()
    assert 0<len(entries)<=4090 and sum(e.file_size for e in entries)<=8388608
    assert len({e.filename for e in entries})==len(entries)
    assert all(re.fullmatch(r'[A-Za-z0-9_.-]{1,128}',e.filename) and e.filename not in ('.','..') for e in entries)
    print(json.dumps([{'name':e.filename,'base64':base64.b64encode(archive.read(e)).decode()} for e in entries]))
`;
  const values = JSON.parse(execFileSync('python3', ['-c', script, path], {
    encoding: 'utf8', timeout: 10_000, maxBuffer: 12 * 1024 * 1024,
  })) as { name: string; base64: string }[];
  return values.map(value => ({ name: value.name, mimeType: 'application/octet-stream', buffer: Buffer.from(value.base64, 'base64') }));
}
