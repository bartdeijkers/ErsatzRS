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
  await page.request.post('/admin-api/settings/ui/save', {
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

  for (const javaScriptEnabled of [true, false]) {
    test(`fresh first channel selects Next and retains explicit Native (JavaScript ${javaScriptEnabled})`, async ({ browser, baseURL }) => {
      const initial = JSON.parse(sqlite(`SELECT json_object('id',"Id",'engine',"StreamingEngine",'mode',"StreamingMode",'profile',"FFmpegProfileId") FROM "Channel" ORDER BY "Id" LIMIT 1;`));
      expect(initial.engine).toBe(1);
      const profile = sqlite(`SELECT * FROM "FFmpegProfile" WHERE "Id"=${initial.profile};`);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      try {
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        await forceEnglish(page);
        await page.goto(`/channels/${initial.id}`);
        if (javaScriptEnabled) {
          await page.waitForFunction(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
        }
        const engine = page.locator('select[name="streaming_engine"]');
        await expect(engine).toHaveValue('1');
        await engine.selectOption('0');
        await page.getByRole('button', { name: 'Save Channel', exact: true }).click();
        await expect(page).toHaveURL(/\/channels$/);
        if (javaScriptEnabled) {
          await page.waitForFunction(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
        }
        expect(sqlite(`SELECT "StreamingEngine" FROM "Channel" WHERE "Id"=${initial.id};`)).toBe('0');
        expect(sqlite(`SELECT "StreamingMode" FROM "Channel" WHERE "Id"=${initial.id};`)).toBe(String(initial.mode));
        expect(sqlite(`SELECT * FROM "FFmpegProfile" WHERE "Id"=${initial.profile};`)).toBe(profile);
        await page.goto(`/channels/${initial.id}`);
        await expect(page.locator('select[name="streaming_engine"]')).toHaveValue('0');
        if (javaScriptEnabled) {
          await page.waitForFunction(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
        }
        expect(errors).toEqual([]);
      } finally {
        sqlite(`UPDATE "Channel" SET "StreamingEngine"=${initial.engine} WHERE "Id"=${initial.id};`);
        await context.close();
      }
    });
  }

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
