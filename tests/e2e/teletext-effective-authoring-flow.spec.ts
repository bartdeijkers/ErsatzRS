import { expect, Locator, Page, test } from '@playwright/test';
import { constants, copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs } from './helpers/teletext-stream-capture';

// Actual served-host Reset -> Lua -> application publication creates every
// scoped interval. This spec never seeds interval/checkpoint/publication rows.
const databasePath = process.env.E2E_DB_PATH;
const specUrl = pathToFileURL(__filename);
const cultures = [
  ['en-us', 'en_us'], ['nl', 'nl'], ['pl', 'pl'], ['es-es', 'es_es'],
  ['es-419', 'es_419'], ['fr-fr', 'fr_fr'], ['de-de', 'de_de'],
  ['it-it', 'it_it'], ['pt-br', 'pt_br'],
] as const;
function translations(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl,
  )), 'utf8');
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
async function language(page: Page, culture: string, dark: boolean): Promise<void> {
  expect((await page.request.post('/admin-api/settings/ui/save', {
    form: { language: culture, is_dark_mode: String(dark) }, maxRedirects: 0,
  })).status()).toBe(303);
}
async function submit(page: Page, form: Locator, key: string, js: boolean): Promise<void> {
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/teletext/'));
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: translations('en_us')(key), exact: true }).click();
  expect((await posted).status()).toBe(303);
  await navigated; await ready(page, js);
}
async function profile(page: Page, fixture: TeletextChannelFixture, theme: string, js: boolean): Promise<number> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  await form.locator('[name="name"]').fill(`Synthetic effective ${theme}`);
  await form.locator('[data-setting-key="THEME"] select[name$=".mode"]').selectOption('literal');
  await form.locator('[data-setting-key="THEME"] [name$=".value"]').selectOption(theme);
  await submit(page, form, 'TeletextProfileCreate', js);
  const saved = fixture.profiles().find(row => row.Name === `Synthetic effective ${theme}`);
  expect(saved).toBeDefined(); return saved!.Id;
}
async function baseOff(page: Page, fixture: TeletextChannelFixture, channel: number, id: number, js: boolean): Promise<void> {
  await page.goto(`/channels/${channel}/teletext`); await ready(page, js);
  const addon = page.locator(`[data-addon-id="${fixture.addonId}"]`);
  const form = addon.getByTestId('teletext-binding-form');
  const revision = fixture.profiles().find(row => row.Id === id)!.Revision;
  await form.locator('[name="profile_choice"]').selectOption(`${id}:${revision}`);
  await submit(page, form, 'TeletextBindingSave', js);
  await submit(page, addon.getByTestId('teletext-disable-form'), 'TeletextBindingOff', js);
  expect(fixture.binding(channel)?.Enabled).toBe(0);
}
async function resetScript(page: Page, fixture: TeletextChannelFixture, channel: number, id: number | null, count: number): Promise<void> {
  fixture.writeScriptedControls(channel, id, count);
  const response = await page.request.post('/admin-api/rebuild_playout', {
    form: { id: String(channel), mode: 'reset', preserve_current: 'false' }, maxRedirects: 0,
  });
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body).toEqual({ accepted: 1, coalesced: 0 });
  await expect.poll(() => {
    const rows = fixture.scriptedIntervals(channel);
    if (rows.length !== 1 || rows[0].State !== Number(id !== null) || rows[0].ProfileId !== id) return false;
    const status = fixture.database.prepare('SELECT "Success" FROM "PlayoutBuildStatus" WHERE "PlayoutId"=?').get(channel) as { Success: number } | undefined;
    const items = fixture.database.prepare('SELECT COUNT(*) AS count FROM "PlayoutItem" WHERE "PlayoutId"=?').get(channel) as { count: number };
    return status?.Success === 1 && items.count === count;
  }, { timeout: 45_000 }).toBe(true);
}
async function prepare(page: Page, fixture: TeletextChannelFixture, theme: string, js: boolean, absent = false): Promise<string> {
  const form = page.getByTestId('teletext-effective-authoring-form');
  await expect(form.locator('[name="selection_digest"]')).toHaveValue(/^[a-f0-9]{64}$/);
  await expect(form.locator('[name="scope"]')).toHaveValue('effective');
  await expect(form.getByRole('button')).toBeEnabled();
  await form.locator('[name="name"]').fill(`Synthetic scoped original ${theme}`);
  if (absent) {
    for (const field of ['entry_addon_id', 'entry_page', 'entry_subcode', 'v1_region', 'resource_transmissions']) {
      await expect(form.locator(`[name="${field}"]`)).toHaveCount(0);
    }
  } else {
    await form.locator('[name="entry_addon_id"]').selectOption(fixture.addonId);
    await form.locator('[name="entry_page"]').fill('200');
    await form.locator('[name="entry_subcode"]').fill('0000');
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="resource_transmissions"]').fill('[]');
  }
  await submit(page, form, 'TeletextValidateSaveDraft', js);
  await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
  return page.url();
}
async function assertPreview(page: Page, theme: 'day' | 'night', address = '200'): Promise<void> {
  const model = page.getByTestId('teletext-preview-model');
  await expect(model).toHaveAttribute('data-scope', 'effective');
  await expect(model).toHaveAttribute('data-page', address);
  await expect(model).toHaveAttribute('data-subcode', '0000');
  const image = model.locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete
    && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  const colours = await image.evaluate(element => {
    const canvas = document.createElement('canvas'); canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!; context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, canvas.width, canvas.height);
    const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let red = 0; let blue = 0;
    for (let index = 0; index < data.length; index += 4) {
      if (data[index] === 255 && data[index + 1] === 0 && data[index + 2] === 0) red++;
      if (data[index] === 0 && data[index + 1] === 0 && data[index + 2] === 255) blue++;
    }
    return { red, blue };
  });
  expect(colours[theme === 'day' ? 'red' : 'blue']).toBeGreaterThan(0);
  expect(colours[theme === 'day' ? 'blue' : 'red']).toBe(0);
}
function publication(fixture: TeletextChannelFixture): unknown[][] {
  return ['TeletextPublicationRequest', 'TeletextEffectivePublicationRequestCurrent',
    'TeletextAcceptedGeneration', 'TeletextEffectiveAcceptedGenerationCurrent'].map(table =>
    fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels));
}
function safety(fixture: TeletextChannelFixture): unknown {
  const snapshot = fixture.safetySnapshot() as { configuration: unknown; channels: unknown; playouts: unknown; package: unknown };
  return { configuration: snapshot.configuration, channels: snapshot.channels, playouts: snapshot.playouts, package: snapshot.package,
    bindings: fixture.channels.map(channel => fixture.binding(channel)),
    intervals: fixture.channels.map(channel => fixture.scriptedIntervals(channel)),
    basePointers: ['TeletextPublicationRequestCurrent', 'TeletextAcceptedGenerationCurrent'].map(table =>
      fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels)),
  };
}
function foreign(fixture: TeletextChannelFixture): unknown {
  return {
    bindings: fixture.database.prepare('SELECT * FROM "ChannelTeletextBinding" WHERE "ChannelId" NOT IN (?,?) ORDER BY "ChannelId","AddonId"').all(...fixture.channels),
    profiles: fixture.database.prepare('SELECT * FROM "TeletextProfile" WHERE "AddonId"!=? ORDER BY "Id"').all(fixture.addonId),
    packages: fixture.database.prepare('SELECT * FROM "AddonInstallation" WHERE "AddonId"!=? ORDER BY "AddonId"').all(fixture.addonId),
    publications: ['TeletextPublicationRequest', 'TeletextAcceptedGeneration', 'TeletextEffectivePublicationRequestCurrent', 'TeletextEffectiveAcceptedGenerationCurrent'].map(table =>
      fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" NOT IN (?,?) ORDER BY 1`).all(...fixture.channels)),
  };
}
function utcMilliseconds(value: string): number {
  const normalized = value.replace(' ', 'T');
  const time = Date.parse(/[zZ]|[+-]\d\d:\d\d$/.test(normalized) ? normalized : `${normalized}Z`);
  if (!Number.isFinite(time)) throw new Error('Published synthetic programme has an invalid UTC boundary.');
  return time;
}
async function captureOrdinary(fixture: TeletextChannelFixture, baseURL: string, channel: number,
  output: string, authored: boolean): Promise<void> {
  if (existsSync(output)) throw new Error('Effective admin capture requires a new output file.');
  // Wait for this channel's next genuine published item, independently of the
  // other channel's clock. Never rewrite source/interval/programme timestamps.
  const start = fixture.database.prepare(`SELECT "Start" FROM "PlayoutItem"
    WHERE "PlayoutId"=? AND julianday("Start")>julianday(?) ORDER BY "Start" LIMIT 1`)
    .get(channel, new Date().toISOString()) as { Start: string } | undefined;
  expect(start).toBeDefined();
  const boundary = utcMilliseconds(start!.Start);
  expect(boundary - Date.now()).toBeLessThanOrEqual(12_000);
  await expect.poll(() => Date.now() >= boundary + 25, { timeout: 13_000, intervals: [25] }).toBe(true);
  const captured = await captureTeletextChannelTs(`${baseURL}/iptv/hls-direct/${channel}.ts`, output,
    { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
  expect(captured.ended).toBe('eof');
  const pids = new Map<number, number>();
  let malformed = 0;
  for (let offset = 0; offset < captured.bytes.length; offset += 188) {
    if (captured.bytes[offset] !== 0x47 || (captured.bytes[offset + 1] & 0x80) !== 0) malformed++;
    const pid = ((captured.bytes[offset + 1] & 0x1f) << 8) | captured.bytes[offset + 2];
    pids.set(pid, (pids.get(pid) ?? 0) + 1);
  }
  expect(malformed).toBe(0);
  for (const pid of [0, 256, 257]) expect(pids.get(pid) ?? 0).toBeGreaterThan(0);
  if (authored) expect(pids.get(512) ?? 0).toBeGreaterThan(0);
  else expect(pids.has(512)).toBe(false);
  execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
    '-i', output, '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
  { timeout: 30_000, maxBuffer: 65_536 });
  expect(fixture.calls()).toEqual(['day', 'night']);
  // Independent page decoding, TSDuck and headed client inspection use these
  // retained exact TCP captures; PID/FFmpeg checks alone do not prove rendering.
}

async function localized(page: Page, fixture: TeletextChannelFixture, url: string, theme: 'day' | 'night',
  state: 'unrequested' | 'pending' | 'selected', js: boolean, dark: boolean,
  testInfo: { outputPath(name: string): string }): Promise<void> {
  for (const [culture, module] of cultures) {
    await language(page, culture, dark); await page.goto(url); await ready(page, js);
    const text = translations(module);
    await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
    const active = page.getByTestId('teletext-effective-authoring');
    await expect(active).toHaveAttribute('data-state', 'available');
    await expect(active.getByRole('heading')).toHaveText(text('TeletextEffectiveAuthoringTitle'));
    await expect(active).toContainText(text('TeletextEffectiveAuthoringHelp'));
    await expect(active).toContainText(text('TeletextEffectiveScopeExpiry'));
    await expect(active.locator('[name="entry_addon_id"] option').filter({ hasText: 'Synthetic teletext themes' })).toHaveValue(fixture.addonId);
    await expect(page.getByTestId('teletext-authoring-form').getByRole('button')).toBeDisabled();
    await assertPreview(page, theme);
    const publicationView = page.getByTestId('teletext-publication');
    await expect(publicationView).toHaveAttribute('data-state', state);
    const key = { unrequested: 'TeletextPublicationUnrequested', pending: 'TeletextPublicationPending', selected: 'TeletextPublicationSelected' }[state];
    await expect(page.getByTestId('teletext-publication-status')).toHaveText(text(key));
    if (state === 'selected') {
      await expect(page.getByTestId('teletext-publication-publish-form').getByRole('button')).toBeDisabled();
      // Selection is not a fabricated sink acknowledgement.
      await expect(publicationView).not.toHaveAttribute('data-state', 'applied');
    }
    const editor = page.getByTestId('teletext-editor-controls');
    await expect(editor).toHaveAttribute('data-state', 'unavailable');
    await expect(editor).toContainText(text('TeletextEffectiveEditorUnavailable'));
    await expect(editor.locator('form')).toHaveCount(0);
    await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
    await assertPreview(page, theme, '201');
    if ((culture === 'en-us' || culture === 'nl') && state === 'selected') {
      const target = page.getByTestId('teletext-publication-status');
      await target.scrollIntoViewIfNeeded(); await expect(target).toBeInViewport();
      await page.screenshot({ path: testInfo.outputPath(`${culture}-${theme}-selected.png`), fullPage: false });
    }
  }
}
async function responsive(page: Page, testInfo: { outputPath(name: string): string }): Promise<void> {
  await page.setViewportSize({ width: 320, height: 900 });
  const form = page.getByTestId('teletext-effective-authoring-form');
  const name = form.locator('[name="name"]'); await name.focus(); await page.keyboard.press('Tab');
  const producer = form.locator('[name="entry_addon_id"]'); await expect(producer).toBeFocused();
  await expect.poll(() => producer.evaluate(element => Number.parseFloat(getComputedStyle(element).outlineWidth))).toBeGreaterThan(0);
  await producer.scrollIntoViewIfNeeded(); await expect(producer).toBeInViewport();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('effective-320-keyboard.png'), fullPage: false });
  await page.setViewportSize({ width: 640, height: 900 });
  await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
  await producer.scrollIntoViewIfNeeded(); await expect(producer).toBeInViewport();
  await expect.poll(() => page.evaluate(() => document.documentElement.getBoundingClientRect().right <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('effective-200-percent.png'), fullPage: false });
  await page.evaluate(() => { document.documentElement.style.zoom = ''; });
  await page.setViewportSize({ width: 1280, height: 900 });
}

test.describe.configure({ mode: 'serial' });
for (const js of [true, false]) for (const dark of [false, true]) {
  test(`effective Scripted authoring through real Lua publication (JavaScript ${js}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
    test.skip(!databasePath, 'Requires the runner disposable E2E_DB_PATH.');
    test.skip(process.platform === 'win32', 'Intentional POSIX synthetic producer; no Windows acceptance claim.');
    test.skip(!process.env.FFMPEG_PATH, 'Requires the explicitly selected FFmpeg 9 playback fixture.');
    test.setTimeout(480_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const fixture = new TeletextChannelFixture(databasePath!, { authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
    fixture.configureScripted(); fixture.setAuthoringPolicy();
    const capturePrefix = js && !dark && process.env.ERSATZRS_TELETEXT_EFFECTIVE_ADMIN_CAPTURE
      ? `${process.env.ERSATZRS_TELETEXT_EFFECTIVE_ADMIN_CAPTURE}.${testInfo.project.name}` : undefined;
    if (capturePrefix) {
      const policyFile = fileURLToPath(new URL('../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl));
      const policy = JSON.parse(readFileSync(policyFile, 'utf8'));
      policy.version = 2;
      policy.live_updates = { poll_interval_ms: 25, recovery_timeout_ms: 2000 };
      fixture.setOutputPolicy(JSON.stringify(policy));
      mkdirSync(dirname(capturePrefix), { recursive: true });
      copyFileSync(join(fixture.root, 'synthetic-client-source.ts'), `${capturePrefix}.source.ts`, constants.COPYFILE_EXCL);
    }
    const foreignBefore = foreign(fixture);
    const context = await browser.newContext({ baseURL, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
    const pages = [await context.newPage(), await context.newPage()];
    const errors: string[] = [];
    for (const page of pages) {
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    }
    try {
      await language(pages[0], 'en-us', dark);
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`); await ready(pages[0], js);
      const day = await profile(pages[0], fixture, 'day', js);
      const night = await profile(pages[0], fixture, 'night', js);
      for (const [index, id] of [[0, day], [1, night]] as const) {
        await baseOff(pages[index], fixture, fixture.channels[index], id, js);
        await resetScript(pages[index], fixture, fixture.channels[index], id, 80);
      }
      expect(fixture.calls()).toEqual([]);
      const safe = safety(fixture);
      const urls: string[] = [];
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) {
        await pages[index].goto(`/channels/${fixture.channels[index]}/teletext`); await ready(pages[index], js);
        urls.push(await prepare(pages[index], fixture, theme, js));
        await assertPreview(pages[index], theme);
      }
      expect(fixture.calls()).toEqual(['day', 'night']);
      expect(fixture.designs()).toHaveLength(2);
      expect(fixture.request('day')?.channels).toEqual([]);
      expect(fixture.request('night')?.channels).toEqual([]);
      expect(publication(fixture).map(rows => rows.length)).toEqual([0, 0, 0, 0]);
      const versions = fixture.database.prepare('SELECT "DocumentVersion" FROM "TeletextDesignRevision" WHERE "DesignId" IN (?,?) ORDER BY "DesignId"')
        .all(...fixture.designs().map(design => design.Id));
      expect(versions).toEqual([{ DocumentVersion: 2 }, { DocumentVersion: 2 }]);
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) await localized(pages[index], fixture, urls[index], theme, 'unrequested', js, dark, testInfo);
      expect(safety(fixture)).toEqual(safe);
      await language(pages[0], 'en-us', dark);
      for (const [index, page] of pages.entries()) {
        await page.goto(urls[index]); await ready(page, js);
        await submit(page, page.getByTestId('teletext-publication-request-form'), 'TeletextPublicationRequest', js);
        await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
      }
      expect(publication(fixture).map(rows => rows.length)).toEqual([2, 2, 0, 0]);
      if (capturePrefix) {
        const beforeCapture = publication(fixture);
        await Promise.all(fixture.channels.map((channel, index) =>
          captureOrdinary(fixture, baseURL!, channel, `${capturePrefix}.pending.${index === 0 ? 'day' : 'night'}.ts`, false)));
        expect(publication(fixture)).toEqual(beforeCapture);
        expect(safety(fixture)).toEqual(safe);
      }
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) await localized(pages[index], fixture, urls[index], theme, 'pending', js, dark, testInfo);
      await language(pages[0], 'en-us', dark);
      for (const [index, page] of pages.entries()) {
        await page.goto(urls[index]); await ready(page, js);
        await page.getByTestId('teletext-preview-model').locator('ul a').click(); await ready(page, js);
        await assertPreview(page, index === 0 ? 'day' : 'night', '201');
        await submit(page, page.getByTestId('teletext-publication-publish-form'), 'TeletextPublicationPublish', js);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-selected');
      }
      expect(publication(fixture).map(rows => rows.length)).toEqual([2, 2, 2, 2]);
      if (capturePrefix) {
        const beforeCapture = publication(fixture);
        await Promise.all(fixture.channels.map((channel, index) =>
          captureOrdinary(fixture, baseURL!, channel, `${capturePrefix}.selected.${index === 0 ? 'day' : 'night'}.ts`, true)));
        expect(publication(fixture)).toEqual(beforeCapture);
        expect(safety(fixture)).toEqual(safe);
      }
      for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) await localized(pages[index], fixture, urls[index], theme, 'selected', js, dark, testInfo);
      expect(safety(fixture)).toEqual(safe);
      expect(fixture.calls()).toEqual(['day', 'night']);
      await responsive(pages[0], testInfo);
      await language(pages[0], 'en-us', dark);
      await resetScript(pages[0], fixture, fixture.channels[0], null, capturePrefix ? 8 : 4);
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`); await ready(pages[0], js);
      await expect(pages[0].getByTestId('teletext-effective-authoring')).toHaveAttribute('data-state', 'absent');
      const absentUrl = await prepare(pages[0], fixture, 'absent', js, true);
      expect(new URL(absentUrl).searchParams.get('absence')).toBe('1');
      expect(new URL(absentUrl).searchParams.has('page')).toBe(false);
      if (capturePrefix) {
        const beforeRejection = publication(fixture);
        const fields = await pages[0].getByTestId('teletext-publication-publish-form').evaluate(element =>
          Object.fromEntries(new FormData(element as HTMLFormElement).entries()) as Record<string, string>);
        const rejected = await pages[0].request.post('/admin-api/teletext/design/publish', {
          form: { ...fields, digest: '0'.repeat(64) }, maxRedirects: 0,
        });
        expect(rejected.status()).toBe(303);
        expect(rejected.headers().location).toContain('error=publication-conflict');
        expect(publication(fixture)).toEqual(beforeRejection);
        await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${capturePrefix}.rejected.ts`, false);
        expect(publication(fixture)).toEqual(beforeRejection);
      }
      await submit(pages[0], pages[0].getByTestId('teletext-publication-request-form'), 'TeletextPublicationRequest', js);
      await submit(pages[0], pages[0].getByTestId('teletext-publication-publish-form'), 'TeletextPublicationPublish', js);
      const selected = publication(fixture);
      expect(selected.map(rows => rows.length)).toEqual([3, 2, 3, 2]);
      if (capturePrefix) {
        await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${capturePrefix}.off.ts`, false);
        expect(publication(fixture)).toEqual(selected);
      }
      for (const [culture, module] of cultures) {
        await language(pages[0], culture, dark); await pages[0].goto(absentUrl); await ready(pages[0], js);
        const text = translations(module);
        const model = pages[0].getByTestId('teletext-preview-model');
        await expect(model).toHaveAttribute('data-scope', 'effective');
        await expect(model).not.toHaveAttribute('data-page');
        await expect(model.locator('img')).toHaveCount(0);
        await expect(model.getByTestId('teletext-preview-absence')).toHaveText(text('TeletextPreviewAbsent'));
        await expect(pages[0].getByTestId('teletext-effective-authoring')).toContainText(text('TeletextEffectiveAuthoringAbsent'));
        await expect(pages[0].getByTestId('teletext-publication-publish-form').locator('[name="preview_absence"]')).toHaveValue('1');
        for (const name of ['preview_page', 'preview_subcode']) await expect(model.locator(`[name="${name}"]`)).toHaveCount(0);
      }
      const expires = utcMilliseconds(fixture.scriptedIntervals(fixture.channels[0])[0].Finish);
      await expect.poll(() => Date.now() > expires, { timeout: capturePrefix ? 105_000 : 55_000 }).toBe(true);
      await pages[0].goto(absentUrl); await ready(pages[0], js);
      await expect(pages[0].getByTestId('teletext-preview-model')).toBeVisible();
      await expect(pages[0].getByTestId('teletext-preview-model').locator('img')).toHaveCount(0);
      await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unavailable');
      await expect(pages[0].getByTestId('teletext-publication-publish-form').getByRole('button')).toBeDisabled();
      const rejectedFields = await pages[0].getByTestId('teletext-publication-publish-form').evaluate(element =>
        Object.fromEntries(new FormData(element as HTMLFormElement).entries()) as Record<string, string>);
      const rejection = await pages[0].request.post('/admin-api/teletext/design/publish', { form: rejectedFields, maxRedirects: 0 });
      expect(rejection.status()).toBe(303); expect(rejection.headers().location).toMatch(/error=(expired|unavailable|publication-conflict)/);
      expect(publication(fixture)).toEqual(selected);
      await pages[1].goto(urls[1]); await ready(pages[1], js); await assertPreview(pages[1], 'night');
      await expect(pages[1].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
      expect(fixture.calls()).toEqual(['day', 'night']);
      expect(fixture.unchangedPackage()).toBe(true); expect(foreign(fixture)).toEqual(foreignBefore); expect(errors).toEqual([]);
    } finally {
      try {
        await pages[0].request.post('/admin-api/settings/ui/save', { form: {
          language: fixture.originalUi.get('pages.language') ?? 'en-us',
          is_dark_mode: String(/^(1|true)$/i.test(fixture.originalUi.get('pages.is_dark_mode') ?? 'True')),
        }, maxRedirects: 0 });
      } finally { await context.close(); fixture.close(); }
    }
  });
}
