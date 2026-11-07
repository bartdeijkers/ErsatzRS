import { expect, Locator, Page, test } from '@playwright/test';
import { copyFileSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs } from './helpers/teletext-stream-capture';

// The runner owns this disposable database and the intentional POSIX package.
// Draft tests prove non-publication; separate tests perform both explicit actions.
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

async function ready(page: Page, javaScriptEnabled: boolean): Promise<void> {
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (javaScriptEnabled) await waitForHydration(page);
}

async function submit(page: Page, form: Locator, button: string, javaScriptEnabled: boolean): Promise<void> {
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/teletext/'));
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: button, exact: true }).click();
  expect((await posted).status()).toBe(303);
  await navigated;
  await ready(page, javaScriptEnabled);
}

async function createProfile(page: Page, fixture: TeletextChannelFixture, theme: string, javaScriptEnabled: boolean): Promise<number> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  const advanced = form.locator('xpath=ancestor::details[@data-testid="teletext-source-advanced"]');
  if (await advanced.count() > 0 && await advanced.getAttribute('open') === null) await advanced.locator(':scope > summary').click();
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  await form.locator('[name="name"]').fill(`Synthetic ${theme}`);
  await form.locator('[data-setting-key="THEME"] select[name$=".mode"]').selectOption('literal');
  await form.locator('[data-setting-key="THEME"] [name$=".value"]').selectOption(theme);
  await submit(page, form, translations('en_us')('TeletextProfileCreate'), javaScriptEnabled);
  const profile = fixture.profiles().find(row => row.Name === `Synthetic ${theme}`)!;
  expect(profile).toBeDefined();
  return profile.Id;
}

async function selectProfile(page: Page, fixture: TeletextChannelFixture, profile: number, javaScriptEnabled: boolean): Promise<void> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-binding-form"]`);
  const revision = fixture.profiles().find(row => row.Id === profile)!.Revision;
  await form.locator('[name="profile_choice"]').selectOption(`${profile}:${revision}`);
  await submit(page, form, translations('en_us')('TeletextBindingSave'), javaScriptEnabled);
}

async function prepareFields(page: Page, fixture: TeletextChannelFixture, theme: string): Promise<Locator> {
  const form = page.getByTestId('teletext-authoring-form');
  await expect(form.locator('[name="selection_digest"]')).toHaveValue(/^[a-f0-9]{64}$/);
  await expect(form.locator('fieldset')).not.toHaveAttribute('disabled', '');
  await expect(form.getByRole('button')).toBeEnabled();
  await form.locator('[name="name"]').fill(`Synthetic original ${theme}`);
  await form.locator('[name="entry_addon_id"]').selectOption(fixture.addonId);
  await form.locator('[name="entry_page"]').fill('200');
  await form.locator('[name="entry_subcode"]').fill('0000');
  await form.locator('[name="v1_region"]').selectOption('western_latin');
  await form.locator('[name="resource_transmissions"]').fill('[]');
  return form;
}

async function pixels(page: Page): Promise<{ red: number; blue: number }> {
  const image = page.getByTestId('teletext-preview-model').locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete
    && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  return image.evaluate(element => {
    const canvas = document.createElement('canvas');
    canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!;
    context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, canvas.width, canvas.height);
    const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let red = 0; let blue = 0;
    for (let index = 0; index < data.length; index += 4) {
      if (data[index] === 255 && data[index + 1] === 0 && data[index + 2] === 0) red++;
      if (data[index] === 0 && data[index + 1] === 0 && data[index + 2] === 255) blue++;
    }
    return { red, blue };
  });
}

async function assertPreview(page: Page, theme: string, address: string): Promise<void> {
  const model = page.getByTestId('teletext-preview-model');
  await expect(model).toHaveAttribute('data-page', address);
  await expect(model).toHaveAttribute('data-subcode', '0000');
  const colours = await pixels(page);
  expect(colours[theme === 'day' ? 'red' : 'blue']).toBeGreaterThan(0);
  expect(colours[theme === 'day' ? 'blue' : 'red']).toBe(0);
}

async function responsivePreview(page: Page, prefix: string, testInfo: { outputPath(name: string): string }): Promise<void> {
  await page.setViewportSize({ width: 320, height: 900 });
  const form = page.getByTestId('teletext-preview-form');
  await form.locator('[name="page"]').focus();
  await page.keyboard.press('Tab');
  const focused = form.locator('[name="subcode"]');
  await expect(focused).toBeFocused();
  await expect.poll(() => focused.evaluate(element => Number.parseFloat(getComputedStyle(element).outlineWidth))).toBeGreaterThan(0);
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await focused.scrollIntoViewIfNeeded();
  await expect(focused).toBeInViewport();
  await page.screenshot({ path: testInfo.outputPath(`${prefix}-320-keyboard.png`), fullPage: false });
  await page.setViewportSize({ width: 640, height: 900 });
  await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
  await focused.scrollIntoViewIfNeeded();
  await expect(focused).toBeInViewport();
  await expect.poll(() => page.evaluate(() => document.documentElement.getBoundingClientRect().right <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath(`${prefix}-200-percent.png`), fullPage: false });
  await page.evaluate(() => { document.documentElement.style.zoom = ''; });
  await page.setViewportSize({ width: 1280, height: 900 });
}

test.describe.configure({ mode: 'serial' });
for (const javaScriptEnabled of [true, false]) {
  for (const dark of [false, true]) {
    test(`teletext static authoring flow (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the runner disposable E2E_DB_PATH.');
      test.setTimeout(300_000);
      testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
      const fixture = new TeletextChannelFixture(databasePath!, { authoredPages: true });
      fixture.setAuthoringPolicy();
      const safe = fixture.safetySnapshot();
      const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 1280, height: 900 } });
      const pages = [await context.newPage(), await context.newPage()];
      const errors: string[] = [];
      for (const page of pages) {
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      }
      try {
        await pages[0].request.post('/admin-api/settings/ui/save', {
          form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0,
        });
        await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`);
        await ready(pages[0], javaScriptEnabled);
        const day = await createProfile(pages[0], fixture, 'day', javaScriptEnabled);
        const night = await createProfile(pages[0], fixture, 'night', javaScriptEnabled);
        const urls: string[] = [];
        for (const [index, theme, profile] of [[0, 'day', day], [1, 'night', night]] as const) {
          const page = pages[index];
          await page.goto(`/channels/${fixture.channels[index]}/teletext`);
          await ready(page, javaScriptEnabled);
          await selectProfile(page, fixture, profile, javaScriptEnabled);
          const form = await prepareFields(page, fixture, theme);
          if (index === 0) {
            const fields = await form.evaluate(element => Object.fromEntries(new FormData(element as HTMLFormElement).entries()) as Record<string, string>);
            for (const [change, code] of [
              [{ selection_digest: '0'.repeat(64) }, 'conflict'],
              [{ entry_page: '999' }, 'invalid-input'],
              [{ resource_transmissions: '[{"resource":null,"counter":0,"counter":1}]' }, 'invalid-input'],
            ] as const) {
              const response = await page.request.post('/admin-api/teletext/design/prepare', {
                form: { ...fields, ...change }, maxRedirects: 0,
              });
              expect(response.status()).toBe(303);
              expect(response.headers().location).toContain(`error=${code}`);
              expect(fixture.calls()).toEqual([]);
              expect(fixture.designs()).toEqual([]);
              expect(fixture.safetySnapshot()).toEqual(safe);
            }
          }
          await submit(page, form, translations('en_us')('TeletextValidateSaveDraft'), javaScriptEnabled);
          await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
          await assertPreview(page, theme, '200');
          urls.push(page.url());
        }
        expect(fixture.calls()).toEqual(['day', 'night']);
        expect(fixture.designs()).toHaveLength(2);
        expect(fixture.request('day')?.channels).toEqual([]);
        expect(fixture.request('night')?.channels).toEqual([]);
        for (const [language, module] of cultures) {
          const text = translations(module);
          await pages[0].request.post('/admin-api/settings/ui/save', {
            form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
          });
          for (const [index, theme] of [[0, 'day'], [1, 'night']] as const) {
            const page = pages[index];
            await page.goto(urls[index]);
            await ready(page, javaScriptEnabled);
            await expect(page.getByRole('heading', { name: text('TeletextPreviewTitle'), exact: true })).toBeVisible();
            await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
            await expect(page.getByTestId('teletext-authoring')).toContainText(text('TeletextAuthoringStaticHelp'));
            await assertPublication(page, text, 'unrequested');
            await assertPreview(page, theme, '200');
            const navigation = page.getByTestId('teletext-preview-model').locator('ul a');
            await expect(navigation).toHaveText('201 / 0000');
            await expect(navigation.locator('..').locator('..').locator(':scope > span'))
              .toHaveText(`${text('TeletextNavIndex')} · 201`);
            await navigation.click();
            await ready(page, javaScriptEnabled);
            await assertPreview(page, theme, '201');
            if (language === 'en-us' || language === 'nl') {
              await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
              await page.screenshot({ path: testInfo.outputPath(`${language}-${theme}-201.png`), fullPage: false });
              await responsivePreview(page, `${language}-${theme}`, testInfo);
            }
          }
          await pages[0].goto('/system/health');
          await ready(pages[0], javaScriptEnabled);
          const row = pages[0].getByRole('row').filter({ has: pages[0].getByText(text('TeletextAuthoringHealthTitle'), { exact: true }) });
          await expect(row).toHaveCount(1);
          await expect(row).toContainText(text('TeletextAuthoringHealthValidDetail'));
          expect(fixture.calls()).toEqual(['day', 'night']);
          expect(fixture.safetySnapshot()).toEqual(safe);
          expect(fixture.unchangedPackage()).toBe(true);
          expect(errors).toEqual([]);
        }
        const wrong = new URL(urls[0]);
        wrong.pathname = `/channels/${fixture.channels[1]}/teletext`;
        await pages[0].goto(wrong.toString());
        await ready(pages[0], javaScriptEnabled);
        await expect(pages[0].getByTestId('teletext-preview-error')).toBeVisible();
        await expect(pages[0].getByTestId('teletext-preview-model')).toHaveCount(0);
        fixture.setAuthoringPolicy(null);
        await pages[0].goto(urls[0]);
        await ready(pages[0], javaScriptEnabled);
        const disabledForm = pages[0].getByTestId('teletext-authoring-form');
        await expect(disabledForm.locator('fieldset')).toHaveAttribute('disabled', '');
        const disabledControls = disabledForm.locator('fieldset input, fieldset select, fieldset textarea, fieldset button');
        await expect(disabledControls).toHaveCount(8);
        await expect(disabledForm.locator('[name="guide_channel_id"]')).toBeDisabled();
        for (const control of await disabledControls.all()) await expect(control).toBeDisabled();
        await expect(pages[0].getByTestId('teletext-preview-error')).toBeVisible();
        fixture.setAuthoringPolicy();
        await pages[0].goto(urls[0]);
        await ready(pages[0], javaScriptEnabled);
        await assertPreview(pages[0], 'day', '200');
        expect(fixture.calls()).toEqual(['day', 'night']);
        expect(fixture.designs()).toHaveLength(2);
        expect(fixture.safetySnapshot()).toEqual(safe);
        expect(errors).toEqual([]);
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
}

async function assertPublication(page: Page, text: (key: string) => string, state: 'unrequested' | 'pending' | 'selected'): Promise<void> {
  const publication = page.getByTestId('teletext-publication');
  await expect(publication).toHaveAttribute('data-state', state);
  const keys = { unrequested: 'TeletextPublicationUnrequested', pending: 'TeletextPublicationPending', selected: 'TeletextPublicationSelected' };
  await expect(page.getByTestId('teletext-publication-status')).toHaveText(text(keys[state]));
  const request = page.getByTestId('teletext-publication-request-form');
  const publish = page.getByTestId('teletext-publication-publish-form');
  await expect(request.getByRole('button')).toHaveText(text('TeletextPublicationRequest'));
  await expect(publish.getByRole('button')).toHaveText(text('TeletextPublicationPublish'));
  if (state === 'unrequested') {
    await expect(request.getByRole('button')).toBeEnabled();
    await expect(publish.getByRole('button')).toBeDisabled();
  } else {
    await expect(request.getByRole('button')).toBeDisabled();
    if (state === 'pending') await expect(publish.getByRole('button')).toBeEnabled();
    else await expect(publish.getByRole('button')).toBeDisabled();
  }
  await expect(request.locator('[name="expected_generation_id"]')).toHaveCount(0);
  await expect(publish.locator('[name="expected_generation_id"]')).toHaveCount(1);
  await expect(request.locator('[name="digest"]')).toHaveValue(/^[a-f0-9]{64}$/);
}

function publicationRows(fixture: TeletextChannelFixture): unknown[][] {
  return ['TeletextPublicationRequest', 'TeletextPublicationRequestCurrent',
    'TeletextAcceptedGeneration', 'TeletextAcceptedGenerationCurrent'].map(table =>
    fixture.database.prepare(`SELECT * FROM "${table}" WHERE "ChannelId" IN (?,?) ORDER BY 1`).all(...fixture.channels));
}

function playbackSafety(fixture: TeletextChannelFixture): unknown {
  const snapshot = fixture.safetySnapshot() as { publication: unknown; configuration: unknown; channels: unknown; playouts: unknown; package: unknown };
  return { configuration: snapshot.configuration, channels: snapshot.channels, playouts: snapshot.playouts, package: snapshot.package };
}

async function localizedPublication(pages: Page[], urls: string[], state: 'pending' | 'selected', javaScriptEnabled: boolean, dark: boolean, testInfo: { outputPath(name: string): string }): Promise<void> {
  for (const [language, module] of cultures) {
    await pages[0].request.post('/admin-api/settings/ui/save', {
      form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
    });
    for (const [index, page] of pages.entries()) {
      await page.goto(urls[index]);
      await ready(page, javaScriptEnabled);
      await assertPublication(page, translations(module), state);
      await assertPreview(page, index === 0 ? 'day' : 'night', '200');
      if (language === 'en-us' || language === 'nl') {
        await page.getByTestId('teletext-publication').scrollIntoViewIfNeeded();
        await expect(page.getByTestId('teletext-publication-status')).toBeInViewport();
        await page.screenshot({ path: testInfo.outputPath(`${language}-${state}-${index}.png`), fullPage: false });
      }
    }
  }
  await pages[0].request.post('/admin-api/settings/ui/save', {
    form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0,
  });
}

async function captureOrdinary(fixture: TeletextChannelFixture, baseURL: string, channel: number, path: string, authored: boolean): Promise<void> {
  const capture = await captureTeletextChannelTs(`${baseURL}/iptv/hls-direct/${channel}.ts`, path,
    { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
  expect(capture.ended).toBe('eof');
  const pids = new Map<number, number>();
  let malformed = 0;
  for (let offset = 0; offset < capture.bytes.length; offset += 188) {
    if (capture.bytes[offset] !== 0x47 || (capture.bytes[offset + 1] & 0x80) !== 0) malformed++;
    const pid = ((capture.bytes[offset + 1] & 0x1f) << 8) | capture.bytes[offset + 2];
    pids.set(pid, (pids.get(pid) ?? 0) + 1);
  }
  expect(malformed).toBe(0);
  expect(pids.get(0) ?? 0).toBeGreaterThan(0);
  expect(pids.get(256) ?? 0).toBeGreaterThan(0);
  expect(pids.get(257) ?? 0).toBeGreaterThan(0);
  if (authored) expect(pids.get(512) ?? 0).toBeGreaterThan(0);
  else expect(pids.has(512)).toBe(false);
  // This is decoded A/V evidence; external TSDuck/libzvbi/client checks inspect
  // the retained exact TCP captures for transport and authored page semantics.
  execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
    '-i', path, '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
  { timeout: 30_000, maxBuffer: 65_536 });
  expect(fixture.calls()).toEqual(['day', 'night']);
}

for (const [javaScriptEnabled, dark] of [[true, false], [false, true]] as const) {
  test(`teletext explicit publication TCP flow (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
    test.skip(!databasePath || !process.env.FFMPEG_PATH, 'Requires disposable E2E_DB_PATH and pinned FFmpeg 9.');
    test.setTimeout(360_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const fixture = new TeletextChannelFixture(databasePath!, { authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH! } });
    fixture.setAuthoringPolicy(); fixture.setOutputPolicy();
    const safe = playbackSafety(fixture);
    const candidate = testInfo.outputPath('candidate');
    copyFileSync(join(fixture.root, 'synthetic-client-source.ts'), `${candidate}.source.ts`);
    const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 1280, height: 900 } });
    const pages = [await context.newPage(), await context.newPage()];
    const errors: string[] = [];
    for (const page of pages) {
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    }
    const text = translations('en_us');
    try {
      await pages[0].request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0 });
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`);
      await ready(pages[0], javaScriptEnabled);
      const day = await createProfile(pages[0], fixture, 'day', javaScriptEnabled);
      const night = await createProfile(pages[0], fixture, 'night', javaScriptEnabled);
      const urls: string[] = [];
      for (const [index, theme, profile] of [[0, 'day', day], [1, 'night', night]] as const) {
        const page = pages[index];
        await page.goto(`/channels/${fixture.channels[index]}/teletext`);
        await ready(page, javaScriptEnabled);
        await selectProfile(page, fixture, profile, javaScriptEnabled);
        await submit(page, await prepareFields(page, fixture, theme), text('TeletextValidateSaveDraft'), javaScriptEnabled);
        await assertPreview(page, theme, '200');
        await assertPublication(page, text, 'unrequested');
        urls.push(page.url());
        await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), javaScriptEnabled);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-requested');
        await assertPublication(page, text, 'pending');
      }
      expect(fixture.calls()).toEqual(['day', 'night']);
      const pending = publicationRows(fixture);
      expect(pending.map(rows => rows.length)).toEqual([2, 2, 0, 0]);
      expect(playbackSafety(fixture)).toEqual(safe);
      await localizedPublication(pages, urls, 'pending', javaScriptEnabled, dark, testInfo);
      fixture.resetSourceStart();
      await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${candidate}.pending.ts`, false);
      expect(publicationRows(fixture)).toEqual(pending);
      for (const [index, page] of pages.entries()) {
        await page.goto(urls[index]); await ready(page, javaScriptEnabled);
        await page.getByTestId('teletext-preview-model').locator('ul a').click();
        await ready(page, javaScriptEnabled);
        await assertPreview(page, index === 0 ? 'day' : 'night', '201');
        await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), javaScriptEnabled);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-selected');
        await assertPreview(page, index === 0 ? 'day' : 'night', '201');
        await assertPublication(page, text, 'selected');
      }
      const selected = publicationRows(fixture);
      expect(selected.map(rows => rows.length)).toEqual([2, 2, 2, 2]);
      const entries = fixture.database.prepare('SELECT json_extract(CAST("Document" AS TEXT),\'$.entry.page\') AS "Page",json_extract(CAST("Document" AS TEXT),\'$.entry.subcode\') AS "Subcode" FROM "TeletextDesignRevision" WHERE "DesignId" IN (?,?) ORDER BY "DesignId"')
        .all(...fixture.designs().map(design => design.Id));
      expect(entries).toEqual([{ Page: 200, Subcode: 0 }, { Page: 200, Subcode: 0 }]);
      await localizedPublication(pages, urls, 'selected', javaScriptEnabled, dark, testInfo);
      expect(playbackSafety(fixture)).toEqual(safe);
      fixture.resetSourceStart();
      await Promise.all([
        captureOrdinary(fixture, baseURL!, fixture.channels[0], `${candidate}.day.ts`, true),
        captureOrdinary(fixture, baseURL!, fixture.channels[1], `${candidate}.night.ts`, true),
      ]);
      expect(publicationRows(fixture)).toEqual(selected);
      fixture.setOutputPolicy('{"version":999}');
      fixture.resetSourceStart();
      await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${candidate}.invalid-policy.ts`, false);
      expect(publicationRows(fixture)).toEqual(selected);
      fixture.setOutputPolicy();
      fixture.resetSourceStart();
      await captureOrdinary(fixture, baseURL!, fixture.channels[0], `${candidate}.recovered.ts`, true);
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`);
      await ready(pages[0], javaScriptEnabled);
      await submit(pages[0], pages[0].locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-disable-form"]`), text('TeletextBindingOff'), javaScriptEnabled);
      expect(fixture.binding(fixture.channels[0])?.Enabled).toBe(0);
      expect(fixture.binding(fixture.channels[1])?.Enabled).toBe(1);
      await pages[0].goto(urls[0]);
      await ready(pages[0], javaScriptEnabled);
      await assertPreview(pages[0], 'day', '200');
      await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unavailable');
      await expect(pages[0].getByTestId('teletext-publication-status')).toHaveText(text('TeletextPublicationUnavailable'));
      await expect(pages[0].getByTestId('teletext-publication-request-form').getByRole('button')).toBeDisabled();
      await expect(pages[0].getByTestId('teletext-publication-publish-form').getByRole('button')).toBeDisabled();
      fixture.resetSourceStart();
      await Promise.all([
        captureOrdinary(fixture, baseURL!, fixture.channels[0], `${candidate}.unavailable-profile.ts`, false),
        captureOrdinary(fixture, baseURL!, fixture.channels[1], `${candidate}.other-channel.ts`, true),
      ]);
      expect(publicationRows(fixture)).toEqual(selected);
      expect(playbackSafety(fixture)).toEqual(safe);
      expect(fixture.unchangedPackage()).toBe(true);
      expect(fixture.designs()).toHaveLength(2);
      expect(fixture.calls()).toEqual(['day', 'night']);
      expect(errors).toEqual([]);
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
