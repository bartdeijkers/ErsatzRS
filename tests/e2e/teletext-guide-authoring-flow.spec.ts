import { Browser, BrowserContext, expect, Locator, Page } from '@playwright/test';
import { test } from './isolated-web-host';
import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { addonContentHash } from './fixtures/addon-metadata';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs } from './helpers/teletext-stream-capture';
import { archiveFiles, type Upload } from './helpers/teletext-native-editor';
import { startupOwner } from './helpers/isolated-host-startup';

// Manual exact-package gate. CI still classifies this base/shared spec as 1.5;
// absent package evidence is an explicit skip, never a synthetic replacement.
const databasePath = process.env.E2E_DB_PATH;
const candidateZip = process.env.ERSATZRS_TVGUIDE_CANDIDATE_ZIP;
const candidateDigest = candidateZip ? sha256(readFileSync(candidateZip)) : undefined;
const addonId = 'org.ersatzrs.addon.tvguide-teletext';
const productionVersion = '0.1.2';
const cultures = [
  ['en-us', 'en_us'], ['nl', 'nl'], ['pl', 'pl'], ['pt-br', 'pt_br'],
  ['fr-fr', 'fr_fr'], ['de-de', 'de_de'], ['es-es', 'es_es'],
  ['es-419', 'es_419'], ['it-it', 'it_it'],
] as const;
const specUrl = pathToFileURL(__filename);
const guideTitle = 'Production SQL Guide';

type Installation = {
  Version: string; Enabled: number; SourceId: string; StateRevision: string;
  PackageSha256: string; ContentSha256: string; InstallPath: string;
};
type SavedOriginal = {
  guide_source: { channel_ids: number[] };
  guide_inputs: { v3: string };
  documents: { response: string }[];
};
let installed: Installation;
let originalUnverified: string | undefined;
let originalLanguage: string | undefined;
let originalDark: string | undefined;
let settingsCaptured = false;

function sha256(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function database(selectedPath = databasePath!): DatabaseSync {
  const db = new DatabaseSync(selectedPath);
  db.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
  return db;
}

function installation(selectedPath = databasePath!): Installation | undefined {
  const db = database(selectedPath);
  try {
    return db.prepare(`SELECT Version,Enabled,SourceId,StateRevision,PackageSha256,
      ContentSha256,InstallPath FROM AddonInstallation WHERE AddonId=?`).get(addonId) as Installation | undefined;
  } finally { db.close(); }
}

function translation(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl,
  )), 'utf8');
  return key => {
    const match = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
    if (!match) throw new Error(`Missing ${module}/${key} translation.`);
    return JSON.parse(match[1]) as string;
  };
}

async function ready(page: Page, js: boolean): Promise<void> {
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (js) await waitForHydration(page);
  // Current-guide preview lives inside the page's native advanced disclosure.
  // Follow the visible control instead of treating hidden rendered data as proof.
  if (new URL(page.url()).searchParams.has('guide_preview')) {
    const advanced = page.getByTestId('teletext-publish-advanced');
    if (await advanced.getAttribute('open') === null) await advanced.locator(':scope > summary').click();
  }
}

async function submit(page: Page, form: Locator, button: string, js: boolean): Promise<void> {
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/teletext/'));
  const navigated = page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() });
  await form.getByRole('button', { name: button, exact: true }).click();
  expect((await posted).status()).toBe(303);
  await navigated;
  await ready(page, js);
}

function compareInstalledFiles(selected = installed): void {
  // Compare actual installed payloads to the frozen input archive. PackageSha256
  // is the host's per-addon archive digest, not the complete repository ZIP hash.
  execFileSync(process.platform === 'win32' ? 'python' : 'python3', ['-c', [
    'import pathlib,sys,zipfile',
    'root=pathlib.Path(sys.argv[1]); prefix="addons/"+sys.argv[3]+"/"',
    'with zipfile.ZipFile(sys.argv[2]) as archive:',
    ' expected={name[len(prefix):]:archive.read(name) for name in archive.namelist() if name.startswith(prefix) and not name.endswith("/")}',
    ' actual={p.relative_to(root).as_posix():p.read_bytes() for p in root.rglob("*") if p.is_file()}',
    ' assert expected and expected==actual, "Installed production payload differs from frozen candidate"',
  ].join('\n'), selected.InstallPath, candidateZip!, addonId], { timeout: 10_000 });
  expect(addonContentHash(selected.InstallPath)).toBe(selected.ContentSha256);
  expect(sha256(readFileSync(candidateZip!))).toBe(candidateDigest);
}

test.describe.configure({ mode: 'serial' });
test.skip(!databasePath || !candidateZip, 'Requires disposable E2E_DB_PATH and an explicit production0.1.2 ERSATZRS_TVGUIDE_CANDIDATE_ZIP.');

test.beforeAll(async ({ browser, baseURL }) => {
  // The manual external case owns its isolated install; its existing controller
  // has no shared wrapper host. Ordinary production suites keep their setup.
  if (process.env.ERSATZRS_TELETEXT_PRODUCTION_EXTERNAL_HOLD === '1') return;
  test.setTimeout(120_000);
  const db = database();
  const setting = (key: string) => (db.prepare('SELECT Value FROM ConfigElement WHERE Key=?').get(key) as { Value: string } | undefined)?.Value;
  try {
    originalUnverified = setting('addons.allow_unverified_sources');
    originalLanguage = setting('pages.language');
    originalDark = setting('pages.is_dark_mode');
    settingsCaptured = true;
  } finally { db.close(); }
  installed = await installProduction(browser, baseURL, databasePath!);
});

async function installProduction(browser: Browser, baseURL: string | undefined, selectedPath: string): Promise<Installation> {
  expect(installation(selectedPath), 'Use a clean disposable database; do not replace an existing production install.').toBeUndefined();
  const db = database(selectedPath);
  try {
    db.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('addons.allow_unverified_sources', 'false');
  } finally { db.close(); }
  const context = await browser.newContext({ baseURL, javaScriptEnabled: true });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  try {
    await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    });
    await page.goto('/settings/addons?view=sources');
    await waitForHydration(page);
    await page.getByRole('button', { name: 'Enable', exact: true }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Confirm', exact: true }).click();
    const upload = page.getByLabel('Repository package', { exact: true });
    await expect(upload).toBeEnabled();
    await upload.setInputFiles(candidateZip!);
    await page.getByRole('button', { name: 'Add repository', exact: true }).click();
    const feedback = page.locator('#repository-package-feedback');
    await expect(feedback).toContainText('Official ErsatzRS add-ons');
    expect(installation(selectedPath)).toBeUndefined();
    await feedback.getByRole('link').click();
    await waitForHydration(page);
    await page.getByTestId('available-addon-row').filter({ hasText: 'TV-guide teletext' }).getByRole('link').click();
    await waitForHydration(page);
    await page.getByRole('button', { name: 'Install', exact: true }).click();
    const confirm = page.getByRole('dialog').getByRole('button', { name: 'Confirm', exact: true });
    await expect(confirm).toBeVisible();
    await confirm.focus();
    await Promise.all([
      page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
      page.keyboard.press('Enter'),
    ]);
    await expect.poll(() => installation(selectedPath)?.Enabled).toBe(0);
    await page.waitForLoadState('load');
    await expect(page.getByRole('button', { name: 'Enable', exact: true })).toBeVisible();
    await waitForHydration(page);
    await page.getByRole('button', { name: 'Enable', exact: true }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect.poll(() => installation(selectedPath)?.Enabled).toBe(1);
    const selected = installation(selectedPath)!;
    expect(selected.Version).toBe(productionVersion);
    expect(selected.SourceId).toBeTruthy();
    expect(selected.StateRevision).toBeTruthy();
    expect(selected.PackageSha256).toMatch(/^[a-f0-9]{64}$/);
    compareInstalledFiles(selected);
    expect(errors).toEqual([]);
    return selected;
  } finally { await context.close(); }
}

test.afterAll(() => {
  if (!databasePath || !candidateZip || !settingsCaptured) return;
  // Immutable installed package/history is retained until the disposable host
  // stops; the wrapper owns removal of its isolated state directory.
  const db = database();
  try {
    for (const [key, value] of [
      ['addons.allow_unverified_sources', originalUnverified],
      ['pages.language', originalLanguage], ['pages.is_dark_mode', originalDark],
    ]) {
      if (value === undefined) db.prepare('DELETE FROM ConfigElement WHERE Key=?').run(key);
      else db.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run(key, value);
    }
  } finally { db.close(); }
});

function seedSqlGuide(fixture: TeletextChannelFixture): number {
  const tables = ['MediaSource', 'Library', 'LibraryPath', 'MediaItem', 'Movie', 'MovieMetadata', 'Playout', 'PlayoutItem'];
  const id = (fixture.database.prepare(`SELECT MAX(id)+1 AS id FROM (
    SELECT ? AS id UNION ALL ${tables.map(table => `SELECT MAX(Id) AS id FROM "${table}"`).join(' UNION ALL ')}
  )`).get(fixture.channels[1]) as { id: number }).id;
  const now = Date.now();
  const dbTime = (offset: number) => new Date(now + offset).toISOString().replace('T', ' ').replace('Z', '');
  fixture.database.exec('BEGIN IMMEDIATE;');
  try {
    fixture.database.prepare('INSERT INTO MediaSource(Id) VALUES (?)').run(id);
    fixture.database.prepare('INSERT INTO LocalMediaSource(Id) VALUES (?)').run(id);
    fixture.database.prepare('INSERT INTO Library(Id,MediaKind,MediaSourceId,Name) VALUES (?,1,?,?)').run(id, id, 'Production guide synthetic facts');
    fixture.database.prepare('INSERT INTO LocalLibrary(Id) VALUES (?)').run(id);
    fixture.database.prepare('INSERT INTO LibraryPath(Id,LibraryId,Path) VALUES (?,?,?)').run(id, id, 'synthetic-guide');
    fixture.database.prepare('INSERT INTO MediaItem(Id,LibraryPathId) VALUES (?,?)').run(id, id);
    fixture.database.prepare('INSERT INTO Movie(Id) VALUES (?)').run(id);
    fixture.database.prepare('INSERT INTO MovieMetadata(Id,MovieId,DateAdded,DateUpdated,MetadataKind,Title) VALUES (?,?,?,?,0,?)')
      .run(id, id, dbTime(0), dbTime(0), guideTitle);
    fixture.database.prepare('INSERT INTO Playout(Id,ChannelId,ScheduleKind,Seed) VALUES (?,?,1,0)').run(id, fixture.channels[1]);
    fixture.database.prepare(`INSERT INTO PlayoutItem(Id,DisableWatermarks,FillerKind,Finish,GuideGroup,
      InPoint,MediaItemId,OutPoint,PlayoutId,Start) VALUES (?,0,0,?,0,'00:00:00',?,'01:00:00',?,?)`)
      .run(id, dbTime(3_300_000), id, id, dbTime(-300_000));
    fixture.database.exec('COMMIT;');
  } catch (error) { fixture.database.exec('ROLLBACK;'); throw error; }
  return id;
}

async function createProfile(page: Page, fixture: TeletextChannelFixture, theme: 'classic' | 'night', js: boolean): Promise<number> {
  const form = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  const advanced = form.locator('xpath=ancestor::details[@data-testid="teletext-source-advanced"]');
  if (await advanced.count() > 0 && await advanced.getAttribute('open') === null) await advanced.locator(':scope > summary').click();
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  const name = `Production ${theme} ${fixture.channels[0]}`;
  await form.locator('[name="name"]').fill(name);
  const settings = theme === 'classic'
    ? { THEME: theme, HEADER: 'DAY GUIDE', PAGE_START: '200', PAGE_END: '299', INDEX_PAGE: '200' }
    : { THEME: theme, HEADER: 'NIGHT GUIDE', PAGE_START: '400', PAGE_END: '499', INDEX_PAGE: '410' };
  for (const [key, value] of Object.entries(settings)) {
    const setting = form.locator(`[data-setting-key="${key}"]`);
    await setting.locator('[name$=".mode"]').selectOption('literal');
    if (key === 'THEME') await setting.locator('[name$=".value"]').selectOption(value);
    else await setting.locator('[name$=".value"]').fill(value);
  }
  await submit(page, form, translation('en_us')('TeletextProfileCreate'), js);
  const profile = fixture.database.prepare('SELECT Id FROM TeletextProfile WHERE AddonId=? AND Name=?').get(addonId, name) as { Id: number };
  expect(profile).toBeDefined();
  return profile.Id;
}

function savedOriginal(fixture: TeletextChannelFixture, channel: number): SavedOriginal {
  const designs = fixture.designs(channel);
  expect(designs).toHaveLength(1);
  const row = fixture.database.prepare('SELECT Document FROM TeletextDesignRevision WHERE DesignId=? AND Revision=?')
    .get(designs[0].Id, designs[0].Revision) as { Document: Uint8Array };
  return JSON.parse(Buffer.from(row.Document).toString('utf8'));
}

for (const js of [true, false]) {
  test(`production TV-guide SQL draft preparation (JavaScript ${js})`, async ({ browser, baseURL }, info) => {
    test.setTimeout(300_000);
    info.annotations.push({ type: 'browser-version', description: browser.version() },
      { type: 'production-package', description: `${addonId}${productionVersion}; repositoryZIP SHA256 ${candidateDigest}; contentSHA256 ${installed.ContentSha256}` });
    // Reuse channel allocation/settings cleanup only. Its configuration-only
    // sentinel remains unbound and must never execute or supply page content.
    const fixture = new TeletextChannelFixture(databasePath!);
    let source: number | undefined;
    let context: BrowserContext | undefined;
    try {
      const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
        '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
      )), 'utf8'));
      delete policy.source; delete policy.recovery_timeout_ms;
      policy.version = 2; policy.operation_timeout_ms = 30_000;
      policy.guide_limits = { max_records: 1000, max_source_bytes: 1_048_576,
        max_model_nodes: 10_000, max_model_bytes: 16_777_216,
        max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
      policy.preview = { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 };
      fixture.setAuthoringPolicy(JSON.stringify(policy));
      source = seedSqlGuide(fixture);
      const safe = fixture.safetySnapshot();
      context = await browser.newContext({ baseURL, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      const urls: string[] = [];
      await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
      await page.goto(`/channels/${fixture.channels[0]}/teletext`);
      await ready(page, js);
      const profiles = [await createProfile(page, fixture, 'classic', js), await createProfile(page, fixture, 'night', js)];
      for (const [index, channel] of fixture.channels.entries()) {
        await page.goto(`/channels/${channel}/teletext`);
        await ready(page, js);
        const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
        await binding.locator('[name="profile_choice"]').selectOption(`${profiles[index]}:1`);
        await submit(page, binding, translation('en_us')('TeletextBindingSave'), js);
        const form = page.getByTestId('teletext-authoring-form');
        await form.locator('[name="name"]').fill(`Production SQL draft ${channel}`);
        await form.locator('[name="entry_addon_id"]').selectOption(addonId);
        await form.locator('[name="entry_page"]').fill(index ? '410' : '200');
        await form.locator('[name="entry_subcode"]').fill('0000');
        await form.locator('[name="v1_region"]').selectOption('western_latin');
        await form.locator('[name="resource_transmissions"]').fill('[]');
        const selected = form.locator('[name="guide_channel_id"]');
        await expect(selected).toBeEnabled();
        await selected.selectOption(String(fixture.channels[1]));
        await expect(selected.locator('option:checked')).toHaveCount(1);
        await submit(page, form, translation('en_us')('TeletextValidateSaveDraft'), js);
        await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
        await expect(page.getByTestId('teletext-preview-model')).toHaveAttribute('data-page', index ? '410' : '200');
        await expect(page.getByTestId('teletext-preview-model').locator('img')).toBeVisible();
        await expect(page.getByTestId('teletext-preview-template')).toContainText('tvguide');
        await expect(page.getByTestId('teletext-preview-theme')).toHaveText(index ? 'night' : 'classic');
        const original = savedOriginal(fixture, channel);
        expect(original.guide_source.channel_ids).toEqual([fixture.channels[1]]);
        expect(JSON.parse(original.guide_inputs.v3).channels.map((entry: { id: number }) => entry.id)).toEqual([fixture.channels[1]]);
        const response = JSON.parse(original.documents[0].response);
        expect(response.service.pages.every((entry: { address: { page: number } }) => entry.address.page >= (index ? 400 : 200) && entry.address.page <= (index ? 499 : 299))).toBe(true);
        expect(JSON.stringify(response.service.pages)).toContain(guideTitle);
        expect(response.authoring.theme.id).toBe(index ? 'night' : 'classic');
        urls.push(page.url());
      }
      for (const [language, module] of cultures) {
        const text = translation(module);
        await page.request.post('/admin-api/settings/ui/save', { form: { language, is_dark_mode: String(module === 'nl') }, maxRedirects: 0 });
        for (const [index, url] of urls.entries()) {
          await page.goto(url);
          await ready(page, js);
          await expect(page.getByLabel(text('TeletextGuideChannels'), { exact: true }).first()).toBeVisible();
          await expect(page.getByTestId('teletext-authoring-form')).toContainText(text('TeletextGuideChannelsHelp'));
          await expect(page.getByLabel(text('TeletextGuideChannels'), { exact: true }).first().locator(`option[value="${fixture.channels[1]}"]`))
            .toHaveText(`${fixture.channels[1]} — Synthetic teletext 2`);
          await expect(page.getByTestId('teletext-preview-theme')).toHaveText(index ? 'night' : 'classic');
          if (language === 'en-us' || language === 'nl') {
            const raster = page.getByTestId('teletext-preview-model').locator('img').first();
            await expect(raster).toBeVisible();
            await raster.scrollIntoViewIfNeeded();
            // This route scrolls inside its main panel. A full-document capture
            // can omit that viewport and leave a large blank image in Firefox.
            await page.screenshot({ path: info.outputPath(`${language}-${index ? 'night' : 'day'}-production-guide.png`) });
          }
        }
      }
      for (const table of ['TeletextPublicationRequest', 'TeletextAcceptedGeneration']) {
        expect(fixture.database.prepare(`SELECT COUNT(*) AS count FROM "${table}" WHERE ChannelId IN (?,?)`).get(...fixture.channels)).toEqual({ count: 0 });
      }
      expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM AddonSetting WHERE AddonId=?').get(addonId)).toEqual({ count: 0 });
      expect(fixture.safetySnapshot()).toEqual(safe);
      expect(fixture.calls()).toEqual([]);
      expect(installation()).toEqual(installed);
      compareInstalledFiles();
      expect(errors).toEqual([]);
      writeFileSync(info.outputPath('production-guide-identity.json'), JSON.stringify({ repository_zip_sha256: candidateDigest,
        addon_id: addonId, version: installed.Version, content_sha256: installed.ContentSha256,
        package_sha256: installed.PackageSha256, output_channels: fixture.channels,
        source_channels: [fixture.channels[1]], javaScriptEnabled: js }, null, 2));
    } finally {
      try { await context?.close(); }
      finally {
        try {
          if (source !== undefined) {
            fixture.database.prepare('DELETE FROM Playout WHERE Id=?').run(source);
            fixture.database.prepare('DELETE FROM MediaSource WHERE Id=?').run(source);
          }
        } finally { fixture.close(); }
      }
    }
  });
}

// A real Reset owns programme publication. No synthetic SQL guide rows, source
// clock rewrites, teletext Lua overrides or producer replay are used here.
async function resetGuidePlayout(page: Page, fixture: TeletextChannelFixture, channel: number,
  expected: { count: number; durationMilliseconds: number; sourceDuration?: string } = { count: 3, durationMilliseconds: 180_000 }): Promise<void> {
  const text = translation('en_us');
  const previous = fixture.database.prepare('SELECT LastBuild FROM PlayoutBuildStatus WHERE PlayoutId=?')
    .get(channel) as { LastBuild: string } | undefined;
  fixture.writeScriptedControls(channel, undefined, expected.count);
  await page.goto('/playouts'); await ready(page, true);
  const name = fixture.database.prepare('SELECT Name FROM Channel WHERE Id=?').get(channel) as { Name: string };
  await page.getByPlaceholder(text('SearchForPlayouts'), { exact: true }).fill(name.Name);
  const row = page.getByRole('row').filter({ has: page.locator(`a[href="/playouts/scripted/${channel}"]`) });
  await expect(row).toHaveCount(1);
  await row.getByRole('button', { name: text('RebuildPlayout'), exact: true }).click();
  const dialog = page.getByRole('dialog', { name: text('RebuildPlayout'), exact: true });
  await dialog.getByLabel(text('PlayoutRebuildMode'), { exact: true }).selectOption('reset');
  await dialog.getByRole('checkbox', { name: text('KeepCurrentProgramme'), exact: true }).uncheck();
  const posted = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname.startsWith('/admin-api/rebuild_playout'));
  await dialog.getByRole('button', { name: text('QueuePlayoutBuild'), exact: true }).click();
  const response = await posted;
  expect(response.status()).toBe(200);
  const fields = new URLSearchParams(response.request().postData() ?? '');
  expect(fields.get('id')).toBe(String(channel));
  expect(fields.get('mode')).toBe('reset');
  expect(fields.get('preserve_current')).toBe('false');
  expect(await response.json()).toEqual({ accepted: 1, coalesced: 0 });
  await expect(dialog).toBeHidden();
  await expect.poll(() => {
    const status = fixture.database.prepare('SELECT Success,LastBuild,LastSuccessfulBuild FROM PlayoutBuildStatus WHERE PlayoutId=?')
      .get(channel) as { Success: number; LastBuild: string; LastSuccessfulBuild: string } | undefined;
    const items = fixture.database.prepare('SELECT COUNT(*) AS count FROM PlayoutItem WHERE PlayoutId=?').get(channel) as { count: number };
    return status?.Success === 1 && status.LastBuild !== previous?.LastBuild
      && status.LastSuccessfulBuild === status.LastBuild && items.count === expected.count
      && fixture.scriptedIntervals(channel).length === 0;
  }, { timeout: 45_000 }).toBe(true);
  const spans = fixture.database.prepare('SELECT Start,Finish,InPoint,OutPoint FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start')
    .all(channel) as { Start: string; Finish: string; InPoint: string; OutPoint: string }[];
  for (const span of spans) {
    if (expected.sourceDuration !== undefined) {
      expect(span.InPoint).toMatch(/^00:00:00(?:\.0+)?$/);
      expect(span.OutPoint).toBe(expected.sourceDuration);
      expect(utc(span.Finish)).toBeGreaterThan(utc(span.Start));
    } else expect(utc(span.Finish) - utc(span.Start)).toBe(expected.durationMilliseconds);
  }
}

function utc(value: string): number {
  const normalized = value.replace(' ', 'T');
  const time = Date.parse(/[zZ]|[+-]\d\d:\d\d$/.test(normalized) ? normalized : `${normalized}Z`);
  if (!Number.isFinite(time)) throw new Error('Published guide programme has an invalid UTC boundary.');
  return time;
}

function originalBytes(fixture: TeletextChannelFixture, channel: number): Buffer {
  const design = fixture.designs(channel)[0];
  const row = fixture.database.prepare('SELECT Document FROM TeletextDesignRevision WHERE DesignId=? AND Revision=?')
    .get(design.Id, design.Revision) as { Document: Uint8Array };
  return Buffer.from(row.Document);
}

function acceptedGuide(fixture: TeletextChannelFixture, channel: number): Record<string, unknown> {
  const row = fixture.database.prepare(`SELECT g.*,r.DesignId,r.Revision FROM TeletextAcceptedGenerationCurrent c
    JOIN TeletextAcceptedGeneration g ON g.Id=c.GenerationId
    JOIN TeletextPublicationRequest r ON r.Id=g.RequestId WHERE c.ChannelId=?`).get(channel);
  expect(row).toBeDefined();
  expect(row!.ArtifactVersion).toBe(5);
  return row!;
}

type ReceiverGuideMap = { theme: string; entry: number; pages: { address: { page: number; subcode: number }; navigation: unknown[] }[] };

async function prepareReceiverGuide(page: Page, fixture: TeletextChannelFixture, channel: number,
  profile: number, night: boolean): Promise<ReceiverGuideMap> {
  const text = translation('en_us');
  await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
  const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
  await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
  await submit(page, binding, text('TeletextBindingSave'), true);
  const form = page.getByTestId('teletext-authoring-form');
  for (const [key, value] of Object.entries({ name: `Production receiver ${channel}`, entry_page: night ? '410' : '200',
    entry_subcode: '0000', resource_transmissions: '[]' })) await form.locator(`[name="${key}"]`).fill(value);
  await form.locator('[name="entry_addon_id"]').selectOption(addonId);
  await form.locator('[name="v1_region"]').selectOption('western_latin');
  await form.locator('[name="guide_channel_id"]').selectOption(String(fixture.channels[1]));
  await submit(page, form, text('TeletextValidateSaveDraft'), true);
  const response = JSON.parse(savedOriginal(fixture, channel).documents[0].response);
  const map: ReceiverGuideMap = { theme: response.authoring.theme.id, entry: response.service.initial_page,
    pages: response.service.pages.map((value: ReceiverGuideMap['pages'][number]) => ({ address: value.address, navigation: value.navigation })) };
  expect(map.theme).toBe(night ? 'night' : 'classic');
  expect(map.pages.some(value => value.address.subcode !== 0), 'Normal finite guide must genuinely paginate').toBe(true);
  await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
  await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
  await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
  return map;
}

// Opt-in human handoff, never an automated renderer acceptance claim. Existing
// base controls remain closed; this joins the real production guide and tracks.
test.describe('production external receiver', () => {
test.skip(process.env.ERSATZRS_TELETEXT_PRODUCTION_EXTERNAL_HOLD !== '1', 'Explicit manual production receiver opt-in required.');
test('production TV-guide ordinary URLs are held for the joint external receiver', async ({ browser, isolatedWebHost }, info) => {
  const external = process.env.ERSATZRS_TELETEXT_EXTERNAL_CLIENT_DIR;
  const source = process.env.ERSATZRS_TELETEXT_EXTERNAL_CAPTION_SOURCE;
  const sourceDigest = process.env.ERSATZRS_TELETEXT_EXTERNAL_CAPTION_SHA256;
  const scanner = process.env.ERSATZRS_TELETEXT_EXTERNAL_SCANNER;
  const scannerDigest = process.env.ERSATZRS_TELETEXT_EXTERNAL_SCANNER_SHA256;
  test.skip(process.env.ERSATZRS_TELETEXT_PRODUCTION_EXTERNAL_HOLD !== '1' || !external || !source || !scanner,
    'Requires explicit production external hold, owned controller, qualified source and exact final packaged Local scanner.');
  test.skip(process.platform === 'win32' || !process.env.FFMPEG_PATH || !process.env.FFPROBE_PATH,
    'Existing external-hold fixture requires POSIX and explicit qualified FFmpeg/ffprobe.');
  expect(sourceDigest).toBe('b6ffa783e1cc96926a846b73fa8182755a11555296294ef9a740c09d9245c560');
  expect(scannerDigest).toMatch(/^[a-f0-9]{64}$/);
  expect(sha256(readFileSync(scanner!))).toBe(scannerDigest);
  const hold = Number(process.env.ERSATZRS_TELETEXT_EXTERNAL_CLIENT_HOLD_SECONDS ?? 75);
  // Reserve bounded conversation time for the opt-in physical receiver trial.
  // This changes no source, worker, delivery or capture deadline.
  expect(Number.isSafeInteger(hold) && hold >= 75 && hold <= 1200).toBe(true);
  test.setTimeout(Math.max(180_000, (hold + 120) * 1000));
  for (const marker of ['host-ready.json', 'client-complete.json', 'client-update-guide.json', 'host-update-complete.json']) {
    expect(existsSync(`${external}/${marker}`), 'Use a fresh owned receiver directory').toBe(false);
  }
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true,
    playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 60, captionSource: { path: source!, sha256: sourceDigest! } } });
  const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage(); const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    const selected = await installProduction(browser, isolatedWebHost.baseURL, isolatedWebHost.databasePath);
    const scan = fixture.configureScannedScripted(scanner!, process.env.FFPROBE_PATH!, info.outputPath('receiver-local-scan.log'));
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL('../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl)), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216, max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    const count = Math.max(8, Math.ceil((hold + 120) / 60) + 1);
    for (const channel of fixture.channels) {
      await page.goto(`/channels/${channel}`); await ready(page, true);
      for (const [name, value] of Object.entries({ preferred_audio_language_code: 'dut', preferred_subtitle_language_code: 'eng',
        subtitle_mode: '3', source_teletext_delivery: '1' })) {
        const select = page.locator(`select[name="${name}"]`);
        await expect(select.locator(`option[value="${value}"]`)).toHaveCount(1);
        await select.selectOption(value, { timeout: 5000 });
      }
      await page.getByRole('button', { name: translation('en_us')('SaveChannel'), exact: true }).click();
      await expect(page).toHaveURL(/\/channels$/); await ready(page, true);
    }
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const profiles = [await createProfile(page, fixture, 'classic', true), await createProfile(page, fixture, 'night', true)];
    // Establish the real guide-source timeline first; Reset the primary output
    // immediately before publication, without editing any programme timestamps.
    for (const channel of [...fixture.channels].reverse()) {
      await resetGuidePlayout(page, fixture, channel, { count, durationMilliseconds: 60_000, sourceDuration: scan.duration });
    }
    const firstProgramme = fixture.database.prepare('SELECT Start,Finish FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start LIMIT 1')
      .get(fixture.channels[0]) as { Start: string; Finish: string };
    expect(Date.now() - utc(firstProgramme.Start), 'Normal publication must begin in source caption A window').toBeLessThan(25_000);
    const maps: ReceiverGuideMap[] = [];
    for (const [index, channel] of fixture.channels.entries()) maps.push(await prepareReceiverGuide(page, fixture, channel, profiles[index], index === 1));
    const originals = fixture.channels.map(channel => sha256(originalBytes(fixture, channel)));
    const accepted = fixture.channels.map(channel => acceptedGuide(fixture, channel));
    const current = () => fixture.channels.map(channel => fixture.database.prepare('SELECT SeedId,RuntimeId,Revision,Revoked FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1').get(channel));
    const before = current();
    const playlist = await page.request.get('/iptv/channels.m3u', { headers: { 'User-Agent': 'Kodi/21.2' } });
    expect(playlist.ok()).toBe(true); const body = await playlist.text();
    expect(body).toContain('#KODIPROP:inputstream=inputstream.ffmpegdirect');
    expect(body).toContain('#KODIPROP:mimetype=video/mp2t');
    const urls = fixture.channels.map(channel => `${isolatedWebHost.baseURL}/iptv/channel/${channel}.ts?mode=ts-legacy`);
    for (const url of urls) expect(body.split(/\r?\n/)).toContain(url);
    writeFileSync(`${external}/playlist.m3u`, body);
    for (const [index, url] of urls.entries()) {
      const path = info.outputPath(`receiver-${index}-prefix.ts`);
      const capture = await captureTeletextChannelTs(url, path, { maximumBytes: Math.floor(1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
      expect(capture.ended).toBe('byte-limit');
      const streams = JSON.parse(execFileSync(process.env.FFPROBE_PATH!, ['-v', 'error', '-show_streams', '-of', 'json', path],
        { timeout: 30_000, maxBuffer: 1024 * 1024 }).toString()).streams;
      expect(streams.map((stream: { codec_name: string }) => stream.codec_name).sort()).toEqual(['aac', 'dvb_subtitle', 'dvb_teletext', 'mpeg2video']);
      expect(streams.find((stream: { codec_type: string }) => stream.codec_type === 'audio').tags.language).toBe('nld');
      expect(streams.find((stream: { codec_name: string }) => stream.codec_name === 'dvb_subtitle').tags.language).toBe('eng');
      expect(streams.find((stream: { codec_name: string }) => stream.codec_name === 'dvb_teletext').id).toBe('0x200');
      const decoded = spawnSync(process.env.FFMPEG_PATH!, ['-v', 'error', '-nostdin', '-i', path, '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
        { timeout: 30_000, maxBuffer: 65_536 });
      writeFileSync(info.outputPath(`receiver-${index}-streams.json`), JSON.stringify(streams, null, 2));
      writeFileSync(info.outputPath(`receiver-${index}-decode.stderr`), decoded.stderr ?? '');
      writeFileSync(info.outputPath(`receiver-${index}-decode.json`), JSON.stringify({ status: decoded.status, signal: decoded.signal,
        error: decoded.error?.message }, null, 2));
      expect(decoded.error).toBeUndefined(); expect(decoded.signal).toBeNull();
      expect(decoded.status).toBe(0); expect(decoded.stderr.length).toBe(0);
    }
    const holdDeadline = Date.now() + hold * 1000;
    const readyMarker = { probe_version: 1, url: urls[0], urls, channels: fixture.channels,
      listen_address: isolatedWebHost.listenAddress, host: isolatedWebHost.processIdentity(), profiles, maps,
      host_sha256: sha256(readFileSync(process.env.E2E_SERVER_BIN!)),
      tools: Object.fromEntries([['ffmpeg', process.env.FFMPEG_PATH!], ['ffprobe', process.env.FFPROBE_PATH!], ['scanner', scanner!]]
        .map(([name, path]) => [name, { sha256: sha256(readFileSync(path)),
          version: name === 'scanner' ? 'Identity joined to the final package receipt; no scanner version CLI'
            : execFileSync(path, ['-version'], { encoding: 'utf8', timeout: 10_000, maxBuffer: 65_536 }).split('\n')[0] }])),
      addon: selected, repository_zip_sha256: candidateDigest, browser: browser.version(), scanner_sha256: scannerDigest,
      source_sha256: sourceDigest, scan, source_duration: scan.duration, real_occurrences_per_channel: count,
      first_programme: firstProgramme, capture_limits: { maximum_bytes: Math.floor(1024 * 1024 / 188) * 188, maximum_time_ms: 30_000 },
      selected_audio: 'nld660Hz', selected_captions: 'eng/page888 converted to DVB; source200 is not forwarded',
      update: { request: 'client-update-guide.json', result: 'host-update-complete.json',
        request_before_utc: new Date(holdDeadline - 65_000).toISOString(),
        requires: { probe_version: 1, url: urls[0], seed_ids: before.map(value => value!.SeedId), refresh: true } },
      scope: 'Production profile/publication/ordinary four-track host preflight only; no external renderer observation or client acceptance' };
    writeFileSync(`${external}/host-ready.json`, JSON.stringify(readyMarker, null, 2));
    let updated = false;
    await expect.poll(async () => {
      if (!updated && existsSync(`${external}/client-update-guide.json`)) {
        expect(JSON.parse(readFileSync(`${external}/client-update-guide.json`, 'utf8'))).toEqual(readyMarker.update.requires);
        expect(Date.now(), 'Refresh must fit the original external-hold deadline').toBeLessThan(holdDeadline - 65_000);
        // Keep this positive refresh inside the same bounded response policy
        // with the full programme count needed by the manual viewing window.
        const title = 'Guide update B';
        expect(fixture.database.prepare('UPDATE MovieMetadata SET Title=? WHERE MovieId=?').run(title, scan.item).changes).toBe(1);
        await expect.poll(() => current().every(row => {
          if (!row || row.RuntimeId === null || row.Revoked !== 0) return false;
          const snapshot = fixture.database.prepare('SELECT Document FROM TeletextGuideRuntimeSnapshot WHERE Id=? AND SeedId=?').get(row.RuntimeId, row.SeedId) as { Document: Uint8Array } | undefined;
          return snapshot !== undefined && Buffer.from(snapshot.Document).toString().includes(title);
        }), { timeout: Math.min(65_000, holdDeadline - Date.now()) }).toBe(true);
        writeFileSync(`${external}/host-update-complete.json`, JSON.stringify({ probe_version: 1, url: urls[0], current: current(), title,
          scope: 'Actual automatic refreshed snapshots; receiver display remains a separate observation' }, null, 2));
        updated = true;
      }
      return existsSync(`${external}/client-complete.json`);
    }, { timeout: hold * 1000 }).toBe(true);
    expect(JSON.parse(readFileSync(`${external}/client-complete.json`, 'utf8'))).toMatchObject({ probe_version: 1, url: urls[0], owned_client_closed: true });
    expect(updated, 'One controlled real guide refresh is required before completion').toBe(true);
    expect(fixture.channels.map(channel => sha256(originalBytes(fixture, channel)))).toEqual(originals);
    expect(fixture.channels.map(channel => acceptedGuide(fixture, channel))).toEqual(accepted);
    expect(installation(isolatedWebHost.databasePath)).toEqual(selected); compareInstalledFiles(selected);
    expect(sha256(readFileSync(source!))).toBe(sourceDigest); expect(fixture.unchangedPackage()).toBe(true); expect(errors).toEqual([]);
    writeFileSync(`${external}/host-success.json`, JSON.stringify({ ...readyMarker, updated, current: current(), errors,
      external_renderer_accepted: false, scope: 'Bounded host/controller completion; marker is not physical Kodi acceptance' }, null, 2));
  } finally { try { await context.close(); } finally { fixture.close(); } }
});
});

async function rasterBytes(page: Page): Promise<Buffer> {
  const image = page.getByTestId('teletext-preview-model').locator('img');
  await expect(image).toBeVisible();
  const source = (await image.getAttribute('src'))!;
  const data = source.match(/^data:image\/[^;,]+;base64,(.+)$/);
  expect(data, 'The native preview must retain its exact inline raster bytes.').not.toBeNull();
  return Buffer.from(data![1], 'base64');
}

function assertContinuousAuthoredPmt(capturePath: string, jsonPath: string, textPath: string): void {
  execFileSync('tstables', ['--tid', '2', '--json-output', jsonPath,
    '--text-output', textPath, capturePath], { timeout: 30_000, maxBuffer: 65_536 });
  const tables = JSON.parse(readFileSync(jsonPath, 'utf8')) as {
    '#name': string; version: number; '#nodes': { '#name': string; elementary_pid?: number }[];
  }[];
  let authoredSeen = false;
  for (const table of tables.filter(table => table['#name'] === 'PMT')) {
    const present = table['#nodes'].some(node => node['#name'] === 'component' && node.elementary_pid === 512);
    if (authoredSeen) expect(present, `Unexpected authored teletext withdrawal at PMT version ${table.version}`).toBe(true);
    authoredSeen ||= present;
  }
  expect(authoredSeen).toBe(true);
}

test('production TV-guide explicit publication and paired ordinary stream output', async ({ browser, baseURL }, info) => {
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  info.annotations.push({ type: 'browser-version', description: browser.version() },
    { type: 'production-package', description: `${addonId}${productionVersion}; repositoryZIP SHA256 ${candidateDigest}; contentSHA256 ${installed.ContentSha256}` });
  const fixture = new TeletextChannelFixture(databasePath!, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  try {
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 3, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    // Locale remains fixed: it is part of guide source authority, not just UI.
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    const profiles = [await createProfile(page, fixture, 'classic', true), await createProfile(page, fixture, 'night', true)];
    for (const channel of fixture.channels) await resetGuidePlayout(page, fixture, channel);
    const currentSource = fixture.database.prepare(`SELECT Id,Start,Finish,MediaItemId FROM PlayoutItem
      WHERE PlayoutId=? AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(fixture.channels[1], new Date().toISOString(), new Date().toISOString()) as { Id: number; Start: string; Finish: string; MediaItemId: number };
    expect(currentSource).toBeDefined();
    const originals: Buffer[] = [];
    const rasters: Buffer[] = [];
    const accepted: Record<string, unknown>[] = [];
    const urls: string[] = [];
    const text = translation('en_us');
    for (const [index, channel] of fixture.channels.entries()) {
      await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
      const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
      await binding.locator('[name="profile_choice"]').selectOption(`${profiles[index]}:1`);
      await submit(page, binding, text('TeletextBindingSave'), true);
      const form = page.getByTestId('teletext-authoring-form');
      await form.locator('[name="name"]').fill(`Production live guide ${channel}`);
      await form.locator('[name="entry_addon_id"]').selectOption(addonId);
      await form.locator('[name="entry_page"]').fill(index ? '410' : '200');
      await form.locator('[name="entry_subcode"]').fill('0000');
      await form.locator('[name="v1_region"]').selectOption('western_latin');
      await form.locator('[name="resource_transmissions"]').fill('[]');
      await form.locator('[name="guide_channel_id"]').selectOption(String(fixture.channels[1]));
      await submit(page, form, text('TeletextValidateSaveDraft'), true);
      await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
      await expect(page.getByTestId('teletext-preview-model')).toHaveAttribute('data-page', index ? '410' : '200');
      await expect(page.getByTestId('teletext-preview-theme')).toHaveText(index ? 'night' : 'classic');
      const original = savedOriginal(fixture, channel);
      expect(original.guide_source.channel_ids).toEqual([fixture.channels[1]]);
      expect(JSON.parse(original.guide_inputs.v3).channels.map((entry: { id: number }) => entry.id)).toEqual([fixture.channels[1]]);
      expect(original.documents[0].response).toContain('Synthetic red video and tone');
      originals.push(originalBytes(fixture, channel)); rasters.push(await rasterBytes(page)); urls.push(page.url());
      await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unrequested');
      await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
      await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
      expect(originalBytes(fixture, channel)).toEqual(originals[index]);
      expect(await rasterBytes(page)).toEqual(rasters[index]);
      await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
      await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-selected');
      await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
      expect(originalBytes(fixture, channel)).toEqual(originals[index]);
      expect(await rasterBytes(page)).toEqual(rasters[index]);
      accepted.push(acceptedGuide(fixture, channel));
      await page.getByTestId('teletext-publication').scrollIntoViewIfNeeded();
      await page.screenshot({ path: info.outputPath(`production-guide-${index ? 'night' : 'day'}-published.png`) });
    }
    // Both selected originals must remain inside the genuine programme role
    // boundary throughout the bounded pair of HTTP observations.
    expect(utc(currentSource.Finish) - Date.now()).toBeGreaterThan(30_000);
    const paths = fixture.channels.map((_, index) => info.outputPath(`production-guide-${index ? 'night' : 'day'}.ts`));
    const captures = await Promise.all(fixture.channels.map((channel, index) => captureTeletextChannelTs(
      `${baseURL}/iptv/hls-direct/${channel}.ts`, paths[index],
      { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 },
    )));
    for (const [index, capture] of captures.entries()) {
      expect(capture.ended).toBe('byte-limit');
      expect(capture.bytes.length % 188).toBe(0);
      const pids = new Set<number>();
      let malformed = 0;
      for (let offset = 0; offset < capture.bytes.length; offset += 188) {
        if (capture.bytes[offset] !== 0x47 || (capture.bytes[offset + 1] & 0x80) !== 0) malformed++;
        pids.add(((capture.bytes[offset + 1] & 0x1f) << 8) | capture.bytes[offset + 2]);
      }
      expect(malformed).toBe(0);
      for (const pid of [0, 256, 257, 512]) expect(pids.has(pid)).toBe(true);
      assertContinuousAuthoredPmt(paths[index], info.outputPath(`production-guide-${index ? 'night' : 'day'}.pmt.json`),
        info.outputPath(`production-guide-${index ? 'night' : 'day'}.pmt.txt`));
      execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
        '-i', paths[index], '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
      { timeout: 30_000, maxBuffer: 65_536 });
      expect(acceptedGuide(fixture, fixture.channels[index])).toEqual(accepted[index]);
      expect(originalBytes(fixture, fixture.channels[index])).toEqual(originals[index]);
      await page.goto(urls[index]); await ready(page, true);
      expect(await rasterBytes(page)).toEqual(rasters[index]);
    }
    expect(fixture.setting('teletext.work.max_concurrent')).toBe('1');
    for (const channel of fixture.channels) expect(fixture.scriptedIntervals(channel)).toEqual([]);
    expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM ChannelTeletextBinding WHERE AddonId=?').get(fixture.addonId)).toEqual({ count: 0 });
    expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM AddonSetting WHERE AddonId=?').get(addonId)).toEqual({ count: 0 });
    expect(fixture.calls()).toEqual([]);
    expect(installation()).toEqual(installed); compareInstalledFiles(); expect(errors).toEqual([]);
    writeFileSync(info.outputPath('production-guide-publication-identity.json'), JSON.stringify({
      repository_zip_sha256: candidateDigest, installation: { version: installed.Version,
        source_id: installed.SourceId, state_revision: installed.StateRevision,
        package_sha256: installed.PackageSha256, content_sha256: installed.ContentSha256 }, output_channels: fixture.channels,
      source_channels: [fixture.channels[1]], source_programme: currentSource, profiles,
      accepted_generations: accepted, original_sha256: originals.map(sha256), raster_sha256: rasters.map(sha256),
      captures: captures.map((capture, index) => ({ channel: fixture.channels[index], sha256: sha256(capture.bytes),
        bytes: capture.bytes.length, ended: capture.ended })), javaScriptEnabled: true,
    }, null, 2));
  } finally {
    try { await context?.close(); } finally { fixture.close(); }
  }
});

// One actual automatic-refresh boundary; unchanged editor/theme/isolation
// matrices retain their own canonical evidence instead of multiplying cases.
test('production TV-guide automatic source refresh on one ordinary HTTP response', async ({ browser, isolatedWebHost }, info) => {
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  const baseURL = isolatedWebHost.baseURL;
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  try {
    const selectedInstallation = await installProduction(browser, baseURL, isolatedWebHost.databasePath);
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    let page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    const channel = fixture.channels[0];
    const source = fixture.channels[1];
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const profile = await createProfile(page, fixture, 'classic', true);
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
    await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
    await submit(page, binding, text('TeletextBindingSave'), true);
    const form = page.getByTestId('teletext-authoring-form');
    for (const [name, value] of [['name', 'Production refresh original'], ['entry_page', '200'],
      ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) {
      await form.locator(`[name="${name}"]`).fill(value);
    }
    await form.locator('[name="entry_addon_id"]').selectOption(addonId);
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="guide_channel_id"]').selectOption(String(source));
    await submit(page, form, text('TeletextValidateSaveDraft'), true);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const historical = new URL(page.url());
    historical.searchParams.set('page', '201'); historical.searchParams.set('subcode', '0000');
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const accepted = acceptedGuide(fixture, channel);
    const original = originalBytes(fixture, channel);
    const initialHost = isolatedWebHost.processIdentity();
    await context.close();
    const restarted = await isolatedWebHost.restart();
    expect(restarted.before).toEqual(initialHost);
    expect(restarted.stop.closed).toBe(true);
    expect(restarted.stop.forced).toBe(false);
    expect(restarted.stop.exitCode).toBe(0);
    expect(restarted.stop.signalCode).toBeNull();
    expect(restarted.after.pid).toBeGreaterThan(0);
    expect(restarted.after.pid).not.toBe(initialHost.pid);
    expect(restarted.after.generation).toBeGreaterThan(initialHost.generation);
    expect(isolatedWebHost.processIdentity()).toEqual(restarted.after);
    expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(historical.toString()); await ready(page, true);
    const originalRaster = await rasterBytes(page);
    const currentUrl = `/channels/${channel}/teletext?guide_preview=1&guide_page=201&guide_subcode=0000`;
    await page.goto(currentUrl); await ready(page, true);
    const currentModel = page.getByTestId('teletext-current-guide-model');
    await expect(currentModel).toBeVisible();
    expect(Buffer.from((await currentModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64')).toEqual(originalRaster);
    const current = () => fixture.database.prepare('SELECT * FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1')
      .get(channel) as { SeedId: number; RuntimeId: number | null; Revision: number; NextDueAt: string; ChangedAt: string | null; Revoked: number };
    const before = current();
    expect(before.RuntimeId).toBeNull();
    const programme = fixture.database.prepare(`SELECT MediaItemId,Finish FROM PlayoutItem WHERE PlayoutId=?
      AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(source, new Date().toISOString(), new Date().toISOString()) as { MediaItemId: number; Finish: string };
    const updatedTitle = 'Updated guide programme';
    expect(fixture.database.prepare('UPDATE MovieMetadata SET Title=? WHERE MovieId=?')
      .run(updatedTitle, programme.MediaItemId).changes).toBe(1);
    await expect.poll(() => current().ChangedAt !== null, { timeout: 10_000 }).toBe(true);
    const changed = current();
    expect(utc(changed.NextDueAt) - utc(changed.ChangedAt!)).toBeGreaterThanOrEqual(59_999);
    expect(utc(programme.Finish) - utc(changed.NextDueAt)).toBeGreaterThan(30_000);
    // Wait BEFORE opening the response. Preserve the established 30s/8MiB
    // observation bounds, genuine debounce and source/role clocks.
    await expect.poll(() => utc(changed.NextDueAt) - Date.now() <= 1500, { timeout: 65_000, intervals: [100] }).toBe(true);
    const path = info.outputPath('production-guide-automatic-refresh.ts');
    const capture = await captureTeletextChannelTs(`${baseURL}/iptv/hls-direct/${channel}.ts`, path,
      { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
    expect(capture.ended).toBe('byte-limit');
    assertContinuousAuthoredPmt(path, info.outputPath('production-guide-automatic-refresh.pmt.json'),
      info.outputPath('production-guide-automatic-refresh.pmt.txt'));
    const after = current();
    expect(after.SeedId).toBe(before.SeedId);
    expect(after.RuntimeId).not.toBeNull();
    const snapshot = fixture.database.prepare('SELECT Document,DocumentDigest FROM TeletextGuideRuntimeSnapshot WHERE Id=? AND SeedId=?')
      .get(after.RuntimeId!, after.SeedId) as { Document: Uint8Array; DocumentDigest: string };
    const runtimeBytes = Buffer.from(snapshot.Document);
    expect(sha256(runtimeBytes)).toBe(snapshot.DocumentDigest);
    expect(runtimeBytes.toString()).toContain(updatedTitle);
    const decoded = execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
      '-txt_format', 'text', '-txt_page', '201', '-txt_duration', '1000', '-i', path,
      '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
    writeFileSync(info.outputPath('production-guide-automatic-refresh.srt'), decoded);
    expect(decoded).toContain('Now: Synthetic red video and tone');
    expect(decoded).toContain(`Now: ${updatedTitle}`);
    expect(decoded.indexOf('Now: Synthetic red video and tone')).toBeLessThan(decoded.indexOf(`Now: ${updatedTitle}`));
    execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
      '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
    await page.goto(currentUrl); await ready(page, true);
    await expect(currentModel).toHaveAttribute('data-snapshot', String(after.RuntimeId));
    const freshRaster = Buffer.from((await currentModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64');
    expect(freshRaster).not.toEqual(originalRaster);
    await currentModel.locator('img').scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-automatic-current.png') });
    await page.goto(historical.toString()); await ready(page, true);
    expect(await rasterBytes(page)).toEqual(originalRaster);
    expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    await submit(page, page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-disable-form"]`), text('TeletextBindingOff'), true);
    await expect.poll(() => current().Revoked, { timeout: 10_000 }).toBe(1);
    expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    expect(fixture.setting('teletext.work.max_concurrent')).toBe('1');
    expect(errors).toEqual([]);
    expect(installation(isolatedWebHost.databasePath)).toEqual(selectedInstallation);
    compareInstalledFiles(selectedInstallation);
    writeFileSync(info.outputPath('production-guide-refresh-identity.json'), JSON.stringify({ repository_zip_sha256: candidateDigest,
      installation: selectedInstallation, restart: restarted,
      channel, source, profile, before, changed, after, accepted_generation: accepted,
      original_sha256: sha256(original), original_raster_sha256: sha256(originalRaster),
      fresh_raster_sha256: sha256(freshRaster), runtime_document_sha256: snapshot.DocumentDigest,
      capture: { bytes: capture.bytes.length, ended: capture.ended, sha256: sha256(capture.bytes) },
      scope: 'Explicit publication retained across a closed graceful host restart, one actual SQL change, one same-response ordinary stream update, independent page201 text decoding, shared current raster, immutable historical original and explicit Off. Not full restart/platform/client acceptance.' }, null, 2));
  } finally {
    try { await context?.close(); } finally { fixture.close(); }
  }
});

test('production TV-guide retains last good on source failure and recovers before its original cap', async ({ browser, isolatedWebHost }, info) => {
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  const baseURL = isolatedWebHost.baseURL;
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  let sourcePlayout: { Id: number; ChannelId: number; ScheduleKind: number; ScheduleFile: string | null } | undefined;
  try {
    const selectedInstallation = await installProduction(browser, baseURL, isolatedWebHost.databasePath);
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    let page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    const channel = fixture.channels[0];
    const source = fixture.channels[1];
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const profile = await createProfile(page, fixture, 'classic', true);
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
    await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
    await submit(page, binding, text('TeletextBindingSave'), true);
    const form = page.getByTestId('teletext-authoring-form');
    for (const [name, value] of [['name', 'Production source recovery original'], ['entry_page', '200'],
      ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) {
      await form.locator(`[name="${name}"]`).fill(value);
    }
    await form.locator('[name="entry_addon_id"]').selectOption(addonId);
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="guide_channel_id"]').selectOption(String(source));
    await submit(page, form, text('TeletextValidateSaveDraft'), true);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const historical = new URL(page.url());
    historical.searchParams.set('page', '201'); historical.searchParams.set('subcode', '0000');
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const accepted = acceptedGuide(fixture, channel);
    const original = originalBytes(fixture, channel);
    const initialHost = isolatedWebHost.processIdentity();
    await context.close();
    const restarted = await isolatedWebHost.restart();
    expect(restarted.before).toEqual(initialHost);
    expect(restarted.stop.closed).toBe(true);
    expect(restarted.stop.forced).toBe(false);
    expect(restarted.stop.exitCode).toBe(0);
    expect(restarted.stop.signalCode).toBeNull();
    expect(restarted.after.pid).toBeGreaterThan(0);
    expect(restarted.after.pid).not.toBe(initialHost.pid);
    expect(restarted.after.generation).toBeGreaterThan(initialHost.generation);
    expect(isolatedWebHost.processIdentity()).toEqual(restarted.after);
    expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(historical.toString()); await ready(page, true);
    const originalRaster = await rasterBytes(page);
    const currentUrl = `/channels/${channel}/teletext?guide_preview=1&guide_page=201&guide_subcode=0000`;
    await page.goto(currentUrl); await ready(page, true);
    const currentModel = page.getByTestId('teletext-current-guide-model');
    await expect(currentModel).toBeVisible();
    expect(Buffer.from((await currentModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64')).toEqual(originalRaster);
    type RefreshCurrent = {
      SeedId: number; RuntimeId: number | null; Revision: number; NextDueAt: string;
      LastSuccessAt: string; ChangedAt: string | null; FirstFailureAt: string | null;
      Failure: number | null; Revoked: number; LeaseOwner: string | null;
    };
    const current = () => fixture.database.prepare('SELECT * FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1')
      .get(channel) as RefreshCurrent;
    const history = () => ({
      designs: fixture.designs(channel),
      revisions: fixture.database.prepare('SELECT DesignId,Revision,Digest FROM TeletextDesignRevision WHERE DesignId=? ORDER BY Revision')
        .all(fixture.designs(channel)[0].Id),
      requests: fixture.database.prepare('SELECT * FROM TeletextPublicationRequest WHERE ChannelId=? ORDER BY Id').all(channel),
      generations: fixture.database.prepare('SELECT * FROM TeletextAcceptedGeneration WHERE ChannelId=? ORDER BY Id').all(channel),
    });
    const retainedHistory = history();
    const before = current();
    expect(savedOriginal(fixture, channel).guide_source.channel_ids).toEqual([source]);
    expect(before.RuntimeId).toBeNull(); expect(before.Failure).toBeNull();
    expect(source).not.toBe(channel);
    expect(fixture.database.prepare('SELECT MirrorSourceChannelId FROM Channel WHERE Id=?').get(channel))
      .toEqual({ MirrorSourceChannelId: null });
    sourcePlayout = fixture.database.prepare('SELECT Id,ChannelId,ScheduleKind,ScheduleFile FROM Playout WHERE ChannelId=?')
      .get(source) as typeof sourcePlayout;
    expect(sourcePlayout).toBeDefined(); expect(sourcePlayout!.ScheduleKind).toBe(4);
    expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM PlayoutExternalJsonSnapshot WHERE PlayoutId=?')
      .get(sourcePlayout!.Id)).toEqual({ count: 0 });
    const seed = fixture.database.prepare('SELECT ValidUntil FROM TeletextGuideRefreshSeed WHERE Id=?')
      .get(before.SeedId) as { ValidUntil: string };
    const programme = fixture.database.prepare(`SELECT MediaItemId,Finish FROM PlayoutItem WHERE PlayoutId=?
      AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(sourcePlayout!.Id, new Date().toISOString(), new Date().toISOString()) as { MediaItemId: number; Finish: string };
    const receipts: Record<string, unknown> = { repository_zip_sha256: candidateDigest,
      installation: selectedInstallation, restart: restarted, channel, source, profile, before,
      source_playout: sourcePlayout, original_sha256: sha256(original), original_raster_sha256: sha256(originalRaster),
      accepted_generation: accepted, retained_history: retainedHistory };
    const persist = () => writeFileSync(info.outputPath('production-guide-failure-recovery-identity.json'), JSON.stringify(receipts, null, 2));
    persist();
    // A missing accepted ExternalJson snapshot is a real guide read failure,
    // rather than an explicitly empty guide or an unrelated output-AV fault.
    expect(fixture.database.prepare('UPDATE Playout SET ScheduleKind=20 WHERE Id=? AND ChannelId=?')
      .run(sourcePlayout!.Id, source).changes).toBe(1);
    await expect.poll(() => current().FirstFailureAt !== null, { timeout: 10_000 }).toBe(true);
    const lost = current();
    expect(lost.Failure).toBe(1); expect(lost.RuntimeId).toBeNull();
    expect(lost.ChangedAt).not.toBeNull(); expect(lost.Revoked).toBe(0);
    expect(lost.LastSuccessAt).toBe(before.LastSuccessAt);
    const originalCap = Math.min(utc(seed.ValidUntil), utc(lost.FirstFailureAt!) + 15 * 60_000);
    expect(originalCap).toBeLessThanOrEqual(utc(programme.Finish));
    expect(utc(lost.NextDueAt) - utc(lost.ChangedAt!)).toBeGreaterThanOrEqual(59_999);
    receipts.lost = lost; receipts.original_stale_cap_utc_ms = originalCap; persist();

    const healthReceipts: unknown[] = [];
    const viewHealth = async (module: string, stale: boolean): Promise<void> => {
      const localized = translation(module);
      await page.goto('/system/health'); await ready(page, true);
      const response = page.waitForResponse(value => value.request().method() === 'POST'
        && new URL(value.url()).pathname.startsWith('/admin-api/refresh_health_snapshot'));
      await page.getByRole('button', { name: localized('Refresh'), exact: true }).click();
      expect((await response).status()).toBe(200);
      await expect(page.getByRole('button', { name: localized('Refresh'), exact: true })).toBeEnabled();
      const row = page.getByRole('row').filter({ has: page.getByText(localized('TeletextGuideMaintenanceTitle'), { exact: true }) });
      await expect(row).toHaveCount(1);
      await expect(row.locator('span[aria-label]').first()).toHaveAttribute('aria-label', localized(stale ? 'Warning' : 'Pass'));
      const stored = (JSON.parse(fixture.setting('health.results')!) as { title: string; status: string; message: string }[])
        .find(result => result.title === 'Teletext guide maintenance');
      expect(stored?.status).toBe(stale ? 'Warning' : 'Pass');
      expect(stored?.message).toContain(stale ? 'fresh=0, stale=1, unavailable=0, revoked=0' : 'fresh=1, stale=0, unavailable=0, revoked=0');
      await row.scrollIntoViewIfNeeded();
      await page.screenshot({ path: info.outputPath(`production-guide-${module}-${stale ? 'stale' : 'recovered'}-health.png`) });
      healthReceipts.push({ module, stale, result: stored });
      receipts.health = healthReceipts; persist();
    };
    // Locale also belongs to the source fence. Localized Stale views are safe
    // while the source is already unavailable; restore en-us before recovery.
    for (const [language, module] of [['en-us', 'en_us'], ['nl', 'nl']] as const) {
      expect((await page.request.post('/admin-api/settings/ui/save', {
        form: { language, is_dark_mode: 'false' }, maxRedirects: 0,
      })).status()).toBe(303);
      await page.goto(currentUrl); await ready(page, true);
      await expect(page.getByTestId('teletext-current-guide-model')).toContainText(translation(module)('TeletextGuideStale'));
      expect(Buffer.from((await page.getByTestId('teletext-current-guide-model').locator('img').getAttribute('src'))!.split(',')[1], 'base64'))
        .toEqual(originalRaster);
      await page.getByTestId('teletext-current-guide-model').scrollIntoViewIfNeeded();
      await page.screenshot({ path: info.outputPath(`production-guide-${module}-stale-preview.png`) });
      await viewHealth(module, true);
    }
    expect((await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    })).status()).toBe(303);
    expect(fixture.setting('pages.language')).toBe('en-us');
    expect(current().FirstFailureAt).toBe(lost.FirstFailureAt);
    expect(history()).toEqual(retainedHistory);

    const capture = async (name: string) => {
      const path = info.outputPath(`production-guide-${name}.ts`);
      const result = await captureTeletextChannelTs(`${baseURL}/iptv/hls-direct/${channel}.ts`, path,
        { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
      expect(result.ended).toBe('byte-limit');
      assertContinuousAuthoredPmt(path, info.outputPath(`production-guide-${name}.pmt.json`), info.outputPath(`production-guide-${name}.pmt.txt`));
      const decoded = execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
        '-txt_format', 'text', '-txt_page', '201', '-txt_duration', '1000', '-i', path,
        '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
      writeFileSync(info.outputPath(`production-guide-${name}.srt`), decoded);
      execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
        '-t', '1', '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
      return { decoded, bytes: result.bytes.length, ended: result.ended, sha256: sha256(result.bytes) };
    };
    await expect.poll(() => utc(lost.NextDueAt) - Date.now() <= 1500, { timeout: 65_000, intervals: [100] }).toBe(true);
    const missingCapture = await capture('missing-source-last-good');
    expect(missingCapture.decoded).toContain('Now: Synthetic red video and tone');
    await expect.poll(() => current().LeaseOwner === null && utc(current().NextDueAt) > utc(lost.NextDueAt), { timeout: 10_000 }).toBe(true);
    const failed = current();
    expect(failed.Failure).toBe(1); expect(failed.RuntimeId).toBeNull();
    expect(failed.FirstFailureAt).toBe(lost.FirstFailureAt);
    expect(failed.LastSuccessAt).toBe(before.LastSuccessAt);
    expect(utc(failed.NextDueAt) - utc(lost.NextDueAt)).toBeGreaterThanOrEqual(59_999);
    expect(Math.min(utc(seed.ValidUntil), utc(failed.FirstFailureAt!) + 15 * 60_000)).toBe(originalCap);
    expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM TeletextGuideRuntimeSnapshot WHERE SeedId=?').get(before.SeedId))
      .toEqual({ count: 0 });
    expect(history()).toEqual(retainedHistory); expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    receipts.failed = failed; receipts.missing_capture = missingCapture; persist();

    expect(fixture.database.prepare('UPDATE Playout SET ScheduleKind=?,ScheduleFile=? WHERE Id=? AND ChannelId=?')
      .run(sourcePlayout!.ScheduleKind, sourcePlayout!.ScheduleFile, sourcePlayout!.Id, source).changes).toBe(1);
    expect(fixture.database.prepare('SELECT Id,ChannelId,ScheduleKind,ScheduleFile FROM Playout WHERE Id=?')
      .get(sourcePlayout!.Id)).toEqual(sourcePlayout);
    const updatedTitle = 'Recovered guide programme';
    expect(fixture.database.prepare('UPDATE MovieMetadata SET Title=? WHERE MovieId=?')
      .run(updatedTitle, programme.MediaItemId).changes).toBe(1);
    expect(utc(failed.NextDueAt) + 30_000).toBeLessThan(originalCap);
    await expect.poll(() => utc(failed.NextDueAt) - Date.now() <= 1500, { timeout: 65_000, intervals: [100] }).toBe(true);
    const recoveryCapture = await capture('source-recovery-transition');
    expect(recoveryCapture.decoded).toContain('Now: Synthetic red video and tone');
    expect(recoveryCapture.decoded).toContain(`Now: ${updatedTitle}`);
    expect(recoveryCapture.decoded.indexOf('Now: Synthetic red video and tone'))
      .toBeLessThan(recoveryCapture.decoded.indexOf(`Now: ${updatedTitle}`));
    const recovered = current();
    expect(recovered.RuntimeId).not.toBeNull(); expect(recovered.SeedId).toBe(before.SeedId);
    expect(recovered.Failure).toBeNull(); expect(recovered.FirstFailureAt).toBeNull();
    expect(recovered.ChangedAt).toBeNull(); expect(recovered.LeaseOwner).toBeNull(); expect(recovered.Revoked).toBe(0);
    expect(utc(recovered.LastSuccessAt)).toBeLessThan(originalCap);
    expect(utc(recovered.LastSuccessAt)).toBeGreaterThan(utc(before.LastSuccessAt));
    const snapshot = fixture.database.prepare('SELECT Document,DocumentDigest FROM TeletextGuideRuntimeSnapshot WHERE Id=? AND SeedId=?')
      .get(recovered.RuntimeId!, recovered.SeedId) as { Document: Uint8Array; DocumentDigest: string };
    const runtimeBytes = Buffer.from(snapshot.Document);
    expect(sha256(runtimeBytes)).toBe(snapshot.DocumentDigest); expect(runtimeBytes.toString()).toContain(updatedTitle);
    await page.goto(currentUrl); await ready(page, true);
    const recoveredModel = page.getByTestId('teletext-current-guide-model');
    await expect(recoveredModel).toHaveAttribute('data-snapshot', String(recovered.RuntimeId));
    await expect(recoveredModel).toContainText(text('TeletextGuideFresh'));
    const recoveredRaster = Buffer.from((await recoveredModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64');
    expect(recoveredRaster).not.toEqual(originalRaster);
    await recoveredModel.scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-en_us-recovered-preview.png') });
    await viewHealth('en_us', false);
    await page.goto(historical.toString()); await ready(page, true);
    expect(await rasterBytes(page)).toEqual(originalRaster);
    expect(history()).toEqual(retainedHistory); expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted); expect(fixture.calls()).toEqual([]);
    expect(fixture.setting('teletext.work.max_concurrent')).toBe('1');
    expect(installation(isolatedWebHost.databasePath)).toEqual(selectedInstallation); compareInstalledFiles(selectedInstallation);
    expect(errors).toEqual([]);
    receipts.recovered = recovered; receipts.recovery_capture = recoveryCapture;
    receipts.runtime_document_sha256 = snapshot.DocumentDigest; receipts.recovered_raster_sha256 = sha256(recoveredRaster);
    receipts.scope = 'Actual missing accepted guide source, retained stale page201 and continuous AV/PMT, real failed due attempt and fixed stale cap, exact-source restoration and same-response recovery before that cap. English/Dutch Stale views; English Fresh recovery. Not producer-timeout, whole restart/platform/client or full P9B acceptance.';
    persist();
  } finally {
    try {
      if (sourcePlayout) fixture.database.prepare('UPDATE Playout SET ScheduleKind=?,ScheduleFile=? WHERE Id=? AND ChannelId=?')
        .run(sourcePlayout.ScheduleKind, sourcePlayout.ScheduleFile, sourcePlayout.Id, sourcePlayout.ChannelId);
    } finally {
      try { await context?.close(); } finally { fixture.close(); }
    }
  }
});

test('production TV-guide follows a natural programme-clock rollover on one ordinary response', async ({ browser, isolatedWebHost }, info) => {
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  const baseURL = isolatedWebHost.baseURL;
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  try {
    const selectedInstallation = await installProduction(browser, baseURL, isolatedWebHost.databasePath);
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    let page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    const channel = fixture.channels[0];
    const source = fixture.channels[1];
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const profile = await createProfile(page, fixture, 'classic', true);
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    const sourceRowsBefore = fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start')
      .all(source) as ({ Id: number; Start: string; Finish: string; MediaItemId: number } & Record<string, unknown>)[];
    expect(sourceRowsBefore).toHaveLength(3);
    expect(utc(sourceRowsBefore[0].Start)).toBeLessThanOrEqual(Date.now());
    expect(utc(sourceRowsBefore[0].Finish)).toBeGreaterThan(Date.now());
    const sourcePath = fixture.database.prepare('SELECT LibraryPathId FROM MediaItem WHERE Id=?')
      .get(sourceRowsBefore[0].MediaItemId) as { LibraryPathId: number };
    const sourceTitles = ['Clock Alpha', 'Clock Bravo', 'Clock Charlie'];
    const sourceMedia: number[] = [];
    const outputRows = fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(channel);
    const metadataAt = new Date().toISOString().replace('T', ' ').replace('Z', '');
    fixture.database.exec('BEGIN IMMEDIATE;');
    try {
      for (const [index, row] of sourceRowsBefore.entries()) {
        const id = (fixture.database.prepare(`SELECT MAX(Id)+1 AS Id FROM (
          SELECT Id FROM MediaItem UNION ALL SELECT Id FROM Movie UNION ALL SELECT Id FROM MovieMetadata)`).get() as { Id: number }).Id;
        fixture.database.prepare('INSERT INTO MediaItem(Id,LibraryPathId) VALUES (?,?)').run(id, sourcePath.LibraryPathId);
        fixture.database.prepare('INSERT INTO Movie(Id) VALUES (?)').run(id);
        fixture.database.prepare('INSERT INTO MovieMetadata(Id,MovieId,DateAdded,DateUpdated,MetadataKind,Title) VALUES (?,?,?,?,0,?)')
          .run(id, id, metadataAt, metadataAt, sourceTitles[index]);
        expect(fixture.database.prepare('UPDATE PlayoutItem SET MediaItemId=? WHERE Id=? AND PlayoutId=?')
          .run(id, row.Id, source).changes).toBe(1);
        sourceMedia.push(id);
      }
      fixture.database.exec('COMMIT;');
    } catch (error) { fixture.database.exec('ROLLBACK;'); throw error; }
    const sourceRows = fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start')
      .all(source) as typeof sourceRowsBefore;
    expect(sourceRows.map(({ MediaItemId, ...unchanged }) => unchanged))
      .toEqual(sourceRowsBefore.map(({ MediaItemId, ...unchanged }) => unchanged));
    expect(sourceRows.map(row => row.MediaItemId)).toEqual(sourceMedia);
    expect(fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(channel)).toEqual(outputRows);
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
    await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
    await submit(page, binding, text('TeletextBindingSave'), true);
    const form = page.getByTestId('teletext-authoring-form');
    for (const [name, value] of [['name', 'Production programme clock original'], ['entry_page', '200'],
      ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) {
      await form.locator(`[name="${name}"]`).fill(value);
    }
    await form.locator('[name="entry_addon_id"]').selectOption(addonId);
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="guide_channel_id"]').selectOption(String(source));
    await submit(page, form, text('TeletextValidateSaveDraft'), true);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const historical = new URL(page.url());
    historical.searchParams.set('page', '201'); historical.searchParams.set('subcode', '0000');
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const accepted = acceptedGuide(fixture, channel);
    const original = originalBytes(fixture, channel);
    const initialHost = isolatedWebHost.processIdentity();
    await context.close();
    const restarted = await isolatedWebHost.restart();
    expect(restarted.before).toEqual(initialHost);
    expect(restarted.stop.closed).toBe(true);
    expect(restarted.stop.forced).toBe(false);
    expect(restarted.stop.exitCode).toBe(0);
    expect(restarted.stop.signalCode).toBeNull();
    expect(restarted.after.pid).toBeGreaterThan(0);
    expect(restarted.after.pid).not.toBe(initialHost.pid);
    expect(restarted.after.generation).toBeGreaterThan(initialHost.generation);
    expect(isolatedWebHost.processIdentity()).toEqual(restarted.after);
    expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(historical.toString()); await ready(page, true);
    const originalRaster = await rasterBytes(page);
    const currentUrl = `/channels/${channel}/teletext?guide_preview=1&guide_page=201&guide_subcode=0000`;
    await page.goto(currentUrl); await ready(page, true);
    const currentModel = page.getByTestId('teletext-current-guide-model');
    await expect(currentModel).toBeVisible();
    expect(Buffer.from((await currentModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64')).toEqual(originalRaster);
    const nanos = (value: string): bigint => {
      const match = value.match(/^(\d{4}-\d\d-\d\d[ T]\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?(?:Z|\+00:00)?$/);
      expect(match, 'Source and qualification clocks must retain exact UTC nanoseconds.').not.toBeNull();
      return BigInt(utc(match![1])) * 1_000_000n + BigInt((match![2] ?? '').padEnd(9, '0'));
    };
    type ClockCurrent = { SeedId: number; RuntimeId: number | null; Revision: number; NextDueAt: string;
      LastSuccessAt: string; Failure: number | null; FirstFailureAt: string | null; LeaseOwner: string | null; Revoked: number };
    const current = () => fixture.database.prepare('SELECT * FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1')
      .get(channel) as ClockCurrent;
    const history = () => ({ designs: fixture.designs(channel),
      revisions: fixture.database.prepare('SELECT DesignId,Revision,Digest FROM TeletextDesignRevision WHERE DesignId=? ORDER BY Revision')
        .all(fixture.designs(channel)[0].Id),
      requests: fixture.database.prepare('SELECT * FROM TeletextPublicationRequest WHERE ChannelId=? ORDER BY Id').all(channel),
      generations: fixture.database.prepare('SELECT * FROM TeletextAcceptedGeneration WHERE ChannelId=? ORDER BY Id').all(channel) });
    const retainedHistory = history();
    const profiles = fixture.database.prepare('SELECT * FROM TeletextProfile WHERE Id=?').all(profile);
    const profileSettings = fixture.database.prepare('SELECT * FROM TeletextProfileSetting WHERE ProfileId=? ORDER BY Key').all(profile);
    const before = current();
    const seed = fixture.database.prepare('SELECT Source,ValidUntil FROM TeletextGuideRefreshSeed WHERE Id=?')
      .get(before.SeedId) as { Source: Uint8Array; ValidUntil: string };
    const seedSource = JSON.parse(Buffer.from(seed.Source).toString()) as { channel_ids: number[]; captured_at: string };
    expect(seedSource.channel_ids).toEqual([source]);
    expect(before.RuntimeId).toBeNull(); expect(before.Failure).toBeNull();
    expect(nanos(seed.ValidUntil)).toBe(nanos(sourceRows[1].Start));
    expect(nanos(before.NextDueAt)).toBe(nanos(sourceRows[1].Start));
    expect(nanos(sourceRows[0].Finish)).toBe(nanos(sourceRows[1].Start));
    expect(nanos(sourceRows[1].Finish)).toBe(nanos(sourceRows[2].Start));
    expect(nanos(seedSource.captured_at)).toBeLessThan(nanos(sourceRows[1].Start));
    const receipts: Record<string, unknown> = { repository_zip_sha256: candidateDigest,
      installation: selectedInstallation, restart: restarted, channel, source, profile, source_titles: sourceTitles,
      source_media: sourceMedia, source_rows: sourceRows, before, seed_source: seedSource, old_role_until: seed.ValidUntil,
      accepted_generation: accepted, original_sha256: sha256(original), original_raster_sha256: sha256(originalRaster) };
    const persist = () => writeFileSync(info.outputPath('production-guide-clock-rollover-identity.json'), JSON.stringify(receipts, null, 2));
    persist();
    const outputNumber = (fixture.database.prepare('SELECT Number FROM Channel WHERE Id=?')
      .get(channel) as { Number: string }).Number;
    expect(outputNumber).toBeTruthy();
    const streamUrl = `${baseURL}/iptv/channel/${encodeURIComponent(outputNumber)}.ts?mode=ts-legacy`;
    receipts.stream_route = streamUrl;
    // The public continuous wrapper owns the single response across AV items;
    // hls-direct is finite per item. Allow wrapper startup before the boundary
    // while retaining the original30s/8MiB observation and real source clocks.
    await expect.poll(() => utc(sourceRows[1].Start) - Date.now() <= 10_000,
      { timeout: 180_000, intervals: [100] }).toBe(true);
    const openedAt = new Date().toISOString();
    expect(nanos(openedAt)).toBeLessThan(nanos(sourceRows[1].Start));
    receipts.capture_opened_at = openedAt; persist();
    const path = info.outputPath('production-guide-clock-rollover.ts');
    const capture = await captureTeletextChannelTs(streamUrl, path,
      { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
    const closedAt = new Date().toISOString();
    receipts.capture_closed_at = closedAt;
    receipts.capture = { bytes: capture.bytes.length, ended: capture.ended, sha256: sha256(capture.bytes) }; persist();
    expect(capture.ended).toBe('byte-limit');
    expect(nanos(closedAt)).toBeGreaterThan(nanos(sourceRows[1].Start));
    const after = current();
    receipts.after = after; persist();
    expect(after.SeedId).toBe(before.SeedId); expect(after.RuntimeId).not.toBeNull();
    expect(after.Failure).toBeNull(); expect(after.FirstFailureAt).toBeNull(); expect(after.LeaseOwner).toBeNull();
    expect(after.Revoked).toBe(0);
    const snapshot = fixture.database.prepare('SELECT Document,DocumentDigest,Source,ValidUntil FROM TeletextGuideRuntimeSnapshot WHERE Id=? AND SeedId=?')
      .get(after.RuntimeId!, after.SeedId) as { Document: Uint8Array; DocumentDigest: string; Source: Uint8Array; ValidUntil: string };
    const runtimeBytes = Buffer.from(snapshot.Document);
    const runtimeSource = JSON.parse(Buffer.from(snapshot.Source).toString()) as { channel_ids: number[]; captured_at: string };
    receipts.runtime_source = runtimeSource; receipts.runtime_valid_until = snapshot.ValidUntil;
    receipts.runtime_document_sha256 = snapshot.DocumentDigest; persist();
    expect(sha256(runtimeBytes)).toBe(snapshot.DocumentDigest);
    expect(runtimeSource.channel_ids).toEqual([source]);
    expect(nanos(runtimeSource.captured_at)).toBeGreaterThanOrEqual(nanos(sourceRows[1].Start));
    expect(nanos(snapshot.ValidUntil)).toBe(nanos(sourceRows[1].Finish));
    expect(nanos(after.NextDueAt)).toBe(nanos(sourceRows[1].Finish));
    const tablesPath = info.outputPath('production-guide-clock-rollover.pmt.json');
    execFileSync('tstables', ['--tid', '2', '--json-output', tablesPath,
      '--text-output', info.outputPath('production-guide-clock-rollover.pmt.txt'), path],
    { timeout: 30_000, maxBuffer: 65_536 });
    const tables = JSON.parse(readFileSync(tablesPath, 'utf8')) as {
      '#name': string; version: number; '#nodes': { '#name': string; elementary_pid?: number }[];
    }[];
    const presence = tables.filter(table => table['#name'] === 'PMT').map(table => ({
      version: table.version, present: table['#nodes'].some(node => node['#name'] === 'component' && node.elementary_pid === 512),
    }));
    receipts.pmt_presence = presence; persist();
    const initialPresence = presence.findIndex(value => value.present);
    expect(initialPresence).toBeGreaterThanOrEqual(0);
    const withdrawal = presence.findIndex((value, index) => index > initialPresence && !value.present);
    expect(withdrawal, 'Expired old-role output must withdraw while the new role is being qualified.').toBeGreaterThan(initialPresence);
    expect(presence.findIndex((value, index) => index > withdrawal && value.present),
      'A newly qualified role must be reannounced on the same response.').toBeGreaterThan(withdrawal);
    const decoded = execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
      '-txt_format', 'text', '-txt_page', '201', '-txt_duration', '1000', '-i', path,
      '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
    writeFileSync(info.outputPath('production-guide-clock-rollover.srt'), decoded);
    expect(decoded).toContain('Now: Clock Alpha'); expect(decoded).toContain('Next: Clock Bravo');
    expect(decoded).toContain('Now: Clock Bravo'); expect(decoded).toContain('Next: Clock Charlie');
    expect(decoded.indexOf('Now: Clock Alpha')).toBeLessThan(decoded.indexOf('Now: Clock Bravo'));
    execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
      '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
    await page.goto(currentUrl); await ready(page, true);
    const refreshedModel = page.getByTestId('teletext-current-guide-model');
    await expect(refreshedModel).toHaveAttribute('data-snapshot', String(after.RuntimeId));
    await expect(refreshedModel).toContainText(text('TeletextGuideFresh'));
    const refreshedRaster = Buffer.from((await refreshedModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64');
    expect(refreshedRaster).not.toEqual(originalRaster);
    await refreshedModel.scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-natural-clock-current.png') });
    await page.goto(historical.toString()); await ready(page, true);
    expect(await rasterBytes(page)).toEqual(originalRaster);
    expect(originalBytes(fixture, channel)).toEqual(original); expect(history()).toEqual(retainedHistory);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    expect(fixture.database.prepare('SELECT * FROM TeletextProfile WHERE Id=?').all(profile)).toEqual(profiles);
    expect(fixture.database.prepare('SELECT * FROM TeletextProfileSetting WHERE ProfileId=? ORDER BY Key').all(profile)).toEqual(profileSettings);
    expect(fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(source)).toEqual(sourceRows);
    expect(fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(channel)).toEqual(outputRows);
    expect(installation(isolatedWebHost.databasePath)).toEqual(selectedInstallation); compareInstalledFiles(selectedInstallation);
    expect(fixture.calls()).toEqual([]); expect(fixture.setting('teletext.work.max_concurrent')).toBe('1');
    expect(errors).toEqual([]);
    receipts.current_raster_sha256 = sha256(refreshedRaster);
    receipts.scope = 'One natural180s programme-role rollover, same-response independent page201 decoding and full captured AV decode, PMT withdrawal/reannouncement, exact UTC source qualification, immutable originals/history/profiles. Capture UTC and PMT states do not establish byte-level expiry timing; reuse native precise-expiry proof. Not platform/client or full P9B acceptance.';
    persist();
  } finally {
    try { await context?.close(); } finally { fixture.close(); }
  }
});

test('production TV-guide TTI footer concept survives explicit publication and automatic refresh', async ({ browser, isolatedWebHost }, info) => {
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  const baseURL = isolatedWebHost.baseURL;
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  try {
    const selectedInstallation = await installProduction(browser, baseURL, isolatedWebHost.databasePath);
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    let page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    const channel = fixture.channels[0];
    const source = fixture.channels[1];
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const profile = await createProfile(page, fixture, 'classic', true);
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
    await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
    await submit(page, binding, text('TeletextBindingSave'), true);
    const form = page.getByTestId('teletext-authoring-form');
    for (const [name, value] of [['name', 'Production TTI footer original'], ['entry_page', '200'],
      ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) {
      await form.locator(`[name="${name}"]`).fill(value);
    }
    await form.locator('[name="entry_addon_id"]').selectOption(addonId);
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="guide_channel_id"]').selectOption(String(source));
    await submit(page, form, text('TeletextValidateSaveDraft'), true);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const historical = new URL(page.url());
    historical.searchParams.set('page', '201'); historical.searchParams.set('subcode', '0000');
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const originalAccepted = acceptedGuide(fixture, channel);
    const originalHead = fixture.designs(channel)[0];
    const original = originalBytes(fixture, channel);
    const originalDocument = JSON.parse(original.toString());
    const readRevision = (revision: number) => Buffer.from((fixture.database.prepare(
      'SELECT Document FROM TeletextDesignRevision WHERE DesignId=? AND Revision=?')
      .get(originalHead.Id, revision) as { Document: Uint8Array }).Document);
    const pointers = () => ({
      requests: fixture.database.prepare('SELECT * FROM TeletextPublicationRequest WHERE ChannelId=? ORDER BY Id').all(channel),
      generations: fixture.database.prepare('SELECT * FROM TeletextAcceptedGeneration WHERE ChannelId=? ORDER BY Id').all(channel),
      selected: acceptedGuide(fixture, channel),
    });
    const originalPointers = pointers();
    const profiles = fixture.database.prepare('SELECT * FROM TeletextProfile WHERE Id=?').all(profile);
    const settings = fixture.database.prepare('SELECT * FROM TeletextProfileSetting WHERE ProfileId=? ORDER BY Key').all(profile);
    const currentUrl = `/channels/${channel}/teletext?guide_preview=1&guide_page=201&guide_subcode=0000`;
    await page.goto(historical.toString()); await ready(page, true);
    const originalRaster = await rasterBytes(page);
    const download = page.waitForEvent('download');
    await page.getByTestId('teletext-editor-export-page-form')
      .getByRole('button', { name: text('TeletextEditorExportPage'), exact: true }).click();
    const archive = info.outputPath('production-guide-page201-original.zip');
    await (await download).saveAs(archive);
    const exported = archiveFiles(archive);
    expect(exported.filter(file => file.name.endsWith('.tti')).map(file => file.name)).toEqual(['page-201-0000.tti']);
    const sidecarFile = exported.find(file => file.name === 'project.ersatzrs.json')!;
    const sidecar = JSON.parse(sidecarFile.buffer.toString());
    expect(sidecar.schema_version).toBe(1);
    expect(sidecar.project.channel_id).toBe(channel);
    expect(sidecar.project.original.design_id).toBe(originalHead.Id);
    expect(sidecar.project.original.revision).toBe(originalHead.Revision);
    const originalResponse = JSON.parse(originalDocument.documents[0].response);
    const metadata = originalResponse.authoring;
    expect(sidecar.project.producers[0].authoring).toEqual(metadata);
    expect(metadata.template).toEqual({ id: 'tvguide', revision: '1' });
    expect(metadata.theme.id).toBe('classic');
    const page201 = originalResponse.service.pages.find((value: { address: { page: number; subcode: number } }) => value.address.page === 201 && value.address.subcode === 0);
    expect(page201).toBeDefined();
    const ownership = metadata.pages.find((value: { page: { page: number; subcode: number } }) => value.page.page === 201 && value.page.subcode === 0);
    expect(ownership.regions.find((value: { id: string }) => value.id === 'footer'))
      .toEqual({ id: 'footer', kind: 'zone', area: 'main', row: 22, column: 2, width: 36, height: 1 });
    const title = page201.rows.find((value: { row: number }) => value.row === 3).runs[0].content.text as string;
    expect(title.length).toBeGreaterThan(0);
    const patchRow = (row: number, oldText: string, replacement: string): Upload[] => {
      expect(replacement.length).toBe(oldText.length);
      let changed = 0;
      const result = exported.map(file => {
        if (file.name !== 'page-201-0000.tti') return file;
        const pattern = new RegExp(`^OL,${row},([^\\r\\n]*)(\\r?)(?=\\n|$)`, 'm');
        const body = file.buffer.toString('latin1');
        expect(body).toMatch(pattern);
        const next = body.replace(pattern, (_line, cells: string, ending: string) => {
          expect(cells.split(oldText)).toHaveLength(2);
          const index = cells.indexOf(oldText);
          changed++;
          return `OL,${row},${cells.slice(0, index)}${replacement}${cells.slice(index + oldText.length)}${ending}`;
        });
        return { ...file, buffer: Buffer.from(next, 'latin1') };
      });
      expect(changed).toBe(1);
      expect(result.find(file => file.name === 'project.ersatzrs.json')!.buffer).toEqual(sidecarFile.buffer);
      return result;
    };
    const upload = async (input: Upload[], name: string) => {
      const form = page.getByTestId('teletext-editor-import-form');
      await expect(form).toHaveAttribute('enctype', 'multipart/form-data');
      await form.locator('[name="name"]').fill(name);
      await form.locator('[name="sidecar"]').setInputFiles(input.filter(file => file.name === 'project.ersatzrs.json'));
      await form.locator('[name="files"]').setInputFiles(input.filter(file => file.name !== 'project.ersatzrs.json'));
      await submit(page, form, text('TeletextEditorImport'), true);
    };
    // A protected title edit cannot advance a concept, request or live pointer.
    await upload(patchRow(3, title, (title[0] === 'X' ? 'Y' : 'X') + title.slice(1)), 'Rejected production title');
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'editorial-protected');
    expect(fixture.designs(channel)[0]).toEqual(originalHead);
    expect(pointers()).toEqual(originalPointers); expect(readRevision(originalHead.Revision)).toEqual(original);
    await page.goto(historical.toString()); await ready(page, true);
    const editedFiles = patchRow(22, 'Your TV guide', 'Custom notice');
    writeFileSync(info.outputPath('production-guide-page201-footer.tti'), editedFiles.find(file => file.name === 'page-201-0000.tti')!.buffer);
    writeFileSync(info.outputPath('production-guide-page201-sidecar.json'), sidecarFile.buffer);
    await upload(editedFiles, 'Production custom footer');
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const editedHead = fixture.designs(channel)[0];
    expect(editedHead.Id).toBe(originalHead.Id); expect(editedHead.Revision).toBe(originalHead.Revision + 1);
    const editedUrl = new URL(page.url()); editedUrl.searchParams.set('page', '201'); editedUrl.searchParams.set('subcode', '0000');
    await page.goto(editedUrl.toString()); await ready(page, true);
    const editedRaster = await rasterBytes(page); expect(editedRaster).not.toEqual(originalRaster);
    await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-footer-concept.png') });
    const editedOriginal = originalBytes(fixture, channel);
    const editedDocument = JSON.parse(editedOriginal.toString());
    const { editorial_overlay: overlay, ...unchangedOriginal } = editedDocument;
    expect(overlay).toBeDefined(); expect(unchangedOriginal).toEqual(originalDocument);
    expect(pointers()).toEqual(originalPointers); expect(readRevision(originalHead.Revision)).toEqual(original);
    await page.goto(currentUrl); await ready(page, true);
    expect(Buffer.from((await page.getByTestId('teletext-current-guide-model').locator('img').getAttribute('src'))!.split(',')[1], 'base64'))
      .toEqual(originalRaster);
    await page.goto(editedUrl.toString()); await ready(page, true);
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const accepted = acceptedGuide(fixture, channel);
    expect(accepted.Id).not.toBe(originalAccepted.Id); expect(accepted.ArtifactVersion).toBe(5);
    expect(accepted.DesignId).toBe(editedHead.Id); expect(accepted.Revision).toBe(editedHead.Revision);
    const settledPointers = pointers();
    const initialHost = isolatedWebHost.processIdentity();
    await context.close();
    const restarted = await isolatedWebHost.restart();
    expect(restarted.before).toEqual(initialHost);
    expect(restarted.stop.closed).toBe(true); expect(restarted.stop.forced).toBe(false);
    expect(restarted.stop.exitCode).toBe(0); expect(restarted.stop.signalCode).toBeNull();
    expect(restarted.after.pid).not.toBe(initialHost.pid);
    expect(restarted.after.generation).toBeGreaterThan(initialHost.generation);
    expect(isolatedWebHost.processIdentity()).toEqual(restarted.after);
    expect(pointers()).toEqual(settledPointers); expect(originalBytes(fixture, channel)).toEqual(editedOriginal);
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
    await page.goto(currentUrl); await ready(page, true);
    expect(Buffer.from((await page.getByTestId('teletext-current-guide-model').locator('img').getAttribute('src'))!.split(',')[1], 'base64'))
      .toEqual(editedRaster);
    type Current = { SeedId: number; RuntimeId: number | null; NextDueAt: string; ChangedAt: string | null; Failure: number | null; FirstFailureAt: string | null };
    const current = () => fixture.database.prepare('SELECT * FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1').get(channel) as Current;
    const before = current(); expect(before.RuntimeId).toBeNull();
    const programme = fixture.database.prepare(`SELECT MediaItemId,Finish FROM PlayoutItem WHERE PlayoutId=?
      AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(source, new Date().toISOString(), new Date().toISOString()) as { MediaItemId: number; Finish: string };
    const receipts: Record<string, unknown> = { repository_zip_sha256: candidateDigest, installation: selectedInstallation,
      restart: restarted, channel, source, profile, original_accepted: originalAccepted, accepted_generation: accepted,
      original_head: originalHead, edited_head: editedHead, original_sha256: sha256(original), edited_original_sha256: sha256(editedOriginal),
      original_raster_sha256: sha256(originalRaster), edited_raster_sha256: sha256(editedRaster),
      export_zip_sha256: sha256(readFileSync(archive)), sidecar_sha256: sha256(sidecarFile.buffer), before };
    const persist = () => writeFileSync(info.outputPath('production-guide-tti-refresh-identity.json'), JSON.stringify(receipts, null, 2));
    persist();
    const updatedTitle = 'Refreshed guide title';
    expect(fixture.database.prepare('UPDATE MovieMetadata SET Title=? WHERE MovieId=?').run(updatedTitle, programme.MediaItemId).changes).toBe(1);
    await expect.poll(() => current().ChangedAt !== null, { timeout: 10_000 }).toBe(true);
    const changed = current(); receipts.changed = changed; persist();
    expect(utc(changed.NextDueAt) - utc(changed.ChangedAt!)).toBeGreaterThanOrEqual(59_999);
    expect(utc(programme.Finish) - utc(changed.NextDueAt)).toBeGreaterThan(30_000);
    await expect.poll(() => utc(changed.NextDueAt) - Date.now() <= 5000, { timeout: 65_000, intervals: [100] }).toBe(true);
    const number = (fixture.database.prepare('SELECT Number FROM Channel WHERE Id=?').get(channel) as { Number: string }).Number;
    const publicUrl = `${baseURL}/iptv/channel/${encodeURIComponent(number)}.ts?mode=ts-legacy`;
    const path = info.outputPath('production-guide-tti-refresh.ts');
    receipts.capture_opened_at = new Date().toISOString(); receipts.public_channel_number = number; persist();
    const capture = await captureTeletextChannelTs(publicUrl, path,
      { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
    expect(capture.ended).toBe('byte-limit');
    receipts.capture = { bytes: capture.bytes.length, ended: capture.ended, sha256: sha256(capture.bytes) }; persist();
    assertContinuousAuthoredPmt(path, info.outputPath('production-guide-tti-refresh.pmt.json'), info.outputPath('production-guide-tti-refresh.pmt.txt'));
    execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
      '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
    const decoded = execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
      '-txt_format', 'text', '-txt_page', '201', '-txt_duration', '1000', '-i', path,
      '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
    writeFileSync(info.outputPath('production-guide-tti-refresh.srt'), decoded);
    expect(decoded).toContain('Now: Synthetic red video and tone'); expect(decoded).toContain(`Now: ${updatedTitle}`);
    const oldIndex = decoded.indexOf('Now: Synthetic red video and tone'), newIndex = decoded.indexOf(`Now: ${updatedTitle}`);
    expect(oldIndex).toBeLessThan(newIndex);
    expect(decoded.slice(oldIndex, newIndex)).toContain('Custom notice');
    expect(decoded.slice(newIndex)).toContain('Custom notice'); expect(decoded).not.toContain('Your TV guide');
    const after = current(); receipts.after = after; persist();
    expect(after.RuntimeId).not.toBeNull(); expect(after.SeedId).toBe(before.SeedId);
    expect(after.Failure).toBeNull(); expect(after.FirstFailureAt).toBeNull();
    const snapshot = fixture.database.prepare('SELECT Document,DocumentDigest FROM TeletextGuideRuntimeSnapshot WHERE Id=? AND SeedId=?')
      .get(after.RuntimeId!, after.SeedId) as { Document: Uint8Array; DocumentDigest: string };
    const runtimeBytes = Buffer.from(snapshot.Document); expect(sha256(runtimeBytes)).toBe(snapshot.DocumentDigest);
    const envelope = JSON.parse(runtimeBytes.toString());
    expect(envelope.seed_id).toBe(before.SeedId); expect(envelope.origin_generation_id).toBe(accepted.Id);
    expect(envelope.origin_request_id).toBe(accepted.RequestId); expect(envelope.origin_design_id).toBe(editedHead.Id);
    expect(envelope.origin_revision).toBe(editedHead.Revision); expect(envelope.origin_digest).toBe(editedHead.Digest);
    expect(envelope.role).toBe(1);
    const freshBaseline = JSON.parse(envelope.baseline);
    expect(freshBaseline.guide_source.channel_ids).toEqual([source]); expect(freshBaseline.selection).toEqual(originalDocument.selection);
    expect(freshBaseline.documents[0].response).toContain(updatedTitle);
    expect(JSON.parse(freshBaseline.documents[0].response).authoring).toEqual(metadata);
    await page.goto(currentUrl); await ready(page, true);
    const model = page.getByTestId('teletext-current-guide-model');
    await expect(model).toHaveAttribute('data-snapshot', String(after.RuntimeId)); await expect(model).toContainText(text('TeletextGuideFresh'));
    const refreshedRaster = Buffer.from((await model.locator('img').getAttribute('src'))!.split(',')[1], 'base64');
    expect(refreshedRaster).not.toEqual(editedRaster);
    await model.locator('img').scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-footer-refreshed-current.png') });
    await page.goto(editedUrl.toString()); await ready(page, true); expect(await rasterBytes(page)).toEqual(editedRaster);
    await page.goto(historical.toString()); await ready(page, true); expect(await rasterBytes(page)).toEqual(originalRaster);
    expect(readRevision(originalHead.Revision)).toEqual(original); expect(originalBytes(fixture, channel)).toEqual(editedOriginal);
    expect(pointers()).toEqual(settledPointers);
    expect(fixture.database.prepare('SELECT * FROM TeletextProfile WHERE Id=?').all(profile)).toEqual(profiles);
    expect(fixture.database.prepare('SELECT * FROM TeletextProfileSetting WHERE ProfileId=? ORDER BY Key').all(profile)).toEqual(settings);
    expect(installation(isolatedWebHost.databasePath)).toEqual(selectedInstallation); compareInstalledFiles(selectedInstallation);
    expect(fixture.calls()).toEqual([]); expect(fixture.setting('teletext.work.max_concurrent')).toBe('1'); expect(errors).toEqual([]);
    receipts.runtime_document_sha256 = snapshot.DocumentDigest; receipts.refreshed_raster_sha256 = sha256(refreshedRaster);
    receipts.scope = 'Real production page201 TTI and unchanged versioned sidecar, protected Generated rejection, footer Zone concept without activation, explicit Base5 publication and preserved footer across genuine guide refresh on one public ts-legacy response. No QTM/client/platform or whole P9B acceptance.';
    persist();
  } finally {
    try { await context?.close(); } finally { fixture.close(); }
  }
});

test('native Windows packaged TV-guide TTI footer survives publication and automatic refresh', async ({ browser, baseURL }, info) => {
  test.skip(process.platform !== 'win32' || process.env.ERSATZRS_TVGUIDE_PACKAGED_WINDOWS !== '1',
    'Requires the explicitly owned native Windows packaged-host controller.');
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  info.annotations.push({ type: 'browser-version', description: browser.version() },
    { type: 'production-package', description: `${addonId}${productionVersion}; repositoryZIP SHA256 ${candidateDigest}` });
  const fixture = new TeletextChannelFixture(databasePath!, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  try {
    const selectedInstallation = installed;
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    let page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    const channel = fixture.channels[0];
    const source = fixture.channels[1];
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const profile = await createProfile(page, fixture, 'classic', true);
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
    await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
    await submit(page, binding, text('TeletextBindingSave'), true);
    const form = page.getByTestId('teletext-authoring-form');
    for (const [name, value] of [['name', 'Production TTI footer original'], ['entry_page', '200'],
      ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) {
      await form.locator(`[name="${name}"]`).fill(value);
    }
    await form.locator('[name="entry_addon_id"]').selectOption(addonId);
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="guide_channel_id"]').selectOption(String(source));
    await submit(page, form, text('TeletextValidateSaveDraft'), true);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const historical = new URL(page.url());
    historical.searchParams.set('page', '201'); historical.searchParams.set('subcode', '0000');
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const originalAccepted = acceptedGuide(fixture, channel);
    const originalHead = fixture.designs(channel)[0];
    const original = originalBytes(fixture, channel);
    const originalDocument = JSON.parse(original.toString());
    const readRevision = (revision: number) => Buffer.from((fixture.database.prepare(
      'SELECT Document FROM TeletextDesignRevision WHERE DesignId=? AND Revision=?')
      .get(originalHead.Id, revision) as { Document: Uint8Array }).Document);
    const pointers = () => ({
      requests: fixture.database.prepare('SELECT * FROM TeletextPublicationRequest WHERE ChannelId=? ORDER BY Id').all(channel),
      generations: fixture.database.prepare('SELECT * FROM TeletextAcceptedGeneration WHERE ChannelId=? ORDER BY Id').all(channel),
      selected: acceptedGuide(fixture, channel),
    });
    const originalPointers = pointers();
    const profiles = fixture.database.prepare('SELECT * FROM TeletextProfile WHERE Id=?').all(profile);
    const settings = fixture.database.prepare('SELECT * FROM TeletextProfileSetting WHERE ProfileId=? ORDER BY Key').all(profile);
    const currentUrl = `/channels/${channel}/teletext?guide_preview=1&guide_page=201&guide_subcode=0000`;
    await page.goto(historical.toString()); await ready(page, true);
    const originalRaster = await rasterBytes(page);
    const download = page.waitForEvent('download');
    await page.getByTestId('teletext-editor-export-page-form')
      .getByRole('button', { name: text('TeletextEditorExportPage'), exact: true }).click();
    const archive = info.outputPath('production-guide-page201-original.zip');
    await (await download).saveAs(archive);
    const exported = archiveFiles(archive);
    expect(exported.filter(file => file.name.endsWith('.tti')).map(file => file.name)).toEqual(['page-201-0000.tti']);
    const sidecarFile = exported.find(file => file.name === 'project.ersatzrs.json')!;
    const sidecar = JSON.parse(sidecarFile.buffer.toString());
    expect(sidecar.schema_version).toBe(1);
    expect(sidecar.project.channel_id).toBe(channel);
    expect(sidecar.project.original.design_id).toBe(originalHead.Id);
    expect(sidecar.project.original.revision).toBe(originalHead.Revision);
    const originalResponse = JSON.parse(originalDocument.documents[0].response);
    const metadata = originalResponse.authoring;
    expect(sidecar.project.producers[0].authoring).toEqual(metadata);
    expect(metadata.template).toEqual({ id: 'tvguide', revision: '1' });
    expect(metadata.theme.id).toBe('classic');
    const page201 = originalResponse.service.pages.find((value: { address: { page: number; subcode: number } }) => value.address.page === 201 && value.address.subcode === 0);
    expect(page201).toBeDefined();
    const ownership = metadata.pages.find((value: { page: { page: number; subcode: number } }) => value.page.page === 201 && value.page.subcode === 0);
    expect(ownership.regions.find((value: { id: string }) => value.id === 'footer'))
      .toEqual({ id: 'footer', kind: 'zone', area: 'main', row: 22, column: 2, width: 36, height: 1 });
    const title = page201.rows.find((value: { row: number }) => value.row === 3).runs[0].content.text as string;
    expect(title.length).toBeGreaterThan(0);
    const patchRow = (row: number, oldText: string, replacement: string): Upload[] => {
      expect(replacement.length).toBe(oldText.length);
      let changed = 0;
      const result = exported.map(file => {
        if (file.name !== 'page-201-0000.tti') return file;
        const pattern = new RegExp(`^OL,${row},([^\\r\\n]*)(\\r?)(?=\\n|$)`, 'm');
        const body = file.buffer.toString('latin1');
        expect(body).toMatch(pattern);
        const next = body.replace(pattern, (_line, cells: string, ending: string) => {
          expect(cells.split(oldText)).toHaveLength(2);
          const index = cells.indexOf(oldText);
          changed++;
          return `OL,${row},${cells.slice(0, index)}${replacement}${cells.slice(index + oldText.length)}${ending}`;
        });
        return { ...file, buffer: Buffer.from(next, 'latin1') };
      });
      expect(changed).toBe(1);
      expect(result.find(file => file.name === 'project.ersatzrs.json')!.buffer).toEqual(sidecarFile.buffer);
      return result;
    };
    const upload = async (input: Upload[], name: string) => {
      const form = page.getByTestId('teletext-editor-import-form');
      await expect(form).toHaveAttribute('enctype', 'multipart/form-data');
      await form.locator('[name="name"]').fill(name);
      await form.locator('[name="sidecar"]').setInputFiles(input.filter(file => file.name === 'project.ersatzrs.json'));
      await form.locator('[name="files"]').setInputFiles(input.filter(file => file.name !== 'project.ersatzrs.json'));
      await submit(page, form, text('TeletextEditorImport'), true);
    };
    // A protected title edit cannot advance a concept, request or live pointer.
    await upload(patchRow(3, title, (title[0] === 'X' ? 'Y' : 'X') + title.slice(1)), 'Rejected production title');
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'editorial-protected');
    expect(fixture.designs(channel)[0]).toEqual(originalHead);
    expect(pointers()).toEqual(originalPointers); expect(readRevision(originalHead.Revision)).toEqual(original);
    await page.goto(historical.toString()); await ready(page, true);
    const editedFiles = patchRow(22, 'Your TV guide', 'Custom notice');
    writeFileSync(info.outputPath('production-guide-page201-footer.tti'), editedFiles.find(file => file.name === 'page-201-0000.tti')!.buffer);
    writeFileSync(info.outputPath('production-guide-page201-sidecar.json'), sidecarFile.buffer);
    await upload(editedFiles, 'Production custom footer');
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const editedHead = fixture.designs(channel)[0];
    expect(editedHead.Id).toBe(originalHead.Id); expect(editedHead.Revision).toBe(originalHead.Revision + 1);
    const editedUrl = new URL(page.url()); editedUrl.searchParams.set('page', '201'); editedUrl.searchParams.set('subcode', '0000');
    await page.goto(editedUrl.toString()); await ready(page, true);
    const editedRaster = await rasterBytes(page); expect(editedRaster).not.toEqual(originalRaster);
    await page.getByTestId('teletext-preview-model').locator('img').scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-footer-concept.png') });
    const editedOriginal = originalBytes(fixture, channel);
    const editedDocument = JSON.parse(editedOriginal.toString());
    const { editorial_overlay: overlay, ...unchangedOriginal } = editedDocument;
    expect(overlay).toBeDefined(); expect(unchangedOriginal).toEqual(originalDocument);
    expect(pointers()).toEqual(originalPointers); expect(readRevision(originalHead.Revision)).toEqual(original);
    await page.goto(currentUrl); await ready(page, true);
    expect(Buffer.from((await page.getByTestId('teletext-current-guide-model').locator('img').getAttribute('src'))!.split(',')[1], 'base64'))
      .toEqual(originalRaster);
    await page.goto(editedUrl.toString()); await ready(page, true);
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const accepted = acceptedGuide(fixture, channel);
    expect(accepted.Id).not.toBe(originalAccepted.Id); expect(accepted.ArtifactVersion).toBe(5);
    expect(accepted.DesignId).toBe(editedHead.Id); expect(accepted.Revision).toBe(editedHead.Revision);
    const settledPointers = pointers();
    // The native Python controller owns the packaged launcher and initializes
    // policy4 before source discovery. This case neither replaces nor signals that host.
    expect(pointers()).toEqual(settledPointers); expect(originalBytes(fixture, channel)).toEqual(editedOriginal);
    await page.goto(currentUrl); await ready(page, true);
    expect(Buffer.from((await page.getByTestId('teletext-current-guide-model').locator('img').getAttribute('src'))!.split(',')[1], 'base64'))
      .toEqual(editedRaster);
    type Current = { SeedId: number; RuntimeId: number | null; NextDueAt: string; ChangedAt: string | null; Failure: number | null; FirstFailureAt: string | null };
    const current = () => fixture.database.prepare('SELECT * FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1').get(channel) as Current;
    const before = current(); expect(before.RuntimeId).toBeNull();
    const programme = fixture.database.prepare(`SELECT MediaItemId,Finish FROM PlayoutItem WHERE PlayoutId=?
      AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(source, new Date().toISOString(), new Date().toISOString()) as { MediaItemId: number; Finish: string };
    const receipts: Record<string, unknown> = { repository_zip_sha256: candidateDigest, installation: selectedInstallation,
      packaged_native_windows: true, channel, source, profile, original_accepted: originalAccepted, accepted_generation: accepted,
      original_head: originalHead, edited_head: editedHead, original_sha256: sha256(original), edited_original_sha256: sha256(editedOriginal),
      original_raster_sha256: sha256(originalRaster), edited_raster_sha256: sha256(editedRaster),
      export_zip_sha256: sha256(readFileSync(archive)), sidecar_sha256: sha256(sidecarFile.buffer), before };
    const persist = () => writeFileSync(info.outputPath('production-guide-tti-refresh-identity.json'), JSON.stringify(receipts, null, 2));
    persist();
    const updatedTitle = 'Refreshed guide title';
    expect(fixture.database.prepare('UPDATE MovieMetadata SET Title=? WHERE MovieId=?').run(updatedTitle, programme.MediaItemId).changes).toBe(1);
    await expect.poll(() => current().ChangedAt !== null, { timeout: 10_000 }).toBe(true);
    const changed = current(); receipts.changed = changed; persist();
    expect(utc(changed.NextDueAt) - utc(changed.ChangedAt!)).toBeGreaterThanOrEqual(59_999);
    expect(utc(programme.Finish) - utc(changed.NextDueAt)).toBeGreaterThan(30_000);
    await expect.poll(() => utc(changed.NextDueAt) - Date.now() <= 5000, { timeout: 65_000, intervals: [100] }).toBe(true);
    const number = (fixture.database.prepare('SELECT Number FROM Channel WHERE Id=?').get(channel) as { Number: string }).Number;
    const publicUrl = `${baseURL}/iptv/channel/${encodeURIComponent(number)}.ts?mode=ts-legacy`;
    const path = info.outputPath('production-guide-tti-refresh.ts');
    receipts.capture_opened_at = new Date().toISOString(); receipts.public_channel_number = number; persist();
    const capture = await captureTeletextChannelTs(publicUrl, path,
      { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
    expect(capture.ended).toBe('byte-limit');
    receipts.capture = { bytes: capture.bytes.length, ended: capture.ended, sha256: sha256(capture.bytes) }; persist();
    assertContinuousAuthoredPmt(path, info.outputPath('production-guide-tti-refresh.pmt.json'), info.outputPath('production-guide-tti-refresh.pmt.txt'));
    execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
      '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
    const decoded = execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
      '-txt_format', 'text', '-txt_page', '201', '-txt_duration', '1000', '-i', path,
      '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
    writeFileSync(info.outputPath('production-guide-tti-refresh.srt'), decoded);
    expect(decoded).toContain('Now: Synthetic red video and tone'); expect(decoded).toContain(`Now: ${updatedTitle}`);
    const oldIndex = decoded.indexOf('Now: Synthetic red video and tone'), newIndex = decoded.indexOf(`Now: ${updatedTitle}`);
    expect(oldIndex).toBeLessThan(newIndex);
    expect(decoded.slice(oldIndex, newIndex)).toContain('Custom notice');
    expect(decoded.slice(newIndex)).toContain('Custom notice'); expect(decoded).not.toContain('Your TV guide');
    const after = current(); receipts.after = after; persist();
    expect(after.RuntimeId).not.toBeNull(); expect(after.SeedId).toBe(before.SeedId);
    expect(after.Failure).toBeNull(); expect(after.FirstFailureAt).toBeNull();
    const snapshot = fixture.database.prepare('SELECT Document,DocumentDigest FROM TeletextGuideRuntimeSnapshot WHERE Id=? AND SeedId=?')
      .get(after.RuntimeId!, after.SeedId) as { Document: Uint8Array; DocumentDigest: string };
    const runtimeBytes = Buffer.from(snapshot.Document); expect(sha256(runtimeBytes)).toBe(snapshot.DocumentDigest);
    const envelope = JSON.parse(runtimeBytes.toString());
    expect(envelope.seed_id).toBe(before.SeedId); expect(envelope.origin_generation_id).toBe(accepted.Id);
    expect(envelope.origin_request_id).toBe(accepted.RequestId); expect(envelope.origin_design_id).toBe(editedHead.Id);
    expect(envelope.origin_revision).toBe(editedHead.Revision); expect(envelope.origin_digest).toBe(editedHead.Digest);
    expect(envelope.role).toBe(1);
    const freshBaseline = JSON.parse(envelope.baseline);
    expect(freshBaseline.guide_source.channel_ids).toEqual([source]); expect(freshBaseline.selection).toEqual(originalDocument.selection);
    expect(freshBaseline.documents[0].response).toContain(updatedTitle);
    expect(JSON.parse(freshBaseline.documents[0].response).authoring).toEqual(metadata);
    await page.goto(currentUrl); await ready(page, true);
    const model = page.getByTestId('teletext-current-guide-model');
    await expect(model).toHaveAttribute('data-snapshot', String(after.RuntimeId)); await expect(model).toContainText(text('TeletextGuideFresh'));
    const refreshedRaster = Buffer.from((await model.locator('img').getAttribute('src'))!.split(',')[1], 'base64');
    expect(refreshedRaster).not.toEqual(editedRaster);
    await model.locator('img').scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('production-guide-footer-refreshed-current.png') });
    await page.goto(editedUrl.toString()); await ready(page, true); expect(await rasterBytes(page)).toEqual(editedRaster);
    await page.goto(historical.toString()); await ready(page, true); expect(await rasterBytes(page)).toEqual(originalRaster);
    expect(readRevision(originalHead.Revision)).toEqual(original); expect(originalBytes(fixture, channel)).toEqual(editedOriginal);
    expect(pointers()).toEqual(settledPointers);
    expect(fixture.database.prepare('SELECT * FROM TeletextProfile WHERE Id=?').all(profile)).toEqual(profiles);
    expect(fixture.database.prepare('SELECT * FROM TeletextProfileSetting WHERE ProfileId=? ORDER BY Key').all(profile)).toEqual(settings);
    expect(installation(databasePath!)).toEqual(selectedInstallation); compareInstalledFiles(selectedInstallation);
    expect(fixture.calls()).toEqual([]); expect(fixture.setting('teletext.work.max_concurrent')).toBe('1'); expect(errors).toEqual([]);
    receipts.runtime_document_sha256 = snapshot.DocumentDigest; receipts.refreshed_raster_sha256 = sha256(refreshedRaster);
    receipts.scope = 'Opt-in native Windows packaged-host production page201 TTI and unchanged versioned sidecar, protected Generated rejection, footer Zone concept without activation, explicit Base5 publication and preserved footer across genuine guide refresh on one public ts-legacy response. Native Windows Node owns SQLite; controller owns launcher and teardown. No native graceful/restart, QTM/client or whole P9B acceptance.';
    persist();
  } finally {
    try { await context?.close(); } finally { fixture.close(); }
  }
});

test('production TV-guide actual command timeout retains last-good output and releases capacity', async ({ browser, isolatedWebHost }, info) => {
  test.skip(process.platform !== 'linux' || process.env.ERSATZRS_TVGUIDE_COMMAND_TIMEOUT_OBSERVER !== '1',
    'Requires an explicitly armed Linux pidfd observer for the owned production Deno command.');
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9.');
  test.setTimeout(300_000);
  info.annotations.push({ type: 'browser-version', description: browser.version() },
    { type: 'production-package', description: `${addonId}${productionVersion}; repositoryZIP SHA256 ${candidateDigest}` });
  const baseURL = isolatedWebHost.baseURL;
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  try {
    const selectedInstallation = await installProduction(browser, baseURL, isolatedWebHost.databasePath);
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216,
      max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    let page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    const channel = fixture.channels[0];
    const source = fixture.channels[1];
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const profile = await createProfile(page, fixture, 'classic', true);
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
    await binding.locator('[name="profile_choice"]').selectOption(`${profile}:1`);
    await submit(page, binding, text('TeletextBindingSave'), true);
    const form = page.getByTestId('teletext-authoring-form');
    for (const [name, value] of [['name', 'Production command timeout original'], ['entry_page', '200'],
      ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) {
      await form.locator(`[name="${name}"]`).fill(value);
    }
    await form.locator('[name="entry_addon_id"]').selectOption(addonId);
    await form.locator('[name="v1_region"]').selectOption('western_latin');
    await form.locator('[name="guide_channel_id"]').selectOption(String(source));
    await submit(page, form, text('TeletextValidateSaveDraft'), true);
    await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
    const historical = new URL(page.url());
    historical.searchParams.set('page', '201'); historical.searchParams.set('subcode', '0000');
    await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
    await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
    const accepted = acceptedGuide(fixture, channel);
    const original = originalBytes(fixture, channel);
    const initialHost = isolatedWebHost.processIdentity();
    await context.close();
    const restarted = await isolatedWebHost.restart();
    expect(restarted.before).toEqual(initialHost);
    expect(restarted.stop.closed).toBe(true);
    expect(restarted.stop.forced).toBe(false);
    expect(restarted.stop.exitCode).toBe(0);
    expect(restarted.stop.signalCode).toBeNull();
    expect(restarted.after.pid).toBeGreaterThan(0);
    expect(restarted.after.pid).not.toBe(initialHost.pid);
    expect(restarted.after.generation).toBeGreaterThan(initialHost.generation);
    expect(isolatedWebHost.processIdentity()).toEqual(restarted.after);
    expect(originalBytes(fixture, channel)).toEqual(original);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(historical.toString()); await ready(page, true);
    const originalRaster = await rasterBytes(page);
    const currentUrl = `/channels/${channel}/teletext?guide_preview=1&guide_page=201&guide_subcode=0000`;
    await page.goto(currentUrl); await ready(page, true);
    const currentModel = page.getByTestId('teletext-current-guide-model');
    await expect(currentModel).toBeVisible();
    expect(Buffer.from((await currentModel.locator('img').getAttribute('src'))!.split(',')[1], 'base64')).toEqual(originalRaster);
    type TimeoutCurrent = {
      SeedId: number; RuntimeId: number | null; Revision: number; NextDueAt: string; LastSuccessAt: string;
      ChangedAt: string | null; FirstFailureAt: string | null; Failure: number | null; Revoked: number;
      LeaseOwner: string | null; LeaseUntil: string | null;
    };
    type StoppedReceipt = {
      schema: number; qualified: boolean; host: { pid: number; start_tick: string };
      seed_id: number; channel_id: number; role: number; signal: string; pidfd: boolean;
      stopped_at_utc: string; operation_deadline_utc: string; state: string;
      livelease: { owner: string; until: string };
      producer: { pid: number; start_tick: string; exe_sha256: string; entry_sha256: string };
    };
    const current = () => fixture.database.prepare('SELECT * FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1')
      .get(channel) as TimeoutCurrent;
    const history = () => ({ designs: fixture.designs(channel),
      revisions: fixture.database.prepare('SELECT DesignId,Revision,Digest FROM TeletextDesignRevision WHERE DesignId=? ORDER BY Revision')
        .all(fixture.designs(channel)[0].Id),
      requests: fixture.database.prepare('SELECT * FROM TeletextPublicationRequest WHERE ChannelId=? ORDER BY Id').all(channel),
      generations: fixture.database.prepare('SELECT * FROM TeletextAcceptedGeneration WHERE ChannelId=? ORDER BY Id').all(channel) });
    const retainedHistory = history();
    const before = current(); expect(before.RuntimeId).toBeNull(); expect(before.Failure).toBeNull();
    expect(savedOriginal(fixture, channel).guide_source.channel_ids).toEqual([source]);
    const seed = fixture.database.prepare('SELECT ValidUntil FROM TeletextGuideRefreshSeed WHERE Id=?')
      .get(before.SeedId) as { ValidUntil: string };
    const hostIdentity = isolatedWebHost.processIdentity();
    const host = startupOwner(hostIdentity.pid);
    expect(host, 'The exact isolated host needs a live /proc start identity before arming.').toBeDefined();
    const denoPath = realpathSync(execFileSync('which', ['deno'], { encoding: 'utf8', timeout: 3000, maxBuffer: 4096 }).trim());
    const denoSha = sha256(readFileSync(denoPath));
    const entryRelative = 'libexec/pages.mts';
    const entryPath = realpathSync(`${selectedInstallation.InstallPath}/${entryRelative}`);
    const entrySha = sha256(readFileSync(entryPath));
    const receipts: Record<string, unknown> = { repository_zip_sha256: candidateDigest, installation: selectedInstallation,
      restart: restarted, channel, source, profile, before, host: { pid: host!.pid, start_tick: host!.startTick },
      producer_executable_sha256: denoSha, producer_entry_sha256: entrySha,
      accepted_generation: accepted, original_sha256: sha256(original), original_raster_sha256: sha256(originalRaster) };
    const persist = () => writeFileSync(info.outputPath('production-guide-command-timeout-identity.json'), JSON.stringify(receipts, null, 2));
    persist();
    const programme = fixture.database.prepare(`SELECT MediaItemId,Finish FROM PlayoutItem WHERE PlayoutId=?
      AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(source, new Date().toISOString(), new Date().toISOString()) as { MediaItemId: number; Finish: string };
    const updatedTitle = 'Timeout guide title';
    expect(fixture.database.prepare('UPDATE MovieMetadata SET Title=? WHERE MovieId=?').run(updatedTitle, programme.MediaItemId).changes).toBe(1);
    await expect.poll(() => current().FirstFailureAt !== null, { timeout: 10_000 }).toBe(true);
    const lost = current();
    expect(lost.Failure).toBe(1); expect(lost.RuntimeId).toBeNull(); expect(lost.ChangedAt).not.toBeNull();
    expect(lost.LastSuccessAt).toBe(before.LastSuccessAt);
    expect(utc(lost.NextDueAt) - utc(lost.ChangedAt!)).toBeGreaterThanOrEqual(59_999);
    const originalCap = Math.min(utc(seed.ValidUntil), utc(lost.FirstFailureAt!) + 15 * 60_000);
    expect(utc(lost.NextDueAt) + authoring.operation_timeout_ms + 30_000).toBeLessThan(originalCap);
    receipts.lost = lost; receipts.original_stale_cap_utc_ms = originalCap; persist();
    const arm = { schema: 1, host: { pid: host!.pid, start_tick: host!.startTick },
      installation: { addon_id: addonId, install_path: selectedInstallation.InstallPath,
        content_sha256: selectedInstallation.ContentSha256, entry_relative: entryRelative, entry_sha256: entrySha },
      producer_executable: { path: denoPath, sha256: denoSha }, database_path: isolatedWebHost.databasePath,
      channel_id: channel, seed_id: before.SeedId, role: 1, due_at: lost.NextDueAt,
      lease_condition: 'nonempty-same-seed', expected_manual_generation_id: accepted.Id,
      operation_timeout_ms: authoring.operation_timeout_ms };
    writeFileSync(info.outputPath('production-guide-command-timeout-arm.json'), JSON.stringify(arm, null, 2), { flag: 'wx' });
    const stoppedPath = info.outputPath('production-guide-command-timeout-stopped.json');
    // Only the separate owned pidfd observer may signal a process. An absent or
    // unqualified receipt is a tool failure, never evidence of a product timeout.
    await expect.poll(() => existsSync(stoppedPath), { timeout: 65_000, intervals: [25] }).toBe(true);
    const stopped = JSON.parse(readFileSync(stoppedPath, 'utf8')) as StoppedReceipt;
    receipts.stopped = stopped; persist();
    expect(stopped.schema).toBe(1); expect(stopped.qualified).toBe(true);
    expect(stopped.host).toEqual(arm.host); expect(stopped.seed_id).toBe(before.SeedId);
    expect(stopped.channel_id).toBe(channel); expect(stopped.role).toBe(1);
    expect(stopped.signal).toBe('SIGSTOP'); expect(stopped.pidfd).toBe(true);
    expect(['T', 't']).toContain(stopped.state);
    expect(stopped.producer.pid).toBeGreaterThan(0);
    expect(stopped.producer.exe_sha256).toBe(denoSha); expect(stopped.producer.entry_sha256).toBe(entrySha);
    expect(startupOwner(host!.pid)?.startTick).toBe(host!.startTick);
    expect(startupOwner(stopped.producer.pid)?.startTick).toBe(stopped.producer.start_tick);
    const claimed = current(); receipts.claimed = claimed; persist();
    expect(claimed.SeedId).toBe(before.SeedId); expect(claimed.RuntimeId).toBeNull();
    expect(claimed.LeaseOwner).toBe(stopped.livelease.owner); expect(claimed.LeaseOwner).toBeTruthy();
    // Heartbeats may update wall-clock lease metadata while retaining the
    // original operation allowance; source/stale caps remain separately exact.
    expect(claimed.LeaseUntil).not.toBeNull();
    expect(utc(claimed.LeaseUntil!)).toBeGreaterThan(utc(stopped.stopped_at_utc));
    expect(utc(claimed.LeaseUntil!) - utc(stopped.stopped_at_utc)).toBeLessThanOrEqual(authoring.operation_timeout_ms);
    expect(stopped.operation_deadline_utc).toBe(stopped.livelease.until);
    expect(utc(stopped.operation_deadline_utc)).toBeGreaterThan(utc(stopped.stopped_at_utc));
    expect(utc(stopped.operation_deadline_utc) - utc(stopped.stopped_at_utc)).toBeLessThanOrEqual(authoring.operation_timeout_ms);
    expect(claimed.FirstFailureAt).toBe(lost.FirstFailureAt); expect(claimed.LastSuccessAt).toBe(before.LastSuccessAt);
    expect(history()).toEqual(retainedHistory);
    await expect.poll(() => utc(stopped.operation_deadline_utc) - Date.now() <= 5000,
      { timeout: 30_000, intervals: [100] }).toBe(true);
    const path = info.outputPath('production-guide-command-timeout.ts');
    receipts.capture_opened_at = new Date().toISOString(); persist();
    const number = (fixture.database.prepare('SELECT Number FROM Channel WHERE Id=?').get(channel) as { Number: string }).Number;
    const capture = await captureTeletextChannelTs(`${baseURL}/iptv/channel/${encodeURIComponent(number)}.ts?mode=ts-legacy`, path,
      { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 });
    receipts.capture_closed_at = new Date().toISOString();
    receipts.capture = { bytes: capture.bytes.length, ended: capture.ended, sha256: sha256(capture.bytes) }; persist();
    expect(capture.ended).toBe('byte-limit');
    expect(utc(receipts.capture_opened_at as string)).toBeLessThan(utc(stopped.operation_deadline_utc));
    expect(utc(receipts.capture_closed_at as string)).toBeGreaterThan(utc(stopped.operation_deadline_utc));
    assertContinuousAuthoredPmt(path, info.outputPath('production-guide-command-timeout.pmt.json'), info.outputPath('production-guide-command-timeout.pmt.txt'));
    execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
      '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { timeout: 30_000, maxBuffer: 65_536 });
    const decoded = execFileSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
      '-txt_format', 'text', '-txt_page', '201', '-txt_duration', '1000', '-i', path,
      '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
    writeFileSync(info.outputPath('production-guide-command-timeout.srt'), decoded);
    expect(decoded).toContain('Now: Synthetic red video and tone'); expect(decoded).not.toContain(updatedTitle);
    await expect.poll(() => current().Failure === 2 && current().LeaseOwner === null && current().LeaseUntil === null,
      { timeout: 10_000 }).toBe(true);
    const failed = current(); receipts.failed = failed; persist();
    expect(failed.FirstFailureAt).toBe(lost.FirstFailureAt); expect(failed.LastSuccessAt).toBe(before.LastSuccessAt);
    expect(failed.ChangedAt).toBe(lost.ChangedAt); expect(failed.RuntimeId).toBeNull(); expect(failed.Revoked).toBe(0);
    expect(utc(failed.NextDueAt) - utc(stopped.operation_deadline_utc)).toBeGreaterThanOrEqual(59_999);
    expect(Math.min(utc(seed.ValidUntil), utc(failed.FirstFailureAt!) + 15 * 60_000)).toBe(originalCap);
    await expect.poll(() => startupOwner(stopped.producer.pid)?.startTick !== stopped.producer.start_tick,
      { timeout: 5000 }).toBe(true);
    receipts.producer_reaped = true; persist();
    await page.goto(currentUrl); await ready(page, true);
    const model = page.getByTestId('teletext-current-guide-model');
    await expect(model).toContainText(text('TeletextGuideStale'));
    expect(Buffer.from((await model.locator('img').getAttribute('src'))!.split(',')[1], 'base64')).toEqual(originalRaster);
    await page.screenshot({ path: info.outputPath('production-guide-command-timeout-stale-current.png') });
    await page.goto('/system/health'); await ready(page, true);
    const refreshed = page.waitForResponse(value => value.request().method() === 'POST'
      && new URL(value.url()).pathname.startsWith('/admin-api/refresh_health_snapshot'));
    await page.getByRole('button', { name: text('Refresh'), exact: true }).click(); expect((await refreshed).status()).toBe(200);
    await expect(page.getByRole('button', { name: text('Refresh'), exact: true })).toBeEnabled();
    const row = page.getByRole('row').filter({ has: page.getByText(text('TeletextGuideMaintenanceTitle'), { exact: true }) });
    await expect(row.locator('span[aria-label]').first()).toHaveAttribute('aria-label', text('Warning'));
    const health = (JSON.parse(fixture.setting('health.results')!) as { title: string; status: string; message: string }[])
      .find(value => value.title === 'Teletext guide maintenance');
    expect(health?.status).toBe('Warning'); expect(health?.message).toContain('fresh=0, stale=1, unavailable=0, revoked=0');
    receipts.health = health; await page.screenshot({ path: info.outputPath('production-guide-command-timeout-health.png') }); persist();
    // This historical original preview takes the ordinary fresh work admission.
    // With cap1, a successful native reconstruction proves no stuck/quarantined
    // producer hold was silently treated as released merely by future Drop.
    expect((await page.goto(historical.toString()))?.status()).toBe(200); await ready(page, true);
    expect(await rasterBytes(page)).toEqual(originalRaster);
    receipts.capacity_one_native_preview = true;
    expect(originalBytes(fixture, channel)).toEqual(original); expect(history()).toEqual(retainedHistory);
    expect(acceptedGuide(fixture, channel)).toEqual(accepted);
    expect(fixture.database.prepare('SELECT COUNT(*) AS count FROM TeletextGuideRuntimeSnapshot WHERE SeedId=?').get(before.SeedId))
      .toEqual({ count: 0 });
    expect(current().RuntimeId).toBeNull(); expect(current().FirstFailureAt).toBe(lost.FirstFailureAt);
    expect(fixture.setting('teletext.work.max_concurrent')).toBe('1'); expect(fixture.calls()).toEqual([]);
    expect(installation(isolatedWebHost.databasePath)).toEqual(selectedInstallation); compareInstalledFiles(selectedInstallation);
    expect(startupOwner(host!.pid)?.startTick).toBe(host!.startTick); expect(errors).toEqual([]);
    receipts.scope = 'One explicitly armed real production Deno command stopped by independently identity-qualified pidfd after live ownership, original command timeout, last-good page201/AV/continuousPMT, persisted PreparationFailed retry and fixed stale cap, actual process reap and cap1 native preview. No synthetic pre-spawn timeout, producer recovery, platform/client or whole P9B acceptance.';
    persist();
  } finally {
    try { await context?.close(); } finally { fixture.close(); }
  }
});

// One real production-addon Lua lifecycle. The collection/media fixture supplies
// AV only; Lua resolves existing production profiles through the normal app API.
const reusedLuaDirectEvidence = process.env.ERSATZRS_TVGUIDE_LUA_REUSE_DIRECT;
test(reusedLuaDirectEvidence
  ? 'production TV-guide Lua Legacy repair with qualified paired direct evidence'
  : 'production TV-guide Lua profile On Off Continue and natural expiry to Base', async ({ browser, isolatedWebHost }, info) => {
  test.skip(process.env.ERSATZRS_TVGUIDE_LUA_FLOW !== '1', 'Explicit production Lua lifecycle qualification.');
  test.skip(process.platform === 'win32', 'This disposable host proof is POSIX; native Windows has separate evidence.');
  test.skip(!process.env.FFMPEG_PATH, 'Requires pinned FFmpeg9 and its independent libzvbi decoder.');
  test.setTimeout(300_000);
  const baseURL = isolatedWebHost.baseURL;
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, {
    authoredPages: true, playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 180 },
  });
  let context: BrowserContext | undefined;
  const receipts: Record<string, unknown> = { repository_zip_sha256: candidateDigest,
    scope: 'Real production profile On/effective6, Off/effective4, command-free Continue and natural60s Off expiry to Base5; paired direct-TS ordinary On/isolation responses and one public Legacy Off-to-Base response. No simultaneous paired Legacy-capacity, restart, physical client or whole P9B claim.' };
  if (reusedLuaDirectEvidence) receipts.scope = 'Affected ordinary Legacy Off-to-Base transport repair, with fresh admin/profile/publication/Lua checks. Previously qualified paired direct responses are explicitly reused within the checked source/runtime boundary; no new paired direct receiver claim.';
  const persist = (): void => writeFileSync(info.outputPath('production-guide-lua-identity.json'), JSON.stringify(receipts, null, 2));
  type Interval = { PlayoutId: number; AddonId: string; Start: string; Finish: string; State: number; CandidateSnapshot: string | null; ProfileId: number | null };
  const channel = fixture.channels[0], source = fixture.channels[1];
  const intervals = (): Interval[] => fixture.database.prepare(`SELECT *,json_extract(CandidateSnapshot,'$.profile') AS ProfileId
    FROM PlayoutTeletextInterval WHERE PlayoutId=? AND AddonId=? ORDER BY Start`).all(channel, addonId) as Interval[];
  const sourceRows = (): unknown[] => fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(source);
  const generation = (id: number, effective: boolean): Record<string, unknown> | undefined => fixture.database.prepare(`
    SELECT g.*,r.DesignId,r.Revision FROM ${effective ? 'TeletextEffectiveAcceptedGenerationCurrent' : 'TeletextAcceptedGenerationCurrent'} c
    JOIN TeletextAcceptedGeneration g ON g.Id=c.GenerationId JOIN TeletextPublicationRequest r ON r.Id=g.RequestId WHERE c.ChannelId=?`).get(id);
  const document = (url: string): Buffer => {
    const params = new URL(url).searchParams;
    const row = fixture.database.prepare('SELECT Document FROM TeletextDesignRevision WHERE DesignId=? AND Revision=?')
      .get(Number(params.get('design_id')), Number(params.get('revision'))) as { Document: Uint8Array };
    expect(row).toBeDefined(); return Buffer.from(row.Document);
  };
  try {
    const selectedInstallation = await installProduction(browser, baseURL, isolatedWebHost.databasePath);
    receipts.installation = selectedInstallation; receipts.channels = fixture.channels; persist();
    fixture.configureScripted();
    const policy = JSON.parse(readFileSync(fileURLToPath(new URL(
      '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
    )), 'utf8'));
    const guideLimits = { max_records: 1000, max_source_bytes: 1_048_576,
      max_model_nodes: 10_000, max_model_bytes: 16_777_216, max_digest_fields: 100_000, max_digest_bytes: 16_777_216 };
    const authoring = { ...policy, version: 2, operation_timeout_ms: 30_000, guide_limits: guideLimits,
      preview: { max_pages: 16, max_model_records: 32_768, max_pixels: 168_000, max_diagnostics: 1400, max_work: 50_000_000 } };
    delete authoring.source; delete authoring.recovery_timeout_ms;
    fixture.setAuthoringPolicy(JSON.stringify(authoring));
    fixture.setOutputPolicy(JSON.stringify({ ...policy, version: 4, guide_limits: guideLimits,
      live_updates: { poll_interval_ms: 25, recovery_timeout_ms: 2000 } }));
    fixture.database.prepare('REPLACE INTO ConfigElement(Key,Value) VALUES (?,?)').run('teletext.work.max_concurrent', '1');
    context = await browser.newContext({ baseURL, javaScriptEnabled: true, viewport: { width: 1280, height: 900 } });
    const page = await context.newPage(), errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    const text = translation('en_us');
    await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0 });
    await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
    const classic = await createProfile(page, fixture, 'classic', true), night = await createProfile(page, fixture, 'night', true);
    receipts.profiles = { classic, night }; persist();
    for (const id of fixture.channels) await resetGuidePlayout(page, fixture, id);
    const retainedSource = sourceRows();
    const sourceNow = fixture.database.prepare(`SELECT * FROM PlayoutItem WHERE PlayoutId=?
      AND julianday(Start)<=julianday(?) AND julianday(Finish)>julianday(?) ORDER BY Start LIMIT 1`)
      .get(source, new Date().toISOString(), new Date().toISOString()) as { Start: string; Finish: string };
    expect(sourceNow).toBeDefined(); receipts.source_programme = sourceNow; persist();
    const baseUrls: string[] = [], baseOriginals: Buffer[] = [], baseAccepted: Record<string, unknown>[] = [];
    // Successful Publish may race cap1 background observation on the historical
    // GET. Only an exact Busy document with an already-selected identity gets
    // one explicit read of that same immutable URL; mutations are never retried.
    const publication = async (effective: boolean, artifact: number): Promise<Record<string, unknown>> => {
      await submit(page, page.getByTestId('teletext-publication-request-form'), text('TeletextPublicationRequest'), true);
      await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
      const pendingUrl = page.url(), before = document(pendingUrl);
      await submit(page, page.getByTestId('teletext-publication-publish-form'), text('TeletextPublicationPublish'), true);
      await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'publication-selected');
      const target = Number(new URL(page.url()).pathname.split('/')[2]);
      const accepted = generation(target, effective);
      expect(accepted?.ArtifactVersion).toBe(artifact);
      expect(accepted?.DesignId).toBe(Number(new URL(pendingUrl).searchParams.get('design_id')));
      expect(accepted?.Revision).toBe(Number(new URL(pendingUrl).searchParams.get('revision')));
      const error = page.getByTestId('teletext-preview-error');
      if (await error.count()) {
        await expect(error).toHaveText(text('TeletextWorkBusy'));
        const selectedUrl = page.url(); await page.goto(selectedUrl); await ready(page, true);
      }
      await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
      expect(document(page.url())).toEqual(before); return accepted!;
    };
    const historicalReads: { url: string; busy_reread: boolean }[] = [];
    const historical = async (url: string): Promise<void> => {
      const before = document(url);
      const pointers = () => fixture.channels.map(id => ({ channel: id,
        base: generation(id, false), effective: generation(id, true) }));
      const acceptedBefore = pointers();
      await page.goto(url); await ready(page, true);
      expect(document(url)).toEqual(before); expect(pointers()).toEqual(acceptedBefore);
      const error = page.getByTestId('teletext-preview-error');
      const busy = await error.count() !== 0;
      if (busy) {
        await expect(error).toHaveText(text('TeletextWorkBusy'));
        const identity = new URL(url).searchParams;
        expect(acceptedBefore.some(entry => [entry.base, entry.effective].some(value =>
          value?.DesignId === Number(identity.get('design_id')) && value?.Revision === Number(identity.get('revision'))))).toBe(true);
        // One explicit immutable read, only for the established cap1 Busy
        // response. No other error, publication or mutation is retried.
        await page.goto(url); await ready(page, true);
      }
      expect(page.url()).toBe(url);
      expect(document(url)).toEqual(before); expect(pointers()).toEqual(acceptedBefore);
      historicalReads.push({ url, busy_reread: busy }); receipts.historical_reads = historicalReads; persist();
    };
    const prepare = async (effective: boolean, name: string, absent = false): Promise<string> => {
      await page.goto(`/channels/${channel}/teletext`); await ready(page, true);
      const form = page.getByTestId(effective ? 'teletext-effective-authoring-form' : 'teletext-authoring-form');
      const publishAdvanced = page.getByTestId('teletext-publish-advanced');
      if (await publishAdvanced.getAttribute('open') === null) await publishAdvanced.locator(':scope > summary').click();
      await form.locator('[name="name"]').fill(name);
      if (absent) {
        await expect(page.getByTestId('teletext-effective-authoring')).toHaveAttribute('data-state', 'absent');
        for (const field of ['entry_addon_id', 'entry_page', 'guide_channel_id', 'resource_transmissions']) {
          await expect(form.locator(`[name="${field}"]`)).toHaveCount(0);
        }
      } else {
        await form.locator('[name="entry_addon_id"]').selectOption(addonId);
        await form.locator('[name="entry_page"]').fill(effective ? '410' : '200');
        await form.locator('[name="entry_subcode"]').fill('0000');
        await form.locator('[name="v1_region"]').selectOption('western_latin');
        await form.locator('[name="resource_transmissions"]').fill('[]');
        await form.locator('[name="guide_channel_id"]').selectOption(String(source));
      }
      await submit(page, form, text('TeletextValidateSaveDraft'), true);
      await expect(page.getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
      return page.url();
    };
    for (const id of fixture.channels) {
      await page.goto(`/channels/${id}/teletext`); await ready(page, true);
      const binding = page.locator(`[data-addon-id="${addonId}"] [data-testid="teletext-binding-form"]`);
      await binding.locator('[name="profile_choice"]').selectOption(`${classic}:1`);
      await submit(page, binding, text('TeletextBindingSave'), true);
      const form = page.getByTestId('teletext-authoring-form');
      for (const [name, value] of [['name', `Production Lua Base ${id}`], ['entry_page', '200'],
        ['entry_subcode', '0000'], ['resource_transmissions', '[]']] as const) await form.locator(`[name="${name}"]`).fill(value);
      await form.locator('[name="entry_addon_id"]').selectOption(addonId);
      await form.locator('[name="v1_region"]').selectOption('western_latin');
      await form.locator('[name="guide_channel_id"]').selectOption(String(source));
      await submit(page, form, text('TeletextValidateSaveDraft'), true);
      const url = page.url(); baseUrls.push(url); baseOriginals.push(document(url));
      baseAccepted.push(await publication(false, 5));
    }
    const profilesBefore = fixture.database.prepare('SELECT * FROM TeletextProfile WHERE AddonId=? ORDER BY Id').all(addonId);
    const bindingsBefore = fixture.database.prepare('SELECT * FROM ChannelTeletextBinding WHERE ChannelId IN (?,?) ORDER BY ChannelId,AddonId')
      .all(...fixture.channels);
    receipts.base_generations = baseAccepted; receipts.base_original_sha256 = baseOriginals.map(sha256); persist();
    const queue = async (mode: 'reset' | 'continue', profile: number | null | undefined): Promise<void> => {
      const previous = fixture.database.prepare('SELECT LastBuild FROM PlayoutBuildStatus WHERE PlayoutId=?').get(channel) as { LastBuild: string };
      const oldIntervals = intervals();
      const count = (fixture.database.prepare('SELECT COUNT(*) AS count FROM PlayoutItem WHERE PlayoutId=?').get(channel) as { count: number }).count;
      if (typeof profile === 'number') expect(profilesBefore.some(row => row.Id === profile && !row.IsDeleted)).toBe(true);
      const controls = profile === undefined ? '' : profile === null ? `assert(teletext_enabled('${addonId}',false).ok)`
        : `assert(teletext_profile('${addonId}',${profile}).ok)\nassert(teletext_enabled('${addonId}',true).ok)`;
      const script = `add_collection('clip','Synthetic Scripted ${fixture.addonId}')\n${controls}\n`
        + (mode === 'reset' ? "add_duration('clip','00:01:00',{trim=true,stop_before_end=true})\n" : "add_count('clip',3)\n");
      writeFileSync(`${fixture.root}/synthetic-controls-${channel}.lua`, script);
      await page.goto('/playouts'); await ready(page, true);
      const name = fixture.database.prepare('SELECT Name FROM Channel WHERE Id=?').get(channel) as { Name: string };
      await page.getByPlaceholder(text('SearchForPlayouts'), { exact: true }).fill(name.Name);
      const row = page.getByRole('row').filter({ has: page.locator(`a[href="/playouts/scripted/${channel}"]`) });
      await expect(row).toHaveCount(1);
      await row.getByRole('button', { name: text('RebuildPlayout'), exact: true }).click();
      const dialog = page.getByRole('dialog', { name: text('RebuildPlayout'), exact: true });
      await dialog.getByLabel(text('PlayoutRebuildMode'), { exact: true }).selectOption(mode);
      const keep = dialog.getByRole('checkbox', { name: text('KeepCurrentProgramme'), exact: true });
      if (mode === 'continue') { await expect(keep).toBeChecked(); await expect(keep).toBeDisabled(); }
      else await keep.uncheck();
      const posted = page.waitForResponse(response => response.request().method() === 'POST'
        && new URL(response.url()).pathname.startsWith('/admin-api/rebuild_playout'));
      await dialog.getByRole('button', { name: text('QueuePlayoutBuild'), exact: true }).click();
      const response = await posted; expect(response.status()).toBe(200);
      const fields = new URLSearchParams(response.request().postData() ?? '');
      expect(fields.get('id')).toBe(String(channel)); expect(fields.get('mode')).toBe(mode);
      expect(fields.get('preserve_current')).toBe(String(mode === 'continue'));
      expect(await response.json()).toEqual({ accepted: 1, coalesced: 0 }); await expect(dialog).toBeHidden();
      await expect.poll(() => {
        const status = fixture.database.prepare('SELECT Success,LastBuild,LastSuccessfulBuild FROM PlayoutBuildStatus WHERE PlayoutId=?')
          .get(channel) as { Success: number; LastBuild: string; LastSuccessfulBuild: string };
        const items = (fixture.database.prepare('SELECT COUNT(*) AS count FROM PlayoutItem WHERE PlayoutId=?').get(channel) as { count: number }).count;
        const actual = intervals();
        return status?.Success === 1 && status.LastBuild !== previous?.LastBuild && status.LastSuccessfulBuild === status.LastBuild
          && items === (mode === 'reset' ? 1 : count + 3)
          && (mode === 'continue' ? JSON.stringify(actual) === JSON.stringify(oldIntervals)
            : actual.length === 1 && actual[0].State === Number(profile !== null) && actual[0].ProfileId === profile);
      }, { timeout: 45_000 }).toBe(true);
      expect(sourceRows()).toEqual(retainedSource);
      if (mode === 'reset') {
        const items = fixture.database.prepare('SELECT Start,Finish,InPoint,OutPoint FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start')
          .all(channel) as { Start: string; Finish: string; InPoint: string; OutPoint: string }[];
        expect(items).toHaveLength(1); expect(utc(items[0].Finish) - utc(items[0].Start)).toBe(60_000);
        expect(intervals()[0].Start).toBe(items[0].Start); expect(intervals()[0].Finish).toBe(items[0].Finish);
      }
    };
    type CaptureTiming = { first_read_utc?: string; last_read_utc?: string; first_authored_utc?: string;
      first_off_av_utc?: string; last_off_av_utc?: string; packets: number;
      pmt_transitions: { authored: boolean; received_utc: string; packet: number }[] };
    const capture = async (id: number, name: string, address: number, theme: string,
      authoredObserved?: () => void, offFinish?: number, mode: 'direct' | 'legacy' = 'legacy'): Promise<{ path: string; decoded: string; timing: CaptureTiming }> => {
      const number = (fixture.database.prepare('SELECT Number FROM Channel WHERE Id=?').get(id) as { Number: string }).Number;
      const path = info.outputPath(`${name}.ts`), opened = new Date().toISOString();
      const route = mode === 'direct' ? `/iptv/hls-direct/${id}.ts` : `/iptv/channel/${number}.ts?mode=ts-legacy`;
      const timing: CaptureTiming = { packets: 0, pmt_transitions: [] };
      let carry = Buffer.alloc(0), carryUtc: string | undefined, pmtPid: number | undefined;
      // These fixture PAT/PMT sections fit one TS payload. Ignore incomplete
      // sections rather than inventing a partial state; TSDuck still verifies
      // the complete retained transport independently below.
      const section = (packet: Buffer, table: number): Buffer | undefined => {
        if ((packet[1] & 0x40) === 0 || (packet[3] & 0x10) === 0) return undefined;
        let offset = 4;
        if ((packet[3] & 0x20) !== 0) offset += 1 + packet[4];
        if (offset >= 188) return undefined;
        offset += 1 + packet[offset];
        if (offset + 3 > 188 || packet[offset] !== table) return undefined;
        const length = 3 + ((packet[offset + 1] & 0x0f) << 8 | packet[offset + 2]);
        if (offset + length > 188) return undefined;
        return packet.subarray(offset, offset + length);
      };
      const result = await captureTeletextChannelTs(`${baseURL}${route}`, path,
        { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 }, (bytes, receivedUtc) => {
          timing.first_read_utc ??= receivedUtc; timing.last_read_utc = receivedUtc;
          const previousCarry = carry.length, previousCarryUtc = carryUtc;
          const joined = previousCarry ? Buffer.concat([carry, bytes]) : Buffer.from(bytes);
          let offset = 0;
          for (; offset + 188 <= joined.length; offset += 188) {
            const packet = joined.subarray(offset, offset + 188);
            if (packet[0] !== 0x47) throw new Error('Observed synthetic TS packet lost framing.');
            const packetUtc = offset < previousCarry ? previousCarryUtc! : receivedUtc;
            const pid = (packet[1] & 0x1f) << 8 | packet[2];
            timing.packets++;
            if (pid === 512 && timing.first_authored_utc === undefined) {
              timing.first_authored_utc = packetUtc; authoredObserved?.();
            }
            const payloadOffset = 4 + ((packet[3] & 0x20) !== 0 ? 1 + packet[4] : 0);
            const hasPayload = (packet[3] & 0x10) !== 0 && payloadOffset < 188;
            if ((pid === 256 || pid === 257) && hasPayload && offFinish !== undefined && utc(packetUtc) < offFinish) {
              timing.first_off_av_utc ??= packetUtc; timing.last_off_av_utc = packetUtc;
            }
            if (pid === 0) {
              const pat = section(packet, 0);
              if (pat) for (let entry = 8; entry + 4 <= pat.length - 4; entry += 4) {
                if ((pat[entry] << 8 | pat[entry + 1]) !== 0) pmtPid = (pat[entry + 2] & 0x1f) << 8 | pat[entry + 3];
              }
            } else if (pid === pmtPid) {
              const pmt = section(packet, 2);
              if (pmt && pmt.length >= 16) {
                let authored = false;
                for (let entry = 12 + ((pmt[10] & 0x0f) << 8 | pmt[11]); entry + 5 <= pmt.length - 4;
                  entry += 5 + ((pmt[entry + 3] & 0x0f) << 8 | pmt[entry + 4])) {
                  authored ||= ((pmt[entry + 1] & 0x1f) << 8 | pmt[entry + 2]) === 512;
                }
                if (timing.pmt_transitions.at(-1)?.authored !== authored) {
                  expect(timing.pmt_transitions.length, 'Only bounded observed PMT state transitions are retained.').toBeLessThan(4);
                  timing.pmt_transitions.push({ authored, received_utc: packetUtc, packet: timing.packets });
                }
              }
            }
          }
          carry = Buffer.from(joined.subarray(offset));
          carryUtc = carry.length ? (offset < previousCarry ? previousCarryUtc : receivedUtc) : undefined;
          expect(carry.length).toBeLessThan(188);
        });
      expect(carry.length).toBe(0);
      receipts[name] = { channel: id, number, route,
        opened_utc: opened, capture_completed_utc: new Date().toISOString(), timing,
        bytes: result.bytes.length, ended: result.ended, sha256: sha256(result.bytes) }; persist();
      expect(result.ended).toBe('byte-limit'); expect(result.bytes.length % 188).toBe(0);
      const pids = new Set<number>();
      let malformed = 0;
      for (let offset = 0; offset < result.bytes.length; offset += 188) {
        if (result.bytes[offset] !== 0x47 || (result.bytes[offset + 1] & 0x80) !== 0) malformed++;
        pids.add(((result.bytes[offset + 1] & 0x1f) << 8) | result.bytes[offset + 2]);
      }
      expect(malformed).toBe(0);
      for (const pid of [0, 256, 257, 512]) expect(pids.has(pid)).toBe(true);
      const av = spawnSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin', '-i', path,
        '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'], { encoding: 'utf8', timeout: 30_000, maxBuffer: 65_536 });
      writeFileSync(info.outputPath(`${name}.av.stderr`), av.stderr ?? '');
      expect(av.error).toBeUndefined(); expect(av.signal).toBeNull(); expect(av.status).toBe(0); expect(av.stderr).toBe('');
      const subtitle = spawnSync(process.env.FFMPEG_PATH!, ['-hide_banner', '-loglevel', 'error', '-nostdin',
        '-txt_format', 'text', '-txt_page', String(address), '-txt_duration', '1000', '-i', path,
        '-map', '0:s:0', '-c:s', 'srt', '-f', 'srt', '-'], { encoding: 'utf8', timeout: 30_000, maxBuffer: 1_048_576 });
      writeFileSync(info.outputPath(`${name}.teletext.stderr`), subtitle.stderr ?? '');
      writeFileSync(info.outputPath(`${name}.srt`), subtitle.stdout ?? '');
      expect(subtitle.error).toBeUndefined(); expect(subtitle.signal).toBeNull(); expect(subtitle.status).toBe(0); expect(subtitle.stderr).toBe('');
      const decoded = subtitle.stdout;
      expect(decoded).toContain(theme); expect(decoded).toContain('Now: Synthetic red video and tone');
      return { path, decoded, timing };
    };
    await queue('reset', night);
    const on = intervals(); expect(on[0].CandidateSnapshot).not.toBeNull();
    receipts.on_interval = on; persist();
    const onUrl = await prepare(true, 'Production Lua Night guide');
    await expect(page.getByTestId('teletext-preview-theme')).toHaveText('night');
    const onOriginal = document(onUrl), onRaster = await rasterBytes(page);
    const effectiveOn = await publication(true, 6);
    receipts.on_generation = effectiveOn; receipts.on_original_sha256 = sha256(onOriginal); persist();
    expect(utc(on[0].Finish) - Date.now()).toBeGreaterThan(30_000);
    expect(utc(sourceNow.Finish) - Date.now()).toBeGreaterThan(30_000);
    let onCapture: Awaited<ReturnType<typeof capture>>, isolatedCapture: Awaited<ReturnType<typeof capture>>;
    if (reusedLuaDirectEvidence) {
      const origin = pathToFileURL(reusedLuaDirectEvidence);
      const evidence = JSON.parse(readFileSync(origin, 'utf8'));
      const relativeFile = (name: string): string => fileURLToPath(new URL(name, origin));
      expect(evidence.version).toBe(1);
      expect(evidence.consumer_sha256).toBe(sha256(readFileSync(process.env.E2E_SERVER_BIN!)));
      expect(evidence.runtime_sha256).toBe(sha256(readFileSync(process.env.PHASE9_REAL_FFMPEG!)));
      const before = JSON.parse(readFileSync(relativeFile(evidence.source_before), 'utf8')).files;
      const after = JSON.parse(readFileSync(relativeFile(evidence.source_after), 'utf8')).files;
      expect(Object.keys(after).sort()).toEqual(Object.keys(before).sort());
      expect(Object.keys(after).filter(name => before[name] !== after[name]).sort()).toEqual([
        'crates/ersatzrs-ffmpeg/src/concat.rs',
        'crates/ersatzrs-ffmpeg/tests/snapshots/pipeline_snapshots__concat_playlist_snapshot.snap',
        'crates/ersatzrs-web/src/transport/iptv/stream_wrapper_teletext_runtime_tests.rs',
      ]);
      const priorFile = relativeFile(evidence.direct_identity);
      const prior = JSON.parse(readFileSync(priorFile, 'utf8'));
      expect(prior.repository_zip_sha256).toBe(candidateDigest);
      const retained = (name: string, theme: string): Awaited<ReturnType<typeof capture>> => {
        const artifact = (suffix: string): string => fileURLToPath(new URL(`${name}.${suffix}`, pathToFileURL(priorFile)));
        const bytes = readFileSync(artifact('ts'));
        expect(bytes.length).toBe(prior[name].bytes); expect(sha256(bytes)).toBe(prior[name].sha256);
        expect(prior[name].ended).toBe('byte-limit'); expect(prior[name].route).toMatch(/^\/iptv\/hls-direct\/\d+\.ts$/);
        expect(readFileSync(artifact('av.stderr'), 'utf8')).toBe('');
        expect(readFileSync(artifact('teletext.stderr'), 'utf8')).toBe('');
        const decoded = readFileSync(artifact('srt'), 'utf8'); expect(decoded).toContain(theme);
        expect(decoded).toContain('Now: Synthetic red video and tone');
        return { path: artifact('ts'), decoded, timing: prior[name].timing };
      };
      onCapture = retained('production-guide-lua-on-night', 'NIGHT GUIDE');
      isolatedCapture = retained('production-guide-lua-isolated-classic', 'DAY GUIDE');
      receipts.reused_direct_evidence = { receipt_sha256: sha256(readFileSync(origin)),
        prior_identity_sha256: sha256(readFileSync(priorFile)), source_changes: evidence.source_changes,
        runtime_sha256: evidence.runtime_sha256, new_paired_responses: false }; persist();
    } else {
      let authoredObserved!: () => void;
      const onAuthored = new Promise<void>(resolve => { authoredObserved = resolve; });
      const onPending = capture(channel, 'production-guide-lua-on-night', 400, 'NIGHT GUIDE', authoredObserved, undefined, 'direct');
      // Keep the same original response/time allowance. Do not cold-start the
      // second cap1 reconstruction until the first actually delivers teletext.
      // Failure/completion without authored bytes terminates; it is never retried.
      await Promise.race([onAuthored, onPending.then(() => { throw new Error('On response completed before authored delivery.'); })]);
      const paired = await Promise.allSettled([onPending,
        capture(source, 'production-guide-lua-isolated-classic', 201, 'DAY GUIDE', undefined, undefined, 'direct')]);
      // Both original bounded responses settle before fixture cleanup, even when
      // one strict decoder/transport assertion rejects its own observation.
      for (const result of paired) if (result.status === 'rejected') throw result.reason;
      [onCapture, isolatedCapture] = paired.map(result => (result as PromiseFulfilledResult<Awaited<typeof onPending>>).value);
    }
    expect(utc(isolatedCapture.timing.first_read_utc!)).toBeLessThan(utc(onCapture.timing.last_read_utc!));
    expect(utc(isolatedCapture.timing.first_authored_utc!)).toBeLessThan(utc(onCapture.timing.last_read_utc!));
    expect(utc(onCapture.timing.first_authored_utc!)).toBeLessThan(utc(isolatedCapture.timing.last_read_utc!));
    assertContinuousAuthoredPmt(onCapture.path, info.outputPath('lua-on.pmt.json'), info.outputPath('lua-on.pmt.txt'));
    assertContinuousAuthoredPmt(isolatedCapture.path, info.outputPath('lua-isolated.pmt.json'), info.outputPath('lua-isolated.pmt.txt'));
    expect(Date.now()).toBeLessThan(utc(on[0].Finish));
    expect(Date.now()).toBeLessThan(utc(sourceNow.Finish));
    expect(document(onUrl)).toEqual(onOriginal);
    await historical(onUrl); expect(await rasterBytes(page)).toEqual(onRaster);
    for (const [index, id] of fixture.channels.entries()) expect(generation(id, false)).toEqual(baseAccepted[index]);
    expect(generation(source, true)).toBeUndefined();
    await queue('reset', null);
    const off = intervals(); expect(off[0].CandidateSnapshot).toBeNull();
    await queue('continue', undefined); expect(intervals()).toEqual(off);
    const offUrl = await prepare(true, 'Production Lua Off absence', true);
    expect(new URL(offUrl).searchParams.get('absence')).toBe('1');
    const offOriginal = document(offUrl), effectiveOff = await publication(true, 4);
    await expect(page.getByTestId('teletext-preview-model').locator('img')).toHaveCount(0);
    receipts.off_interval = off; receipts.off_generation = effectiveOff; receipts.off_original_sha256 = sha256(offOriginal); persist();
    const finish = utc(off[0].Finish);
    // The source channel is separate: its unchanged role must cover the full
    // response after the genuine output-only Off interval naturally expires.
    expect(utc(sourceNow.Finish) - finish).toBeGreaterThan(30_000);
    expect(Date.now()).toBeLessThan(finish - 5000);
    await expect.poll(() => Date.now() >= finish - 5000, { timeout: 65_000, intervals: [25] }).toBe(true);
    expect(Date.now()).toBeLessThan(finish);
    const returned = await capture(channel, 'production-guide-lua-off-expiry-base', 201, 'DAY GUIDE', undefined, finish);
    expect(utc(returned.timing.first_off_av_utc!)).toBeLessThan(finish - 1000);
    expect(utc(returned.timing.last_off_av_utc!) - utc(returned.timing.first_off_av_utc!)).toBeGreaterThanOrEqual(1000);
    expect(utc(returned.timing.first_authored_utc!)).toBeGreaterThanOrEqual(finish);
    expect(returned.timing.pmt_transitions.map(value => value.authored)).toEqual([false, true]);
    expect(utc(returned.timing.pmt_transitions[0].received_utc)).toBeLessThan(finish);
    expect(utc(returned.timing.pmt_transitions[1].received_utc)).toBeGreaterThanOrEqual(finish);
    execFileSync('tstables', ['--tid', '2', '--json-output', info.outputPath('lua-expiry.pmt.json'),
      '--text-output', info.outputPath('lua-expiry.pmt.txt'), returned.path], { timeout: 30_000, maxBuffer: 65_536 });
    const tables = JSON.parse(readFileSync(info.outputPath('lua-expiry.pmt.json'), 'utf8')) as {
      '#name': string; '#nodes': { '#name': string; elementary_pid?: number }[];
    }[];
    const authoredStates = tables.filter(row => row['#name'] === 'PMT').map(row => row['#nodes']
      .some(node => node['#name'] === 'component' && node.elementary_pid === 512));
    expect(authoredStates[0], 'The same response starts with explicit Off and AV.').toBe(false);
    expect(authoredStates).toContain(true);
    const firstBase = authoredStates.indexOf(true);
    expect(authoredStates.slice(firstBase).every(Boolean)).toBe(true);
    expect(returned.decoded).not.toContain('NIGHT GUIDE');
    receipts.expiry = { exact_finish: off[0].Finish, pmt_authored_states: authoredStates,
      observed_after_utc: new Date().toISOString() }; persist();
    await historical(offUrl);
    await expect(page.getByTestId('teletext-publication')).toHaveAttribute('data-state', 'unavailable');
    await expect(page.getByTestId('teletext-publication-publish-form').getByRole('button')).toBeDisabled();
    await historical(baseUrls[0]);
    await expect(page.getByTestId('teletext-preview-theme')).toHaveText('classic');
    await page.screenshot({ path: info.outputPath('production-guide-lua-returned-base.png') });
    expect(intervals()).toEqual(off); expect(sourceRows()).toEqual(retainedSource);
    expect(generation(channel, true)).toEqual(effectiveOff);
    for (const [index, id] of fixture.channels.entries()) {
      expect(generation(id, false)).toEqual(baseAccepted[index]); expect(document(baseUrls[index])).toEqual(baseOriginals[index]);
    }
    expect(fixture.database.prepare('SELECT * FROM TeletextProfile WHERE AddonId=? ORDER BY Id').all(addonId)).toEqual(profilesBefore);
    expect(fixture.database.prepare('SELECT * FROM ChannelTeletextBinding WHERE ChannelId IN (?,?) ORDER BY ChannelId,AddonId')
      .all(...fixture.channels)).toEqual(bindingsBefore);
    expect(fixture.setting('teletext.work.max_concurrent')).toBe('1'); expect(fixture.calls()).toEqual([]);
    expect(installation(isolatedWebHost.databasePath)).toEqual(selectedInstallation); compareInstalledFiles(selectedInstallation);
    expect(errors).toEqual([]); receipts.complete = true; persist();
  } finally { try { await context?.close(); } finally { persist(); fixture.close(); } }
});

// Part6: preserve the qualified sparse caption oracle while adding measured
// 0.55s video bursts. Exact sources and the real scanner are explicit inputs;
// this manual production-package trial never substitutes synthetic track rows.
test('production TV-guide ordinary bitrate bursts retain scanned captions across a programme boundary', async ({ browser, isolatedWebHost }, info) => {
  const baseSource = process.env.ERSATZRS_TELETEXT_RECOVERY_BASE_SOURCE;
  const burstSource = process.env.ERSATZRS_TELETEXT_RECOVERY_BURST_SOURCE;
  const scanner = process.env.ERSATZRS_TELETEXT_RECOVERY_SCANNER;
  test.skip(!baseSource || !burstSource || !scanner || !process.env.FFMPEG_PATH || !process.env.FFPROBE_PATH,
    'Requires qualified base/burst sources, the actual Local scanner and FFmpeg9.');
  test.skip(process.platform === 'win32', 'Existing POSIX fixture; native Windows remains part7.');
  test.setTimeout(240_000);
  expect(sha256(readFileSync(baseSource!))).toBe('b6ffa783e1cc96926a846b73fa8182755a11555296294ef9a740c09d9245c560');
  expect(sha256(readFileSync(burstSource!))).toBe('e70b7cf1f0c18cd5c1b8316f4b1e6f8a8bca45282a30514f451796d9debfa05e');
  const fixture = new TeletextChannelFixture(isolatedWebHost.databasePath, { authoredPages: true,
    playback: { ffmpeg: process.env.FFMPEG_PATH!, durationSeconds: 60,
      captionSource: { path: baseSource!, sha256: sha256(readFileSync(baseSource!)) } } });
  const context = await browser.newContext({ baseURL: isolatedWebHost.baseURL });
  const page = await context.newPage(); const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    const selected = await installProduction(browser, isolatedWebHost.baseURL, isolatedWebHost.databasePath);
    const initialScan = fixture.configureScannedScripted(scanner!, process.env.FFPROBE_PATH!, info.outputPath('initial-local-scan.log'));
    copyFileSync(burstSource!, join(fixture.root, 'scanned-library', 'Synthetic Caption.ts'));
    const library = fixture.channels[0] + 10;
    const scanLog = execFileSync(scanner!, ['--provider', 'local', '--library-id', String(library), '--deep-scan',
      '--ffprobe-path', process.env.FFPROBE_PATH!], { encoding: 'utf8', timeout: 30_000, maxBuffer: 1024 * 1024,
      env: { ...process.env, DATABASE_URL: `sqlite:${isolatedWebHost.databasePath}`,
        ERSATZRS_DATABASE__URL: `sqlite:${isolatedWebHost.databasePath}`, ErsatzRS_DATABASE__URL: `sqlite:${isolatedWebHost.databasePath}`,
        ERSATZRS_APP_STATE_DIR: join(fixture.root, 'scanner-state'), ERSATZRS_APP_DATA_DIR: join(fixture.root, 'scanner-data'),
        TMPDIR: join(fixture.root, 'scanner-tmp'), ERSATZRS_FFMPEG_DOWNLOAD: 'false' } });
    writeFileSync(info.outputPath('burst-local-scan.log'), scanLog);
    const media = fixture.database.prepare(`SELECT v.MovieId AS item,v.Id AS version,v.Duration AS duration
      FROM MediaVersion v JOIN MediaItem m ON m.Id=v.MovieId JOIN LibraryPath p ON p.Id=m.LibraryPathId
      WHERE p.LibraryId=?`).all(library) as { item: number; version: number; duration: string }[];
    expect(media).toHaveLength(1); expect(media[0].item).toBe(initialScan.item);
    const inventory = fixture.database.prepare('SELECT "Index",Codec,Language FROM MediaStream WHERE MediaVersionId=? ORDER BY "Index"')
      .all(media[0].version);
    expect(inventory.map(row => [row.Index, row.Codec, row.Language])).toEqual([
      [0, 'h264', null], [1, 'aac', 'eng'], [2, 'dvb_teletext', 'eng,eng'], [3, 'aac', 'nld'],
    ]);
    expect(fixture.database.prepare('SELECT 1 FROM SourceTeletextMetadata WHERE MediaVersionId=? AND StreamIndex=2 AND Pid=258')
      .get(media[0].version)).toBeDefined();
    expect(fixture.database.prepare('SELECT 1 FROM ManagedLibraryLease WHERE LibraryId=?').get(library)).toBeUndefined();
    for (const channel of fixture.channels) {
      await page.goto(`/channels/${channel}`); await ready(page, true);
      for (const [name, value] of Object.entries({ preferred_audio_language_code: 'dut', preferred_subtitle_language_code: 'eng',
        subtitle_mode: '3', source_teletext_delivery: '1' })) await page.locator(`select[name="${name}"]`).selectOption(value);
      await page.getByRole('button', { name: translation('en_us')('SaveChannel'), exact: true }).click();
      await expect(page).toHaveURL(/\/channels$/); await ready(page, true);
    }
    await page.goto(`/channels/${fixture.channels[0]}/teletext`); await ready(page, true);
    await page.getByTestId('teletext-broadcast-check').locator(':scope > summary').click();
    const capacity = page.getByTestId('teletext-capacity-form');
    await capacity.locator('[name="profile"]').selectOption('tvguide-16');
    await capacity.locator('[name="confirm_scope"]').check();
    await Promise.all([page.waitForURL(url => url.search === '?saved=capacity'), capacity.locator('button[type="submit"]').click()]);
    await ready(page, true);
    const profiles = [await createProfile(page, fixture, 'classic', true), await createProfile(page, fixture, 'night', true)];
    for (const channel of [...fixture.channels].reverse()) await resetGuidePlayout(page, fixture, channel,
      { count: 8, durationMilliseconds: 60_000, sourceDuration: media[0].duration });
    const rows = fixture.channels.map(channel => fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(channel));
    const first = rows[0][0] as { Start: string; Finish: string };
    expect(Date.now() - utc(first.Start)).toBeLessThan(25_000);
    const maps = [];
    for (const [index, channel] of fixture.channels.entries()) maps.push(await prepareReceiverGuide(page, fixture, channel, profiles[index], index === 1));
    const originals = fixture.channels.map(channel => sha256(originalBytes(fixture, channel)));
    const accepted = fixture.channels.map(channel => acceptedGuide(fixture, channel));
    const current = () => fixture.database.prepare('SELECT SeedId,RuntimeId,Revision,Revoked FROM TeletextGuideRefreshCurrent WHERE ChannelId=? AND Role=1')
      .get(fixture.channels[0]) as { SeedId: number; RuntimeId: number | null; Revision: number; Revoked: number };
    const before = current();
    writeFileSync(info.outputPath('burst-caption-seed-receipt.json'), JSON.stringify({ first, rows, media, inventory, maps, before, originals, accepted }, null, 2));
    // Fixed RGBA glyph/control hashes from the qualified page888 source decoded
    // by libzvbi. Full 1280x720 DVB hashes are independently qualified through
    // an FFmpeg-only AV/caption mux. No oracle crop, rescale or OCR tolerance
    // can hide extra or missing pixels.
    const sourceHashes = { clear: '830edf21ee6bc8ed9dd01f0982075cb1',
      a: '5518fb92c7291a2b6361428648ea83f4', b: '9251724c00efb989a709aadaa678e5b7' };
    const cueHashes = { clear: '7b3ba1b1b5fedb562f08fe3428e99dc2',
      a: 'e4935f1ddeaa71da89c6519b573d70e7', b: 'efc315343b410d63c3c4334356dea670' };
    const captionFrames = (path: string, index: number, name: string, source = false) => {
      const args = ['-v', 'error', '-nostdin', ...(source ? ['-fix_teletext_pts', '0', '-txt_page:2', '888',
        '-txt_format:2', 'bitmap', '-txt_default_region:2', '-1'] : []), '-i', path,
        '-filter_complex', `[0:${index}]format=rgba[caption]`, '-map', '[caption]',
        '-an', '-fps_mode', 'passthrough', '-f', 'framemd5', '-'];
      const raw = execFileSync(process.env.FFMPEG_PATH!, args, { timeout: 30_000, maxBuffer: 256 * 1024 }).toString();
      writeFileSync(info.outputPath(`${name}.captions.framemd5`), raw);
      expect(raw).toContain(source ? '#dimensions 0: 492x250' : '#dimensions 0: 1280x720');
      const frames = raw.split('\n').filter(line => line && !line.startsWith('#')).map(line => {
        const fields = line.split(',').map(field => field.trim()); expect(Number(fields[4])).toBe(source ? 492 * 250 * 4 : 1280 * 720 * 4);
        expect(Object.values(source ? sourceHashes : cueHashes)).toContain(fields[5]); return { pts: Number(fields[2]), hash: fields[5] };
      });
      expect(frames.length).toBeGreaterThan(0); return frames;
    };
    const sourceFrames = captionFrames(burstSource!, 2, 'qualified-source', true);
    expect([...new Set(sourceFrames.map(frame => frame.hash))]).toEqual([sourceHashes.clear, sourceHashes.a, sourceHashes.b]);
    const cueReceipts: Record<string, { pts: number; hash: string }[]> = {};
    const subtitleEvents: Record<string, { pts: number; num_rects: number }[]> = {};
    const videoEnds: Record<string, number> = {};
    const completeAvPrefix = (path: string, name: string, streams: { codec_type: string; id: string }[]) => {
      const video = Number(streams.find(stream => stream.codec_type === 'video')!.id);
      const audio = Number(streams.find(stream => stream.codec_type === 'audio')!.id);
      const trace = execFileSync('tsp', ['-I', 'file', path, '-P', 'pes', '--pid', String(video), '--pid', String(audio),
        '--trace-packets', '--packet-index', '-O', 'drop'], { timeout: 30_000, maxBuffer: 256 * 1024 }).toString();
      writeFileSync(info.outputPath(`${name}.av-pes.txt`), trace);
      // TSDuck's default does not flush the final unbounded video PES as if it
      // were complete. Only its positively completed PES endpoints qualify.
      const ends = new Map([[video, new Set<number>()], [audio, new Set<number>()]]);
      for (const match of trace.matchAll(/PID 0x([0-9a-f]+), TS packets ([\d,]+)-([\d,]+),/gi)) {
        ends.get(parseInt(match[1], 16))!.add(Number(match[3].replaceAll(',', '')));
      }
      const raw = readFileSync(path); let lastAudio = -1; let boundary = 0;
      for (let ordinal = 0; ordinal < raw.length / 188; ordinal++) {
        const offset = ordinal * 188; if (raw[offset] !== 0x47) throw new Error('Captured transport packet framing changed.');
        const pid = ((raw[offset + 1] & 31) << 8) | raw[offset + 2];
        if (!(raw[offset + 3] & 0x10)) continue;
        if (pid === audio) lastAudio = ordinal;
        if (pid === video && ends.get(video)!.has(ordinal) && ends.get(audio)!.has(lastAudio)) boundary = (ordinal + 1) * 188;
      }
      expect(boundary).toBeGreaterThan(raw.length * 0.9);
      const complete = info.outputPath(`${name}.complete-av.ts`); writeFileSync(complete, raw.subarray(0, boundary));
      writeFileSync(info.outputPath(`${name}.complete-av.json`), JSON.stringify({ raw_bytes: raw.length,
        complete_av_prefix_bytes: boundary, omitted_terminal_bytes: raw.length - boundary, video, audio,
        scope: 'Raw capture and strict transport gate remain unchanged; this prefix ends at positively completed AV PES packets.' }, null, 2));
      return complete;
    };
    const hostLog = info.outputPath('isolated-settings-host.log');
    const captures: { name: string; path: string; opened: string; closed: string; channel_id: number;
      log_start: number; log_first_chunk: number; log_end: number }[] = [];
    const captureRaw = async (channel: number, name: string, legacy = false) => {
      const path = info.outputPath(`${name}.ts`); const opened = new Date().toISOString();
      const logStart = statSync(hostLog).size; let firstChunkLog: number | undefined;
      const route = legacy ? `/iptv/channel/${channel}.ts?mode=ts-legacy` : `/iptv/hls-direct/${channel}.ts`;
      const result = await captureTeletextChannelTs(`${isolatedWebHost.baseURL}${route}`, path,
        { maximumBytes: Math.floor(8 * 1024 * 1024 / 188) * 188, maximumTimeMs: 30_000 },
        () => { firstChunkLog ??= statSync(hostLog).size; });
      expect(result.ended).toBe('byte-limit');
      expect(firstChunkLog).toBeDefined();
      captures.push({ name, path, opened, closed: new Date().toISOString(), channel_id: channel,
        log_start: logStart, log_first_chunk: firstChunkLog!, log_end: statSync(hostLog).size });
      writeFileSync(info.outputPath('burst-capture-requests.json'), JSON.stringify(captures, null, 2));
      return path;
    };
    const rawGateFailures: { name: string; status: number | null }[] = [];
    let boundaryGuidePath: string | undefined;
    const validateCapture = (path: string, name: string) => {
      if (name !== 'burst-programme-boundary') {
        assertContinuousAuthoredPmt(path, info.outputPath(`${name}.pmt.json`), info.outputPath(`${name}.pmt.txt`));
      } else {
        // ADR0124 requires withdrawal before a blocked expiry recovery. Prove
        // its actual owner, saved UTC cap and sink ACKs instead of requiring
        // continuous signalling across that legitimate authority boundary.
        const prefix = info.outputPath(name);
        execFileSync('tstables', ['--tid', '2', '--packet-index', '--json-output', `${prefix}.pmt.json`,
          '--text-output', `${prefix}.pmt.txt`, path], { timeout: 30_000, maxBuffer: 65_536 });
        const sections = spawnSync('tstables', ['--tid', '2', '--all-sections', '--log', '--packet-index', path],
          { timeout: 30_000, maxBuffer: 256 * 1024 });
        expect(sections.status).toBe(0);
        writeFileSync(`${prefix}.pmt-sections.txt`, Buffer.concat([sections.stdout ?? Buffer.alloc(0), sections.stderr ?? Buffer.alloc(0)]));
        const request = captures.find(capture => capture.name === name)!;
        const fullLog = readFileSync(hostLog);
        const sliceStart = fullLog.lastIndexOf(10, request.log_start - 1) + 1;
        const sliceEnd = fullLog.lastIndexOf(10, request.log_end - 1) + 1;
        expect(sliceEnd).toBeGreaterThan(request.log_first_chunk);
        writeFileSync(`${prefix}.request.log`, fullLog.subarray(sliceStart, sliceEnd));
        writeFileSync(`${prefix}.request.json`, JSON.stringify({ ...request, log_start: request.log_start - sliceStart,
          log_first_chunk: request.log_first_chunk - sliceStart, log_end: sliceEnd - sliceStart,
          source_log_prefix_bytes: fullLog.length, source_log_prefix_sha256: sha256(fullLog),
          source_log_slice_start: sliceStart, source_log_slice_end: sliceEnd }, null, 2));
        const metadata = fixture.database.prepare(`
          SELECT ChannelId AS channel_id,Id AS seed_id,GenerationId AS generation_id,NULL AS runtime_id,ValidUntil AS valid_until
          FROM TeletextGuideRefreshSeed WHERE ChannelId=? AND Role=1
          UNION ALL
          SELECT s.ChannelId AS channel_id,s.Id AS seed_id,s.GenerationId AS generation_id,r.Id AS runtime_id,r.ValidUntil AS valid_until
          FROM TeletextGuideRuntimeSnapshot r JOIN TeletextGuideRefreshSeed s ON s.Id=r.SeedId
          WHERE s.ChannelId=? AND s.Role=1`).all(request.channel_id, request.channel_id) as { valid_until: string }[];
        for (const row of metadata) row.valid_until = row.valid_until.replace(' ', 'T')
          + (/(?:Z|[+-]\d{2}:\d{2})$/.test(row.valid_until) ? '' : 'Z');
        writeFileSync(`${prefix}.metadata.json`, JSON.stringify(metadata, null, 2));
        execFileSync('python3', ['scripts/e2e/teletext-guide-boundary.py', '--capture', path,
          '--log', `${prefix}.request.log`, '--request', `${prefix}.request.json`, '--pmt', `${prefix}.pmt.json`,
          '--sections', `${prefix}.pmt-sections.txt`, '--metadata', `${prefix}.metadata.json`, '--report', `${prefix}.boundary.json`],
        { timeout: 30_000, maxBuffer: 65_536 });
        const proof = JSON.parse(readFileSync(`${prefix}.boundary.json`, 'utf8')) as { recovered_packet: number };
        boundaryGuidePath = `${prefix}.recovered-guide.ts`;
        writeFileSync(boundaryGuidePath, readFileSync(path).subarray(proof.recovered_packet * 188));
      }
      const streams = JSON.parse(execFileSync(process.env.FFPROBE_PATH!, ['-v', 'error', '-show_streams', '-of', 'json', path],
        { timeout: 30_000, maxBuffer: 1024 * 1024 }).toString()).streams;
      expect(streams.map((s: { codec_name: string }) => s.codec_name).sort()).toEqual(['aac', 'dvb_subtitle', 'dvb_teletext', 'h264']);
      expect(streams.find((s: { codec_type: string }) => s.codec_type === 'audio').tags.language).toBe('nld');
      expect(streams.find((s: { codec_name: string }) => s.codec_name === 'dvb_subtitle').tags.language).toBe('eng');
      const captionIndex = streams.find((s: { codec_name: string }) => s.codec_name === 'dvb_subtitle').index;
      cueReceipts[name] = captionFrames(path, captionIndex, name);
      // Probe actual transmitted subtitle events with demux timestamp repair
      // disabled. The sub2video renderer's EOF/timeout blank is not a clear PES.
      const events = JSON.parse(execFileSync(process.env.FFPROBE_PATH!, ['-v', 'error', '-fix_teletext_pts', '0',
        '-select_streams', String(captionIndex), '-show_frames', '-show_entries', 'frame=pts,num_rects', '-of', 'json', path],
        { timeout: 30_000, maxBuffer: 256 * 1024 }).toString()).frames;
      subtitleEvents[name] = events;
      const complete = completeAvPrefix(path, name, streams);
      const packets = JSON.parse(execFileSync(process.env.FFPROBE_PATH!, ['-v', 'error', '-select_streams', 'v:0',
        '-show_packets', '-show_entries', 'packet=pts_time', '-of', 'json', complete],
        { timeout: 30_000, maxBuffer: 256 * 1024 }).toString()).packets;
      videoEnds[name] = Math.max(...packets.map((packet: { pts_time: string }) => Math.round(Number(packet.pts_time) * 1_000_000)));
      writeFileSync(info.outputPath(`${name}.subtitle-events.json`), JSON.stringify({ events, videoEnd: videoEnds[name] }, null, 2));
      const decoded = spawnSync(process.env.FFMPEG_PATH!, ['-v', 'error', '-nostdin', '-i', complete, '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
        { timeout: 30_000, maxBuffer: 65_536 });
      writeFileSync(info.outputPath(`${name}.decode.stderr`), decoded.stderr ?? '');
      expect(decoded.status).toBe(0); expect(decoded.stderr.length).toBe(0);
      writeFileSync(info.outputPath(`${name}.streams.json`), JSON.stringify(streams, null, 2));
      const gate = spawnSync('bash', ['-c', 'source scripts/e2e/tsduck-checks.sh; STRICT_PCR_JITTER=1; analyze_ts "$1" "$2" "$3"; check_pcr_jitter "$1" "$2"; check_continuity_plugin "$1" "$2"; checks_summary',
        'gate', path, name, info.outputPath(`${name}.analyze.json`)], { timeout: 30_000, maxBuffer: 256 * 1024 });
      writeFileSync(info.outputPath(`${name}.gate.log`), Buffer.concat([gate.stdout ?? Buffer.alloc(0), gate.stderr ?? Buffer.alloc(0)]));
      if (gate.status !== 0) rawGateFailures.push({ name, status: gate.status });
      return path;
    };
    // Launch all windows against the real programme clock before synchronous
    // decoding/TSDuck work. Each response retains its original 8MiB/30s limits.
    const timed = async (seconds: number, name: string, legacy = false) => {
      await expect.poll(() => Date.now() - utc(first.Start) >= seconds * 1000,
        { timeout: 70_000, intervals: [100] }).toBe(true);
      return captureRaw(fixture.channels[0], name, legacy);
    };
    const paths = await Promise.all([
      ...fixture.channels.map((channel, index) => captureRaw(channel, `burst-cold-${index}`)),
      timed(24, 'burst-caption-b'), timed(38, 'burst-caption-clear'), timed(54, 'burst-programme-boundary', true),
    ]);
    const names = ['burst-cold-0', 'burst-cold-1', 'burst-caption-b', 'burst-caption-clear', 'burst-programme-boundary'];
    for (const [index, path] of paths.entries()) validateCapture(path, names[index]);
    const cold = paths.slice(0, 2); const boundary = paths[4];
    for (const name of ['burst-cold-0', 'burst-cold-1']) expect(cueReceipts[name].some(frame => frame.hash === cueHashes.a)).toBe(true);
    const guideText = (path: string, pageNumber: number) => execFileSync(process.env.FFMPEG_PATH!, ['-v', 'error', '-nostdin', '-fix_teletext_pts', '0',
      '-txt_page', String(pageNumber), '-txt_format', 'text', '-txt_duration', '1000', '-i', path,
      '-map', '0:i:512', '-c:s', 'srt', '-f', 'srt', '-'], { timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }).toString();
    expect(guideText(cold[0], 200)).toContain('DAY GUIDE'); expect(guideText(cold[0], 201)).toContain('Now:');
    expect(guideText(cold[1], 410)).toContain('NIGHT GUIDE'); expect(guideText(cold[1], 200)).toBe('');
    expect(cueReceipts['burst-caption-b'].some(frame => frame.hash === cueHashes.b)).toBe(true);
    const cleared = cueReceipts['burst-caption-clear'];
    expect(cleared.some(frame => frame.hash === cueHashes.b)).toBe(true);
    expect(cleared[cleared.length - 1].hash).toBe(cueHashes.clear);
    const beforeClear = subtitleEvents['burst-caption-clear'].find(frame => frame.num_rects > 0)!;
    expect(beforeClear).toBeDefined();
    expect(subtitleEvents['burst-caption-clear'].some(frame => frame.num_rects === 0
      && frame.pts > beforeClear.pts && frame.pts <= videoEnds['burst-caption-clear'])).toBe(true);
    const rollover = cueReceipts['burst-programme-boundary'];
    expect(rollover[0].hash).toBe(cueHashes.clear);
    expect(rollover.some(frame => frame.hash === cueHashes.a)).toBe(true);
    expect(rollover.some(frame => frame.hash === cueHashes.b)).toBe(false);
    const last = captures.find(capture => capture.name === 'burst-programme-boundary')!;
    expect(utc(last.opened)).toBeLessThan(utc(first.Finish)); expect(utc(last.closed)).toBeGreaterThan(utc(first.Finish));
    expect(boundaryGuidePath).toBeDefined();
    expect(guideText(boundaryGuidePath!, 200)).toContain('DAY GUIDE');
    expect(guideText(boundaryGuidePath!, 201)).toContain('Now:');
    expect(current().SeedId).toBe(before.SeedId); expect(current().RuntimeId).not.toBeNull(); expect(current().Revoked).toBe(0);
    expect(current().Revision).toBeGreaterThan(before.Revision);
    for (const [index, channel] of fixture.channels.entries()) {
      expect(sha256(originalBytes(fixture, channel))).toBe(originals[index]); expect(acceptedGuide(fixture, channel)).toEqual(accepted[index]);
      expect(fixture.database.prepare('SELECT * FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start').all(channel)).toEqual(rows[index]);
    }
    expect(installation(isolatedWebHost.databasePath)).toEqual(selected); compareInstalledFiles(selected);
    expect(fixture.calls()).toEqual([]); expect(errors).toEqual([]);
    writeFileSync(info.outputPath('burst-caption-host-receipt.json'), JSON.stringify({ source_sha256: sha256(readFileSync(burstSource!)),
      scanner_sha256: sha256(readFileSync(scanner!)), media, inventory, maps, captures, sourceFrames, cueReceipts, subtitleEvents, videoEnds, rawGateFailures, before, after: current(),
      scope: 'actual Local rescan, ordinary profile, published producer, cold/clear/boundary transport and fixed page888 RGBA cue/clear oracle' }, null, 2));
    expect(rawGateFailures).toEqual([]);
  } finally { await context.close(); fixture.close(); }
});
