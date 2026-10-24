import { expect, Page, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const databasePath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

// Row250: expose incomplete filesystem repair through the existing Health flow.
for (const javaScriptEnabled of [true, false]) {
  for (const isDarkMode of [true, false]) {
    test(`subtitle cache repair health (JavaScript ${javaScriptEnabled}, dark ${isDarkMode})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E database.');
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000;');
      const key = 'ffmpeg.subtitle_cache_nul_repair_pending';
      const original = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get(key) as { Value: string } | undefined;
      const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 320, height: 800 } });
      const write = (value: string) => database.prepare('INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)').run(key, value);
      try {
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        for (const [language, refresh, message] of [
          ['en-us', 'Refresh', 'Subtitle cache repair has not finished. Some cached subtitles may not display. Automatic cache maintenance will retry. If this warning persists, check write access to the subtitle cache and available disk space.'],
          ['nl', 'Vernieuwen', 'Het herstellen van de ondertitelcache is nog niet afgerond. Sommige ondertitels in de cache worden mogelijk niet weergegeven. Het automatische cacheonderhoud probeert het opnieuw. Blijft deze waarschuwing staan, controleer dan de schrijftoegang tot de ondertitelcache en de beschikbare schijfruimte.'],
        ]) {
          write('true');
          expect((await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: String(isDarkMode), language }, maxRedirects: 0 })).status()).toBe(303);
          expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
          await page.goto('/system/health');
          if (javaScriptEnabled) await hydrated(page);
          const row = page.getByRole('row').filter({ hasText: message });
          await expect(row).toBeVisible();
          expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2)).toBe(true);
          const stored = () => {
            const snapshot = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get('health.results') as { Value: string };
            return (JSON.parse(snapshot.Value) as { title: string; status: string }[]).find(result => result.title === 'Subtitle Cleanup');
          };
          expect(stored()?.status).toBe('Warning');
          await row.screenshot({ path: testInfo.outputPath(`subtitle-cache-health-${language}.png`) });
          write('false');
          const refreshed = page.waitForResponse(response => response.request().method() === 'POST' && response.url().includes(
            javaScriptEnabled ? '/admin-api/refresh_health_snapshot' : '/admin-api/system/health/refresh',
          ));
          await page.getByRole('button', { name: refresh, exact: true }).click();
          expect((await refreshed).status()).toBe(javaScriptEnabled ? 200 : 303);
          await expect(row).not.toBeVisible();
          expect(stored()?.status).toBe('Pass');
          expect(database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get(key)).toEqual({ Value: 'false' });
        }
        expect(errors).toEqual([]);
      } finally {
        if (original) write(original.Value);
        else database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run(key);
        await context.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
        await context.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 });
        database.close();
        await context.close();
      }
    });
  }
}

async function hydrated(page: Page): Promise<void> {
  await page.waitForFunction(
    () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    undefined,
    { timeout: 15_000 },
  );
}

// These fixtures belong only to the disposable E2E database. Browser projects
// run serially because the health snapshot is intentionally shared by the host.
test.describe('shared health results', () => {
  for (const javaScriptEnabled of [true, false]) {
    test(`refresh publishes current media state (JavaScript ${javaScriptEnabled})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E database.');
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
      const id = 7_136_000 + (testInfo.project.name === 'firefox' ? 100 : 0) + Number(javaScriptEnabled);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const cleanup = () => {
        database.prepare('DELETE FROM "MediaItem" WHERE "Id" = ?').run(id);
        database.prepare('DELETE FROM "LibraryPath" WHERE "Id" = ?').run(id);
        database.prepare('DELETE FROM "Library" WHERE "Id" = ?').run(id);
        database.prepare('DELETE FROM "MediaSource" WHERE "Id" = ?').run(id);
      };
      try {
        cleanup();
        database.prepare('INSERT INTO "MediaSource" ("Id") VALUES (?)').run(id);
        database.prepare('INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (?, ?, 1, ?)').run(id, 'Synthetic health refresh', id);
        database.prepare('INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (?, ?, ?)').run(id, '/synthetic-health-refresh', id);
        database.prepare('INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, ?, 0)').run(id, id);
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
        const baseline = await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 });
        expect(baseline.status()).toBe(303);
        await page.goto('/system/health');
        const row = page.getByRole('row').filter({ has: page.getByText('File Not Found', { exact: true }) });
        await expect(row).toBeVisible();
        await expect(row).not.toContainText(`MediaItem ${id}`);
        if (javaScriptEnabled) {
          await hydrated(page);
          expect(errors).toEqual([]);
          // Establish the revision baseline before changing the synthetic item.
          await page.waitForResponse(response => response.url().includes('/admin-api/media_state_revision') && response.ok());
        }
        database.prepare('UPDATE "MediaItem" SET "State" = 1 WHERE "Id" = ?').run(id);
        if (!javaScriptEnabled) {
          // Ordinary navigation must reuse the completed cache.
          await page.reload();
          await expect(row).not.toContainText(`MediaItem ${id}`);
        }
        const refreshed = page.waitForResponse(response => response.request().method() === 'POST' && response.url().includes(
          javaScriptEnabled ? '/admin-api/refresh_health_snapshot' : '/admin-api/system/health/refresh',
        ));
        await page.getByRole('button', { name: 'Refresh', exact: true }).click();
        expect((await refreshed).status()).toBe(javaScriptEnabled ? 200 : 303);
        await expect(row).toContainText(`MediaItem ${id}`);
        await expect(page.getByRole('button', { name: 'Refresh', exact: true })).toBeEnabled();
        const stored = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?').get('health.results') as { Value: string };
        expect(stored.Value).toContain(`MediaItem ${id}`);
        if (javaScriptEnabled) {
          // Let the poll observe the missing state before simulating recovery;
          // a 0 -> 1 -> 0 change between polls has no different revision token.
          await page.waitForResponse(response => response.url().includes('/admin-api/media_state_revision') && response.ok());
          database.prepare('UPDATE "MediaItem" SET "State" = 0 WHERE "Id" = ?').run(id);
          // Existing automatic media-revision refresh remains functional.
          await expect(row).not.toContainText(`MediaItem ${id}`);
          await page.getByRole('button', { name: 'Refresh', exact: true }).click();
          await page.goto('/system/logs');
          await hydrated(page);
          await page.goto('/system/health');
          await hydrated(page);
          await expect(page.getByRole('button', { name: 'Refresh', exact: true })).toBeEnabled();
        }
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'nl' }, maxRedirects: 0,
        });
        await page.goto('/system/health');
        await expect(page.getByRole('button', { name: 'Vernieuwen', exact: true })).toBeVisible();
        await expect(page.getByText('Resultaten worden vijf minuten bewaard.', { exact: false })).toBeVisible();
        expect(errors).toEqual([]);
        await page.screenshot({ path: `test-results/health-refresh-${testInfo.project.name}-${javaScriptEnabled}.png`, fullPage: true });
      } finally {
        try {
          cleanup();
          await context.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 });
          await context.request.post('/admin-api/settings/ui/save', {
            form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
          });
        } finally {
          database.close();
          await context.close();
        }
      }
    });
  }
});

// ADR 0106: incompatible AV1 channel settings need actionable, localized guidance.
for (const javaScriptEnabled of [true, false]) {
  test(`AV1 output health guidance (JavaScript ${javaScriptEnabled})`, async ({ browser, baseURL }) => {
    test.skip(!databasePath, 'Requires the disposable E2E database.');
    const database = new DatabaseSync(databasePath!);
    database.exec('PRAGMA busy_timeout=10000;');
    const original = database.prepare(`SELECT c."Id", c."FFmpegProfileId", c."StreamingMode",
      c."StreamingEngine", c."IsEnabled", p."VideoFormat", p."TranscodeVideo"
      FROM "Channel" c JOIN "FFmpegProfile" p ON p."Id"=c."FFmpegProfileId"
      ORDER BY c."Id" LIMIT 1`).get() as Record<string, number>;
    expect(original).toBeTruthy();
    const context = await browser.newContext({ baseURL, javaScriptEnabled });
    try {
      database.prepare('UPDATE "FFmpegProfile" SET "VideoFormat"=4,"TranscodeVideo"=1 WHERE "Id"=?').run(original.FFmpegProfileId);
      database.prepare('UPDATE "Channel" SET "StreamingEngine"=0,"StreamingMode"=1,"IsEnabled"=1 WHERE "Id"=?').run(original.Id);
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
      for (const [language, message] of [
        ['en-us', 'AV1 output requires native HLS. Change the channel engine/mode or select H.264/HEVC in its FFmpeg profile.'],
        ['nl', 'AV1-uitvoer vereist native HLS. Wijzig de engine/modus van het kanaal of kies H.264/HEVC in het FFmpeg-profiel.'],
      ]) {
        await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language }, maxRedirects: 0 });
        await page.goto('/system/health');
        await expect(page.getByText(message, { exact: true })).toBeVisible();
        if (javaScriptEnabled) {
          await page.waitForFunction(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
        }
        expect(errors).toEqual([]);
      }
      expect(database.prepare('SELECT "VideoFormat" FROM "FFmpegProfile" WHERE "Id"=?').get(original.FFmpegProfileId)).toEqual({ VideoFormat: 4 });
    } finally {
      database.prepare('UPDATE "FFmpegProfile" SET "VideoFormat"=?,"TranscodeVideo"=? WHERE "Id"=?').run(original.VideoFormat, original.TranscodeVideo, original.FFmpegProfileId);
      database.prepare('UPDATE "Channel" SET "StreamingMode"=?,"StreamingEngine"=?,"IsEnabled"=? WHERE "Id"=?').run(original.StreamingMode, original.StreamingEngine, original.IsEnabled, original.Id);
      await context.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
      database.close();
      await context.close();
    }
  });
}
