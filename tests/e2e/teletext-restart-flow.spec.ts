import { expect, Locator, Page } from '@playwright/test';
import { copyFileSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { test } from './isolated-web-host';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs } from './helpers/teletext-stream-capture';

// Intentional Unix-only synthetic package, with an owned real host/database.
// Publication setup is English; restart views prove EN and NL durability.
const specUrl = pathToFileURL(__filename);
function text(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(`../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl)), 'utf8');
  return key => {
    const found = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
    if (!found) throw new Error(`Missing ${module}/${key} translation.`);
    return JSON.parse(found[1]) as string;
  };
}
async function ready(page: Page, js: boolean): Promise<void> {
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (js) await waitForHydration(page);
}
async function submit(page: Page, form: Locator, button: string, js: boolean): Promise<void> {
  const posted = page.waitForResponse(response => response.request().method() === 'POST' && new URL(response.url()).pathname.startsWith('/admin-api/teletext/'));
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: button, exact: true }).click();
  expect((await posted).status()).toBe(303);
  await navigated; await ready(page, js);
}
async function createProfile(page: Page, fixture: TeletextChannelFixture, theme: string, js: boolean): Promise<number> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  await form.locator('[name="name"]').fill(`Restart ${theme}`);
  await form.locator('[data-setting-key="THEME"] select[name$=".mode"]').selectOption('literal');
  await form.locator('[data-setting-key="THEME"] [name$=".value"]').selectOption(theme);
  await submit(page, form, text('en_us')('TeletextProfileCreate'), js);
  const profile = fixture.profiles().find(row => row.Name === `Restart ${theme}`);
  expect(profile).toBeDefined(); return profile!.Id;
}
async function bind(page: Page, fixture: TeletextChannelFixture, id: number, js: boolean): Promise<void> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-binding-form"]`);
  await form.locator('[name="profile_choice"]').selectOption(`${id}:${fixture.profiles().find(row => row.Id === id)!.Revision}`);
  await submit(page, form, text('en_us')('TeletextBindingSave'), js);
}
async function prepare(page: Page, fixture: TeletextChannelFixture, name: string, js: boolean): Promise<string> {
  const form = page.getByTestId('teletext-authoring-form');
  await expect(form.locator('[name="selection_digest"]')).toHaveValue(/^[a-f0-9]{64}$/);
  await form.locator('[name="name"]').fill(name);
  await form.locator('[name="entry_addon_id"]').selectOption(fixture.addonId);
  await form.locator('[name="entry_page"]').fill('200');
  await form.locator('[name="entry_subcode"]').fill('0000');
  await form.locator('[name="v1_region"]').selectOption('western_latin');
  await form.locator('[name="resource_transmissions"]').fill('[]');
  await submit(page, form, text('en_us')('TeletextValidateSaveDraft'), js);
  await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
  return page.url();
}
async function raster(page: Page, theme: 'day' | 'night', address: string): Promise<void> {
  const model = page.getByTestId('teletext-preview-model');
  await expect(model).toHaveAttribute('data-page', address);
  await expect(model).toHaveAttribute('data-subcode', '0000');
  const image = model.locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  const colours = await image.evaluate(element => {
    const canvas = document.createElement('canvas'); canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!; context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, canvas.width, canvas.height);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let red = 0; let blue = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index] === 255 && pixels[index + 1] === 0 && pixels[index + 2] === 0) red++;
      if (pixels[index] === 0 && pixels[index + 1] === 0 && pixels[index + 2] === 255) blue++;
    }
    return { red, blue };
  });
  expect(colours[theme === 'day' ? 'red' : 'blue']).toBeGreaterThan(0);
  expect(colours[theme === 'day' ? 'blue' : 'red']).toBe(0);
}
function publicationRows(fixture: TeletextChannelFixture): unknown[][] {
  return ['TeletextPublicationRequest', 'TeletextPublicationRequestCurrent', 'TeletextAcceptedGeneration', 'TeletextAcceptedGenerationCurrent']
    .map(table => fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels));
}
function restartSafety(fixture: TeletextChannelFixture): { snapshot: unknown; workerEpoch: string } {
  const snapshot = fixture.safetySnapshot() as { configuration: { Key: string; Value: string }[] };
  const row = snapshot.configuration.find(item => item.Key === 'addon.preparation.worker');
  expect(row).toBeDefined();
  const checkpoint = JSON.parse(row!.Value);
  // run_preparation_worker creates a new UUID for each real worker lifetime.
  // Check every field before normalizing only that identity; retain the key,
  // version, Idle state, null attempt and all other configuration values.
  expect(checkpoint).toEqual({ version: 1, epoch: expect.stringMatching(
    /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/), phase: 'Idle', attempt: null });
  row!.Value = JSON.stringify({ ...checkpoint, epoch: '<worker-lifetime>' });
  return { snapshot, workerEpoch: checkpoint.epoch };
}
async function capture(baseURL: string, channel: number, output: string): Promise<void> {
  const result = await captureTeletextChannelTs(`${baseURL}/iptv/hls-direct/${channel}.ts`, output,
    { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
  expect(result.ended).toBe('eof');
  const pids = new Map<number, number>(); let malformed = 0;
  for (let offset = 0; offset < result.bytes.length; offset += 188) {
    if (result.bytes[offset] !== 0x47 || (result.bytes[offset + 1] & 0x80) !== 0) malformed++;
    const pid = ((result.bytes[offset + 1] & 0x1f) << 8) | result.bytes[offset + 2];
    pids.set(pid, (pids.get(pid) ?? 0) + 1);
  }
  expect(malformed).toBe(0);
  for (const pid of [0, 256, 257, 512]) expect(pids.get(pid) ?? 0).toBeGreaterThan(0);
  execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', output,
    '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
}

test.describe.configure({ mode: 'serial' });
for (const [language, module, js, dark] of [['en-us', 'en_us', true, false], ['nl', 'nl', false, true]] as const) {
  test(`teletext real restart flow (${language}, JavaScript ${js}, dark ${dark})`, async ({ browser, isolatedWebHost }, testInfo) => {
    test.skip(process.platform === 'win32', 'Intentional POSIX synthetic teletext package; Windows runtime acceptance remains separate.');
    test.skip(!process.env.FFMPEG_PATH, 'Requires the selected FFmpeg 9 executable.');
    test.setTimeout(300_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
    fixture.setAuthoringPolicy(); fixture.setOutputPolicy();
    const contextOptions = { baseURL: isolatedWebHost.baseURL, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } };
    let context = await browser.newContext(contextOptions);
    let pages = [await context.newPage(), await context.newPage()];
    const errors: string[] = [];
    const watchErrors = (): void => {
      for (const page of pages) {
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      }
    };
    watchErrors();
    const prefix = testInfo.outputPath('candidate');
    copyFileSync(join(fixture.root, 'synthetic-client-source.ts'), `${prefix}.source.ts`);
    try {
      await pages[0].request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0 });
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`); await ready(pages[0], js);
      const day = await createProfile(pages[0], fixture, 'day', js);
      const night = await createProfile(pages[0], fixture, 'night', js);
      const selectedUrls: string[] = [];
      for (const [index, theme, profile] of [[0, 'day', day], [1, 'night', night]] as const) {
        const page = pages[index];
        await page.goto(`/channels/${fixture.channels[index]}/teletext`); await ready(page, js);
        await bind(page, fixture, profile, js);
        selectedUrls.push(await prepare(page, fixture, `Restart selected ${theme}`, js));
        await submit(page, page.getByTestId('teletext-publication-request-form'), text('en_us')('TeletextPublicationRequest'), js);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-requested');
        await submit(page, page.getByTestId('teletext-publication-publish-form'), text('en_us')('TeletextPublicationPublish'), js);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-selected');
      }
      await pages[0].goto(selectedUrls[0]); await ready(pages[0], js);
      const pendingUrl = await prepare(pages[0], fixture, 'Restart pending day only', js);
      await submit(pages[0], pages[0].getByTestId('teletext-publication-request-form'), text('en_us')('TeletextPublicationRequest'), js);
      await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
      const retained = publicationRows(fixture);
      expect(retained.map(rows => rows.length)).toEqual([3, 2, 2, 2]);
      expect(fixture.calls()).toEqual(['day', 'night', 'day']);
      expect(fixture.designs()).toHaveLength(3);
      expect(new URL(pendingUrl).searchParams.get('design_id')).not.toBe(new URL(selectedUrls[0]).searchParams.get('design_id'));
      await pages[0].request.post('/admin-api/settings/ui/save', { form: { language, is_dark_mode: String(dark) }, maxRedirects: 0 });
      fixture.resetSourceStart();
      await Promise.all([
        capture(isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.before.day.ts`),
        capture(isolatedWebHost.baseURL, fixture.channels[1], `${prefix}.before.night.ts`),
      ]);
      expect(publicationRows(fixture)).toEqual(retained);
      const safe = restartSafety(fixture);
      const calls = fixture.calls();
      const identity = isolatedWebHost.processIdentity();
      expect(identity.pid).toBeGreaterThan(0);
      fixture.resetSourceStart();
      // Finite-client durability proof: close this test's UI connections before
      // stop, then reload persisted views in a fresh context after host health.
      // This does not prove shutdown with still-open admin clients.
      await context.close();
      const restarted = await isolatedWebHost.restart();
      expect(restarted.before).toEqual(identity);
      expect(restarted.stop.closed).toBe(true);
      expect(restarted.stop.forced).toBe(false);
      expect(restarted.stop.exitCode).toBe(0);
      expect(restarted.stop.signalCode).toBeNull();
      expect(restarted.after.pid).toBeGreaterThan(0);
      expect(restarted.after.pid).not.toBe(identity.pid);
      expect(restarted.after.generation).toBeGreaterThan(identity.generation);
      expect(isolatedWebHost.processIdentity()).toEqual(restarted.after);
      context = await browser.newContext(contextOptions);
      pages = [await context.newPage(), await context.newPage()];
      watchErrors();
      expect((await pages[0].request.get('/healthz')).ok()).toBe(true);
      testInfo.annotations.push({ type: 'host-restart', description: JSON.stringify(restarted) });
      expect(publicationRows(fixture)).toEqual(retained);
      const restartedSafe = restartSafety(fixture);
      expect(restartedSafe.snapshot).toEqual(safe.snapshot);
      expect(restartedSafe.workerEpoch).not.toBe(safe.workerEpoch);
      expect(fixture.calls()).toEqual(calls);
      const tr = text(module);
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) {
        const page = pages[index];
        await page.goto(selectedUrls[index]); await ready(page, js);
        await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
        await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
        await expect(page.getByTestId('teletext-publication-status')).toHaveText(tr('TeletextPublicationSelected'));
        if (index === 0) {
          await expect(page.getByTestId('teletext-publication-replaces-pending')).toBeVisible();
          await expect(page.getByTestId('teletext-publication-replaces-pending')).toHaveText(tr('TeletextPublicationReplacePending'));
        }
        await raster(page, theme, '200');
        await page.getByTestId('teletext-publication').scrollIntoViewIfNeeded();
        await page.screenshot({ path: testInfo.outputPath(`${language}-selected-${theme}.png`), fullPage: false });
        await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
        await raster(page, theme, '201');
      }
      await pages[0].goto(pendingUrl); await ready(pages[0], js);
      await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
      await expect(pages[0].getByTestId('teletext-publication-status')).toHaveText(tr('TeletextPublicationPending'));
      await expect(pages[0].getByTestId('teletext-publication-publish-form').getByRole('button')).toBeEnabled();
      await raster(pages[0], 'day', '200');
      await pages[0].getByTestId('teletext-publication').scrollIntoViewIfNeeded();
      await pages[0].screenshot({ path: testInfo.outputPath(`${language}-pending-day.png`), fullPage: false });
      fixture.resetSourceStart();
      await Promise.all([
        capture(isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.after.day.ts`),
        capture(isolatedWebHost.baseURL, fixture.channels[1], `${prefix}.after.night.ts`),
      ]);
      expect(publicationRows(fixture)).toEqual(retained);
      expect(restartSafety(fixture)).toEqual(restartedSafe);
      expect(fixture.calls()).toEqual(calls);
      expect(fixture.unchangedPackage()).toBe(true);
      expect(errors).toEqual([]);
    } finally {
      await context.close();
      // Retained package/media files remain until isolated host teardown has
      // confirmed process closure; the wrapper owns eventual directory cleanup.
      fixture.close();
    }
  });
}
