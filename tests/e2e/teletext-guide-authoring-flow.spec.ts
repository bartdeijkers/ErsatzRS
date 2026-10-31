import { Browser, BrowserContext, expect, Locator, Page } from '@playwright/test';
import { test } from './isolated-web-host';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { addonContentHash } from './fixtures/addon-metadata';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';
import { captureTeletextChannelTs } from './helpers/teletext-stream-capture';
import { archiveFiles, type Upload } from './helpers/teletext-native-editor';

// Manual exact-package gate. CI still classifies this base/shared spec as 1.5;
// absent package evidence is an explicit skip, never a synthetic replacement.
const databasePath = process.env.E2E_DB_PATH;
const candidateZip = process.env.ERSATZRS_TVGUIDE_CANDIDATE_ZIP;
const candidateDigest = candidateZip ? sha256(readFileSync(candidateZip)) : undefined;
const addonId = 'org.ersatzrs.addon.tvguide-teletext';
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
test.skip(!databasePath || !candidateZip, 'Requires disposable E2E_DB_PATH and an explicit production0.1.1 ERSATZRS_TVGUIDE_CANDIDATE_ZIP.');

test.beforeAll(async ({ browser, baseURL }) => {
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
    expect(selected.Version).toBe('0.1.1');
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
      { type: 'production-package', description: `${addonId}0.1.1; repositoryZIP SHA256 ${candidateDigest}; contentSHA256 ${installed.ContentSha256}` });
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
async function resetGuidePlayout(page: Page, fixture: TeletextChannelFixture, channel: number): Promise<void> {
  const text = translation('en_us');
  const previous = fixture.database.prepare('SELECT LastBuild FROM PlayoutBuildStatus WHERE PlayoutId=?')
    .get(channel) as { LastBuild: string } | undefined;
  fixture.writeScriptedControls(channel, undefined, 3);
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
      && status.LastSuccessfulBuild === status.LastBuild && items.count === 3
      && fixture.scriptedIntervals(channel).length === 0;
  }, { timeout: 45_000 }).toBe(true);
  const spans = fixture.database.prepare('SELECT Start,Finish FROM PlayoutItem WHERE PlayoutId=? ORDER BY Start')
    .all(channel) as { Start: string; Finish: string }[];
  for (const span of spans) expect(utc(span.Finish) - utc(span.Start)).toBe(180_000);
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
    { type: 'production-package', description: `${addonId}0.1.1; repositoryZIP SHA256 ${candidateDigest}; contentSHA256 ${installed.ContentSha256}` });
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
