import { expect, Locator, Page } from '@playwright/test';
import { copyFileSync, existsSync, readFileSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { basename, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { test } from './isolated-web-host';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs, observeTeletextUntilStop } from './helpers/teletext-stream-capture';

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
async function saveWrapperChoice(page: Page, selected: string, module: string, js: boolean): Promise<void> {
  await page.goto('/settings/ffmpeg'); await ready(page, js);
  const form = page.locator('form[action="/admin-api/settings/ffmpeg/save"]');
  const choice = form.locator('select[name="default_mpeg_ts_script"]');
  await expect(choice.locator('option[value=""]')).toHaveText(text(module)('BuiltInMpegTsWrapper'));
  await choice.selectOption(selected);
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname === '/admin-api/settings/ffmpeg/save');
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: text(module)('SaveSettings'), exact: true }).click();
  expect((await posted).status()).toBe(303); await navigated; await ready(page, js);
  await expect(choice).toHaveValue(selected);
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
for (const standalone of [false, true]) for (const [language, module, js, dark] of [['en-us', 'en_us', true, false], ['nl', 'nl', false, true]] as const) for (const openClients of standalone ? [false, true] : [false]) {
  test(`teletext ${standalone ? 'standalone Level 1.5 ' : ''}real restart flow (${language}, JavaScript ${js}, dark ${dark}${openClients ? ', open admin clients' : ''})`, async ({ browser, isolatedWebHost }, testInfo) => {
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
      // Qualify the retained open-client stall separately from finite-client
      // durability. Captures have reached genuine EOF in both cases; active
      // viewer shutdown remains a separate acceptance requirement.
      testInfo.annotations.push({ type: 'restart-admin-clients', description: openClients ? 'remain open' : 'closed before signal' });
      if (!openClients) await context.close();
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
      if (openClients) {
        expect(context.pages()).toEqual(pages);
        for (const page of pages) expect(page.isClosed()).toBe(false);
      } else {
        context = await browser.newContext(contextOptions);
        pages = [await context.newPage(), await context.newPage()];
        watchErrors();
      }
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

// Intentional Linux process-identity qualification; native six-RID acceptance
// remains separate. Only owned descendants' command lines are inspected.
function ownedViewerProcesses(parent: number, source: string): { pid: number; start: string; command: string; readsSource: boolean; preparationWorker: boolean; nextController: boolean }[] {
  const rows = execFileSync('ps', ['-eo', 'pid=,ppid=,comm='], { encoding: 'utf8', timeout: 3000, maxBuffer: 1024 * 1024 })
    .trim().split('\n').map(line => {
      const match = line.match(/^\s*(\d+)\s+(\d+)\s+(.+)$/);
      if (!match) throw new Error('Invalid bounded process identity output.');
      return { pid: Number(match[1]), parent: Number(match[2]), command: match[3] };
    });
  const owned = new Set([parent]);
  for (let changed = true; changed;) {
    changed = false;
    for (const row of rows) if (owned.has(row.parent) && !owned.has(row.pid)) { owned.add(row.pid); changed = true; }
  }
  return rows.filter(row => row.pid !== parent && owned.has(row.pid)).flatMap(row => {
    try {
      const stat = readFileSync(`/proc/${row.pid}/stat`, 'utf8');
      const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
      const command = readFileSync(`/proc/${row.pid}/cmdline`, 'utf8');
      return [{ pid: row.pid, start: fields[19], command: row.command, readsSource: command.includes(source), preparationWorker: command.split('\0').includes('--internal-addon-preparation-worker'), nextController: basename(command.split('\0')[0]) === 'ersatztv-channel' }];
    } catch (error) { if (!existsSync(`/proc/${row.pid}`)) return []; throw error; }
  });
}
function sameProcessAlive(process: { pid: number; start: string }): boolean {
  try {
    const stat = readFileSync(`/proc/${process.pid}/stat`, 'utf8');
    return stat.slice(stat.lastIndexOf(')') + 2).split(' ')[19] === process.start;
  } catch (error) { if (!existsSync(`/proc/${process.pid}`)) return false; throw error; }
}

for (const mode of ['Legacy', 'Hybrid', 'Next'] as const) {
test(`standalone Level 1.5 active ${mode} viewers release source processes and leases on shutdown`, async ({ browser, isolatedWebHost }, testInfo) => {
  test.skip(process.platform !== 'linux', 'Intentional Linux /proc identity proof; native platform acceptance remains mandatory.');
  test.skip(testInfo.project.name !== 'chromium', 'Reuse existing localized/browser proof for unchanged UI.');
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.skip(mode === 'Next' && !process.env.ETV_NEXT_FOLDER, 'Requires an explicitly pinned Next runtime; no download/adoption claim.');
  test.setTimeout(150_000);
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true, standaloneLevel15: true, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
  fixture.setAuthoringPolicy(); fixture.setOutputPolicy();
  if (mode === 'Next') {
    fixture.configureScripted();
    fixture.database.prepare('UPDATE "MediaVersion" SET "Duration"=\'00:01:00\' WHERE "Id"=?').run(fixture.channels[0] + 10);
    // Test-owned initial channel/wrapper setup; localized settings authority is
    // already qualified separately. Do not infer opaque-script equivalence.
    fixture.database.prepare('UPDATE "Channel" SET "StreamingEngine"=1 WHERE "Id"=?').run(fixture.channels[0]);
    fixture.database.prepare('INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?) ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value"')
      .run('ffmpeg.default_mpegts_script', '');
  }
  // Hybrid uses the qualified ordinary settings form. Next uses explicit
  // initial fixture configuration; opaque scripts retain their own contract.
  const configuredScript = fixture.setting('ffmpeg.default_mpegts_script');
  // Source duration exceeds the30s observer budget, but finite duration alone
  // cannot prove a live worker: process identity/liveness is asserted below.
  const original = join(fixture.root, 'synthetic-client-source.ts');
  const extended = join(fixture.root, 'synthetic-client-source-long.ts');
  execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
    '-stream_loop', '4', '-i', original, '-t', '60', '-map', '0:v:0', '-map', '0:a:0',
    '-c', 'copy', '-muxrate', '4953600', '-pcr_period', '20', '-mpegts_start_pid', '256',
    '-mpegts_pmt_start_pid', '4096', '-f', 'mpegts', extended], { timeout: 30_000, maxBuffer: 65_536 });
  renameSync(extended, original);
  const source = JSON.parse(execFileSync(process.env.FFPROBE_PATH!, ['-v', 'error', '-show_entries',
    'format=duration', '-of', 'json', original], { encoding: 'utf8', timeout: 10_000, maxBuffer: 65_536 }));
  expect(Number(source.format.duration)).toBeGreaterThan(59);
  const sourceBytes = statSync(original).size;
  if (mode === 'Next') copyFileSync(original, testInfo.outputPath('next-source.ts'));
  const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  let stopping = false;
  const viewers: ReturnType<typeof observeTeletextUntilStop>[] = [];
  const owned = new Map<number, ReturnType<typeof ownedViewerProcesses>[number]>();
  const capacity = async (active: number): Promise<void> => {
    await expect.poll(async () => {
      const html = await (await page.request.get('/settings/ffmpeg')).text();
      return html.includes(`${text('en_us')('StreamsInUse')}: ${active} / 2 (`);
    }, { timeout: 10_000 }).toBe(true);
  };
  try {
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    if (mode === 'Hybrid') {
      await saveWrapperChoice(page, '', 'en_us', true);
      expect(fixture.setting('ffmpeg.default_mpegts_script')).toBe('');
    }
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const profiles = [await createProfile(page, fixture, 'day', true), await createProfile(page, fixture, 'night', true)];
    for (const [index, profile] of profiles.entries()) {
      await page.goto(`/channels/${fixture.channels[index]}/teletext`); await ready(page, true);
      await bind(page, fixture, profile, true);
      await prepare(page, fixture, `Active viewer ${index}`, true);
      await submit(page, page.getByTestId('teletext-publication-request-form'), text('en_us')('TeletextPublicationRequest'), true);
      await submit(page, page.getByTestId('teletext-publication-publish-form'), text('en_us')('TeletextPublicationPublish'), true);
    }
    if (mode === 'Next') {
      // External JSON adaptation is intentionally native-only. Reuse the
      // existing Local/Scripted fixture and real Reset for Next; no generated
      // playout/snapshot authority is seeded or rewritten.
      await queueScripted(page, fixture, fixture.channels[0], 'reset', undefined, 1);
      const programme = fixture.database.prepare('SELECT "Finish" FROM "PlayoutItem" WHERE "PlayoutId"=?')
        .get(fixture.channels[0]) as { Finish: string };
      expect(scopeUtc(programme.Finish) - Date.now()).toBeGreaterThan(30_000);
    }
    const retained = scopeAuthority(fixture) as { safety: ReturnType<typeof restartSafety> };
    if (mode !== 'Next') fixture.resetSourceStart();
    const host = isolatedWebHost.processIdentity();
    const resident = ownedViewerProcesses(host.pid, join(fixture.root, 'synthetic-client-source.ts'));
    expect(resident.length).toBeGreaterThan(0);
    expect(resident.every(process => process.preparationWorker)).toBe(true);
    for (const process of resident) owned.set(process.pid, process);
    writeFileSync(testInfo.outputPath('host-resident-processes.json'), JSON.stringify(resident, null, 2));
    const isResident = (process: { pid: number; start: string }): boolean => resident.some(original => original.pid === process.pid && original.start === process.start);
    const openViewer = async (): Promise<ReturnType<typeof observeTeletextUntilStop>> => {
      const route = mode === 'Legacy' ? 'ts-legacy' : 'ts';
      const viewer = observeTeletextUntilStop(`${isolatedWebHost.baseURL}/iptv/channel/${fixture.channels[0]}.ts?mode=${route}`, () => stopping);
      viewers.push(viewer); await viewer.ready; await capacity(2);
      const processes = ownedViewerProcesses(host.pid, join(fixture.root, 'synthetic-client-source.ts'));
      writeFileSync(testInfo.outputPath(`active-${viewers.length}.processes.json`), JSON.stringify(processes, null, 2));
      expect(processes.some(process => process.readsSource)).toBe(true);
      for (const process of processes) owned.set(process.pid, process);
      const prefix = viewer.prefix(); const pids = new Set<number>();
      for (let offset = 0; offset < prefix.length; offset += 188) {
        expect(prefix[offset]).toBe(0x47);
        pids.add(((prefix[offset + 1] & 0x1f) << 8) | prefix[offset + 2]);
      }
      const prefixPath = testInfo.outputPath(`active-${viewers.length}.prefix.ts`);
      writeFileSync(prefixPath, prefix);
      if (mode === 'Next') {
        // Inspect the actual selected output track, including filtered Next
        // audio. A correct waveform alone cannot prove its language identity.
        const probe = JSON.parse(execFileSync(process.env.FFPROBE_PATH!, ['-v', 'error', '-select_streams', 'a:0',
          '-show_entries', 'stream_tags=language', '-of', 'json', prefixPath], { encoding: 'utf8', timeout: 10_000, maxBuffer: 65_536 }));
        expect(probe.streams).toHaveLength(1);
        expect(probe.streams[0].tags?.language).toBe('eng');
        writeFileSync(testInfo.outputPath(`active-${viewers.length}.audio-metadata.json`), JSON.stringify(probe, null, 2));
      }
      for (const pid of [0, 256, 257, 512]) {
        expect(pids.has(pid), `Missing PID${pid}; observed ${[...pids].sort((a, b) => a - b)}`).toBe(true);
      }
      if (mode !== 'Legacy') {
        // Same bounded decode as the existing finite capture helper. Full source
        // clock/caption/selected-audio gates remain reused in their exact scope.
        execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', prefixPath,
          '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
      }
      return viewer;
    };
    // Default2 owns a wrapper+encoder pair. Hybrid shares the HLS encoder;
    // viewer disconnect must release its wrapper, not cancel another owner.
    // The existing StopOnDisconnect idle deadline remains unchanged.
    const first = await openViewer();
    const firstClosed = await first.close(); expect(firstClosed.ended).toBe('client-close');
    await capacity(mode === 'Legacy' ? 0 : 1);
    const retainedSource = [...owned.values()].filter(process => !isResident(process)).filter(sameProcessAlive);
    if (mode === 'Legacy') expect(retainedSource).toEqual([]);
    else {
      expect(retainedSource.filter(process => process.readsSource)).toHaveLength(1);
      expect(retainedSource.filter(process => process.nextController)).toHaveLength(mode === 'Next' ? 1 : 0);
      expect(retainedSource).toHaveLength(mode === 'Next' ? 2 : 1);
    }
    expect(resident.every(sameProcessAlive)).toBe(true);
    // Keep this active-reader cancellation proof separate from longer
    // transport captures, which may legitimately outlive a finite reader.
    // Shared engines reuse the source: never move its programme clock in SQLite.
    if (mode === 'Legacy') fixture.resetSourceStart();
    const active = await openViewer();
    if (mode !== 'Legacy') expect(retainedSource.every(sameProcessAlive)).toBe(true);
    stopping = true;
    const result = await isolatedWebHost.restart();
    const day = await active.closed;
    expect(day.bytes).toBeGreaterThanOrEqual(active.prefix().length);
    expect(day.bytes).toBeLessThan(sourceBytes / 2);
    expect(result.stop).toMatchObject({ closed: true, forced: false, exitCode: 0, signalCode: null });
    await expect.poll(() => [...owned.values()].filter(sameProcessAlive), { timeout: 5000 }).toEqual([]);
    await capacity(0);
    const recovered = scopeAuthority(fixture) as { safety: ReturnType<typeof restartSafety> };
    expect(recovered.safety.workerEpoch).not.toBe(retained.safety.workerEpoch);
    expect({ ...recovered, safety: recovered.safety.snapshot }).toEqual({ ...retained, safety: retained.safety.snapshot });
    expect(fixture.calls()).toEqual(['day', 'night']); expect(fixture.unchangedPackage()).toBe(true);
    if (mode === 'Hybrid') {
      await page.goto('/settings/ffmpeg'); await ready(page, true);
      await expect(page.locator('select[name="default_mpeg_ts_script"]')).toHaveValue('');
    }
    writeFileSync(testInfo.outputPath('active-viewer-success.json'), JSON.stringify({ result, viewers: [firstClosed, day], processes: [...owned.values()], resident, source: { duration: source.format.duration, bytes: sourceBytes },
      mode, nextRuntimeFolderExplicit: mode === 'Next', retainedSource, configuredScript, effectiveScript: fixture.setting('ffmpeg.default_mpegts_script'),
      scope: `Linux ordinary ${mode} at default2: live2, caller disconnect${mode === 'Legacy' ? 0 : 1}, reopened2, host restart0; known source/process identities disappear. Hybrid/Next retain and reuse their shared HLS encoder; Next also retains its distinct controller until host shutdown. Idle expiry is separate. No complete AV/transport/decoder/native-platform/9A acceptance; unchanged normal-stream evidence is reused.` }, null, 2));
  } finally {
    stopping = true;
    await Promise.allSettled(viewers.map(viewer => viewer.close()));
    await context.close(); fixture.close();
  }
});
}

for (const variant of [{ module: 'en_us', language: 'en-us', js: true, dark: false },
  { module: 'nl', language: 'nl', js: false, dark: true }]) {
  test(`built-in MPEG-TS choice persists through localized settings and host restart (${variant.language}/${variant.js ? 'JS' : 'no-JS'})`,
    async ({ browser, browserName, isolatedWebHost }, testInfo) => {
      test.skip(process.platform === 'win32', 'Intentional Unix graceful-host restart; native Windows launcher acceptance remains separate.');
      test.setTimeout(180_000);
      const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL,
        javaScriptEnabled: variant.js, viewport: { width: 1280, height: 900 } });
      const page = await context.newPage(); const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error' && /hydrate|marker|wasm|checksum/i.test(message.text())) errors.push(message.text()); });
      const db = new DatabaseSync(isolatedWebHost.databasePath, { readOnly: true });
      const saved = (): string | undefined => (db.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?')
        .get('ffmpeg.default_mpegts_script') as { Value: string } | undefined)?.Value;
      // Settings Save may materialize absent defaults; compare effective values.
      const preferences = () => Object.fromEntries(Object.entries({
        'ffmpeg.preferred_language_code': 'eng', 'ffmpeg.preferred_subtitle_language_code': '',
        'ffmpeg.fallback_subtitle_language_code': '', 'ffmpeg.active_transcode_limit': '2',
      }).map(([key, fallback]) => [key, (db.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?')
        .get(key) as { Value: string } | undefined)?.Value ?? fallback]));
      const setLanguage = async (module: string, language: string, dark: boolean) => {
        expect((await page.request.post('/admin-api/settings/ui/save', { form: { language, is_dark_mode: String(dark) }, maxRedirects: 0 })).status()).toBe(303);
        await page.goto('/settings/ffmpeg'); await ready(page, variant.js);
        await expect(page.locator('select[name="default_mpeg_ts_script"] option[value=""]')).toHaveText(text(module)('BuiltInMpegTsWrapper'));
        await expect(page.getByText(text(module)('DefaultMpegTsScriptHelp'), { exact: true })).toBeVisible();
      };
      try {
        const before = preferences();
        if (variant.js && browserName === 'chromium') {
          for (const [index, module] of ['en_us', 'nl', 'de_de', 'fr_fr', 'it_it', 'pl', 'pt_br', 'es_es', 'es_419'].entries()) {
            await setLanguage(module, module.replace('_', '-'), index % 2 === 1);
            const choice = page.locator('select[name="default_mpeg_ts_script"]');
            await choice.selectOption(''); await choice.scrollIntoViewIfNeeded();
            await page.screenshot({ path: testInfo.outputPath(`native-wrapper-${module}.png`) });
          }
        }
        await setLanguage(variant.module, variant.language, variant.dark);
        await saveWrapperChoice(page, '', variant.module, variant.js); expect(saved()).toBe('');
        await page.reload(); await ready(page, variant.js);
        await expect(page.locator('select[name="default_mpeg_ts_script"]')).toHaveValue('');
        await page.locator('select[name="default_mpeg_ts_script"]').scrollIntoViewIfNeeded();
        await page.screenshot({ path: testInfo.outputPath('native-wrapper-saved.png') });
        const restarted = await isolatedWebHost.restart();
        expect(restarted.stop).toMatchObject({ closed: true, forced: false, exitCode: 0, signalCode: null });
        await page.reload(); await ready(page, variant.js);
        await expect(page.locator('select[name="default_mpeg_ts_script"]')).toHaveValue('');
        expect(saved()).toBe(''); expect(preferences()).toEqual(before);
        await saveWrapperChoice(page, 'default', variant.module, variant.js); expect(saved()).toBe('default');
        await saveWrapperChoice(page, '', variant.module, variant.js); expect(saved()).toBe('');
        expect(preferences()).toEqual(before); expect(errors).toEqual([]);
        writeFileSync(testInfo.outputPath('native-wrapper-settings-success.json'), JSON.stringify({
          language: variant.language, javaScript: variant.js, browser: browserName, restarted,
          native_after_save_reload_restart: true, default_can_be_reselected: true,
          preferences_unchanged: true, errors, scope: 'Localized admin settings only; ordinary Hybrid and complete base/9A acceptance remain separate.' }, null, 2));
      } finally { db.close(); await context.close(); }
    });
}
