import { expect, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const databasePath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

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
          await page.waitForFunction(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
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
          await page.goto('/system/health');
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
