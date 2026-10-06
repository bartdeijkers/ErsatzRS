import { expect, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join } from 'path';

function sqlite(sql: string): string {
  const database = process.env.E2E_DB_PATH;
  if (!database) throw new Error('Isolated E2E database is required');
  return execFileSync('sqlite3', [database], {
    input: `.timeout 10000\n${sql}`, encoding: 'utf8',
  }).trim();
}

function countLibraries(): string { return sqlite('SELECT COUNT(*) FROM Library;'); }

for (const javaScriptEnabled of [true, false]) {
  test.describe(javaScriptEnabled ? 'browser' : 'no JavaScript', () => {
    test.use({ javaScriptEnabled });
    test('television discovery previews the unsaved draft and preserves sources', async ({ page }, testInfo) => {
      const root = mkdtempSync(join(tmpdir(), 'ersatzrs-discovery-'));
      const name = `Discovery ${testInfo.project.name} ${javaScriptEnabled} & draft`;
      const files = [
        'Show/Season 1/Episode.MPEG', 'Show/Season 1/Episode.nl.srt',
        'Show/Season 1/Unknown.xyz', 'Show/Trailers/Trailer.ts',
        'Package/Nested Show/Season 1/Episode.mp4', 'Empty/tvshow.nfo',
      ];
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      for (const file of files) {
        mkdirSync(dirname(join(root, file)), { recursive: true });
        writeFileSync(join(root, file), 'synthetic bytes preserved by read-only preview');
      }
      try {
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        const before = countLibraries();
        await page.setViewportSize({ width: 390, height: 844 });
        await page.goto('/media/sources/local/add');
        if (javaScriptEnabled) await page.waitForFunction(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
        await page.locator('#local-library-name').fill(name);
        await page.locator('#local-library-kind').selectOption('2');
        await page.locator('#local-library-paths').fill(root);
        await page.getByRole('button', { name: 'Preview television folders' }).click();
        const results = page.locator('[data-folder-discovery]');
        await expect(results).toContainText('Unknown.xyz');
        await expect(results).toContainText('Unsupported nesting');
        await expect(results).toContainText('Shows without discovered media');
        await expect(results.locator('dt').filter({ hasText: /^Episodes$/ }).locator('xpath=following-sibling::dd[1]')).toHaveText('1');
        await expect(results.locator('dt').filter({ hasText: /^Extra video candidates$/ }).locator('xpath=following-sibling::dd[1]')).toHaveText('1');
        await expect(page.locator('#local-library-name')).toHaveValue(name);
        await expect(page.locator('#local-library-kind')).toHaveValue('2');
        await expect(page.locator('#local-library-paths')).toHaveValue(root);
        expect(countLibraries()).toBe(before);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        await page.screenshot({ path: testInfo.outputPath('folder-preview-mobile.png'), fullPage: true });
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'false', language: 'nl' }, maxRedirects: 0,
        });
        await page.getByRole('button', { name: 'Preview television folders' }).click();
        await expect(page.getByRole('button', { name: 'Tv-mappen bekijken' })).toBeVisible();
        await expect(results).toContainText('Genegeerde bestandsextensie');
        await page.setViewportSize({ width: 1440, height: 900 });
        await page.screenshot({ path: testInfo.outputPath('folder-preview-dutch-desktop.png'), fullPage: true });
        for (const file of files) expect(readFileSync(join(root, file), 'utf8')).toBe('synthetic bytes preserved by read-only preview');
        await page.getByRole('button', { name: 'Toevoegen', exact: true }).click();
        await expect(page).toHaveURL(/\/media\/sources\/local$/);
        expect(Number(countLibraries())).toBe(Number(before) + 1);
        expect(sqlite(`SELECT MediaKind FROM Library WHERE Name='${name}';`)).toBe('2');
        expect(errors).toEqual([]);
      } finally {
        sqlite(`PRAGMA foreign_keys=ON; DELETE FROM Library WHERE Name='${name}';`);
        rmSync(root, { recursive: true, force: true });
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
      }
    });
  });
}
