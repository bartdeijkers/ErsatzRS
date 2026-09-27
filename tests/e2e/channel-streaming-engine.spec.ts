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
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        await page.goto('/channels/add');
        const engine = page.locator('select[name="streaming_engine"]');
        const mode = page.locator('select[name="streaming_mode"]');
        await expect(engine).toHaveValue('0');
        await mode.selectOption('1');
        await engine.selectOption('1');
        if (javaScriptEnabled) {
          await expect(mode).toHaveValue('4');
          await expect(mode.locator('option[value="5"]')).toBeEnabled();
          await expect(mode.locator('option[value="1"]')).toBeDisabled();
          await expect(mode.locator('option[value="2"]')).toBeDisabled();
        } else {
          // A normal HTML form still submits the stale mode; the server must normalize it.
          await expect(mode).toHaveValue('1');
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
        const subtitles = page.locator('select[name="next_engine_text_subtitle_mode"]');
        await expect(subtitles).toBeVisible();
        await expect(subtitles).toHaveValue('0');
        const subtitleHelp = page.getByText('Converted text subtitles are available through HLS only. Select Burn for subtitles in MPEG-TS.', { exact: true });
        await expect(subtitleHelp).toBeVisible();
        for (const width of [1280, 375]) {
          await page.setViewportSize({ width, height: 900 });
          const bounds = await subtitles.boundingBox();
          expect(bounds).not.toBeNull();
          expect(bounds!.x).toBeGreaterThanOrEqual(0);
          expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
          const helpBounds = await subtitleHelp.boundingBox();
          expect(helpBounds).not.toBeNull();
          expect(helpBounds!.x).toBeGreaterThanOrEqual(0);
          expect(helpBounds!.x + helpBounds!.width).toBeLessThanOrEqual(width);
          expect(await page.evaluate(() => document.documentElement.scrollWidth))
            .toBeLessThanOrEqual(width);
        }
        await mode.selectOption('5');
        await subtitles.selectOption('1');
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        await page.goto(`/channels/${id}`);
        await expect(mode).toHaveValue('5');
        expect(sqlite(`SELECT "StreamingMode" FROM "Channel" WHERE "Id" = ${id};`)).toBe('5');
        await expect(subtitles).toHaveValue('1');
        expect(sqlite(`SELECT "NextEngineTextSubtitleMode" FROM "Channel" WHERE "Id" = ${id};`)).toBe('1');
        // A metadata-only save and fresh SSR render must retain Hybrid in Next.
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        await page.goto(`/channels/${id}`);
        await expect(mode).toHaveValue('5');
        await mode.selectOption('4');
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        await page.goto(`/channels/${id}`);
        await expect(mode).toHaveValue('4');
        expect(sqlite(`SELECT "StreamingMode" FROM "Channel" WHERE "Id" = ${id};`)).toBe('4');
        await engine.selectOption('0');
        if (javaScriptEnabled) await expect(subtitles).toBeHidden();
        if (javaScriptEnabled) await mode.selectOption('5');
        await page.getByRole('button', { name: 'Save Channel' }).click();
        await expect(page).toHaveURL(/\/channels$/);
        await page.goto(`/channels/${id}`);
        await expect(engine).toHaveValue('0');
        await expect(subtitles).toHaveValue('1');
        await expect(subtitles).toBeHidden();
        await expect(subtitleHelp).toBeHidden();
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
