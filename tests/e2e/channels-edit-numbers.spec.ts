import { Browser, expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type ChannelSeed = {
  firstId: number;
  secondId: number;
  thirdId: number;
  firstNumber: string;
  secondNumber: string;
  firstName: string;
  secondName: string;
};

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for channel-number seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function seedForProject(testInfo: TestInfo): ChannelSeed {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const base = 120_000 + projectOffset;
  return {
    firstId: base + 1,
    secondId: base + 2,
    thirdId: base + 3,
    firstNumber: `${base + 1}`,
    secondNumber: `${base + 2}`,
    firstName: `E2E Numbers First ${testInfo.project.name}`,
    secondName: `E2E Numbers Second ${testInfo.project.name}`,
  };
}

function uuidFor(id: number): string {
  return `00000000-0000-0000-0000-${id.toString().padStart(12, '0')}`;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function seedChannels(testInfo: TestInfo): ChannelSeed {
  const seed = seedForProject(testInfo);
  const profileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  if (!profileId) {
    throw new Error('E2E database has no FFmpegProfile row for channel seeding');
  }

  sqlite(`
DELETE FROM "Playout" WHERE "ChannelId" IN (${seed.firstId}, ${seed.secondId}, ${seed.thirdId});
DELETE FROM "Artwork" WHERE "ChannelId" IN (${seed.firstId}, ${seed.secondId}, ${seed.thirdId});
DELETE FROM "Channel" WHERE "Id" IN (${seed.firstId}, ${seed.secondId}, ${seed.thirdId});

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES
  (${seed.firstId}, '${uuidFor(seed.firstId)}', '${seed.firstNumber}', ${seed.firstNumber}, '${sqlEscape(seed.firstName)}', 'ErsatzTV',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1),
  (${seed.secondId}, '${uuidFor(seed.secondId)}', '${seed.secondNumber}', ${seed.secondNumber}, '${sqlEscape(seed.secondName)}', 'ErsatzTV',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1),
  (${seed.thirdId}, '${uuidFor(seed.thirdId)}', '${seed.thirdId}', ${seed.thirdId}, 'E2E Numbers Third ${testInfo.project.name}', 'ErsatzTV',
   ${profileId}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1);
`);
  return seed;
}

function channelNumber(id: number): string {
  return sqlite(`SELECT "Number" FROM "Channel" WHERE "Id" = ${id};`);
}

function channelSortNumber(id: number): string {
  return sqlite(`SELECT printf('%.1f', "SortNumber") FROM "Channel" WHERE "Id" = ${id};`);
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
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

async function gotoChannelNumbersFromList(page: Page): Promise<void> {
  await page.goto('/channels');
  await expect(page.getByRole('heading', { name: 'Channels' })).toBeVisible();
  await page.getByRole('link', { name: 'Edit Numbers' }).click();
  await expect(page).toHaveURL(/\/channels\/numbers$/);
  await expect(page.getByRole('heading', { name: 'Edit Channel Numbers' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Add Channel' })).toHaveCount(0);
}

async function swapNumbers(page: Page, seed: ChannelSeed): Promise<void> {
  await page.getByLabel(`Channel Number ${seed.firstName}`).fill(seed.secondNumber);
  await page.getByLabel(`Channel Number ${seed.secondName}`).fill(seed.firstNumber);
  await page.getByRole('button', { name: 'Save Channels' }).first().click();
  await expect(page).toHaveURL(/\/channels$/);
}

async function swapNumbersNoJs(browser: Browser, baseURL: string | undefined, seed: ChannelSeed): Promise<void> {
  const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
  const noJsPage = await context.newPage();
  await noJsPage.goto('/channels/numbers');
  await expect(noJsPage.getByRole('heading', { name: 'Edit Channel Numbers' })).toBeVisible();
  await noJsPage.getByLabel(`Channel Number ${seed.firstName}`).fill(seed.firstNumber);
  await noJsPage.getByLabel(`Channel Number ${seed.secondName}`).fill(seed.secondNumber);
  await noJsPage.getByRole('button', { name: 'Save Channels' }).first().click();
  await expect(noJsPage).toHaveURL(/\/channels$/);
  await context.close();
}

test.describe('channels edit numbers parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed channels');

  test('Edit Numbers opens the channel-number editor, rejects invalid values, and saves with no-JS fallback', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'Channel-number editing mutates shared channel rows; Chromium proof avoids project races.',
    );

    const seed = seedChannels(testInfo);
    await forceEnglish(page);

    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoChannelNumbersFromList(page);
    await expect(page.getByText(seed.firstName)).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'channels-edit-numbers-desktop'),
      fullPage: true,
    });

    await page.getByLabel(`Channel Number ${seed.firstName}`).fill('abc');
    await page.getByRole('button', { name: 'Save Channels' }).first().click();
    await expect(page).toHaveURL(/\/channels\/numbers\?error=/);
    await expect(page.getByRole('alert')).toContainText('Invalid channel number');
    expect(channelNumber(seed.firstId)).toBe(seed.firstNumber);
    expect(channelNumber(seed.secondId)).toBe(seed.secondNumber);

    await swapNumbers(page, seed);
    expect(channelNumber(seed.firstId)).toBe(seed.secondNumber);
    expect(channelNumber(seed.secondId)).toBe(seed.firstNumber);
    expect(channelSortNumber(seed.firstId)).toBe(`${seed.secondNumber}.0`);
    expect(channelSortNumber(seed.secondId)).toBe(`${seed.firstNumber}.0`);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/channels/numbers');
    await expect(page.getByRole('heading', { name: 'Edit Channel Numbers' })).toBeVisible();
    await expect(page.locator('[data-channel-number-mobile-actions="true"]')).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'channels-edit-numbers-mobile'),
      fullPage: true,
    });

    await swapNumbersNoJs(browser, baseURL, seed);
    expect(channelNumber(seed.firstId)).toBe(seed.firstNumber);
    expect(channelNumber(seed.secondId)).toBe(seed.secondNumber);
  });
});
