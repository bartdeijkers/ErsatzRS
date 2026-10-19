import { expect, Locator, Page } from '@playwright/test';
import { copyFileSync, readFileSync, writeFileSync } from 'node:fs';
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
async function prepare(page: Page, fixture: TeletextChannelFixture, name: string, js: boolean, effective = false): Promise<string> {
  const form = page.getByTestId(effective ? 'teletext-effective-authoring-form' : 'teletext-authoring-form');
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
async function raster(page: Page, theme: 'day' | 'night', address: string, subcode = '0000'): Promise<void> {
  const model = page.getByTestId('teletext-preview-model');
  await expect(model).toHaveAttribute('data-page', address);
  await expect(model).toHaveAttribute('data-subcode', subcode);
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
async function capture(baseURL: string, channel: number, output: string, authored = true): Promise<void> {
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
  for (const pid of [0, 256, 257]) expect(pids.get(pid) ?? 0).toBeGreaterThan(0);
  if (authored) expect(pids.get(512) ?? 0).toBeGreaterThan(0);
  else expect(pids.get(512) ?? 0).toBe(0);
  execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', output,
    '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
}

test.describe.configure({ mode: 'serial' });
for (const standalone of [false, true]) for (const [language, module, js, dark] of [['en-us', 'en_us', true, false], ['nl', 'nl', false, true]] as const) {
  test(`teletext ${standalone ? 'standalone Level 1.5 ' : ''}real restart flow (${language}, JavaScript ${js}, dark ${dark})`, async ({ browser, isolatedWebHost }, testInfo) => {
    test.skip(process.platform === 'win32', 'Intentional POSIX synthetic teletext package; Windows runtime acceptance remains separate.');
    test.skip(!process.env.FFMPEG_PATH, 'Requires the selected FFmpeg 9 executable.');
    test.setTimeout(300_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true, standaloneLevel15: standalone, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
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
        if (standalone) {
          await raster(page, theme, '200');
          await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
          await raster(page, theme, '200', '0020');
          await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
          await raster(page, theme, '201');
        }
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
        if (standalone) {
          await raster(page, theme, '200', '0020');
          await page.getByTestId('teletext-preview-model').scrollIntoViewIfNeeded();
          await page.screenshot({ path: testInfo.outputPath(`${language}-subpage-${theme}.png`), fullPage: false });
          await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
        }
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


// Same existing isolated host and real hydrated scheduler action as the effective
// authoring flow. No interval/checkpoint/publication rows are seeded or rewritten.
async function queueScripted(page: Page, fixture: TeletextChannelFixture, channel: number,
  mode: 'reset' | 'continue', profile: number | null | undefined, count: number, leading = 0): Promise<void> {
  const before = fixture.database.prepare('SELECT COUNT(*) AS count FROM "PlayoutItem" WHERE "PlayoutId"=?').get(channel) as { count: number };
  const previous = fixture.database.prepare('SELECT "LastBuild" FROM "PlayoutBuildStatus" WHERE "PlayoutId"=?').get(channel) as { LastBuild: string } | undefined;
  const intervals = fixture.scriptedIntervals(channel);
  fixture.writeScriptedControls(channel, profile, count, leading);
  const tr = text('en_us');
  await page.goto('/playouts'); await ready(page, true);
  const name = fixture.database.prepare('SELECT "Name" FROM "Channel" WHERE "Id"=?').get(channel) as { Name: string };
  await page.getByPlaceholder(tr('SearchForPlayouts'), { exact: true }).fill(name.Name);
  const row = page.getByRole('row').filter({ has: page.locator(`a[href="/playouts/scripted/${channel}"]`) });
  await expect(row).toHaveCount(1);
  await row.getByRole('button', { name: tr('RebuildPlayout'), exact: true }).click();
  const dialog = page.getByRole('dialog', { name: tr('RebuildPlayout'), exact: true });
  await dialog.getByLabel(tr('PlayoutRebuildMode'), { exact: true }).selectOption(mode);
  const keep = dialog.getByRole('checkbox', { name: tr('KeepCurrentProgramme'), exact: true });
  if (mode === 'continue') { await expect(keep).toBeChecked(); await expect(keep).toBeDisabled(); }
  else await keep.uncheck();
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/rebuild_playout'));
  await dialog.getByRole('button', { name: tr('QueuePlayoutBuild'), exact: true }).click();
  const response = await posted;
  expect(response.status()).toBe(200);
  const fields = new URLSearchParams(response.request().postData() ?? '');
  expect(fields.get('id')).toBe(String(channel)); expect(fields.get('mode')).toBe(mode);
  expect(fields.get('preserve_current')).toBe(String(mode === 'continue'));
  expect(await response.json()).toEqual({ accepted: 1, coalesced: 0 });
  await expect(dialog).toBeHidden();
  await expect.poll(() => {
    const status = fixture.database.prepare('SELECT "Success","LastBuild","LastSuccessfulBuild" FROM "PlayoutBuildStatus" WHERE "PlayoutId"=?').get(channel) as { Success: number; LastBuild: string; LastSuccessfulBuild: string } | undefined;
    const items = fixture.database.prepare('SELECT COUNT(*) AS count FROM "PlayoutItem" WHERE "PlayoutId"=?').get(channel) as { count: number };
    const actual = fixture.scriptedIntervals(channel);
    const scopeMatches = mode === 'continue' ? JSON.stringify(actual) === JSON.stringify(intervals)
      : profile === undefined ? actual.length === 0
      : actual.length === 1 && actual[0].State === Number(profile !== null) && actual[0].ProfileId === profile;
    return status?.Success === 1 && status.LastBuild !== previous?.LastBuild
      && status.LastSuccessfulBuild === status.LastBuild && scopeMatches
      && items.count === count + leading + (mode === 'continue' ? before.count : 0);
  }, { timeout: 45_000 }).toBe(true);
}

function scopeUtc(value: string): number {
  const normalized = value.replace(' ', 'T');
  const time = Date.parse(/[zZ]|[+-]\d\d:\d\d$/.test(normalized) ? normalized : `${normalized}Z`);
  if (!Number.isFinite(time)) throw new Error('Published synthetic programme has an invalid UTC boundary.');
  return time;
}

async function captureScheduled(fixture: TeletextChannelFixture, baseURL: string, channel: number,
  output: string, authored = true): Promise<void> {
  // Reuse the effective-authoring fixture's genuine programme-boundary fence.
  // No script/interval/programme/source clock is rewritten by this observation.
  const start = fixture.database.prepare(`SELECT "Start" FROM "PlayoutItem"
    WHERE "PlayoutId"=? AND julianday("Start")>julianday(?) ORDER BY "Start" LIMIT 1`)
    .get(channel, new Date().toISOString()) as { Start: string } | undefined;
  expect(start).toBeDefined();
  const boundary = scopeUtc(start!.Start);
  expect(boundary - Date.now()).toBeLessThanOrEqual(12_000);
  await expect.poll(() => Date.now() >= boundary + 25, { timeout: 13_000, intervals: [25] }).toBe(true);
  await capture(baseURL, channel, output, authored);
}

function scopeAuthority(fixture: TeletextChannelFixture): unknown {
  return {
    publication: [...publicationRows(fixture), ...['TeletextEffectivePublicationRequestCurrent', 'TeletextEffectiveAcceptedGenerationCurrent']
      .map(table => fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels))],
    scheduling: ['PlayoutTeletextInterval', 'PlayoutScriptedCheckpoint', 'PlayoutBuildStatus']
      .map(table => fixture.database.prepare(`SELECT * FROM "${table}" WHERE "PlayoutId" IN (?,?) ORDER BY 1,2`).all(...fixture.channels)),
    safety: restartSafety(fixture),
  };
}

// New lifecycle proof is scoped to one headed EN/JS/light run. Existing composed
// NL/no-JS/dark, other-browser, editor and live-expiry receipts remain separate.
test('standalone Level 1.5 Lua scopes survive real host restarts and expire to accepted base', async ({ browser, isolatedWebHost }, testInfo) => {
  test.skip(process.platform === 'win32', 'Intentional POSIX synthetic package; native Windows remains separate.');
  test.skip(testInfo.project.name !== 'chromium', 'This bounded lifecycle proof reuses existing browser/localization coverage.');
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true, standaloneLevel15: true, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
  fixture.configureScripted(); fixture.setAuthoringPolicy();
  const policyFile = fileURLToPath(new URL('../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl));
  const policy = JSON.parse(readFileSync(policyFile, 'utf8'));
  // Version1 is frozen original/startup-only. Existing version2 owns effective
  // scoped selection and expiry; this is unrelated to presentation Level2.5.
  policy.version = 2;
  policy.live_updates = { poll_interval_ms: 25, recovery_timeout_ms: 2000 };
  fixture.setOutputPolicy(JSON.stringify(policy));
  for (const channel of fixture.channels) fixture.database.prepare('UPDATE "Channel" SET "Name"=? WHERE "Id"=?').run(`Synthetic restart Lua ${channel}`, channel);
  const prefix = testInfo.outputPath('scope');
  copyFileSync(join(fixture.root, 'synthetic-client-source.ts'), `${prefix}.source.ts`);
  let context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
  let page = await context.newPage();
  const errors: string[] = [];
  const watch = (): void => {
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  };
  watch();
  const restarts: unknown[] = [];
  const restart = async (stage: string, boundary?: number): Promise<void> => {
    if (boundary !== undefined) expect(Date.now()).toBeLessThan(boundary);
    const before = scopeAuthority(fixture) as { safety: ReturnType<typeof restartSafety> };
    const calls = fixture.calls();
    await context.close();
    const result = await isolatedWebHost.restart();
    expect(result.stop.closed).toBe(true); expect(result.stop.forced).toBe(false);
    expect(result.stop.exitCode).toBe(0); expect(result.stop.signalCode).toBeNull();
    expect(result.after.pid).not.toBe(result.before.pid);
    const after = scopeAuthority(fixture) as { safety: ReturnType<typeof restartSafety> };
    expect(after.safety.workerEpoch).not.toBe(before.safety.workerEpoch);
    expect({ ...after, safety: after.safety.snapshot }).toEqual({ ...before, safety: before.safety.snapshot });
    expect(fixture.calls()).toEqual(calls);
    if (boundary !== undefined) expect(Date.now()).toBeLessThan(boundary);
    restarts.push({ stage, ...result, observed_utc: new Date().toISOString(), boundary });
    writeFileSync(`${prefix}.restarts.json`, JSON.stringify(restarts, null, 2));
    context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage(); watch();
  };
  const publish = async (): Promise<void> => {
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('en_us')('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('en_us')('TeletextPublicationPublish'), true);
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
  };
  try {
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const day = await createProfile(page, fixture, 'day', true);
    const night = await createProfile(page, fixture, 'night', true);
    const bases: string[] = [];
    for (const [index, theme, profile] of [[0, 'day', day], [1, 'night', night]] as const) {
      await queueScripted(page, fixture, fixture.channels[index], 'reset', undefined, 80);
      expect(fixture.scriptedIntervals(fixture.channels[index])).toEqual([]);
      await page.goto(`/channels/${fixture.channels[index]}/teletext`); await ready(page, true);
      await bind(page, fixture, profile, true);
      bases.push(await prepare(page, fixture, `Scoped restart base ${theme}`, true)); await publish();
    }
    await queueScripted(page, fixture, fixture.channels[0], 'reset', night, 80);
    const enabled = fixture.scriptedIntervals(fixture.channels[0]);
    expect(enabled).toHaveLength(1); expect(enabled[0].State).toBe(1); expect(enabled[0].ProfileId).toBe(night);
    const onFinish = scopeUtc(enabled[0].Finish); expect(Number.isFinite(onFinish)).toBe(true);
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const scopedOn = await prepare(page, fixture, 'Scoped restart Lua Night', true, true); await publish();
    await raster(page, 'night', '200');
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.on.before.ts`);
    await restart('inside-on', onFinish);
    await page.goto(scopedOn); await ready(page, true); await raster(page, 'night', '200');
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
    await page.screenshot({ path: testInfo.outputPath('on-after-restart.png') });
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.on.after.ts`);
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[1], `${prefix}.isolated.night.ts`);
    await queueScripted(page, fixture, fixture.channels[0], 'reset', null, 6);
    const off = fixture.scriptedIntervals(fixture.channels[0]);
    expect(off).toHaveLength(1); expect(off[0].State).toBe(0); expect(off[0].ProfileId).toBeNull();
    // The actual fresh Continue build supplies future AV with no teletext control.
    await queueScripted(page, fixture, fixture.channels[0], 'continue', undefined, 80);
    expect(fixture.scriptedIntervals(fixture.channels[0])).toEqual(off);
    const finish = scopeUtc(off[0].Finish); expect(Number.isFinite(finish)).toBe(true);
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const form = page.getByTestId('teletext-effective-authoring-form');
    await expect(form.locator('[name="scope"]')).toHaveValue('effective');
    await form.locator('[name="name"]').fill('Scoped restart Lua Off');
    await submit(page, form, text('en_us')('TeletextValidateSaveDraft'), true);
    const scopedOff = page.url(); await publish();
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.off.before.ts`, false);
    await restart('inside-off', finish);
    await page.goto(scopedOff); await ready(page, true);
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
    await expect(page.getByTestId('teletext-preview-model').locator('img')).toHaveCount(0);
    await page.screenshot({ path: testInfo.outputPath('off-after-restart.png') });
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.off.after.ts`, false);
    expect(Date.now()).toBeLessThan(finish);
    // Use the persisted real UTC boundary. No host/application clock is supplied.
    await expect.poll(() => Date.now() > finish, { timeout: 85_000 }).toBe(true);
    await restart('after-off-expiry');
    await page.goto(scopedOff); await ready(page, true);
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unavailable');
    await page.goto(bases[0]); await ready(page, true); await raster(page, 'day', '200');
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.expired.day.ts`);
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[1], `${prefix}.expired.night.ts`);
    expect(fixture.calls()).toEqual(['day', 'night', 'night']);
    expect(fixture.unchangedPackage()).toBe(true); expect(errors).toEqual([]);
    writeFileSync(`${prefix}.success.json`, JSON.stringify({ restarts, scopes: { enabled, off }, calls: fixture.calls(), browser: browser.version(),
      scope: 'Headed EN/JS/light real-host scoped On/Off/expiry, finite ordinary responses. No open-client shutdown, all-locales/platform/complete base or9A acceptance.' }, null, 2));
  } finally { try { await context.close(); } finally { fixture.close(); } }
});

// Future On and post-On expiry are distinct from the retained insideOn/Off trial.
// Real Lua builds own all intervals and UTC boundaries; no table/clock mutation.
test('standalone Level 1.5 future On survives host replacement and expires to base', async ({ browser, isolatedWebHost }, testInfo) => {
  test.skip(process.platform === 'win32', 'Intentional POSIX synthetic package; native Windows remains separate.');
  test.skip(testInfo.project.name !== 'chromium', 'Existing composed browser/localization evidence remains scoped.');
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true, standaloneLevel15: true, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
  fixture.configureScripted(); fixture.setAuthoringPolicy();
  const policy = JSON.parse(readFileSync(fileURLToPath(new URL('../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl)), 'utf8'));
  policy.version = 2; policy.live_updates = { poll_interval_ms: 25, recovery_timeout_ms: 2000 };
  fixture.setOutputPolicy(JSON.stringify(policy));
  for (const channel of fixture.channels) fixture.database.prepare('UPDATE "Channel" SET "Name"=? WHERE "Id"=?').run(`Synthetic future Lua ${channel}`, channel);
  const prefix = testInfo.outputPath('future');
  copyFileSync(join(fixture.root, 'synthetic-client-source.ts'), `${prefix}.source.ts`);
  let context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
  let page = await context.newPage();
  const errors: string[] = [];
  const watch = (): void => {
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  };
  watch();
  const restarts: unknown[] = [];
  const restart = async (stage: string, beforeBoundary?: number): Promise<void> => {
    if (beforeBoundary !== undefined) expect(Date.now()).toBeLessThan(beforeBoundary);
    const before = scopeAuthority(fixture) as { safety: ReturnType<typeof restartSafety> };
    const calls = fixture.calls(); await context.close();
    const result = await isolatedWebHost.restart();
    expect(result.stop).toMatchObject({ closed: true, forced: false, exitCode: 0, signalCode: null });
    expect(result.after.pid).not.toBe(result.before.pid);
    const after = scopeAuthority(fixture) as { safety: ReturnType<typeof restartSafety> };
    expect(after.safety.workerEpoch).not.toBe(before.safety.workerEpoch);
    expect({ ...after, safety: after.safety.snapshot }).toEqual({ ...before, safety: before.safety.snapshot });
    expect(fixture.calls()).toEqual(calls);
    if (beforeBoundary !== undefined) expect(Date.now()).toBeLessThan(beforeBoundary);
    restarts.push({ stage, ...result, observed_utc: new Date().toISOString(), beforeBoundary });
    writeFileSync(`${prefix}.restarts.json`, JSON.stringify(restarts, null, 2));
    context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage(); watch();
  };
  const publish = async (): Promise<void> => {
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('en_us')('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('en_us')('TeletextPublicationPublish'), true);
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
  };
  try {
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const day = await createProfile(page, fixture, 'day', true);
    const night = await createProfile(page, fixture, 'night', true);
    const bases: string[] = [];
    for (const [index, theme, profile] of [[0, 'day', day], [1, 'night', night]] as const) {
      await queueScripted(page, fixture, fixture.channels[index], 'reset', undefined, 80);
      await page.goto(`/channels/${fixture.channels[index]}/teletext`); await ready(page, true);
      await bind(page, fixture, profile, true);
      bases.push(await prepare(page, fixture, `Future scope base ${theme}`, true)); await publish();
    }
    await queueScripted(page, fixture, fixture.channels[0], 'reset', night, 6, 6);
    const on = fixture.scriptedIntervals(fixture.channels[0]);
    expect(on).toHaveLength(1); expect(on[0].State).toBe(1); expect(on[0].ProfileId).toBe(night);
    const start = scopeUtc(on[0].Start); const finish = scopeUtc(on[0].Finish);
    expect(start).toBeGreaterThan(Date.now()); expect(finish - start).toBe(72_000);
    await queueScripted(page, fixture, fixture.channels[0], 'continue', undefined, 80);
    expect(fixture.scriptedIntervals(fixture.channels[0])).toEqual(on);
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.future.before.day.ts`);
    await restart('before-future-on', start);
    await page.goto(bases[0]); await ready(page, true); await raster(page, 'day', '200');
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
    await page.screenshot({ path: testInfo.outputPath('future-before-on.png') });
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.future.after.day.ts`);
    expect(Date.now()).toBeLessThan(start);
    await expect.poll(() => Date.now() >= start, { timeout: 85_000 }).toBe(true);
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const scoped = await prepare(page, fixture, 'Future Lua Night now active', true, true); await publish();
    await raster(page, 'night', '200');
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.active.night.ts`);
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[1], `${prefix}.isolated.night.ts`);
    expect(Date.now()).toBeLessThan(finish);
    await expect.poll(() => Date.now() > finish, { timeout: 85_000 }).toBe(true);
    await restart('after-on-expiry');
    await page.goto(scoped); await ready(page, true);
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unavailable');
    await page.goto(bases[0]); await ready(page, true); await raster(page, 'day', '200');
    await page.screenshot({ path: testInfo.outputPath('future-after-on-expiry.png') });
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[0], `${prefix}.expired.day.ts`);
    await captureScheduled(fixture, isolatedWebHost.baseURL, fixture.channels[1], `${prefix}.expired.night.ts`);
    expect(fixture.scriptedIntervals(fixture.channels[0])).toEqual(on);
    expect(fixture.calls()).toEqual(['day', 'night', 'night']);
    expect(fixture.unchangedPackage()).toBe(true); expect(errors).toEqual([]);
    writeFileSync(`${prefix}.success.json`, JSON.stringify({ restarts, scopes: { on }, calls: fixture.calls(), browser: browser.version(),
      scope: 'Headed EN/JS/light genuine futureOn and postOnexpiry host recovery with finite ordinary captures. No implicit future publication, open-client shutdown, complete lifecycle/base/full9A or platform claim.' }, null, 2));
  } finally { try { await context.close(); } finally { fixture.close(); } }
});
