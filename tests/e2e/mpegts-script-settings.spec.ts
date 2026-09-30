import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const dbPath = process.env.E2E_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('E2E_DB_PATH is required');
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`, encoding: 'utf8',
  }).trim();
}

test.describe('MPEG-TS script settings', () => {
  test.skip(!dbPath, 'Requires the isolated Playwright wrapper database');

  test('preserves an operator name for the default script', async ({ page }) => {
    const definition = join(dirname(dbPath!), 'app-data', 'ErsatzRS', 'scripts', 'mpegts', 'default', 'mpegts.yml');
    const original = readFileSync(definition, 'utf8');
    try {
      writeFileSync(definition, original.replace(/^name:.*$/m, 'name: UCU162 operator default'));
      await page.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
      });
      await page.goto('/settings/ffmpeg');
      await expect(page.locator('select[name="default_mpeg_ts_script"] option[value="default"]')).toHaveText('UCU162 operator default');
    } finally {
      writeFileSync(definition, original);
    }
  });

  for (const javaScriptEnabled of [true, false]) {
    test(`discovers, saves and recovers deleted scripts (${javaScriptEnabled ? 'JS' : 'no-JS'})`, async ({ browser, baseURL }, testInfo) => {
      const id = `ucu162-${testInfo.project.name}-${javaScriptEnabled ? 'js' : 'nojs'}`;
      const folder = join(dirname(dbPath!), 'app-data', 'ErsatzRS', 'scripts', 'mpegts', id);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      const settled = async () => {
        if (javaScriptEnabled) {
          await expect.poll(() => page.evaluate(() =>
            (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
          ), { timeout: 15_000 }).toBe(true);
        }
      };
      const original = sqlite("SELECT Value FROM ConfigElement WHERE Key='ffmpeg.default_mpegts_script';");
      mkdirSync(folder, { recursive: true });
      writeFileSync(join(folder, 'mpegts.yml'), 'name: UCU162 custom\nlinux_script: run.sh\nwindows_script: run.bat\n');
      try {
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        const tracks = sqlite("SELECT Key || '=' || Value FROM ConfigElement WHERE Key IN ('ffmpeg.preferred_language_code', 'ffmpeg.preferred_subtitle_language_code', 'ffmpeg.fallback_subtitle_language_code') ORDER BY Key;");
        await page.goto('/settings/ffmpeg');
        await settled();
        const select = page.locator('select[name="default_mpeg_ts_script"]');
        await expect(select.locator(`option[value="${id}"]`)).toHaveText('UCU162 custom');
        await select.selectOption(id);
        await page.getByRole('button', { name: 'Save Settings', exact: true }).click();
        await settled();
        await expect.poll(() => sqlite("SELECT Value FROM ConfigElement WHERE Key='ffmpeg.default_mpegts_script';")).toBe(id);
        await page.goto('/settings/ffmpeg');
        await settled();
        await expect(select).toHaveValue(id);
        writeFileSync(join(folder, 'mpegts.yml'), 'name: UCU162 edited\nlinux_script: changed.sh\n');
        await page.reload();
        await settled();
        await expect(select.locator(`option[value="${id}"]`)).toHaveText('UCU162 edited');
        await expect(select).toHaveValue(id);
        rmSync(folder, { recursive: true });
        await page.reload();
        await settled();
        await expect(select.locator(`option[value="${id}"]`)).toHaveCount(0);
        await expect(select).toHaveValue('default');
        expect(sqlite("SELECT Value FROM ConfigElement WHERE Key='ffmpeg.default_mpegts_script';")).toBe(id);
        await page.getByRole('button', { name: 'Save Settings', exact: true }).click();
        await settled();
        await expect.poll(() => sqlite("SELECT Value FROM ConfigElement WHERE Key='ffmpeg.default_mpegts_script';")).toBe('default');
        expect(sqlite("SELECT Key || '=' || Value FROM ConfigElement WHERE Key IN ('ffmpeg.preferred_language_code', 'ffmpeg.preferred_subtitle_language_code', 'ffmpeg.fallback_subtitle_language_code') ORDER BY Key;")).toBe(tracks);
        expect(errors).toEqual([]);
      } finally {
        rmSync(folder, { recursive: true, force: true });
        sqlite(`UPDATE ConfigElement SET Value='${original.replaceAll("'", "''")}' WHERE Key='ffmpeg.default_mpegts_script';`);
        await context.close();
      }
    });
  }
});
