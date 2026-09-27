import { expect, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const sourceId = 9_900_711;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('An isolated E2E database is required');
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

test.describe('Plex reauthentication and retained missing servers', () => {
  test.skip(!dbPath, 'Requires the isolated Playwright runner database');
  test.beforeEach(async ({ page }) => {
    sqlite(`DELETE FROM "MediaSource" WHERE "Id"=${sourceId};
      INSERT INTO "MediaSource" ("Id") VALUES (${sourceId});
      INSERT INTO "PlexMediaSource" ("Id","ServerName","ClientIdentifier","MissingSince")
        VALUES (${sourceId},'Retained Plex','e2e-retained-plex','2026-01-02T03:04:05Z');
      INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId")
        VALUES (${sourceId},'Retained library',1,${sourceId});
      INSERT INTO "PlexLibrary" ("Id","Key","ShouldSyncItems") VALUES (${sourceId},'synthetic',1);
      INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (${sourceId},${sourceId},'plex://synthetic');
      INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (${sourceId},${sourceId},0);`);
    await page.request.post('/admin-api/settings/ui/save', {
      form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
    });
  });
  test.afterEach(() => sqlite(`DELETE FROM "MediaSource" WHERE "Id"=${sourceId};`));

  test('re-authentication requests a fresh identity and failed attempts retain content', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const requests: string[] = [];
    await page.route('**/admin-api/start_plex_pin_flow*', async route => {
      requests.push(route.request().postData() ?? '');
      await route.fulfill({ status: 503, contentType: 'text/plain', body: 'synthetic provider failure' });
    });
    await page.goto('/media/sources/plex');
    await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
    await page.getByRole('button', { name: 'Re-authenticate with Plex', exact: true }).click();
    await expect(page.getByText('Error: Plex sign-in failed or expired. Try again.', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Retry', exact: true }).click();
    await expect.poll(() => requests.length).toBe(2);
    expect(requests.every(body => new URLSearchParams(body).get('force_new_credentials') === 'true')).toBeTruthy();
    expect(sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "Id"=${sourceId}`)).toBe('1');
    expect(errors).toEqual([]);
  });

  test('PIN polling failure can be retried and completion reloads retained sources', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    let attempts = 0;
    let polls = 0;
    await page.route('**/admin-api/start_plex_pin_flow*', async route => {
      attempts += 1;
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify([
        'synthetic-pin', 'https://app.plex.tv/auth#synthetic-test',
      ]) });
    });
    await page.route('**/admin-api/check_plex_pin*', async route => {
      polls += 1;
      if (attempts === 1 && polls > 1) {
        await route.fulfill({ status: 503, body: 'synthetic expired flow' });
      } else {
        await route.fulfill({ contentType: 'application/json', body: attempts > 1 ? 'true' : 'false' });
      }
    });
    await page.goto('/media/sources/plex');
    await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
    await page.getByRole('button', { name: 'Re-authenticate with Plex', exact: true }).click();
    await expect(page.getByRole('link', { name: /authorize/i })).toHaveAttribute('href', 'https://app.plex.tv/auth#synthetic-test');
    await expect(page.getByRole('button', { name: 'Sign out of Plex', exact: true })).toBeDisabled();
    await expect(page.getByRole('row').filter({ hasText: 'Retained Plex' }).getByRole('button', { name: 'Delete', exact: true })).toBeDisabled();
    await expect(page.getByText('Error: Plex sign-in failed or expired. Try again.', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Sign out of Plex', exact: true })).toBeEnabled();
    const navigation = page.waitForEvent('framenavigated', frame => frame === page.mainFrame());
    await page.getByRole('button', { name: 'Retry', exact: true }).click();
    await navigation;
    await expect(page.getByRole('row').filter({ hasText: 'Retained Plex' })).toBeVisible();
    expect(attempts).toBe(2);
    expect(polls).toBe(3);
    expect(sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "Id"=${sourceId}`)).toBe('1');
    expect(errors).toEqual([]);
  });

  test('missing-server removal requires confirmation and rejects a returned server', async ({ page }, testInfo) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.setViewportSize({ width: 360, height: 800 });
    await page.goto('/media/sources/plex');
    await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
    const row = page.getByRole('row').filter({ hasText: 'Retained Plex' });
    await expect(row.getByText('Server missing from Plex', { exact: true })).toBeVisible();
    await expect(row.getByRole('button', { name: 'Refresh', exact: true })).toBeDisabled();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBeTruthy();
    await testInfo.attach('plex-missing-server-narrow', {
      body: await page.screenshot({ fullPage: true }), contentType: 'image/png',
    });
    await row.getByRole('button', { name: 'Delete', exact: true }).click();
    const dialog = page.locator('#plex-remove-server-dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText('Retained Plex', { exact: true })).toBeVisible();
    await expect(dialog.getByText(/Files on the Plex server are not deleted/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "Id"=${sourceId}`)).toBe('1');
    await row.getByRole('button', { name: 'Delete', exact: true }).click();
    sqlite(`UPDATE "PlexMediaSource" SET "MissingSince"=NULL WHERE "Id"=${sourceId}`);
    await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(page.locator('[role="alert"]').filter({ hasText: /.+/ }).last()).toBeVisible();
    expect(sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "Id"=${sourceId}`)).toBe('1');
    sqlite(`UPDATE "PlexMediaSource" SET "MissingSince"='2026-01-02T03:04:05Z' WHERE "Id"=${sourceId}`);
    await page.reload();
    await row.getByRole('button', { name: 'Delete', exact: true }).click();
    await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(row).toHaveCount(0);
    expect(sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "Id"=${sourceId}`)).toBe('0');
    expect(errors).toEqual([]);
  });
});
