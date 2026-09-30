import { expect, Page, test } from '@playwright/test';
import { execFileSync } from 'child_process';

function sqlite(sql: string): string {
  const path = process.env.E2E_DB_PATH;
  if (!path) throw new Error('The isolated E2E database is required');
  return execFileSync('sqlite3', [path], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${sql}`, encoding: 'utf8',
  }).trim();
}

async function settled(page: Page): Promise<void> {
  await expect.poll(() => page.evaluate(() =>
    (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
  ), { timeout: 15_000 }).toBe(true);
}

function watchErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
  return errors;
}

test.beforeEach(async ({ page }) => {
  await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
  });
});

test('Deco group change clears the saved playlist choice before replacement', async ({ page }, testInfo) => {
  const id = testInfo.project.name === 'firefox' ? 264100 : 164100;
  const errors = watchErrors(page);
  sqlite(`
    INSERT INTO PlaylistGroup (Id, Name) VALUES (${id}, 'UCU164 group A ${id}'), (${id + 1}, 'UCU164 group B ${id}');
    INSERT INTO Playlist (Id, Name, PlaylistGroupId, IsSystem)
      VALUES (${id}, 'UCU164 playlist A', ${id}, 0), (${id + 1}, 'UCU164 playlist B', ${id + 1}, 0);
    INSERT INTO DecoGroup (Id, Name) VALUES (${id}, 'UCU164 deco group ${id}');
  `);
  const response = await page.request.post('/admin-api/decos/create', {
    form: { group_id: String(id), name: `UCU164 deco ${id}` }, maxRedirects: 0,
  });
  expect(response.status()).toBe(303);
  const location = response.headers().location;
  expect(location).toMatch(/^\/decos\/\d+$/);
  const deco = Number(location.split('/').pop());
  sqlite(`
    UPDATE Deco SET BreakContentMode=2 WHERE Id=${deco};
    INSERT INTO DecoBreakContent (DecoId, CollectionType, PlaylistId, Placement)
      VALUES (${deco}, 6, ${id}, 0);
  `);
  await page.goto(location);
  await settled(page);
  await page.getByRole('row').filter({ hasText: 'UCU164 playlist A' }).click();
  const group = page.locator('#deco-editor-form label').filter({ hasText: /^Playlist Group$/ })
    .locator('xpath=following-sibling::select[1]');
  const playlist = page.locator('#deco-editor-form label').filter({ hasText: /^Playlist$/ })
    .locator('xpath=following-sibling::select[1]');
  await expect(playlist).toHaveValue(String(id));
  await group.selectOption(String(id + 1));
  await expect(playlist).toHaveValue('0');
  await expect(page.locator('input[name="break_content_playlist_id"]')).toHaveValue('');
  await playlist.selectOption(String(id + 1));
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page).toHaveURL(/\/decos$/);
  await settled(page);
  await expect.poll(() => sqlite(`SELECT PlaylistId FROM DecoBreakContent WHERE DecoId=${deco};`))
    .toBe(String(id + 1));
  await page.goto(location);
  await settled(page);
  await expect(page.getByRole('row').filter({ hasText: 'UCU164 playlist B' })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('deco-replacement.png'), fullPage: true });
  expect(errors).toEqual([]);
});

test('Playout group changes clear child IDs and preserve date filters through save', async ({ page }, testInfo) => {
  const id = testInfo.project.name === 'firefox' ? 264200 : 164200;
  const errors = watchErrors(page);
  const profile = sqlite('SELECT Id FROM FFmpegProfile ORDER BY Id LIMIT 1;');
  expect(profile).not.toBe('');
  sqlite(`
    INSERT INTO TemplateGroup (Id, Name) VALUES (${id}, 'UCU164 template group A ${id}'), (${id + 1}, 'UCU164 template group B ${id}');
    INSERT INTO Template (Id, TemplateGroupId, Name, DateUpdated)
      VALUES (${id}, ${id}, 'UCU164 template A', '0'), (${id + 1}, ${id + 1}, 'UCU164 template B', '0');
    INSERT INTO DecoTemplateGroup (Id, Name) VALUES (${id}, 'UCU164 deco template group A ${id}'), (${id + 1}, 'UCU164 deco template group B ${id}');
    INSERT INTO DecoTemplate (Id, DecoTemplateGroupId, Name, DateUpdated)
      VALUES (${id}, ${id}, 'UCU164 deco template A', '0'), (${id + 1}, ${id + 1}, 'UCU164 deco template B', '0');
    INSERT INTO Channel (Id, UniqueId, Number, SortNumber, Name, "Group", FFmpegProfileId,
      StreamingMode, StreamSelectorMode, PlayoutSource, PlayoutMode, TranscodeMode, IdleBehavior,
      MusicVideoCreditsMode, SongVideoMode, SubtitleMode, IsEnabled, ShowInEpg)
      VALUES (${id}, '00000000-0000-0000-0000-${String(id).padStart(12, '0')}', '${id}', ${id},
      'UCU164 channel', 'Synthetic', ${profile}, 5, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1);
    INSERT INTO Playout (Id, ChannelId, ScheduleKind, Seed) VALUES (${id}, ${id}, 2, 1);
    INSERT INTO PlayoutTemplate (PlayoutId, TemplateId, DecoTemplateId, DateUpdated, "Index",
      DaysOfWeek, DaysOfMonth, MonthsOfYear, LimitToDateRange, StartMonth, StartDay, EndMonth, EndDay)
      VALUES (${id}, ${id}, ${id}, '0', 1, '[1,2,3]', '[1,15]', '[4]', 1, 4, 1, 4, 30);
  `);
  const filters = () => sqlite(`SELECT DaysOfWeek || '|' || DaysOfMonth || '|' || MonthsOfYear || '|' ||
    LimitToDateRange || '|' || StartMonth || '|' || StartDay || '|' || EndMonth || '|' || EndDay
    FROM PlayoutTemplate WHERE PlayoutId=${id};`);
  const originalFilters = filters();
  await page.goto(`/playouts/${id}/templates`);
  await settled(page);
  const save = page.getByRole('button', { name: 'Save Templates', exact: true });
  await expect(page.locator('#template')).toHaveValue(String(id));
  await expect(page.locator('#template-group')).toHaveValue(String(id));
  await expect(page.locator('#deco-template')).toHaveValue(String(id));
  await expect(page.locator('#deco-template-group')).toHaveValue(String(id));
  await page.locator('#template-group').selectOption(String(id + 1));
  await expect(page.locator('#template')).toHaveValue('0');
  await expect(page.locator('#template-group')).toHaveValue(String(id + 1));
  await page.locator('#deco-template-group').selectOption(String(id + 1));
  await expect(page.locator('#deco-template')).toHaveValue('0');
  await page.locator('#template').selectOption(String(id + 1));
  await save.click();
  await expect.poll(() => sqlite(`SELECT TemplateId || '|' || COALESCE(DecoTemplateId, 'none')
    FROM PlayoutTemplate WHERE PlayoutId=${id};`)).toBe(`${id + 1}|none`);
  await expect(save).toBeEnabled();
  expect(filters()).toBe(originalFilters);
  await page.reload();
  await settled(page);
  await expect(page.locator('#template')).toHaveValue(String(id + 1));
  await expect(page.locator('#deco-template')).toHaveValue('0');
  await page.locator('#deco-template-group').selectOption(String(id + 1));
  await page.locator('#deco-template').selectOption(String(id + 1));
  await save.click();
  await expect.poll(() => sqlite(`SELECT DecoTemplateId FROM PlayoutTemplate WHERE PlayoutId=${id};`))
    .toBe(String(id + 1));
  await expect(save).toBeEnabled();
  expect(filters()).toBe(originalFilters);
  await page.reload();
  await settled(page);
  await expect(page.locator('#deco-template')).toHaveValue(String(id + 1));
  await page.screenshot({ path: testInfo.outputPath('playout-replacement.png'), fullPage: true });
  expect(errors).toEqual([]);
});
