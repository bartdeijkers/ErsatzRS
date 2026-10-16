import { expect, Locator, Page, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Source map: the owned native bank fixture serves the production Router.
// /channels/{id}/teletext renders SSR preview/editor/publication forms. Export,
// import, request-publication and publish use their existing /admin-api/teletext/
// design endpoints. Only plain HTML forms are exercised, without JavaScript;
// this proves no matched WASM bundle or hydration. The native fixture verifies
// the durable concept before acknowledging permission to use Publish, then
// proves the same generation on ordinary stream URLs and separate real clients.
// Run one explicitly owned native fixture per headed browser, serially.
const readyPath = process.env.ERSATZRS_TELETEXT_RESOURCE_BANK_READY;
const bound = 64 * 1024;
const cultures = [['en-us', 'en_us'], ['nl', 'nl'], ['de-de', 'de_de'],
  ['fr-fr', 'fr_fr'], ['es-es', 'es_es'], ['es-419', 'es_419'], ['it-it', 'it_it'],
  ['pt-br', 'pt_br'], ['pl', 'pl']] as const;
type Channel = {
  channel_id: number; theme: 'day' | 'night'; addon_id: string;
  design_id: number; revision: number; digest: string; original_digest: string;
  profile_id: number; profile_revision: number; generation_id: number;
  original_preview_url: string; admin_url: string;
};
type Ready = {
  probe_version: number; base_url: string; channels: Channel[];
  bank: { name: string; original_path: string; edited_path: string;
    original_sha256: string; edited_sha256: string; sparse: boolean };
};
type Upload = { name: string; mimeType: string; buffer: Buffer };
type Raster = { width: number; height: number; pixels: number[] };
type Concept = { probe_version: number; design_id: number; revision: number; digest: string };

function hash(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
function bounded(path: string, maximum = bound): Buffer {
  const stat = lstatSync(path);
  expect(stat.isFile() && !stat.isSymbolicLink()).toBe(true);
  expect(stat.size).toBeGreaterThan(0); expect(stat.size).toBeLessThanOrEqual(maximum);
  const bytes = readFileSync(path);
  expect(bytes.length).toBeLessThanOrEqual(maximum);
  return bytes;
}
function marker(directory: string, name: string, value: unknown): void {
  const bytes = Buffer.from(JSON.stringify(value, null, 2));
  expect(bytes.length).toBeLessThanOrEqual(bound);
  writeFileSync(join(directory, name), bytes, { flag: 'wx' });
}
function translation(module: string, key: string): string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, pathToFileURL(__filename))), 'utf8');
  const found = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
  if (!found) throw new Error(`Missing resource-bank translation ${module}/${key}.`);
  return JSON.parse(found[1]) as string;
}
function preview(channel: Channel, revision = channel.revision, page = 124): string {
  return `/channels/${channel.channel_id}/teletext?design_id=${channel.design_id}&revision=${revision}&page=${page}&subcode=0020`;
}
async function visit(page: Page, url: string): Promise<void> {
  expect((await page.goto(url))?.status()).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await expect(page.getByTestId('teletext-preview-model')).toBeVisible();
}
async function submit(page: Page, form: Locator): Promise<void> {
  const [response] = await Promise.all([
    page.waitForResponse(value => value.request().method() === 'POST'
      && new URL(value.url()).pathname.startsWith('/admin-api/teletext/design/')),
    page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() }),
    form.locator('button[type="submit"]').click(),
  ] as const);
  expect(response.status()).toBe(303);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
}
async function field(form: Locator, name: string): Promise<string> {
  return form.locator(`[name="${name}"]`).inputValue();
}
async function publication(page: Page): Promise<{ generation_id: number; request_id: number; digest: string }> {
  const form = page.getByTestId('teletext-publication-publish-form');
  return {
    generation_id: Number(await field(form, 'expected_generation_id')),
    request_id: Number(await field(form, 'expected_request_id')),
    digest: await field(form, 'digest'),
  };
}
async function raster(page: Page, number: number): Promise<Raster> {
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
function rasterHash(value: Raster): string { return hash(Buffer.from(value.pixels)); }
function glyphs(value: Raster, theme: 'day' | 'night', number: number, sparse: boolean, edited = false): void {
  const mirrored = sparse || number === 125;
  const pattern = mirrored
    ? [0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900]
    : [0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009];
  if (edited && theme === 'day' && (sparse || number === 124)) pattern[0] = 0x803;
  const foreground = theme === 'day' ? [255, 0, 0, 255] : [0, 0, 255, 255];
  for (const column of [8, 10]) for (let y = 0; y < 10; y++) for (let x = 0; x < 12; x++) {
    const offset = ((10 + y) * value.width + (4 + column) * 12 + x) * 4;
    expect(value.pixels.slice(offset, offset + 4)).toEqual(pattern[y] & (1 << (11 - x))
      ? foreground : [0, 0, 0, 255]);
  }
}
function changedPixels(before: Raster, after: Raster, changed: boolean): number[] {
  expect([after.width, after.height]).toEqual([before.width, before.height]);
  const actual: number[] = [];
  for (let index = 0; index < before.pixels.length; index += 4) {
    if (before.pixels.slice(index, index + 4).some((value, offset) => value !== after.pixels[index + offset])) {
      actual.push(index / 4);
      expect(after.pixels.slice(index, index + 4)).toEqual([255, 0, 0, 255]);
    }
  }
  expect(actual).toEqual(changed ? [10 * before.width + 154, 10 * before.width + 178] : []);
  return actual;
}
function archiveFiles(path: string): Upload[] {
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

test('retained QTM bank uses localized native upload, preview and explicit publication', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires one explicitly owned native resource-bank fixture per browser.');
  test.setTimeout(600_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1);
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.channels).toHaveLength(2);
  const [day, night] = ['day', 'night'].map(theme => ready.channels.find(channel => channel.theme === theme)!);
  expect(day).toBeDefined(); expect(night).toBeDefined();
  expect(day.addon_id).toBe(night.addon_id); expect(day.profile_id).not.toBe(night.profile_id);
  expect(ready.bank.name).toMatch(/^[A-Za-z0-9_.-]+\.tti$/);
  expect(typeof ready.bank.sparse).toBe('boolean');
  const original = bounded(ready.bank.original_path, 8 * 1024 * 1024);
  const edited = bounded(ready.bank.edited_path, 8 * 1024 * 1024);
  expect(hash(original)).toBe(ready.bank.original_sha256);
  expect(hash(edited)).toBe(ready.bank.edited_sha256); expect(original.equals(edited)).toBe(false);
  const context = await browser.newContext({ baseURL: ready.base_url,
    javaScriptEnabled: false, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  const observations: unknown[] = [];
  const profiles: unknown[] = [];
  const before = new Map<string, Raster>();
  let completed: unknown;
  try {
    for (const channel of [day, night]) {
      await visit(page, channel.original_preview_url);
      expect((await publication(page)).generation_id).toBe(channel.generation_id);
      expect((await publication(page)).digest).toBe(channel.digest);
      const addon = page.locator(`[data-testid="teletext-addon"][data-addon-id="${channel.addon_id}"]`);
      const selected = addon.getByTestId('teletext-binding-form').locator('[name="profile_choice"]');
      await expect(selected).toHaveValue(`${channel.profile_id}:${channel.profile_revision}`);
      await selected.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${channel.theme}-binding.png`) });
      const details = addon.locator(`[data-testid="teletext-profile"][data-profile-id="${channel.profile_id}"]`);
      if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
      const theme = details.locator('[data-setting-key="THEME"] [name$=".value"]');
      await expect(theme).toBeVisible(); await expect(theme).toHaveValue(channel.theme);
      profiles.push({ channel_id: channel.channel_id, addon_id: channel.addon_id,
        profile_choice: await selected.inputValue(), theme: await theme.inputValue() });
      await details.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${channel.theme}-profile.png`) });
      for (const number of [124, 125]) {
        await visit(page, preview(channel, channel.revision, number));
        const originalRaster = await raster(page, number);
        glyphs(originalRaster, channel.theme, number, ready.bank.sparse);
        before.set(`${channel.theme}-${number}`, originalRaster);
      }
    }
    for (const [language, module] of cultures) for (const dark of [false, true]) {
      expect((await page.request.post('/admin-api/settings/ui/save', {
        form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
      })).status()).toBe(303);
      await visit(page, preview(day));
      await expect(page.locator('html')).toHaveAttribute('lang', language);
      await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
      const form = page.getByTestId('teletext-editor-import-form');
      await expect(form.locator('[name="sidecar"]')).toHaveAccessibleName(translation(module, 'TeletextEditorSidecar'));
      await expect(form.locator('[name="files"]')).toHaveAccessibleName(translation(module, 'TeletextEditorFiles'));
      await expect(form.locator('button')).toHaveText(translation(module, 'TeletextEditorImport'));
      await expect(page.getByTestId('teletext-publication-request-form').locator('button')).toHaveText(translation(module, 'TeletextPublicationRequest'));
      await expect(page.getByTestId('teletext-publication-publish-form').locator('button')).toHaveText(translation(module, 'TeletextPublicationPublish'));
      await expect(page.getByTestId('teletext-publication-status')).toHaveText(translation(module, 'TeletextPublicationSelected'));
      await form.scrollIntoViewIfNeeded();
      const screenshot = `${language}-${dark ? 'dark' : 'light'}-upload.png`;
      await page.screenshot({ path: testInfo.outputPath(screenshot) });
      observations.push({ language, dark, screenshot, native_forms: true });
    }
    expect((await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    })).status()).toBe(303);
    await visit(page, preview(day));
    const download = page.waitForEvent('download');
    await page.getByTestId('teletext-editor-export-all-form').locator('button').click();
    const archive = testInfo.outputPath('original-bank.zip');
    await (await download).saveAs(archive);
    const files = archiveFiles(archive);
    const bank = files.filter(file => file.name === ready.bank.name);
    expect(bank).toHaveLength(1); expect(bank[0].buffer.equals(original)).toBe(true);
    const sidecar = files.filter(file => file.name === 'project.ersatzrs.json');
    expect(sidecar).toHaveLength(1);
    const project = JSON.parse(sidecar[0].buffer.toString('utf8')).project;
    expect(project.channel_id).toBe(day.channel_id);
    const mapping = project.assets.find((asset: { name: string }) => asset.name === ready.bank.name);
    expect(mapping.scope.kind).toBe('drcs_tti_set');
    expect(mapping.scope.resource_names).toHaveLength(2);
    expect(mapping.scope.resource_names[0] === null).toBe(ready.bank.sparse);
    const resources = mapping.scope.resource_names.filter((name: string | null) => name !== null)
      .map((name: string) => project.assets.find((asset: { name: string }) => asset.name === name).scope);
    for (const resource of resources) {
      expect(resource.kind).toBe('resource'); expect(resource.identity.kind).toBe('drcs');
      expect(resource.identity.page).toEqual({ magazine: 1, page: 192 });
    }
    expect(resources.map((resource: { identity: { subpage: number } }) => resource.identity.subpage))
      .toEqual(ready.bank.sparse ? [1] : [0, 1]);
    expect(resources.map((resource: { physical_subcode: number }) => resource.physical_subcode))
      .toEqual(ready.bank.sparse ? [0x0111] : [0x0110, 0x0111]);
    const upload = page.getByTestId('teletext-editor-import-form');
    await upload.locator('[name="name"]').fill(`Retained QTM bank ${testInfo.project.name}`);
    await upload.locator('[name="sidecar"]').setInputFiles(sidecar);
    await upload.locator('[name="files"]').setInputFiles(files.filter(file => file.name !== 'project.ersatzrs.json')
      .map(file => file.name === ready.bank.name ? { ...file, buffer: edited } : file));
    await submit(page, upload);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const imported = page.getByTestId('teletext-publication-publish-form');
    const concept: Concept = { probe_version: 1, design_id: Number(await field(imported, 'design_id')),
      revision: Number(await field(imported, 'revision')), digest: await field(imported, 'digest') };
    expect(concept.design_id).toBe(day.design_id); expect(concept.revision).toBe(day.revision + 1);
    expect(concept.digest).toMatch(/^[a-f0-9]{64}$/); expect(concept.digest).not.toBe(day.digest);
    expect((await publication(page)).generation_id).toBe(day.generation_id);
    const previews: unknown[] = [];
    for (const number of [124, 125, 124]) {
      if (number !== 124 || previews.length > 0) {
        await page.getByTestId('teletext-preview-model').locator(`nav a[href*="page=${number}"]`).click();
      }
      const actual = await raster(page, number);
      glyphs(actual, 'day', number, ready.bank.sparse, true);
      const changed = changedPixels(before.get(`day-${number}`)!, actual, ready.bank.sparse || number === 124);
      await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
      const screenshot = `edited-preview-${previews.length}-${number}.png`;
      await page.screenshot({ path: testInfo.outputPath(screenshot) });
      previews.push({ page: number, sha256: rasterHash(actual), changed_pixels: changed, screenshot });
    }
    for (const number of [124, 125]) {
      await visit(page, preview(night, night.revision, number));
      expect(rasterHash(await raster(page, number))).toBe(rasterHash(before.get(`night-${number}`)!));
      expect((await publication(page)).generation_id).toBe(night.generation_id);
    }
    marker(directory, 'browser-concept-ready.json', concept);
    const ack = join(directory, 'publication-ready.json');
    await expect.poll(() => existsSync(ack), { timeout: 60_000, intervals: [100, 250, 500] }).toBe(true);
    expect(JSON.parse(bounded(ack).toString('utf8'))).toEqual(concept);
    await visit(page, preview(day, concept.revision));
    await submit(page, page.getByTestId('teletext-publication-request-form'));
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
    expect((await publication(page)).generation_id).toBe(day.generation_id);
    const requested = (await publication(page)).request_id; expect(requested).toBeGreaterThan(0);
    await submit(page, page.getByTestId('teletext-publication-publish-form'));
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
    const selected = await publication(page);
    expect(selected.generation_id).toBeGreaterThan(day.generation_id); expect(selected.digest).toBe(concept.digest);
    await page.getByTestId('teletext-publication').scrollIntoViewIfNeeded();
    await page.screenshot({ path: testInfo.outputPath('explicitly-published.png') });
    await visit(page, preview(night));
    expect(await publication(page)).toMatchObject({ generation_id: night.generation_id, digest: night.digest });
    expect(errors).toEqual([]);
    marker(directory, 'browser-proof.json', { probe_version: 1, browser: testInfo.project.name,
      browser_version: browser.version(), java_script_enabled: false, hydration_proven: false,
      ready_sha256: hash(bounded(readyPath!)), original_sha256: hash(original), edited_sha256: hash(edited),
      downloaded_bank_sha256: hash(bank[0].buffer), archive_sha256: hash(bounded(archive, 10 * 1024 * 1024)),
      bank_name: ready.bank.name, resources, profiles, observations, previews, errors,
      original_rasters: [...before].map(([address, value]) => ({ address, sha256: rasterHash(value) })),
      original_channels: ready.channels.map(channel => ({ channel_id: channel.channel_id, theme: channel.theme,
        design_id: channel.design_id, revision: channel.revision, digest: channel.digest,
        generation_id: channel.generation_id })),
      other_channel_unchanged: true, publication: { ...concept, ...selected }, screenshot_directory: testInfo.outputDir });
    completed = { ...concept, generation_id: selected.generation_id, request_id: requested };
  } finally {
    expect((await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    })).status()).toBe(303);
    await context.close();
  }
  // Native code acknowledges the concept before publication and independently
  // checks this exact accepted generation before exposing client readiness.
  expect(completed).toBeDefined();
  marker(directory, 'browser-complete.json', completed);
});
