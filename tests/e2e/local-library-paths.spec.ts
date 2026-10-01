import { expect, Page, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { mkdtempSync, mkdirSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

test.afterEach(async ({ request }) => {
  // Restore the wrapper baseline even when a localized assertion fails.
  const response = await request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
  });
  expect(response.status()).toBe(303);
});

async function settled(page: Page, javaScriptEnabled: boolean): Promise<void> {
  if (javaScriptEnabled) {
    await page.waitForFunction(
      () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
      undefined,
      { timeout: 15_000 },
    );
  }
}

function sqlite(sql: string): string {
  const database = process.env.E2E_DB_PATH;
  if (!database) throw new Error('The isolated E2E database is required');
  return execFileSync('sqlite3', [database], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${sql}`, encoding: 'utf8',
  }).trim();
}

for (const javaScriptEnabled of [true, false]) {
  test.describe(javaScriptEnabled ? 'browser' : 'no JavaScript', () => {
    test.use({ javaScriptEnabled });

    test('local library rejects a relative path, preserves the draft and retains scanned media on edit', async ({ page }, testInfo) => {
      const directory = mkdtempSync(join(tmpdir(), 'ersatzrs-path-e2e-'));
      const literal = join(directory, '100%20Movies');
      const child = join(literal, 'child');
      mkdirSync(child, { recursive: true });
      const name = `UCU167 ${testInfo.project.name} ${javaScriptEnabled}`;
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      try {
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        await page.goto('/media/sources/local/add');
        await settled(page, javaScriptEnabled);
        await page.locator('#local-library-name').fill(name);
        await page.locator('#local-library-paths').fill('.');
        await page.getByRole('button', { name: 'Add', exact: true }).click();
        await expect(page.getByRole('alert')).toContainText('Path [.] must be an absolute path');
        await expect(page.locator('#local-library-name')).toHaveValue(name);
        await expect(page.locator('#local-library-paths')).toHaveValue('.');
        expect(sqlite(`SELECT COUNT(*) FROM Library WHERE Name='${name}';`)).toBe('0');
        await page.screenshot({ path: testInfo.outputPath('relative-path-error.png'), fullPage: true });
        await page.locator('#local-library-paths').fill(literal);
        await page.getByRole('button', { name: 'Add', exact: true }).click();
        await expect(page).toHaveURL(/\/media\/sources\/local$/);
        await settled(page, javaScriptEnabled);
        const id = sqlite(`SELECT Id FROM Library WHERE Name='${name}';`);
        expect(id).toMatch(/^\d+$/);
        const pathId = sqlite(`SELECT Id FROM LibraryPath WHERE LibraryId=${id};`);
        expect(sqlite(`SELECT Path FROM LibraryPath WHERE Id=${pathId};`)).toBe(literal);
        const mediaId = Number(sqlite(`INSERT INTO MediaItem (LibraryPathId, State) VALUES (${pathId}, 0); SELECT last_insert_rowid();`));
        await page.goto(`/media/sources/local/${id}/edit`);
        await settled(page, javaScriptEnabled);
        await page.locator('#local-library-name').fill(`${name} edited`);
        await page.getByRole('button', { name: 'Save', exact: true }).click();
        await expect(page).toHaveURL(/\/media\/sources\/local$/);
        await settled(page, javaScriptEnabled);
        expect(sqlite(`SELECT LibraryPathId FROM MediaItem WHERE Id=${mediaId};`)).toBe(pathId);
        await page.goto('/media/sources/local/add');
        await settled(page, javaScriptEnabled);
        await page.locator('#local-library-name').fill(`${name} conflict`);
        await page.locator('#local-library-paths').fill(child);
        await page.getByRole('button', { name: 'Add', exact: true }).click();
        await expect(page.getByRole('alert')).toContainText(`Path [${child}] overlaps another library path`);
        expect(sqlite(`SELECT COUNT(*) FROM Library WHERE Name='${name} conflict';`)).toBe('0');
        expect(errors).toEqual([]);
      } finally {
        rmSync(directory, { recursive: true, force: true });
      }
    });

    test('local library path errors use the selected language', async ({ page }, testInfo) => {
      await page.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'false', language: 'nl' }, maxRedirects: 0,
      });
      await page.goto('/media/sources/local/add');
      await settled(page, javaScriptEnabled);
      await page.locator('#local-library-name').fill('Synthetic Dutch draft');
      const unsafePath = '</textarea><script data-path-probe>invalid</script>';
      await page.locator('#local-library-paths').fill(unsafePath);
      await page.getByRole('button', { name: 'Toevoegen', exact: true }).click();
      await expect(page.getByRole('alert')).toContainText('moet een absoluut pad zijn');
      await expect(page.locator('#local-library-paths')).toHaveValue(unsafePath);
      await expect(page.locator('script[data-path-probe]')).toHaveCount(0);
      await expect(page.locator('html')).toHaveAttribute('lang', 'nl');
      await expect(page.locator('body')).toHaveAttribute('data-theme', 'light');
      await page.screenshot({ path: testInfo.outputPath('localized-path-error.png'), fullPage: true });
    });
  });
}
