import { expect, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const databasePath = process.env.E2E_DB_PATH;
test.skip(!databasePath, 'Run through scripts/e2e/playwright-e2e.sh with an isolated database.');
test.describe.configure({ mode: 'serial' });

test('All loads every row, resets the page, and preserves saved list preferences', async ({ page }, info) => {
  test.setTimeout(120_000);
  const db = new DatabaseSync(databasePath!);
  db.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
  const first = 8_000_000 + (info.project.name === 'firefox' ? 1_000 : 0);
  const last = first + 30;
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const cleanup = () => {
    for (const table of ['Playout', 'Channel', 'FillerPreset', 'RerunCollection', 'MultiCollection',
      'SmartCollection', 'Collection', 'ProgramSchedule', 'FFmpegProfile']) {
      db.prepare(`DELETE FROM "${table}" WHERE "Id" BETWEEN ? AND ?`).run(first, last);
    }
  };
  try {
    cleanup();
    const profile = db.prepare('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1').get() as { Id: number };
    const columns = db.prepare('PRAGMA table_info("FFmpegProfile")').all() as { name: string }[];
    const names = columns.map(({ name }) => `"${name.replaceAll('"', '""')}"`);
    const expressions = columns.map(({ name }, index) => name === 'Id' || name === 'Name' ? '?' : names[index]);
    const cloneProfile = db.prepare(`INSERT INTO "FFmpegProfile" (${names.join(',')})
      SELECT ${expressions.join(',')} FROM "FFmpegProfile" WHERE "Id"=?`);
    for (let id = first; id <= last; id++) {
      const name = `Pagination ${info.project.name} ${id}`;
      const values = columns.filter(({ name }) => name === 'Id' || name === 'Name')
        .map(column => column.name === 'Id' ? id : name);
      cloneProfile.run(...values, profile.Id);
      db.prepare(`INSERT INTO "Channel" ("Id","UniqueId","Number","SortNumber","Name","FFmpegProfileId",
        "StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode","IdleBehavior",
        "MusicVideoCreditsMode","SongVideoMode","SubtitleMode") VALUES (?,?,?,?,?,?,5,0,0,0,0,0,0,0,0)`)
        .run(id, `00000000-0000-0000-0000-${String(id).padStart(12, '0')}`, String(id), id, name, profile.Id);
      db.prepare('INSERT INTO "Collection" ("Id","Name","UseCustomPlaybackOrder") VALUES (?,?,0)').run(id, name);
      db.prepare('INSERT INTO "SmartCollection" ("Id","Name","Query") VALUES (?,?,?)').run(id, name, 'type:movie');
      db.prepare('INSERT INTO "MultiCollection" ("Id","Name") VALUES (?,?)').run(id, name);
      db.prepare(`INSERT INTO "RerunCollection" ("Id","Name","CollectionType","CollectionId","FirstRunPlaybackOrder","RerunPlaybackOrder")
        VALUES (?,?,0,?,0,0)`).run(id, name, id);
      db.prepare(`INSERT INTO "FillerPreset" ("Id","Name","AllowWatermarks","CollectionType","CollectionId","FillerKind","FillerMode","UseChaptersAsMediaItems")
        VALUES (?,?,0,0,?,0,0,0)`).run(id, name, id);
      db.prepare(`INSERT INTO "ProgramSchedule" ("Id","Name","FixedStartTimeBehavior","KeepMultiPartEpisodesTogether",
        "RandomStartPoint","ShuffleScheduleItems","TreatCollectionsAsShows") VALUES (?,?,0,0,0,0,0)`).run(id, name);
      db.prepare('INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ProgramScheduleId","Seed") VALUES (?,?,1,?,1)').run(id, id, id);
      db.prepare(`INSERT INTO "PlayoutAnchor" ("PlayoutId","InDurationFiller","InFlood","NextGuideGroup","NextStart")
        VALUES (?,0,0,0,'2099-01-01 00:00:00')`).run(id);
    }
    await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0,
    });
    const routes = [
      ['/channels', 'Channel', false], ['/ffmpeg', 'FFmpegProfile', false],
      ['/media/collections', 'Collection', false], ['/media/smart-collections', 'SmartCollection', false],
      ['/media/multi-collections', 'MultiCollection', true], ['/media/rerun-collections', 'RerunCollection', true],
      ['/media/filler/presets', 'FillerPreset', true], ['/schedules', 'ProgramSchedule', true],
      ['/playouts', 'Playout', false],
    ] as const;
    for (const [route, table, saved] of routes) {
      await test.step(route, async () => {
        await page.goto(route);
        await expect.poll(() => page.evaluate(() =>
          (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated)).toBe(true);
        const choice = page.locator('[data-pagination-page-size]').first();
        const pager = page.locator('[data-pagination]').first();
        const rows = page.locator('main table').first().locator('tbody tr:visible');
        const total = (db.prepare(`SELECT COUNT(*) AS total FROM "${table}"`).get() as { total: number }).total;
        await expect(choice.locator('option').last()).toHaveText('All');
        await choice.selectOption('10');
        await expect(rows).toHaveCount(10);
        await pager.getByRole('button', { name: 'Next page', exact: true }).click();
        await expect(pager).toContainText('Page 2');
        await choice.selectOption('0');
        await expect(rows).toHaveCount(total);
        await expect(pager).toContainText('Page 1 of 1');
        await expect(pager.getByRole('button', { name: 'Previous page', exact: true })).toBeDisabled();
        await expect(pager.getByRole('button', { name: 'Next page', exact: true })).toBeDisabled();
        if (saved) {
          await page.reload();
          await expect(choice).toHaveValue('0');
          await expect(rows).toHaveCount(total);
        }
        await choice.selectOption('10');
        await expect(rows).toHaveCount(10);
      });
    }
    expect(errors).toEqual([]);
  } finally {
    cleanup();
    db.close();
  }
});
