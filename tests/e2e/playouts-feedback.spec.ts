import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

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
  '${channelId}', ${channelId}, '${channelName}', 'ErsatzTV',
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
  await page.request.post('/api/settings/ui/save', {
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

  test('shows queued feedback in the shared toast surface', async ({ page }, testInfo) => {
    const channelName = seedPlayout(testInfo);
    await forceEnglish(page);

    await page.goto('/playouts');
    await waitForHydration(page);
    const row = page.getByRole('row').filter({ hasText: channelName });
    await expect(row).toBeVisible();
    await row.getByRole('button', { name: 'Refresh Playout' }).click();

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
    await expect(page.locator('pre').filter({ hasText: 'primary context' })).toBeVisible();
    await page.getByRole('button', { name: 'Close' }).click();

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
});
