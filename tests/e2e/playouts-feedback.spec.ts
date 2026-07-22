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

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
}

test.describe('playout rebuild feedback', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed a playout');

  test('shows queued feedback in the shared toast surface', async ({ page }, testInfo) => {
    const channelName = seedPlayout(testInfo);
    await forceEnglish(page);

    await page.goto('/playouts');
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
});
