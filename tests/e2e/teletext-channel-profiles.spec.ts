import { expect, Locator, Page, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';

// The shared shell hydrates; the teletext page uses populated native forms.
// Run both projects sequentially with --headed --workers=1 on the wrapper DB.
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

function addon(page: Page, fixture: TeletextChannelFixture): Locator {
  return page.getByTestId('teletext-addon').filter({ has: page.locator(`input[name="addon_id"][value="${fixture.addonId}"]`) });
}

async function ensureFormOpen(form: Locator): Promise<void> {
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) {
    await details.locator(':scope > summary').click();
  }
  await expect(form).toBeVisible();
}

async function visit(page: Page, channel: number, javaScriptEnabled: boolean): Promise<void> {
  await page.goto(`/channels/${channel}/teletext`);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await expect(page.getByTestId('channel-teletext-page')).toBeVisible();
  if (javaScriptEnabled) await waitForHydration(page);
}

async function submit(page: Page, form: Locator, button: string, code: string, javaScriptEnabled: boolean): Promise<void> {
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/teletext/'));
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: button, exact: true }).click();
  expect((await posted).status()).toBe(303);
  await navigated;
  await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', code);
  if (javaScriptEnabled) await waitForHydration(page);
}

async function createProfile(page: Page, fixture: TeletextChannelFixture, name: string, theme: string, javaScriptEnabled: boolean): Promise<number> {
  const form = addon(page, fixture).locator('[data-testid="teletext-profile-form"][data-profile-id=""]');
  await ensureFormOpen(form);
  await form.locator('[name="name"]').fill(name);
  const field = form.locator('[data-setting-key="THEME"]');
  await field.locator('select[name$=".mode"]').selectOption('literal');
  await field.locator('[name$=".value"]').selectOption(theme);
  await submit(page, form, translations('en_us')('TeletextProfileCreate'), 'profile', javaScriptEnabled);
  const profile = fixture.profiles().find(row => row.Name === name);
  expect(profile).toBeDefined();
  expect(fixture.theme(profile!.Id)).toBe(theme);
  return profile!.Id;
}

async function bind(page: Page, fixture: TeletextChannelFixture, profile: number, button: string, javaScriptEnabled: boolean): Promise<void> {
  const form = addon(page, fixture).getByTestId('teletext-binding-form');
  const current = fixture.profiles().find(row => row.Id === profile)!;
  await form.locator('[name="profile_choice"]').selectOption(`${profile}:${current.Revision}`);
  await submit(page, form, button, 'binding', javaScriptEnabled);
}

async function literalLabelRoundTrip(
  page: Page, fixture: TeletextChannelFixture, profile: number,
  saveLabel: string, javaScriptEnabled: boolean,
): Promise<void> {
  const form = addon(page, fixture).locator(`[data-testid="teletext-profile-form"][data-profile-id="${profile}"]`);
  const field = form.locator('[data-setting-key="LABEL"]');
  const payload = '\n</textarea><script>window.__teletextInjected=true</script>&amp; &\n';
  await ensureFormOpen(form);
  await field.locator('select[name$=".mode"]').selectOption('literal');
  await field.locator('[name$=".value"]').fill(payload);
  await submit(page, form, saveLabel, 'profile', javaScriptEnabled);
  await page.reload();
  if (javaScriptEnabled) await waitForHydration(page);
  await ensureFormOpen(form);
  await expect(field.locator('textarea[name$=".value"]')).toHaveValue(payload);
  await expect(page.locator('script').filter({ hasText: 'window.__teletextInjected=true' })).toHaveCount(0);
  expect(await page.evaluate(() => (window as Window & { __teletextInjected?: boolean }).__teletextInjected)).toBeUndefined();
  const stored = fixture.database.prepare('SELECT "Kind","Value","Reference" FROM "TeletextProfileSetting" WHERE "ProfileId"=? AND "Key"=\'LABEL\'')
    .get(profile) as { Kind: number; Value: string; Reference: null };
  expect(stored.Kind).toBe(0);
  expect(stored.Reference).toBeNull();
  // Native form submission canonicalizes line breaks to CRLF; textarea DOM
  // values normalize them to LF. Every remaining character must round-trip.
  expect(stored.Value.replace(/\r\n/g, '\n')).toBe(payload);
  await field.locator('select[name$=".mode"]').selectOption('inherit');
  await submit(page, form, saveLabel, 'profile', javaScriptEnabled);
  expect(fixture.database.prepare('SELECT "Value" FROM "TeletextProfileSetting" WHERE "ProfileId"=? AND "Key"=\'LABEL\'').get(profile)).toBeUndefined();
}

async function fields(form: Locator): Promise<Record<string, string>> {
  return form.evaluate(element => Object.fromEntries(new FormData(element as HTMLFormElement).entries()) as Record<string, string>);
}

async function missingProfile(page: Page, fixture: TeletextChannelFixture): Promise<void> {
  const form = addon(page, fixture).getByTestId('teletext-binding-form');
  const body = await fields(form);
  body.profile_choice = '2147483647:1';
  const before = fixture.binding(fixture.channels[0]);
  const rejected = await page.request.post('/admin-api/teletext/binding/save', { form: body, maxRedirects: 0 });
  expect(rejected.status()).toBe(303);
  expect(rejected.headers().location).toContain('error=missing-profile');
  expect(fixture.binding(fixture.channels[0])).toEqual(before);
}

async function restoreUi(page: Page, fixture: TeletextChannelFixture): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: {
      language: fixture.originalUi.get('pages.language') ?? 'en-us',
      is_dark_mode: String(/^(1|true)$/i.test(fixture.originalUi.get('pages.is_dark_mode') ?? 'True')),
    }, maxRedirects: 0,
  });
}

async function responsive(
  page: Page, fixture: TeletextChannelFixture, capture: (name: string) => string,
): Promise<void> {
  await page.setViewportSize({ width: 320, height: 900 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  const form = addon(page, fixture).locator('[data-testid="teletext-profile-form"][data-profile-id=""]');
  await ensureFormOpen(form);
  await form.locator('[name="name"]').focus();
  await page.keyboard.press('Tab');
  expect(await form.evaluate(element => element.contains(document.activeElement))).toBe(true);
  await page.getByTestId('channel-teletext-page').screenshot({ path: capture('320-keyboard.png') });
  // Test instrumentation reproduces 200% CSS zoom in either JS mode; native
  // form interaction above remains available with browser JavaScript disabled.
  await page.setViewportSize({ width: 640, height: 900 });
  await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
  await expect.poll(() => page.evaluate(() => document.documentElement.getBoundingClientRect().right <= innerWidth + 1)).toBe(true);
  await page.getByTestId('channel-teletext-page').screenshot({ path: capture('200-percent.png') });
  await page.evaluate(() => { document.documentElement.style.zoom = ''; });
  await page.setViewportSize({ width: 1280, height: 900 });
}

test.describe.configure({ mode: 'serial' });
for (const javaScriptEnabled of [true, false]) {
  for (const dark of [false, true]) {
    test(`channel teletext profiles (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the runner disposable E2E_DB_PATH.');
      test.setTimeout(240_000);
      testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
      const fixture = new TeletextChannelFixture(databasePath!);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const stale = await context.newPage();
      const errors: string[] = [];
      for (const current of [page, stale]) {
        current.on('pageerror', error => errors.push(error.message));
        current.on('console', message => {
          if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) errors.push(message.text());
        });
      }
      try {
        const safe = fixture.safetySnapshot();
        await page.request.post('/admin-api/settings/ui/save', {
          form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0,
        });
        await visit(page, fixture.channels[0], javaScriptEnabled);
        const day = await createProfile(page, fixture, 'Day <synthetic> & profile', 'day', javaScriptEnabled);
        const night = await createProfile(page, fixture, 'Night synthetic profile', 'night', javaScriptEnabled);
        await missingProfile(page, fixture);
        for (const [language, module] of cultures) {
          const text = translations(module);
          await page.request.post('/admin-api/settings/ui/save', {
            form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
          });
          await visit(page, fixture.channels[0], javaScriptEnabled);
          await expect(page.getByRole('heading', { name: text('TeletextChannelTitle'), exact: true })).toBeVisible();
          await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
          await ensureFormOpen(addon(page, fixture).locator('[data-testid="teletext-profile-form"][data-profile-id=""]'));
          await expect(addon(page, fixture).getByRole('button', { name: text('TeletextProfileCreate'), exact: true })).toBeVisible();
          await expect(page.getByTestId('channel-teletext-page')).toContainText(text('TeletextChannelBaseHelp'));
          const profile = addon(page, fixture).locator(`[data-testid="teletext-profile-form"][data-profile-id="${day}"]`);
          await ensureFormOpen(profile);
          await expect(profile.getByLabel(text('TeletextProfileName'), { exact: true })).toHaveValue('Day <synthetic> & profile');
          await expect(profile.getByRole('button', { name: text('TeletextProfileSave'), exact: true })).toBeVisible();
          const label = profile.locator('[data-setting-key="LABEL"]');
          await label.locator('select[name$=".mode"]').selectOption('literal');
          await label.locator('[name$=".value"]').fill('');
          await submit(page, profile, text('TeletextProfileSave'), 'profile', javaScriptEnabled);
          const stored = fixture.database.prepare('SELECT "Kind","Value","Reference" FROM "TeletextProfileSetting" WHERE "ProfileId"=? AND "Key"=\'LABEL\'').get(day);
          expect(stored).toEqual({ Kind: 0, Value: '', Reference: null });
          expect(fixture.theme(day)).toBe('day');
          if (language === 'en-us' || language === 'nl') {
            await literalLabelRoundTrip(page, fixture, day, text('TeletextProfileSave'), javaScriptEnabled);
          }
          await bind(page, fixture, day, text('TeletextBindingSave'), javaScriptEnabled);
          await visit(page, fixture.channels[1], javaScriptEnabled);
          await bind(page, fixture, night, text('TeletextBindingSave'), javaScriptEnabled);
          const second = fixture.binding(fixture.channels[1]);
          expect(second?.Enabled).toBe(1);
          expect(second?.ProfileId).toBe(night);
          expect(fixture.binding(fixture.channels[0])?.Enabled).toBe(1);
          expect(fixture.binding(fixture.channels[0])?.ProfileId).toBe(day);
          expect(fixture.theme(day)).toBe('day');
          expect(fixture.theme(night)).toBe('night');
          await page.getByTestId('channel-teletext-page').screenshot({ path: testInfo.outputPath(`${language}-night.png`) });
          await visit(page, fixture.channels[0], javaScriptEnabled);
          await bind(page, fixture, night, text('TeletextBindingSave'), javaScriptEnabled);
          expect(fixture.binding(fixture.channels[0])?.ProfileId).toBe(night);
          expect(fixture.binding(fixture.channels[1])).toEqual(second);
          await visit(stale, fixture.channels[0], javaScriptEnabled);
          await submit(page, addon(page, fixture).getByTestId('teletext-disable-form'), text('TeletextBindingOff'), 'off', javaScriptEnabled);
          expect(fixture.binding(fixture.channels[0])?.Enabled).toBe(0);
          const held = fixture.binding(fixture.channels[0]);
          await submit(stale, addon(stale, fixture).getByTestId('teletext-binding-form'), text('TeletextBindingSave'), 'conflict', javaScriptEnabled);
          await expect(stale.getByTestId('teletext-feedback')).toHaveText(text('TeletextAdminConflict'));
          expect(fixture.binding(fixture.channels[0])).toEqual(held);
          expect(fixture.binding(fixture.channels[1])).toEqual(second);
          await bind(page, fixture, day, text('TeletextBindingSave'), javaScriptEnabled);
          await page.getByTestId('channel-teletext-page').screenshot({ path: testInfo.outputPath(`${language}-day.png`) });
          if (language === 'en-us' || language === 'nl') {
            await responsive(page, fixture, name => testInfo.outputPath(`${language}-${name}`));
          }
          // A missing enum option makes the retained selected profile invalid.
          // The explicit Off action remains usable without activating packages.
          fixture.database.prepare('UPDATE "TeletextProfileSetting" SET "Value"=\'unsupported\' WHERE "ProfileId"=? AND "Key"=\'THEME\'').run(night);
          await visit(page, fixture.channels[1], javaScriptEnabled);
          await expect(addon(page, fixture)).toContainText(text('TeletextProfileInvalidSettings'));
          await expect(addon(page, fixture).getByTestId('teletext-disable-form')).toBeVisible();
          await submit(page, addon(page, fixture).getByTestId('teletext-disable-form'), text('TeletextBindingOff'), 'off', javaScriptEnabled);
          expect(fixture.binding(fixture.channels[1])?.Enabled).toBe(0);
          fixture.database.prepare('UPDATE "TeletextProfileSetting" SET "Value"=\'night\' WHERE "ProfileId"=? AND "Key"=\'THEME\'').run(night);
          expect(fixture.safetySnapshot()).toEqual(safe);
          expect(fixture.unchangedPackage()).toBe(true);
          expect(errors).toEqual([]);
        }
      } finally {
        try { await restoreUi(page, fixture); }
        finally { await context.close(); fixture.close(); }
      }
    });
  }
}

async function captureViewport(page: Page, target: Locator, path: string): Promise<void> {
  await target.scrollIntoViewIfNeeded();
  await expect(target).toBeVisible();
  await expect(target).toBeInViewport();
  await page.screenshot({ path, fullPage: false });
}

async function captureResponsiveViewport(
  page: Page, form: Locator, capture: (name: string) => string,
): Promise<void> {
  await page.setViewportSize({ width: 320, height: 900 });
  await ensureFormOpen(form);
  await form.locator('[name="name"]').focus();
  await page.keyboard.press('Tab');
  const focused = form.locator(':focus');
  await expect(focused).toHaveCount(1);
  await expect(focused).toBeFocused();
  await expect.poll(() => focused.evaluate(element => Number.parseFloat(getComputedStyle(element).outlineWidth))).toBeGreaterThan(0);
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await captureViewport(page, focused, capture('320-keyboard.png'));
  await page.setViewportSize({ width: 640, height: 900 });
  await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
  await expect.poll(() => page.evaluate(() => document.documentElement.getBoundingClientRect().right <= innerWidth + 1)).toBe(true);
  await captureViewport(page, focused, capture('200-percent.png'));
  await page.evaluate(() => { document.documentElement.style.zoom = ''; });
  await page.setViewportSize({ width: 1280, height: 900 });
}

// Functional identities above retain their original source and receipts. This
// focused supplement captures visible viewports in the shell's inner scroller.
for (const javaScriptEnabled of [true, false]) {
  for (const dark of [false, true]) {
    test(`channel teletext visual proof (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the runner disposable E2E_DB_PATH.');
      test.setTimeout(120_000);
      testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
      const fixture = new TeletextChannelFixture(databasePath!);
      const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 1280, height: 900 } });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      try {
        const safe = fixture.safetySnapshot();
        await page.request.post('/admin-api/settings/ui/save', {
          form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0,
        });
        await visit(page, fixture.channels[0], javaScriptEnabled);
        const day = await createProfile(page, fixture, 'Day synthetic visual profile', 'day', javaScriptEnabled);
        const night = await createProfile(page, fixture, 'Night synthetic visual profile', 'night', javaScriptEnabled);
        for (const [language, module] of cultures.filter(([culture]) => culture === 'en-us' || culture === 'nl')) {
          const text = translations(module);
          await page.request.post('/admin-api/settings/ui/save', {
            form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
          });
          for (const [index, profile, theme] of [[0, day, 'day'], [1, night, 'night']] as const) {
            await visit(page, fixture.channels[index], javaScriptEnabled);
            await bind(page, fixture, profile, text('TeletextBindingSave'), javaScriptEnabled);
            await expect(page.getByRole('heading', { name: text('TeletextChannelTitle'), exact: true })).toBeVisible();
            await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
            const section = addon(page, fixture);
            await captureViewport(page, section.getByTestId('teletext-binding-form'), testInfo.outputPath(`${language}-channel-${index + 1}-binding.png`));
            const form = section.locator(`[data-testid="teletext-profile-form"][data-profile-id="${profile}"]`);
            await ensureFormOpen(form);
            const field = form.locator('[data-setting-key="THEME"]');
            await expect(field.locator('[name$=".value"]')).toHaveValue(theme);
            await captureViewport(page, field, testInfo.outputPath(`${language}-channel-${index + 1}-theme.png`));
          }
          expect(fixture.binding(fixture.channels[0])?.ProfileId).toBe(day);
          expect(fixture.binding(fixture.channels[1])?.ProfileId).toBe(night);
          expect(fixture.binding(fixture.channels[0])?.Enabled).toBe(1);
          expect(fixture.binding(fixture.channels[1])?.Enabled).toBe(1);
          await visit(page, fixture.channels[0], javaScriptEnabled);
          const form = addon(page, fixture).locator(`[data-testid="teletext-profile-form"][data-profile-id="${day}"]`);
          await captureResponsiveViewport(page, form, name => testInfo.outputPath(`${language}-${name}`));
          expect(fixture.safetySnapshot()).toEqual(safe);
          expect(fixture.unchangedPackage()).toBe(true);
          expect(errors).toEqual([]);
        }
      } finally {
        try { await restoreUi(page, fixture); }
        finally { await context.close(); fixture.close(); }
      }
    });
  }
}
