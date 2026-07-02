import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for channel seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function uuidFor(id: number): string {
  return `00000000-0000-0000-0000-${id.toString().padStart(12, '0')}`;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
    form: {
      is_dark_mode: 'true',
      language: 'en-us',
    },
    maxRedirects: 0,
  });
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

test.describe('channel add default group', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to verify persistence');

  test('Add Channel pre-fills Group with ErsatzRS and persists it on save', async ({
    page,
  }, testInfo) => {
    await forceEnglish(page);

    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/channels/add');
    await expect(page.getByRole('heading', { name: 'Add Channel' })).toBeVisible();
    await expect(page.locator('input[name="group"]')).toHaveValue('ErsatzRS');
    await page.screenshot({
      path: screenshotPath(testInfo, 'channels-add-default-group-desktop'),
      fullPage: true,
    });

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/channels/add');
    await expect(page.locator('input[name="group"]')).toHaveValue('ErsatzRS');
    await page.screenshot({
      path: screenshotPath(testInfo, 'channels-add-default-group-mobile'),
      fullPage: true,
    });

    test.skip(
      testInfo.project.name !== 'chromium',
      'Saving creates shared channel rows; Chromium proof avoids project races.',
    );

    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/channels/add');
    const channelName = `E2E Default Group ${Date.now()}`;
    await page.locator('input[name="name"]').fill(channelName);
    await page.getByRole('button', { name: 'Add Channel' }).click();
    await expect(page).toHaveURL(/\/channels$/);

    const savedGroup = sqlite(
      `SELECT "Group" FROM "Channel" WHERE "Name" = '${channelName}';`,
    );
    expect(savedGroup).toBe('ErsatzRS');
  });

  test('editing an existing channel keeps its saved group', async ({ page }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'Editing mutates shared channel rows; Chromium proof avoids project races.',
    );

    const channelId = 130_001;
    const profileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
    if (!profileId) {
      throw new Error('E2E database has no FFmpegProfile row for channel seeding');
    }
    sqlite(`
DELETE FROM "Playout" WHERE "ChannelId" = ${channelId};
DELETE FROM "Artwork" WHERE "ChannelId" = ${channelId};
DELETE FROM "Channel" WHERE "Id" = ${channelId};

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES
  (${channelId}, '${uuidFor(channelId)}', '${channelId}', ${channelId}, 'E2E Existing Group', 'Legacy Group',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1);
`);

    await forceEnglish(page);
    await page.goto(`/channels/${channelId}`);
    await expect(page.getByRole('heading', { name: 'Edit Channel' })).toBeVisible();
    await expect(page.locator('input[name="group"]')).toHaveValue('Legacy Group');

    await page.getByRole('button', { name: 'Save Channel' }).click();
    await expect(page).toHaveURL(/\/channels$/);

    const savedGroup = sqlite(`SELECT "Group" FROM "Channel" WHERE "Id" = ${channelId};`);
    expect(savedGroup).toBe('Legacy Group');
  });
});
