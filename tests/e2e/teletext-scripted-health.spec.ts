import { expect, Locator, Page, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';

// Only the runner's disposable database is permitted. Private interval framing
// below is a health fixture; app -> Lua -> SQL publication has separate proof.
const databasePath = process.env.E2E_DB_PATH;
const specUrl = pathToFileURL(__filename);
const cultures = [
  ['en-us', 'en_us'], ['nl', 'nl'], ['pl', 'pl'], ['es-es', 'es_es'],
  ['es-419', 'es_419'], ['fr-fr', 'fr_fr'], ['de-de', 'de_de'],
  ['it-it', 'it_it'], ['pt-br', 'pt_br'],
] as const;
function translations(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(`../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl)), 'utf8');
  return key => {
    const literal = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
    if (!literal) throw new Error(`Missing ${module}/${key} translation.`);
    return JSON.parse(literal[1]) as string;
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
// The channel page folds shared settings and new profiles; open them like an operator would.
async function openFolded(form: Locator): Promise<void> {
  for (const testId of ['teletext-source-advanced', 'teletext-profile']) {
    const details = form.locator(`xpath=ancestor::details[@data-testid="${testId}"]`);
    if (await details.count() > 0 && await details.first().getAttribute('open') === null) {
      await details.first().locator(':scope > summary').click();
    }
  }
}
async function profileAndBinding(page: Page, fixture: TeletextChannelFixture, js: boolean): Promise<number> {
  await page.goto(`/channels/${fixture.channels[1]}/teletext`); await ready(page, js);
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  await openFolded(form);
  await form.locator('[name="name"]').fill('Synthetic health profile');
  await submit(page, form, translations('en_us')('TeletextProfileCreate'), js);
  const profile = fixture.profiles().find(row => row.Name === 'Synthetic health profile')!;
  expect(profile).toBeDefined();
  const binding = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-binding-form"]`);
  await openFolded(binding);
  await binding.locator('[name="profile_choice"]').selectOption(`${profile.Id}:${profile.Revision}`);
  await submit(page, binding, translations('en_us')('TeletextBindingSave'), js);
  return profile.Id;
}
function sqlDate(milliseconds: number): string {
  return new Date(milliseconds).toISOString().replace('T', ' ').replace('Z', '');
}
function snapshot(fixture: TeletextChannelFixture, profileId: number): string {
  const profile = fixture.database.prepare('SELECT * FROM "TeletextProfile" WHERE "Id"=?').get(profileId) as Record<string, string | number>;
  const installation = fixture.database.prepare('SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId"=?').get(fixture.addonId) as { ManifestJson: string };
  const manifest = JSON.parse(installation.ManifestJson) as {
    capabilities: { id: string }[]; settings: { key: string; default: string }[];
  };
  const contract = [3, 2, 1].find(version => manifest.capabilities.some(capability => capability.id === `teletext.pages.v${version}`));
  expect(contract).toBeDefined();
  expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM "TeletextProfileSetting" WHERE "ProfileId"=?').get(profileId)).toEqual({ count: 0 });
  // Version1 codec names mirror the private persisted snapshot schema. Values
  // come from the actual native-saved profile and inherited fixture defaults.
  return JSON.stringify({ version: 1, channel: fixture.channels[1], addon: fixture.addonId,
    profile: profileId, revision: profile.Revision, contract,
    installation: { addon: fixture.addonId, revision: profile.InstallationStateRevision,
      version: profile.InstallationVersion, package: profile.InstallationPackageSha256,
      content: profile.InstallationContentSha256, manifest: profile.InstallationManifestSha256 },
    settings: manifest.settings.map(setting => ({ key: setting.key, kind: 0, value: setting.default })) });
}
function seedPlayouts(fixture: TeletextChannelFixture): void {
  const nextBuild = sqlDate(Date.now() + 3_600_000);
  for (const channel of fixture.channels) {
    fixture.database.prepare('INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (?,?,4,0)').run(channel, channel);
    // Avoid background rebuilding an intentionally projection-only fixture.
    fixture.database.prepare('INSERT INTO "PlayoutBuildStatus" ("PlayoutId","LastBuild","Success","LastSuccessfulBuild") VALUES (?,?,1,?)').run(channel, nextBuild, nextBuild);
  }
}
function interval(fixture: TeletextChannelFixture, channel: number, value: string | null, expired = false): void {
  fixture.database.prepare('DELETE FROM "PlayoutTeletextInterval" WHERE "PlayoutId"=?').run(channel);
  fixture.database.prepare('INSERT INTO "PlayoutTeletextInterval" ("PlayoutId","AddonId","Start","Finish","State","CandidateSnapshot") VALUES (?,?,?,?,?,?)')
    .run(channel, fixture.addonId, sqlDate(Date.now() - 3_600_000), sqlDate(Date.now() + (expired ? -60_000 : 3_600_000)), Number(value !== null), value);
}
function storedResult(fixture: TeletextChannelFixture): { title: string; status: string; message: string } {
  const value = fixture.setting('health.results');
  expect(value).toBeTruthy();
  const result = (JSON.parse(value!) as { title: string; status: string; message: string }[]).find(row => row.title === 'Scripted Playout State');
  expect(result).toBeDefined(); return result!;
}
async function refresh(page: Page, js: boolean, fixture: TeletextChannelFixture): Promise<void> {
  const intervals = fixture.database.prepare('SELECT * FROM "PlayoutTeletextInterval" WHERE "PlayoutId" IN (?,?) ORDER BY "PlayoutId","Start"').all(...fixture.channels);
  const safe = fixture.safetySnapshot();
  const response = page.waitForResponse(value => value.request().method() === 'POST'
    && (js ? new URL(value.url()).pathname.startsWith('/admin-api/refresh_health_snapshot')
      : new URL(value.url()).pathname === '/admin-api/system/health/refresh'));
  const navigated = js ? undefined : page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await page.getByRole('button', { name: translations('en_us')('Refresh'), exact: true }).click();
  expect((await response).status()).toBe(js ? 200 : 303);
  if (navigated) { await navigated; await ready(page, js); }
  await expect(page.getByRole('button', { name: translations('en_us')('Refresh'), exact: true })).toBeEnabled();
  expect(fixture.database.prepare('SELECT * FROM "PlayoutTeletextInterval" WHERE "PlayoutId" IN (?,?) ORDER BY "PlayoutId","Start"').all(...fixture.channels)).toEqual(intervals);
  expect(fixture.safetySnapshot()).toEqual(safe);
}
async function localizedViews(page: Page, fixture: TeletextChannelFixture, js: boolean, dark: boolean,
  phase: string, messages: [string, number][], pass: boolean, allCultures: boolean,
  testInfo: { outputPath(name: string): string }): Promise<void> {
  for (const [language, module] of allCultures ? cultures : cultures.slice(0, 2)) {
    const text = translations(module);
    expect((await page.request.post('/admin-api/settings/ui/save', { form: { language, is_dark_mode: String(dark) }, maxRedirects: 0 })).status()).toBe(303);
    await page.goto('/system/health'); await ready(page, js);
    await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
    const row = page.getByRole('row').filter({ has: page.getByText(text('ScriptedPlayoutState'), { exact: true }) });
    await expect(row).toHaveCount(1);
    await expect(row.locator('span[aria-label]').first()).toHaveAttribute('aria-label', text(pass ? 'Pass' : 'Fail'));
    for (const [key, channel] of messages) await expect(row).toContainText(text(key).replace('{channel}', String(channel)));
    await expect(row).not.toContainText('Scripted teletext interval for channel');
    await expect(row).not.toContainText('scoped native generation');
    await expect(row).not.toContainText('"version":99');
    if (pass) await expect(row.locator('td').nth(1)).toHaveText('');
    if (language === 'en-us' || language === 'nl') {
      await row.scrollIntoViewIfNeeded(); await expect(row).toBeInViewport();
      await page.screenshot({ path: testInfo.outputPath(`${language}-${phase}.png`), fullPage: false });
    }
    expect(fixture.calls()).toEqual([]);
    expect(fixture.unchangedPackage()).toBe(true);
  }
}

test.describe.configure({ mode: 'serial' });
for (const js of [true, false]) for (const dark of [false, true]) {
  test(`Scripted teletext Health readiness (JavaScript ${js}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
    test.skip(!databasePath, 'Requires disposable E2E_DB_PATH.');
    test.skip(process.platform === 'win32', 'Intentional POSIX package fixture; this does not claim Windows runtime acceptance.');
    test.setTimeout(240_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const fixture = new TeletextChannelFixture(databasePath!);
    const context = await browser.newContext({ baseURL, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    try {
      await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0 });
      const profile = await profileAndBinding(page, fixture, js);
      seedPlayouts(fixture);
      const on = snapshot(fixture, profile);
      const unavailable = `Scripted teletext interval for channel ${fixture.channels[0]}: active override is unavailable; the published timeline is retained.`;
      const notReady = `Scripted teletext interval for channel ${fixture.channels[1]}: scoped native generation is not available; the published timeline is retained.`;
      for (const phase of ['unavailable-off', 'malformed-on', 'valid-off', 'expired'] as const) {
        if (phase === 'unavailable-off') {
          interval(fixture, fixture.channels[0], null);
          interval(fixture, fixture.channels[1], on);
        } else if (phase === 'malformed-on') {
          interval(fixture, fixture.channels[0], '{"version":99}');
          interval(fixture, fixture.channels[1], null);
        } else if (phase === 'valid-off') {
          interval(fixture, fixture.channels[0], '{"version":99}', true);
          fixture.database.prepare('UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"=?').run(fixture.addonId);
          fixture.database.prepare('UPDATE "TeletextProfile" SET "IsDeleted"=1 WHERE "Id"=?').run(profile);
        } else {
          interval(fixture, fixture.channels[1], '{"version":99}', true);
        }
        expect((await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0 })).status()).toBe(303);
        await page.goto('/system/health'); await ready(page, js);
        await refresh(page, js, fixture);
        const result = storedResult(fixture);
        const pass = phase === 'valid-off' || phase === 'expired';
        expect(result.status).toBe(pass ? 'Pass' : 'Fail');
        expect(result.message).toBe(phase === 'unavailable-off' ? `${unavailable}\n${notReady}` : phase === 'malformed-on' ? unavailable : '');
        const messages: [string, number][] = pass ? [] : [['ScriptedTeletextOverrideUnavailable', fixture.channels[0]]];
        if (phase === 'unavailable-off') messages.push(['ScriptedTeletextGenerationUnavailable', fixture.channels[1]]);
        await localizedViews(page, fixture, js, dark, phase, messages, pass, phase === 'unavailable-off', testInfo);
        if (pass) {
          expect(fixture.database.prepare('SELECT "Enabled" FROM "AddonInstallation" WHERE "AddonId"=?').get(fixture.addonId)).toEqual({ Enabled: 0 });
          expect(fixture.database.prepare('SELECT "IsDeleted" FROM "TeletextProfile" WHERE "Id"=?').get(profile)).toEqual({ IsDeleted: 1 });
        }
        expect(errors).toEqual([]);
      }
    } finally {
      try {
        fixture.database.prepare('DELETE FROM "Playout" WHERE "Id" IN (?,?)').run(...fixture.channels);
        fixture.close();
        expect((await context.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
      } finally { await context.close(); }
    }
  });
}
