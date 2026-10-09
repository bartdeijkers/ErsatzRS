import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { waitForHydration as waitForIslandHydration } from './hydration';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for playout seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function seedPlayout(testInfo: TestInfo): string {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const channelId = 160_000 + projectOffset;
  const playoutId = channelId + 1;
  const channelName = `E2E Playout Feedback ${testInfo.project.name}`;
  const profileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  if (!profileId) {
    throw new Error('E2E database has no FFmpegProfile row for playout seeding');
  }

  sqlite(`
DELETE FROM "Playout" WHERE "Id" = ${playoutId};
DELETE FROM "Channel" WHERE "Id" = ${channelId};

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES (
  ${channelId}, '00000000-0000-0000-0000-${channelId.toString().padStart(12, '0')}',
  '${channelId}', ${-channelId}, '${channelName}', 'ErsatzTV',
  ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1
);

INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
VALUES (${playoutId}, ${channelId}, 2, 1);
`);

  return channelName;
}

type DetailedPlayoutSeed = {
  classicChannelName: string;
  externalChannelName: string;
  deleteChannelName: string;
  classicPlayoutId: number;
  externalPlayoutId: number;
  deletePlayoutId: number;
  primaryTitle: string;
  fillerTitle: string;
};

function seedDetailedPlayoutWorkflow(testInfo: TestInfo): DetailedPlayoutSeed {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const base = 360_000 + projectOffset;
  const classicChannelId = base + 1;
  const externalChannelId = base + 2;
  const deleteChannelId = base + 3;
  const classicPlayoutId = base + 101;
  const externalPlayoutId = base + 102;
  const deletePlayoutId = base + 103;
  const mediaSourceId = base + 201;
  const libraryId = base + 202;
  const libraryPathId = base + 203;
  const primaryMediaId = base + 301;
  const fillerMediaId = base + 302;
  const primaryItemId = base + 401;
  const fillerItemId = base + 402;
  const gapId = base + 403;
  const classicChannelName = `E2E Detailed Classic ${testInfo.project.name}`;
  const externalChannelName = `E2E Detailed JSON ${testInfo.project.name}`;
  const deleteChannelName = `E2E Detailed Delete ${testInfo.project.name}`;
  const primaryTitle = `E2E Primary Movie ${testInfo.project.name}`;
  const fillerTitle = `E2E Filler Movie ${testInfo.project.name}`;
  const profileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  if (!profileId) {
    throw new Error('E2E database has no FFmpegProfile row for playout seeding');
  }

  sqlite(`
DELETE FROM "PlayoutItem" WHERE "Id" IN (${primaryItemId}, ${fillerItemId});
DELETE FROM "PlayoutGap" WHERE "Id" = ${gapId};
DELETE FROM "Playout" WHERE "Id" IN (${classicPlayoutId}, ${externalPlayoutId}, ${deletePlayoutId});
DELETE FROM "Channel" WHERE "Id" IN (${classicChannelId}, ${externalChannelId}, ${deleteChannelId});
DELETE FROM "MovieMetadata" WHERE "MovieId" IN (${primaryMediaId}, ${fillerMediaId});
DELETE FROM "Movie" WHERE "Id" IN (${primaryMediaId}, ${fillerMediaId});
DELETE FROM "MediaItem" WHERE "Id" IN (${primaryMediaId}, ${fillerMediaId});
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "MediaSource" WHERE "Id" = ${mediaSourceId};
DELETE FROM "ConfigElement" WHERE "Key" = 'pages.playouts.detail_show_filler';

INSERT INTO "MediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
VALUES (${libraryId}, 1, ${mediaSourceId}, 'E2E Detailed Playout Library');
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
VALUES (${libraryPathId}, '/e2e/playouts', ${libraryId});
INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES
  (${primaryMediaId}, ${libraryPathId}, 0),
  (${fillerMediaId}, ${libraryPathId}, 0);
INSERT INTO "Movie" ("Id") VALUES (${primaryMediaId}), (${fillerMediaId});
INSERT INTO "MovieMetadata" (
  "Id", "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title"
) VALUES
  (${base + 311}, '2026-07-22 00:00:00', '2026-07-22 00:00:00', 0, ${primaryMediaId}, '${primaryTitle}'),
  (${base + 312}, '2026-07-22 00:00:00', '2026-07-22 00:00:00', 0, ${fillerMediaId}, '${fillerTitle}');

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES
  (${classicChannelId}, '10000000-0000-0000-0000-${classicChannelId.toString().padStart(12, '0')}',
   '${classicChannelId}', ${-classicChannelId}, '${classicChannelName}', 'ErsatzRS',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1),
  (${externalChannelId}, '20000000-0000-0000-0000-${externalChannelId.toString().padStart(12, '0')}',
   '${externalChannelId}', ${-externalChannelId}, '${externalChannelName}', 'ErsatzRS',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1),
  (${deleteChannelId}, '30000000-0000-0000-0000-${deleteChannelId.toString().padStart(12, '0')}',
   '${deleteChannelId}', ${-deleteChannelId}, '${deleteChannelName}', 'ErsatzRS',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1);

INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES
  (${classicPlayoutId}, ${classicChannelId}, 1, 1, NULL),
  (${externalPlayoutId}, ${externalChannelId}, 20, 1, '/e2e/original.json'),
  (${deletePlayoutId}, ${deleteChannelId}, 3, 1, 'schedule: []');

INSERT INTO "PlayoutItem" (
  "Id", "DisableWatermarks", "FillerKind", "Finish", "GuideGroup", "InPoint",
  "MediaItemId", "OutPoint", "PlayoutId", "Start", "SchedulingContext"
) VALUES
  (${primaryItemId}, 0, 0, '2099-01-01 00:30:00', 0, '00:00:00',
   ${primaryMediaId}, '00:30:00', ${classicPlayoutId}, '2099-01-01 00:00:00', '{"Test":"primary context"}'),
  (${fillerItemId}, 0, 1, '2099-01-01 01:00:00', 0, '00:00:00',
   ${fillerMediaId}, '00:30:00', ${classicPlayoutId}, '2099-01-01 00:30:00', NULL);
INSERT INTO "PlayoutGap" ("Id", "PlayoutId", "Start", "Finish")
VALUES (${gapId}, ${classicPlayoutId}, '2099-01-01 01:00:00', '2099-01-01 01:05:00');
`);

  return {
    classicChannelName,
    externalChannelName,
    deleteChannelName,
    classicPlayoutId,
    externalPlayoutId,
    deletePlayoutId,
    primaryTitle,
    fillerTitle,
  };
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () => page.evaluate(
        () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
      ),
      { timeout: 15_000, message: 'playouts page did not finish hydration' },
    )
    .toBe(true);
}

test.describe('playout rebuild feedback', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed a playout');

  test('kind editors hydrate resolved forms and attach Scripted and Block controls', async ({ page, browser }, testInfo) => {
    test.setTimeout(90_000);
    const seed = seedDetailedPlayoutWorkflow(testInfo);
    const originalScript = 'synthetic/original-playout-editor.lua';
    sqlite(`
      UPDATE "Playout" SET "ScheduleKind" = 2, "ScheduleFile" = NULL WHERE "Id" = ${seed.externalPlayoutId};
      UPDATE "Playout" SET "ScheduleKind" = 4, "ScheduleFile" = '${originalScript}' WHERE "Id" = ${seed.deletePlayoutId};
    `);
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (/failed_to_cast_marker_node|hydration.*mismatch/i.test(message.text())) errors.push(message.text());
    });
    await forceEnglish(page);
    const noJs = await browser.newContext({ javaScriptEnabled: false, baseURL: process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409' });
    try {
      const fallback = await noJs.newPage();
      await fallback.goto(`/playouts/scripted/${seed.deletePlayoutId}`);
      await expect(fallback.getByLabel('Scripted Schedule', { exact: true })).toBeVisible();
      await expect(fallback.getByRole('button', { name: 'Save Scripted Schedule', exact: true })).toBeVisible();
      await expect(fallback.locator('[data-admin-resource]')).toHaveAttribute('data-state', 'ready');
    } finally {
      await noJs.close();
    }
    try {
      for (const [language, width] of [['en-us', 1440], ['nl', 390]] as const) {
        await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'false', language }, maxRedirects: 0 });
        await page.setViewportSize({ width, height: 900 });
        for (const [kind, id] of [
          ['scripted', seed.deletePlayoutId],
          ['classic', seed.classicPlayoutId],
          ['block', seed.externalPlayoutId],
        ] as const) {
          await page.goto(`/playouts/${kind}/${id}`);
          await waitForIslandHydration(page);
          await expect.poll(() => page.evaluate(
            () => (window as Window & { __ersatzrs_hydration_phase?: string }).__ersatzrs_hydration_phase,
          ), { timeout: 15_000, message: `${kind} editor island traversal did not complete` }).toBe('ready');
          await expect(page.locator('[data-admin-resource]')).toHaveAttribute('data-state', 'ready');
          if (kind === 'scripted') {
            const field = page.locator('#scripted-schedule');
            await expect(field).toHaveValue(originalScript);
            const missingScript = `synthetic/not-present-playout-editor-${id}.lua`;
            await field.fill(missingScript);
            const response = page.waitForResponse(result => result.url().includes('/admin-api/save_scripted_playout'));
            await page.getByRole('button', { name: language === 'nl' ? 'Scripted schema opslaan' : 'Save Scripted Schedule', exact: true }).click();
            const result = await response;
            const body = new URLSearchParams(result.request().postData() ?? '');
            expect(body.get('id')).toBe(String(id));
            expect(body.get('schedule_file')).toBe(missingScript);
            await expect(page.getByText(language === 'nl' ? /^Opslaan mislukt:/ : /^Save failed:/)).toBeVisible();
            expect(sqlite(`SELECT "ScheduleFile" FROM "Playout" WHERE "Id" = ${id};`)).toBe(originalScript);
            await page.screenshot({ path: testInfo.outputPath(`scripted-editor-${language}.png`), fullPage: true });
          } else if (kind === 'block') {
            const toggle = page.locator('#enable-default-deco');
            await expect(toggle).not.toBeChecked();
            await expect(page.locator('#deco-group')).toBeHidden();
            await toggle.check();
            await expect(page.locator('#deco-group')).toBeVisible();
            await toggle.uncheck();
            await expect(page.locator('#deco-group')).toBeHidden();
          } else {
            await expect(page.getByRole('link', { name: language === 'nl' ? "Alternatieve schema's bewerken" : 'Edit Alternate Schedules', exact: true }))
              .toHaveAttribute('href', `/playouts/${id}/alternate-schedules`);
          }
        }
      }
      await page.waitForTimeout(2_000);
      expect(errors).toEqual([]);
    } finally {
      await forceEnglish(page);
    }
  });

  test('Add Playout opens and closes the creation menu', async ({ page }) => {
    await forceEnglish(page);
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto('/playouts');
      await waitForHydration(page);
      const toggle = page.getByRole('button', { name: 'Add Playout', exact: true });
      const links = [
        ['Classic Schedule', '/playouts/add'],
        ['Block Schedule', '/playouts/add/Block'],
        ['Sequential Schedule', '/playouts/add/Sequential'],
        ['Scripted Schedule', '/playouts/add/Scripted'],
        ['JSON (dizqueTV) Schedule', '/playouts/add/ExternalJson'],
      ];
      await expect(page.getByRole('link', { name: links[0][0], exact: true })).toBeHidden();
      await toggle.click();
      for (const [name, href] of links) {
        const link = page.getByRole('link', { name, exact: true });
        await expect(link).toBeVisible();
        await expect(link).toHaveAttribute('href', href);
      }
      await toggle.press('Enter');
      for (const [name] of links) {
        await expect(page.getByRole('link', { name, exact: true })).toBeHidden();
      }
      await toggle.press('Space');
      await expect(page.getByRole('link', { name: links[0][0], exact: true })).toBeVisible();
    }
  });

  test('shows queued feedback in the shared toast surface', async ({ page }, testInfo) => {
    const channelName = seedPlayout(testInfo);
    await forceEnglish(page);

    await page.goto('/playouts');
    await waitForHydration(page);
    const row = page.getByRole('row').filter({ hasText: channelName });
    await expect(row).toBeVisible();
    await row.getByRole('button', { name: 'Update Playout' }).click();
    const rebuildDialog = page.getByRole('dialog', { name: 'Update Playout', exact: true });
    await expect(rebuildDialog.getByLabel('Action', { exact: true })).toHaveValue('refresh');
    await expect(rebuildDialog.getByRole('checkbox', { name: 'Keep the current programme' })).toBeChecked();
    await rebuildDialog.getByRole('button', { name: 'Queue update' }).click();

    const feedback = page.getByText('Playout rebuild queued', { exact: true });
    await expect(feedback).toBeVisible();
    await expect
      .poll(() => feedback.evaluate((element) => {
        const toast = element.closest('div[class*="toast-base"]');
        return {
          inToast: toast !== null,
          containerPosition: toast?.parentElement
            ? window.getComputedStyle(toast.parentElement).position
            : null,
        };
      }))
      .toEqual({ inToast: true, containerPosition: 'fixed' });
  });

  test('explicit rebuild choices survive the dialog and server round trip', async ({ page }, testInfo) => {
    test.setTimeout(60_000);
    const channelName = seedPlayout(testInfo);
    await forceEnglish(page);
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error' && /hydration|failed_to_cast_marker_node/i.test(message.text())) {
        errors.push(message.text());
      }
    });
    await page.goto('/playouts');
    await waitForHydration(page);
    const row = page.getByRole('row').filter({ hasText: channelName });
    const open = row.getByRole('button', { name: 'Update Playout', exact: true });
    const dialog = page.getByRole('dialog', { name: 'Update Playout', exact: true });
    let requests = 0;
    page.on('request', request => { if (request.url().includes('/admin-api/rebuild_playout')) requests += 1; });
    await open.click();
    await dialog.getByLabel('Action', { exact: true }).selectOption('reset');
    await dialog.getByRole('checkbox', { name: 'Keep the current programme' }).uncheck();
    await expect(dialog.getByText('The current programme may be interrupted.', { exact: true })).toBeVisible();
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(requests).toBe(0);

    for (const [mode, keep] of [['continue', true], ['refresh', true], ['reset', false]] as const) {
      await expect(open).toBeEnabled();
      await open.click();
      await expect(dialog.getByLabel('Action', { exact: true })).toHaveValue('refresh');
      const preserve = dialog.getByRole('checkbox', { name: 'Keep the current programme' });
      await expect(preserve).toBeChecked();
      await dialog.getByLabel('Action', { exact: true }).selectOption(mode);
      if (mode === 'continue') {
        await expect(preserve).toBeDisabled();
      } else {
        await preserve.setChecked(keep);
      }
      const response = page.waitForResponse(result => result.url().includes('/admin-api/rebuild_playout'));
      await dialog.getByRole('button', { name: 'Queue update' }).click();
      const result = await response;
      expect(result.ok()).toBe(true);
      const body = new URLSearchParams(result.request().postData() ?? '');
      expect(body.get('mode')).toBe(mode);
      expect(body.get('preserve_current')).toBe(String(keep));
      await expect(dialog).toBeHidden();
    }
    await page.setViewportSize({ width: 390, height: 844 });
    const navigation = page.getByRole('button', { name: 'Toggle navigation' });
    // The responsive listener closes the drawer after the viewport changes.
    // Wait for it instead of racing that listener with a toggle click.
    await expect(navigation).toHaveAttribute('aria-expanded', 'false');
    await expect.poll(() => page.locator('[data-nav-drawer]').evaluate(
      element => element.getBoundingClientRect().right,
    )).toBeLessThanOrEqual(0);
    await expect(open).toBeEnabled();
    await open.click();
    await expect(dialog).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath('explicit-rebuild-mobile.png'), fullPage: true });
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.request.post('/admin-api/settings/ui/save', {
      form: { is_dark_mode: 'false', language: 'nl' }, maxRedirects: 0,
    });
    await page.reload();
    await waitForHydration(page);
    await page.setViewportSize({ width: 1440, height: 900 });
    await row.getByRole('button', { name: 'Uitzending bijwerken', exact: true }).click();
    const dutch = page.getByRole('dialog', { name: 'Uitzending bijwerken', exact: true });
    await expect(dutch.getByLabel('Actie', { exact: true })).toHaveValue('refresh');
    await expect(dutch.getByRole('checkbox', { name: 'Lopend programma behouden' })).toBeChecked();
    await expect(dutch).toContainText('Wijzigingen gaan daarna in.');
    await page.screenshot({ path: testInfo.outputPath('explicit-rebuild-dutch-desktop.png'), fullPage: true });
    await dutch.getByRole('button', { name: 'Annuleren', exact: true }).click();
    await page.waitForTimeout(2_000);
    expect(errors).toEqual([]);
    expect(requests).toBe(3);
    await forceEnglish(page);
  });

  test('external JSON adoption, preserved refresh and Health recovery use the real worker', async ({ page, browser }, testInfo) => {
    test.setTimeout(90_000);
    const name = seedPlayout(testInfo);
    const id = Number(sqlite(`SELECT p."Id" FROM "Playout" p JOIN "Channel" c ON c."Id"=p."ChannelId" WHERE c."Name"='${name}';`));
    const folder = mkdtempSync(join(tmpdir(), 'external-json-admin-'));
    const source = join(folder, 'source.json');
    const content = {
      startTime: new Date(Date.now() - 60_000).toISOString(),
      guideMinimumDurationSeconds: 5,
      programs: ['Current', 'Future', 'Later'].map(title => ({ title, duration: 300_000, file: `synthetic/${title}.mp4`, type: 'movie' })),
    };
    const write = () => writeFileSync(source, JSON.stringify(content));
    const revision = () => Number(sqlite(`SELECT COALESCE(MAX("Revision"),0) FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=${id};`));
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error' && /hydration|failed_to_cast_marker_node/i.test(message.text())) errors.push(message.text());
    });
    write();
    sqlite(`UPDATE "Playout" SET "ScheduleKind"=20,"ScheduleFile"='${source.replace(/'/g, "''")}' WHERE "Id"=${id};`);
    try {
      await forceEnglish(page);
      await page.goto('/playouts');
      await waitForHydration(page);
      const row = page.getByRole('row').filter({ hasText: name });
      const open = row.getByRole('button', { name: 'Update Playout', exact: true });
      const dialog = page.getByRole('dialog', { name: 'Update Playout', exact: true });
      await expect(row.locator('[data-external-json-mode]')).toHaveText('Direct file updates');
      await page.setViewportSize({ width: 390, height: 844 });
      await expect(page.getByRole('button', { name: 'Toggle navigation' })).toHaveAttribute('aria-expanded', 'false');
      await open.click();
      await expect(dialog.locator('[data-external-json-help]')).toContainText('turn off programme preservation once');
      await expect(dialog.getByRole('button', { name: 'Queue update' })).toBeDisabled();
      await expect(dialog.locator('option[value="continue"]')).toBeDisabled();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath('external-json-adoption-mobile.png'), fullPage: true });
      await dialog.getByRole('checkbox', { name: 'Keep the current programme' }).uncheck();
      await dialog.getByRole('button', { name: 'Queue update' }).click();
      await expect.poll(revision).toBe(1);
      await page.setViewportSize({ width: 1440, height: 900 });
      await page.reload();
      await waitForHydration(page);
      await expect(row.locator('[data-external-json-mode]')).toHaveText('Controlled updates');
      await expect(row.locator('[data-playout-source-revision]')).toHaveText('1');
      const noScript = await browser.newContext({ javaScriptEnabled: false, baseURL: new URL(page.url()).origin });
      try {
        const readOnly = await noScript.newPage();
        await readOnly.goto('/playouts');
        const retained = readOnly.getByRole('row').filter({ hasText: name });
        await expect(retained.locator('[data-external-json-mode]')).toHaveText('Controlled updates');
        await expect(retained.locator('[data-playout-source-revision]')).toHaveText('1');
      } finally {
        await noScript.close();
      }
      content.programs[0].title = 'Changed current';
      content.programs[1].title = 'New future';
      write();
      await open.click();
      await expect(dialog.getByRole('checkbox', { name: 'Keep the current programme' })).toBeChecked();
      const response = page.waitForResponse(result => result.url().includes('/admin-api/rebuild_playout'));
      await dialog.getByRole('button', { name: 'Queue update' }).click();
      const accepted = await response;
      expect(accepted.ok()).toBe(true);
      expect(new URLSearchParams(accepted.request().postData() ?? '').get('preserve_current')).toBe('true');
      await expect.poll(revision).toBe(2);
      const saved = JSON.parse(sqlite(`SELECT "Document" FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=${id};`));
      expect(saved.programs.map((item: { title: string }) => item.title)).toEqual(['Current', 'New future', 'Later']);
      expect(new Date(saved.startTime).getTime()).toBe(new Date(content.startTime).getTime());

      // An actual corrupt accepted version must be diagnosed and recoverable
      // only by the explicit non-preserving action.
      sqlite(`UPDATE "PlayoutExternalJsonSnapshot" SET "SchemaVersion"=999 WHERE "PlayoutId"=${id};`);
      expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
      await page.goto('/system/health');
      const health = page.getByRole('row').filter({ hasText: 'External JSON Playout State' });
      await expect(health).toContainText('invalid or unsupported');
      await expect(health.getByRole('link')).toHaveAttribute('href', '/playouts');
      await health.getByRole('link').click();
      await waitForHydration(page);
      await open.click();
      await dialog.getByLabel('Action', { exact: true }).selectOption('reset');
      await expect(dialog).toContainText("cannot reset the external author's rotation");
      await dialog.getByRole('checkbox', { name: 'Keep the current programme' }).uncheck();
      await dialog.getByRole('button', { name: 'Queue update' }).click();
      await expect.poll(revision).toBe(3);
      expect(sqlite(`SELECT "SchemaVersion" FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=${id};`)).toBe('1');

      await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'false', language: 'nl' }, maxRedirects: 0 });
      await page.reload();
      await waitForHydration(page);
      await expect(row.locator('[data-external-json-mode]')).toHaveText('Gecontroleerde updates');
      await row.getByRole('button', { name: 'Uitzending bijwerken', exact: true }).click();
      const dutch = page.getByRole('dialog', { name: 'Uitzending bijwerken', exact: true });
      await expect(dutch).toContainText('Uitzending verlengen voegt alleen toe');
      await expect(dutch).toContainText('externe tijden');
      await page.screenshot({ path: testInfo.outputPath('external-json-controlled-dutch.png'), fullPage: true });
      await dutch.getByRole('button', { name: 'Annuleren', exact: true }).click();
      expect(errors).toEqual([]);
    } finally {
      sqlite(`DELETE FROM "Playout" WHERE "Id"=${id};`);
      rmSync(folder, { recursive: true, force: true });
      await forceEnglish(page);
    }
  });

  test('covers list, detail, preference, and dialog workflows', async ({ page }, testInfo) => {
    test.setTimeout(60_000);
    const seed = seedDetailedPlayoutWorkflow(testInfo);
    await forceEnglish(page);
    await page.goto('/playouts');
    await waitForHydration(page);

    await page.getByRole('button', { name: 'Add Playout' }).click();
    await expect(page.getByRole('link', { name: 'Classic Schedule' })).toHaveAttribute(
      'href',
      '/playouts/add',
    );
    await expect(page.getByRole('link', { name: 'Block Schedule' })).toHaveAttribute(
      'href',
      '/playouts/add/Block',
    );
    await expect(page.getByRole('link', { name: 'Sequential Schedule' })).toHaveAttribute(
      'href',
      '/playouts/add/Sequential',
    );
    await expect(page.getByRole('link', { name: 'Scripted Schedule' })).toHaveAttribute(
      'href',
      '/playouts/add/Scripted',
    );
    await expect(page.getByRole('link', { name: 'JSON (dizqueTV) Schedule' })).toHaveAttribute(
      'href',
      '/playouts/add/ExternalJson',
    );

    const search = page.getByPlaceholder('Search for playouts');
    await search.fill('Detailed JSON');
    await expect(page.getByRole('row').filter({ hasText: seed.externalChannelName })).toBeVisible();
    await expect(page.getByRole('row').filter({ hasText: seed.classicChannelName })).toHaveCount(0);
    await search.fill('');

    const classicRow = page.getByRole('row').filter({ hasText: seed.classicChannelName });
    await expect(classicRow).toBeVisible();
    await expect(classicRow.getByRole('link', { name: 'Edit Playout' })).toHaveAttribute(
      'href',
      `/playouts/classic/${seed.classicPlayoutId}`,
    );
    await classicRow.locator('td').first().click();
    await expect(page.getByRole('heading', { name: 'Playout Detail' })).toBeVisible();
    const primaryRow = page.getByRole('row').filter({ hasText: seed.primaryTitle });
    await expect(primaryRow).toBeVisible();
    await expect(page.getByText('UNSCHEDULED', { exact: true })).toBeVisible();
    await expect(page.getByText(seed.fillerTitle, { exact: true })).toHaveCount(0);

    const showFiller = page.getByRole('checkbox', { name: 'Show Filler' });
    await page.getByText('Show Filler', { exact: true }).click();
    await expect(showFiller).toBeChecked();
    await expect(page.getByText(seed.fillerTitle, { exact: true })).toBeVisible();
    await expect.poll(() => sqlite(`
SELECT "Value" FROM "ConfigElement" WHERE "Key" = 'pages.playouts.detail_show_filler';
`)).toBe('true');

    await primaryRow.getByRole('button', { name: 'Troubleshoot' }).click();
    const contextBody = page.locator('pre').filter({ hasText: 'primary context' });
    await expect(contextBody).toBeVisible();
    // Scope the close to the scheduling context panel. Toasts also expose a
    // "Close" control, so an unscoped role lookup is ambiguous whenever one is
    // on screen - the startup "FFmpeg runtime downloaded" notification fires
    // once per browser session when the host acquired a managed runtime.
    await contextBody.locator('..').getByRole('button', { name: 'Close' }).click();
    await expect(contextBody).toBeHidden();

    await classicRow.getByRole('button', { name: 'Schedule Reset' }).click();
    const resetDialog = page.getByRole('heading', { name: 'Schedule Playout Reset' }).locator('..');
    await resetDialog.locator('select').selectOption('60');
    await resetDialog.getByRole('button', { name: 'Save Changes' }).click();
    await expect.poll(() => sqlite(`
SELECT "DailyRebuildTime" FROM "Playout" WHERE "Id" = ${seed.classicPlayoutId};
`)).toBe('01:00:00');

    const externalRow = page.getByRole('row').filter({ hasText: seed.externalChannelName });
    await externalRow.getByRole('button', { name: 'Edit External Json File' }).click();
    const jsonDialog = page.getByRole('heading', { name: 'Edit External Json File' }).locator('..');
    await jsonDialog.getByRole('textbox').fill('/e2e/updated.json');
    await jsonDialog.getByRole('button', { name: 'Save Changes' }).click();
    await expect.poll(() => sqlite(`
SELECT "ScheduleFile" FROM "Playout" WHERE "Id" = ${seed.externalPlayoutId};
`)).toBe('/e2e/updated.json');

    const deleteRow = page.getByRole('row').filter({ hasText: seed.deleteChannelName });
    await deleteRow.getByRole('button', { name: 'Delete Playout' }).click();
    const deleteDialog = page.getByRole('heading', { name: 'Confirm Delete' }).locator('..');
    await expect(deleteDialog).toContainText(seed.deleteChannelName);
    await deleteDialog.getByRole('button', { name: 'Delete' }).click();
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "Playout" WHERE "Id" = ${seed.deletePlayoutId};
`)).toBe('0');

    await page.getByRole('button', { name: 'Reset All Playouts' }).click();
    await expect(page.getByText(/\d+ playout rebuilds queued/, { exact: true })).toBeVisible();
  });

  test('localizes rejected Classic overlaps and preserves the published preview', async ({ page, browser }, testInfo) => {
    test.setTimeout(90_000);
    const seed = seedDetailedPlayoutWorkflow(testInfo);
    sqlite(`INSERT OR REPLACE INTO "PlayoutBuildStatus" ("PlayoutId", "LastBuild", "Success", "Message")
      VALUES (${seed.classicPlayoutId}, datetime('now', '+1 hour'), 0,
      'invalid playout rebuild: Classic timeline contains overlapping, backward or non-positive intervals');`);
    const snapshot = () => sqlite(`SELECT group_concat("Id" || ':' || "Start" || ':' || "Finish") FROM "PlayoutItem" WHERE "PlayoutId" = ${seed.classicPlayoutId};`);
    const before = snapshot();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (/failed_to_cast_marker_node|hydration.*mismatch/i.test(message.text())) errors.push(message.text());
    });
    for (const [language, message] of [
      ['en-us', 'The schedule contains overlapping or invalid programme times. Published programmes remain unchanged. Review the schedule and rebuild from a verified programme boundary.'],
      ['nl', 'De planning bevat overlappende of ongeldige programmatijden. Gepubliceerde programma’s blijven ongewijzigd. Controleer de planning en bouw opnieuw op vanaf een gecontroleerde programmagrens.'],
    ]) {
      await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'false', language }, maxRedirects: 0 });
      const noJs = await browser.newContext({ javaScriptEnabled: false, baseURL: process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409' });
      try {
        const fallback = await noJs.newPage();
        await fallback.goto('/playouts');
        await expect(fallback.locator('[data-playout-failures]')).toContainText(message);
      } finally { await noJs.close(); }
      await page.goto('/playouts');
      await waitForHydration(page);
      const activity = page.locator('[data-playout-failures]');
      await expect(activity).toContainText(message);
      await activity.getByRole('button', { name: new RegExp(seed.classicChannelName) }).click();
      const detail = page.getByRole('heading', { name: language === 'nl' ? 'Uitzenddetails' : 'Playout Detail', exact: true }).locator('..');
      await expect(detail).toContainText(message);
      await expect(page.getByRole('row').filter({ hasText: seed.primaryTitle })).toBeVisible();
      await page.setViewportSize({ width: 390, height: 844 });
      const navigation = page.getByRole('button', { name: language === 'nl' ? 'Navigatie omschakelen' : 'Toggle navigation' });
      if (await navigation.getAttribute('aria-expanded') === 'true') await navigation.click();
      await expect(navigation).toHaveAttribute('aria-expanded', 'false');
      await expect.poll(() => page.locator('[data-nav-drawer]').evaluate(element => element.getBoundingClientRect().right)).toBeLessThanOrEqual(0);
      await detail.scrollIntoViewIfNeeded();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath(`classic-overlap-${language}.png`), fullPage: true, animations: 'disabled' });
      expect(snapshot()).toBe(before);
    }
    expect(errors).toEqual([]);
    await forceEnglish(page);
  });

  test('shows rejected scripted builds in Activity and preserves the published preview', async ({ page, browser }, testInfo) => {
    test.setTimeout(90_000);
    const seed = seedDetailedPlayoutWorkflow(testInfo);
    const diagnostic = 'Scripted timeline rejected: item 2 starts before the preceding finish/cursor. The published timeline was not changed';
    sqlite(`UPDATE "Playout" SET "ScheduleKind" = 4 WHERE "Id" = ${seed.classicPlayoutId};
      INSERT OR REPLACE INTO "PlayoutBuildStatus" ("PlayoutId", "LastBuild", "Success", "Message")
      VALUES (${seed.classicPlayoutId}, datetime('now', '+1 hour'), 0, '${diagnostic}');`);
    const before = sqlite(`SELECT group_concat("Id" || ':' || "Start" || ':' || "Finish") FROM "PlayoutItem" WHERE "PlayoutId" = ${seed.classicPlayoutId};`);
    await forceEnglish(page);
    const noJs = await browser.newContext({ javaScriptEnabled: false, baseURL: process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409' });
    try {
      const fallback = await noJs.newPage();
      await fallback.goto('/playouts');
      await expect(fallback.locator('[data-playout-failures]')).toContainText(diagnostic);
    } finally { await noJs.close(); }
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto('/playouts');
    await waitForHydration(page);
    await page.getByPlaceholder('Search for playouts').fill(seed.classicChannelName);
    const activity = page.locator('[data-playout-failures]');
    await expect(activity).toContainText(diagnostic);
    await activity.getByRole('button', { name: new RegExp(seed.classicChannelName) }).click();
    await expect(page.getByText('The timeline was rejected. Published programmes remain unchanged. Correct the script and try again.', { exact: true })).toBeVisible();
    await expect(page.getByRole('row').filter({ hasText: seed.primaryTitle })).toBeVisible();
    await page.setViewportSize({ width: 390, height: 844 });
    const navigation = page.getByRole('button', { name: 'Toggle navigation' });
    if (await navigation.getAttribute('aria-expanded') === 'true') await navigation.click();
    await page.getByRole('heading', { name: 'Playout Detail', exact: true }).locator('..').scrollIntoViewIfNeeded();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath('rejected-timeline-mobile.png'), fullPage: true });
    await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'false', language: 'nl' }, maxRedirects: 0 });
    await page.reload();
    await waitForHydration(page);
    await activity.getByRole('button', { name: new RegExp(seed.classicChannelName) }).click();
    await expect(page.getByText('De tijdlijn is afgewezen. Gepubliceerde programma’s blijven ongewijzigd. Corrigeer het script en probeer opnieuw.', { exact: true })).toBeVisible();
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.getByRole('heading', { name: 'Uitzenddetails', exact: true }).locator('..').scrollIntoViewIfNeeded();
    await page.screenshot({ path: testInfo.outputPath('rejected-timeline-dutch-desktop.png'), fullPage: true });
    sqlite(`UPDATE "PlayoutBuildStatus" SET "Success" = 1, "Message" = NULL WHERE "PlayoutId" = ${seed.classicPlayoutId};`);
    await expect(page.getByText('De tijdlijn is afgewezen. Gepubliceerde programma’s blijven ongewijzigd. Corrigeer het script en probeer opnieuw.', { exact: true })).toBeHidden({ timeout: 40_000 });
    await expect(page.getByRole('row').filter({ hasText: seed.primaryTitle })).toBeVisible();
    expect(sqlite(`SELECT group_concat("Id" || ':' || "Start" || ':' || "Finish") FROM "PlayoutItem" WHERE "PlayoutId" = ${seed.classicPlayoutId};`)).toBe(before);
    expect(errors).toEqual([]);
    await forceEnglish(page);
  });

  test('shows durable adaptation status without JavaScript and updates it while open', async ({ page, browser }, testInfo) => {
    test.setTimeout(90_000);
    const seed = seedDetailedPlayoutWorkflow(testInfo);
    const id = seed.classicPlayoutId;
    const library = id + 101;
    sqlite(`
      UPDATE "Playout" SET "ScheduleKind" = 4, "ScriptedSourceRevision" = 3 WHERE "Id" = ${id};
      INSERT OR REPLACE INTO "PlayoutBuildStatus" ("PlayoutId", "LastBuild", "Success", "Message", "LastSuccessfulBuild")
      VALUES (${id}, '2099-01-01 00:00:00', 1, NULL, '2026-10-02 12:34:56');
      INSERT INTO "ScriptedSourceRefreshWork" ("LibraryId", "PlayoutId", "Revision", "NextAttempt", "Diagnostic")
      VALUES (${library}, ${id}, 7, '2099-01-01 00:00:00', 'scripted-source-comparison-failed');
    `);
    await forceEnglish(page);
    const noJs = await browser.newContext({ javaScriptEnabled: false, baseURL: process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409' });
    try {
      const fallback = await noJs.newPage();
      await fallback.goto('/playouts');
      const status = fallback.locator(`[data-playout-adaptation="${id}"]`);
      await expect(status.locator('[data-playout-last-success]')).toHaveText('2026-10-02 12:34:56 UTC');
      await expect(status.locator('[data-playout-source-revision]')).toHaveText('3');
      await expect(fallback.locator('[data-playout-failures]')).toContainText('Source comparison failed; a retry is pending.');
    } finally { await noJs.close(); }
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (/failed_to_cast_marker_node|hydration.*mismatch/i.test(message.text())) errors.push(message.text());
    });
    await page.goto('/playouts');
    await waitForHydration(page);
    await page.getByPlaceholder('Search for playouts').fill(seed.classicChannelName);
    const status = page.locator(`[data-playout-adaptation="${id}"]`);
    await expect(status.locator('[data-playout-source-revision]')).toHaveText('3');
    await page.locator('[data-playout-failures]').getByRole('button', { name: new RegExp(seed.classicChannelName) }).click();
    await expect(page.getByRole('row').filter({ hasText: seed.primaryTitle })).toBeVisible();
    // A successful comparison advances the durable revision and leaves Refresh
    // queued. Keep this fixture ineligible so the real worker cannot race the UI proof.
    sqlite(`
      DELETE FROM "ScriptedSourceRefreshWork" WHERE "PlayoutId" = ${id};
      UPDATE "Playout" SET "ScriptedSourceRevision" = 4 WHERE "Id" = ${id};
      INSERT INTO "ScanPlayoutRefreshIntent" ("PlayoutId", "Generation", "State", "Reason", "CreatedAt", "UpdatedAt", "NextAttempt")
      VALUES (${id}, 1, 0, 'scripted-source-changed', '2026-10-02T12:35:00Z', '2026-10-02T12:35:00Z', '2099-01-01 00:00:00');
    `);
    await expect(status.locator('[data-playout-source-revision]')).toHaveText('4', { timeout: 40_000 });
    await expect(status.locator('[data-playout-refresh-pending]')).toBeVisible();
    await expect(page.locator('[data-playout-failures]')).toBeHidden();
    await expect(status.locator('[data-playout-last-success]')).toHaveText('2026-10-02 12:34:56 UTC');
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 900 });
      const navigation = page.getByRole('button', { name: 'Toggle navigation' });
      if (width === 390) {
        await expect(navigation).toHaveAttribute('aria-expanded', 'false');
        await expect.poll(() => page.locator('[data-nav-drawer]').evaluate(
          element => element.getBoundingClientRect().right,
        )).toBeLessThanOrEqual(0);
      }
      await status.scrollIntoViewIfNeeded();
      const bounds = await status.boundingBox();
      expect(bounds).not.toBeNull();
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath(`adaptation-status-${width}.png`), fullPage: true });
    }
    await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'false', language: 'nl' }, maxRedirects: 0 });
    await page.reload();
    await waitForHydration(page);
    await expect(status).toContainText('Laatste geslaagde opbouw');
    await expect(status).toContainText('Bronrevisie');
    await expect(status).toContainText('Vernieuwing staat klaar');
    expect(errors).toEqual([]);
    await forceEnglish(page);
  });
});
