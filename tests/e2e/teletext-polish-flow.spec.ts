import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import {
  archiveFiles, bounded, cultures, field, hash, marker, preview, publication,
  raster, rasterHash, submit, translation, visit, type Channel, type Raster,
} from './helpers/teletext-native-editor';

// Source map: ChannelTeletextPage -> TeletextDraftPreview and native forms on
// /channels/{id}/teletext. Existing /admin-api/teletext/design endpoints own ZIP
// export, receipt-authenticated import, Request and Publish. The owned fixture
// serves the production router; native state checks acknowledge the concept
// before publication and verify the same generation on ordinary stream routes.
// This is headed SSR/no-JS proof. No island, bundle or hydration claim is made.
const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
type Ready = {
  probe_version: number; base_url: string; channels: Channel[];
  fixture?: 'authored-bank' | 'compiler-owned' | 'compiler-owned-spacing' | 'compiler-owned-turkish';
  editor: { name: string; original_path: string; edited_path: string;
    original_sha256: string; edited_sha256: string };
};
const polish = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0];
const mirrored = [0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900];
// Existing literal Latin G0 A golden (glyph_mask_tests.rs), independent of
// observed browser pixels. The compiler-owned fixture has no authored bank.
const latinA = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
const underlinedPolish = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0xfff];
const underlinedLatinA = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0xfff];
const separatedMosaic = [0, 0x79e, 0x79e, 0, 0x79e, 0x79e, 0x79e, 0, 0x79e, 0x79e];
const turkishTL = [0, 0, 0x3f0, 0xc0, 0xd8, 0xd8, 0xd8, 0xde, 0, 0];

function polishGlyph(value: Raster, rows = polish): void {
  for (let y = 0; y < 10; y++) for (let x = 0; x < 24; x++) {
    const offset = ((10 + y) * value.width + 132 + x) * 4;
    expect(value.pixels.slice(offset, offset + 4)).toEqual(
      rows[y] & (1 << (11 - Math.floor(x / 2))) ? [255, 255, 255, 255] : [0, 0, 0, 255]);
  }
}
function protectedMosaic(value: Raster): void {
  for (let y = 0; y < 10; y++) for (let x = 0; x < 12; x++) {
    const offset = ((10 + y) * value.width + 84 + x) * 4;
    expect(value.pixels.slice(offset, offset + 4)).toEqual(separatedMosaic[y] & (1 << (11 - x))
      ? [255, 255, 255, 255] : [0, 0, 0, 255]);
  }
}
function themeGlyphs(value: Raster, theme: 'day' | 'night', compilerOwned = false): void {
  const foreground = theme === 'day' ? [255, 0, 0, 255] : [0, 0, 255, 255];
  const rows = compilerOwned ? latinA : mirrored;
  for (const column of [8, 10]) for (let y = 0; y < 10; y++) for (let x = 0; x < 12; x++) {
    const offset = ((10 + y) * value.width + (4 + column) * 12 + x) * 4;
    expect(value.pixels.slice(offset, offset + 4)).toEqual(rows[y] & (1 << (11 - x))
      ? foreground : [0, 0, 0, 255]);
  }
}
function changedPixels(before: Raster, after: Raster): number[] {
  expect([after.width, after.height]).toEqual([before.width, before.height]);
  const result: number[] = [];
  for (let index = 0; index < before.pixels.length; index += 4) {
    if (before.pixels.slice(index, index + 4).some((v, offset) => v !== after.pixels[index + offset])) {
      const pixel = index / 4, x = pixel % before.width, y = Math.floor(pixel / before.width);
      expect(x).toBeGreaterThanOrEqual(132); expect(x).toBeLessThan(156);
      expect(y).toBeGreaterThanOrEqual(10); expect(y).toBeLessThan(20);
      result.push(pixel);
    }
  }
  expect(result.length).toBeGreaterThan(0);
  return result;
}

test('genuine national Base QTM page uses localized native import, preview and explicit publication', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires a fresh owned native admin fixture for this browser.');
  test.setTimeout(600_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1);
  expect([undefined, 'authored-bank', 'compiler-owned', 'compiler-owned-spacing', 'compiler-owned-turkish']).toContain(ready.fixture);
  const spacing = ready.fixture === 'compiler-owned-spacing';
  const turkish = ready.fixture === 'compiler-owned-turkish';
  const compilerOwned = ready.fixture === 'compiler-owned' || spacing || turkish;
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.channels).toHaveLength(2);
  const [day, night] = ['day', 'night'].map(theme => ready.channels.find(c => c.theme === theme)!);
  expect(day.addon_id).toBe(night.addon_id); expect(day.profile_id).not.toBe(night.profile_id);
  expect(ready.editor.name).toBe('page-124-0020.tti');
  const original = bounded(ready.editor.original_path), edited = bounded(ready.editor.edited_path);
  expect(hash(original)).toBe(turkish
    ? 'a491d9e62df63aa12143cd21e5513af1f99c99e87b8fb79eab08f8de8c96c588' : spacing
    ? 'f314df192ce7d66ede32eb3ec18d7933839844b326356c48ca11a5d41aa92939'
    : 'a1cb39545a3ed73c13ff7cafc14b999b4a677ef39d1604b434aabee3453e9e3a');
  expect(hash(edited)).toBe(turkish
    ? '446bc9065d5573a79d76d7171fb62ef3703dd24ed828161c71cd2adef1644edb' : spacing
    ? 'a9b6cf3411b9ca992eaa243fe13135c79f24f99be513646529bca34d34b0eb60'
    : '962254bbb0427b5e9d1ff4d25f24d59fb5354203f61c3f1c4db2ae63b59f00fe');
  expect(hash(original)).toBe(ready.editor.original_sha256);
  expect(hash(edited)).toBe(ready.editor.edited_sha256);
  const context = await browser.newContext({ baseURL: ready.base_url,
    javaScriptEnabled: false, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  const observations: unknown[] = [], profiles: unknown[] = [], previews: unknown[] = [];
  const before = new Map<string, Raster>();
  let completed: unknown;
  try {
    for (const channel of [day, night]) {
      await visit(page, channel.original_preview_url);
      expect(await publication(page)).toMatchObject({ generation_id: channel.generation_id, digest: channel.digest });
      const addon = page.locator(`[data-testid="teletext-addon"][data-addon-id="${channel.addon_id}"]`);
      const selected = addon.getByTestId('teletext-binding-form').locator('[name="profile_choice"]');
      await expect(selected).toHaveValue(`${channel.profile_id}:${channel.profile_revision}`);
      const advanced = selected.locator('xpath=ancestor::details[@data-testid="teletext-source-advanced"]');
      if (await advanced.count() > 0 && await advanced.getAttribute('open') === null) await advanced.locator(':scope > summary').click();
      await selected.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${channel.theme}-binding.png`) });
      const details = addon.locator(`[data-testid="teletext-profile"][data-profile-id="${channel.profile_id}"]`);
      if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
      const theme = details.locator('[data-setting-key="THEME"] [name$=".value"]');
      await expect(theme).toHaveValue(channel.theme);
      profiles.push({ channel_id: channel.channel_id, addon_id: channel.addon_id,
        profile_choice: await selected.inputValue(), theme: await theme.inputValue() });
      await details.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${channel.theme}-profile.png`) });
      for (const number of [124, 125]) {
        await visit(page, preview(channel, channel.revision, number));
        const value = await raster(page, number);
        if (number === 125) themeGlyphs(value, channel.theme, compilerOwned);
        if (spacing && channel.theme === 'day' && number === 124) {
          polishGlyph(value, underlinedLatinA);
          protectedMosaic(value);
        }
        if (turkish && channel.theme === 'day' && number === 124) polishGlyph(value, latinA);
        before.set(`${channel.theme}-${number}`, value);
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
    const archive = testInfo.outputPath(turkish ? 'original-turkish.zip' : 'original-polish.zip');
    await (await download).saveAs(archive);
    const files = archiveFiles(archive), display = files.filter(f => f.name === ready.editor.name);
    expect(display).toHaveLength(1); expect(display[0].buffer.equals(original)).toBe(true);
    if (compilerOwned) {
      expect(files.map(f => f.name).sort()).toEqual([
        'page-124-0020.tti', 'page-125-0020.tti', 'project.ersatzrs.json']);
      expect(hash(files.find(f => f.name === 'page-125-0020.tti')!.buffer))
        .toBe('16cf6c27038559d36c3c0d0d4c27d39392c3d43b5f0798b1852906d2244f63f4');
    }
    const sidecar = files.filter(f => f.name === 'project.ersatzrs.json');
    expect(sidecar).toHaveLength(1);
    const project = JSON.parse(sidecar[0].buffer.toString('utf8')).project;
    if (turkish) {
      const retained = JSON.parse(bounded(join(dirname(ready.editor.original_path), 'project.ersatzrs.json')).toString('utf8'));
      const downloaded = JSON.parse(sidecar[0].buffer.toString('utf8'));
      expect(downloaded.project).toEqual(retained.project);
      expect(downloaded.receipt_id).not.toBe(retained.receipt_id);
      expect(downloaded.receipt_digest).toMatch(/^[a-f0-9]{64}$/);
    }
    expect(project.channel_id).toBe(day.channel_id);
    expect(project.pages.find((p: { name: string }) => p.name === ready.editor.name).page)
      .toEqual({ page: 124, subcode: 32 });
    const upload = page.getByTestId('teletext-editor-import-form');
    await upload.locator('[name="name"]').fill(`Genuine ${turkish ? 'Turkish' : 'Polish'} QTM ${testInfo.project.name}`);
    await upload.locator('[name="sidecar"]').setInputFiles(sidecar);
    await upload.locator('[name="files"]').setInputFiles(files.filter(f => f.name !== 'project.ersatzrs.json')
      .map(f => f.name === ready.editor.name ? { ...f, buffer: edited } : f));
    await submit(page, upload);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const imported = page.getByTestId('teletext-publication-publish-form');
    const concept = { probe_version: 1, design_id: Number(await field(imported, 'design_id')),
      revision: Number(await field(imported, 'revision')), digest: await field(imported, 'digest') };
    expect(concept.design_id).toBe(day.design_id); expect(concept.revision).toBe(day.revision + 1);
    expect(concept.digest).toMatch(/^[a-f0-9]{64}$/); expect(concept.digest).not.toBe(day.digest);
    expect((await publication(page)).generation_id).toBe(day.generation_id);
    for (const number of [124, 125, 124]) {
      if (number !== 124 || previews.length > 0) {
        await page.getByTestId('teletext-preview-model').locator(`nav a[href*="page=${number}"]`).click();
      }
      const value = await raster(page, number);
      const changes = number === 124 ? changedPixels(before.get('day-124')!, value) : [];
      if (number === 124) {
        polishGlyph(value, turkish ? turkishTL : spacing ? underlinedPolish : polish);
        if (spacing) protectedMosaic(value);
      } else {
        themeGlyphs(value, 'day', compilerOwned);
        expect(rasterHash(value)).toBe(rasterHash(before.get('day-125')!));
      }
      await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
      const screenshot = `edited-preview-${previews.length}-${number}.png`;
      await page.screenshot({ path: testInfo.outputPath(screenshot) });
      previews.push({ page: number, sha256: rasterHash(value), changed_pixels: changes, screenshot });
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
      fixture: ready.fixture ?? 'authored-bank',
      browser_version: browser.version(), java_script_enabled: false, hydration_proven: false,
      ready_sha256: hash(bounded(readyPath!)), original_sha256: hash(original), edited_sha256: hash(edited),
      downloaded_display_sha256: hash(display[0].buffer), archive_sha256: hash(bounded(archive, 10 * 1024 * 1024)),
      display_name: ready.editor.name, profiles, observations, previews, errors,
      original_rasters: [...before].map(([address, value]) => ({ address, sha256: rasterHash(value) })),
      original_channels: ready.channels.map(c => ({ channel_id: c.channel_id, theme: c.theme,
        design_id: c.design_id, revision: c.revision, digest: c.digest, generation_id: c.generation_id })),
      other_channel_unchanged: true, publication: { ...concept, ...selected }, screenshot_directory: testInfo.outputDir });
    completed = { ...concept, generation_id: selected.generation_id, request_id: requested };
  } finally {
    expect((await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    })).status()).toBe(303);
    await context.close();
  }
  expect(completed).toBeDefined();
  marker(directory, 'browser-complete.json', completed);
});
