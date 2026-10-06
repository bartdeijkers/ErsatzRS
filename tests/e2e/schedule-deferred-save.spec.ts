import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';

function sqlite(sql: string): string {
  const path = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
  if (!path) throw new Error('An isolated E2E database is required');
  return execFileSync('sqlite3', [path], { input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${sql}`, encoding: 'utf8' }).trim();
}

test.afterEach(async ({ request }) => {
  await request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0 });
});

test('deferred Classic save preserves the horizon through hydrated and no-JS forms', async ({ page, browser, baseURL }, info) => {
  test.setTimeout(90_000);
  const id = 730_000 + [...info.project.name].reduce((sum, c) => sum + c.charCodeAt(0), 0);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (/hydration|failed_to_cast_marker_node/i.test(message.text()) && /error|mismatch|failed/i.test(message.text())) errors.push(message.text()); });
  const profile = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  sqlite(`
    INSERT INTO "ProgramSchedule" ("Id", "Name", "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether",
      "RandomStartPoint", "ShuffleScheduleItems", "TreatCollectionsAsShows") VALUES (${id}, 'Deferred ${info.project.name}', 0, 0, 0, 0, 0);
    INSERT INTO "ProgramScheduleItem" ("Id", "ProgramScheduleId", "Index", "CollectionType", "SearchQuery", "SearchTitle",
      "FillWithGroupMode", "GuideMode", "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems", "PlaybackOrder")
      VALUES (${id}, ${id}, 0, 9, 'type:movie', 'Movies', 0, 0, 0, 0, 0, 1);
    INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (${id});
    INSERT INTO "Channel" ("Id", "UniqueId", "Number", "SortNumber", "Name", "Group", "FFmpegProfileId",
      "StreamingMode", "StreamSelectorMode", "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
      "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg")
      VALUES (${id}, '00000000-0000-0000-0000-${id.toString().padStart(12, '0')}', '${id}', ${id},
      'Deferred ${info.project.name}', 'E2E', ${profile}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1);
    INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "ProgramScheduleId", "Seed") VALUES (${id}, ${id}, 1, ${id}, 1);
    INSERT INTO "PlayoutAnchor" ("PlayoutId", "InDurationFiller", "InFlood", "NextGuideGroup", "NextStart")
      VALUES (${id}, 0, 0, 8, '2099-01-01 03:00:00');
    INSERT INTO "PlayoutBuildStatus" ("PlayoutId", "LastBuild", "Success", "Message")
      VALUES (${id}, strftime('%Y-%m-%d %H:%M:%f', 'now'), 1, NULL);
  `);
  const stateSql = `SELECT * FROM "PlayoutAnchor" WHERE "PlayoutId"=${id}; SELECT * FROM "PlayoutBuildStatus" WHERE "PlayoutId"=${id};`;
  const before = sqlite(stateSql);
  await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0 });
  await page.goto(`/schedules/${id}/items`);
  await expect.poll(() => page.evaluate(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated)).toBe(true);
  await page.locator('tbody tr').first().click();
  const title = page.locator('form[class^="item-editor"] label').filter({ hasText: 'Custom Title' }).locator('input');
  await title.fill('Deferred title');
  await page.getByRole('button', { name: 'Save for next extension', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Saved; published programmes remain unchanged.' })).toBeVisible();
  const status = page.getByTestId('schedule-deferred-status');
  await expect(status).toContainText('Waiting for the next extension');
  await expect(status).toContainText('2099-01-01 03:00:00 UTC');
  await expect(status).toContainText('Succeeded');
  expect(sqlite(stateSql)).toBe(before);
  expect(sqlite(`SELECT "Id", "CustomTitle" FROM "ProgramScheduleItem" WHERE "Id"=${id};`)).toBe(`${id}|Deferred title`);
  await page.locator('tbody tr').first().click();
  await title.fill('Still editing');
  // A status poll must not replace an unsaved editor value.
  await page.waitForTimeout(5_500);
  await expect(title).toHaveValue('Still editing');
  await page.locator('form[class^="item-editor"] label').filter({ hasText: 'Playback Order' }).locator('select').selectOption('2');
  await page.getByRole('button', { name: 'Save for next extension', exact: true }).click();
  await expect(page.getByText('Deferred save cannot change sources, playback order or item structure. Use Save and refresh for those changes.', { exact: true })).toBeVisible();
  expect(sqlite(stateSql)).toBe(before);
  await page.reload();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole('button', { name: 'Save for next extension', exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath('deferred-save-mobile.png'), fullPage: true, animations: 'disabled' });
  const nojs = await browser.newContext({ baseURL, javaScriptEnabled: false });
  const fallback = await nojs.newPage();
  await fallback.goto(`/schedules/${id}/items`);
  await expect(fallback.locator('.schedule-items-noscript').getByTestId('schedule-deferred-status')).toContainText('2099-01-01 03:00:00 UTC');
  await fallback.locator('input[name="item_custom_title"]').fill('Deferred without JavaScript');
  await fallback.getByRole('button', { name: 'Save for next extension', exact: true }).click();
  await expect(fallback).toHaveURL(new RegExp(`/schedules/${id}/items$`));
  expect(sqlite(`SELECT "Id", "CustomTitle" FROM "ProgramScheduleItem" WHERE "Id"=${id};`)).toBe(`${id}|Deferred without JavaScript`);
  expect(sqlite(stateSql)).toBe(before);
  await nojs.close();
  await page.request.post('/admin-api/settings/ui/save', { form: { language: 'nl', is_dark_mode: 'true' }, maxRedirects: 0 });
  await page.reload();
  await expect(page.getByRole('button', { name: 'Opslaan voor volgende uitbreiding', exact: true })).toBeVisible();
  await expect(page.getByTestId('schedule-deferred-status')).toContainText('Wacht op de volgende uitbreiding');
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: info.outputPath('deferred-save-mobile-nl.png'), fullPage: true, animations: 'disabled' });
  expect(errors).toEqual([]);
});
