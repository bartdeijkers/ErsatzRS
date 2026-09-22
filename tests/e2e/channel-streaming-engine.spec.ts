import { expect, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('E2E_DB_PATH is required');
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`, encoding: 'utf8',
  }).trim();
}

test.describe('channel streaming engine', () => {
  test.skip(!dbPath, 'Requires a disposable E2E database');

  for (const javaScriptEnabled of [true, false]) {
    test(`persists engine and enforces Next mode with JavaScript=${javaScriptEnabled}`, async ({ browser, baseURL }) => {
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const name = `E2E Engine ${Date.now()} ${javaScriptEnabled}`;
      try {
        await page.request.post('/api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        await page.goto('/channels/add');
        const engine = page.locator('select[name="streaming_engine"]');
        const mode = page.locator('select[name="streaming_mode"]');
        await expect(engine).toHaveValue('0');
        await mode.selectOption('5');
        await engine.selectOption('1');
        if (javaScriptEnabled) {
          await expect(mode).toHaveValue('4');
          await expect(mode.locator('option[value="5"]')).toBeDisabled();
        } else {
          // A normal HTML form still submits the stale mode; the server must normalize it.
          await expect(mode).toHaveValue('5');
        }
        await page.locator('input[name="name"]').fill(name);
        await page.getByRole('button', { name: 'Add Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        const id = sqlite(`SELECT "Id" FROM "Channel" WHERE "Name" = '${name}';`);
        expect(id).toMatch(/^\d+$/);
        expect(sqlite(`SELECT "StreamingEngine", "StreamingMode" FROM "Channel" WHERE "Id" = ${id};`)).toBe('1|4');
        await page.goto(`/channels/${id}`);
        await expect(engine).toHaveValue('1');
        await expect(mode).toHaveValue('4');
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        await page.goto(`/channels/${id}`);
        await engine.selectOption('0');
        if (javaScriptEnabled) await mode.selectOption('5');
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        await page.goto(`/channels/${id}`);
        await expect(engine).toHaveValue('0');
        // Without JS, reloading after the engine change restores all native options.
        await mode.selectOption('5');
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        expect(sqlite(`SELECT "StreamingEngine", "StreamingMode" FROM "Channel" WHERE "Id" = ${id};`)).toBe('0|5');
      } finally {
        sqlite(`DELETE FROM "Channel" WHERE "Name" = '${name}';`);
        await context.close();
      }
    });
  }
});
